//! Buildings on the map: the original 3D objects in their tribe's colours (textured from the
//! level's theme atlas) when the original files are allowed and the building is identified;
//! otherwise the generated building kit (unknown IDs keep a labelled box). Turned by their
//! facing, centred on the ground and leaning with it (`Tilted`).
//! By construction stage (`Building::stage`): a blueprint is a white mark on the ground; under
//! construction or being dismantled, a wooden structure of its shape with the parts already built
//! (`construction`); built, the whole model. An attacked building's walls shake, a hut with people
//! inside smokes from the top of its roof, a built one's torches burn (`flame`). Resting the cursor on a building (or one of the player's
//! plans) for `HOVER_SECS` shows its tooltip: for the player's, braves at work and wood.

use crate::blueprint::{mark_material, mark_mesh, set_mark};
use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::construction::{beams, box_frame, built_part, chimney, layered_box, object_edges, pushed, BEAM, INNER_GAP};
use crate::flame::{self, Flame, FlameFrames};
use crate::generated_buildings;
use crate::grounded::{ground_y, Grounded, Tilted};
use crate::original_models::{atlas_image, object_mesh, solid_part, to_mesh, MeshData, OriginalObjects};
use crate::sites::tribe_color;
use crate::world::{CurrentMap, LevelList};
use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::PrimitiveTopology;
use bevy::render::render_resource::Face;
use bevy::prelude::*;
use game_core::building::{Building, BuildingKind, Stage};
use pop3_format::catalog::{self, villager_hut, Building as Object};
use pop3_format::{Atlas, Theme, WORLD_UNITS_PER_CELL};
use std::collections::HashMap;

/// Half size (cells) of the square the ground slope under a building is measured over.
const FOOTPRINT_HALF: f32 = 1.0;
/// Stand-in box (cells) when there is no model.
const BOX: Vec3 = Vec3::new(1.6, 0.8, 1.6);
/// Theme whose atlas textures buildings on maps without one.
const DEFAULT_THEME: u8 = 0;
/// Colour of the wooden structure.
const WOOD: Color = Color::srgb(0.55, 0.37, 0.2);
/// Attacked: the building tilts about its base by up to this much (radians) at each blow, one
/// blow every `BLOW` seconds, the jolt dying out before the next.
const SHAKE: f32 = 0.012;
const BLOW: f32 = 0.7;
/// Chimney smoke: puffs, seconds for one to rise, how high (cells) and how wide it gets.
const PUFFS: usize = 5;
const PUFF_LIFE: f32 = 2.5;
const PUFF_RISE: f32 = 1.2;
const PUFF_SIZE: f32 = 0.22;
/// Height of the white mark of a blueprint above the ground (render units).
const MARK_LIFT: f32 = 0.02;
/// Size (cells) of the flame over a generated building's brazier.
const BRAZIER_FLAME: f32 = 0.7;

/// The original object drawing a building of `owner`, None if not identified (neutral buildings
/// take the blue version).
pub fn building_object(kind: BuildingKind, owner: u8) -> Option<usize> {
    let tribe = if owner < catalog::TRIBES { owner } else { 0 };
    Some(match kind {
        BuildingKind::Hut { size } => villager_hut(0, tribe, size),
        BuildingKind::DrumTower => Object::DrumTower.object(tribe),
        BuildingKind::Temple => Object::PrayerHut.object(tribe),
        BuildingKind::SpyTraining => Object::SpyHut.object(tribe),
        BuildingKind::WarriorTraining => Object::WarriorTraining.object(tribe),
        BuildingKind::FirewarriorTraining => Object::FirewarriorTraining.object(tribe),
        BuildingKind::BoatHut => Object::BoatHut.object(tribe),
        BuildingKind::AirshipHut => Object::AirshipHut.object(tribe),
        BuildingKind::Vault => catalog::KNOWLEDGE_PYRAMID,
        BuildingKind::Prison => catalog::PRISON,
        BuildingKind::Reconversion | BuildingKind::WallPiece | BuildingKind::Gate | BuildingKind::GuardPost | BuildingKind::Other(_) => return None,
    })
}

/// Turn for a facing in eighths of a turn.
pub fn facing_yaw(facing: u8) -> f32 {
    facing as f32 * std::f32::consts::FRAC_PI_4
}

#[derive(Component)]
pub struct BuildingView(pub usize);

/// Each drawn building's model height (cells), by stored corner: where a lookout stands.
#[derive(Resource, Default)]
pub struct ModelHeights(pub HashMap<(u16, u16), f32>);

/// Where someone inside a building of `kind` stands to be seen: on a tower's lookout platform, as an
/// offset in its own frame (cells, x and z) and a fraction of its model's height. None: out of sight.
pub fn lookout(kind: BuildingKind) -> Option<(Vec2, f32)> {
    match kind {
        // The middle of the platform (the kit's at 1.365 of 2.45 cells), drawn pulled towards the camera
        // so the drum never hides him (`units::PERCH_PULL`).
        BuildingKind::DrumTower => Some((Vec2::ZERO, 0.56)),
        _ => None,
    }
}

/// Where a unit inside the building `b` (model `height` cells tall) is drawn: map position (cells)
/// and height above the ground (cells); None if it is out of sight.
pub fn perch(b: &Building, height: f32) -> Option<(Vec2, f32)> {
    let (offset, fraction) = lookout(b.kind)?;
    let cell = WORLD_UNITS_PER_CELL as f32;
    let (cx, cz) = b.centre();
    let at = Vec2::new(cx as f32, cz as f32) / cell + crate::blueprint::turn_local(offset, b.facing);
    Some((at, height * fraction))
}

/// The white mark of the building at this index of `GameMap::buildings`, a blueprint: redrawn
/// over the ground every frame (it follows the planet's curve as the camera moves).
#[derive(Component)]
struct SiteMark(usize);

/// The part of a view that shakes (the building is being attacked).
#[derive(Component)]
struct Shaking;

/// One puff of smoke over a busy hut, out of `chimney` (building frame, cells), `phase` (0-1)
/// apart from the others.
#[derive(Component)]
struct Puff {
    chimney: Vec3,
    phase: f32,
}

/// The name over a stand-in box, following its building on screen.
#[derive(Component)]
struct BuildingLabel(Entity);

pub struct BuildingsPlugin;

impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModelHeights>()
            .add_systems(Startup, spawn_tooltip)
            .add_systems(Update, building_tooltip.after(crate::hover::HoverSystems).in_set(crate::menu::Gameplay))
            .add_systems(Update, (respawn_buildings, place_labels).chain())
            .add_systems(Update, (shake_walls, rise_smoke))
            .add_systems(PostUpdate, draw_site_marks);
    }
}

/// The theme atlas as a material, None without original files.
fn original_material(levels: &LevelList, theme: u8, images: &mut Assets<Image>, mats: &mut Assets<StandardMaterial>) -> Option<Handle<StandardMaterial>> {
    if !levels.original {
        return None;
    }
    let load = || -> Result<_, pop3_format::LevelError> { Ok((Atlas::load(&levels.data_dir, theme)?, Theme::load(&levels.data_dir, theme)?)) };
    let (atlas, palette) = load().map_err(|e| warn!("building atlas for theme {theme}: {e}")).ok()?;
    Some(mats.add(StandardMaterial {
        base_color_texture: Some(images.add(atlas_image(&atlas, &palette.palette))),
        alpha_mode: AlphaMode::Mask(0.5),
        perceptual_roughness: 0.95,
        double_sided: true,
        cull_mode: None,
        ..default()
    }))
}

/// The generated kit's shared surface atlas, made once the first generated building is shown.
fn generated_skin(images: &mut Assets<Image>, mats: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
    mats.add(StandardMaterial {
        base_color_texture: Some(images.add(crate::generated_buildings::surface_image())),
        perceptual_roughness: 0.9,
        ..default()
    })
}

/// A building's meshes at its stage: the built part of its model (None when nothing is built yet),
/// that part's inner side while open, the wooden structure while building, its chimney top.
struct Staged {
    shown: Option<Handle<Mesh>>,
    inner: Option<Handle<Mesh>>,
    frame: Option<Handle<Mesh>>,
    top: Vec3,
}

/// `full` as shown with `pieces` (`used`, `of`) built, or whole when None (built), no frame.
fn staged(full: &MeshData, pieces: Option<(u8, u8)>, meshes: &mut Assets<Mesh>) -> Staged {
    let top = chimney(full);
    let Some((used, of)) = pieces else {
        return Staged { shown: Some(meshes.add(to_mesh(full.clone()))), inner: None, frame: None, top };
    };
    let part = built_part(full, used, of);
    if part.indices.is_empty() {
        return Staged { shown: None, inner: None, frame: None, top };
    }
    let inner = meshes.add(to_mesh(pushed(&part, INNER_GAP)));
    Staged { shown: Some(meshes.add(to_mesh(part))), inner: Some(inner), frame: None, top }
}

/// Kit meshes shared by every building of one kind and owner, made the first time one shows:
/// the built model with its chimney top, and the construction frame.
#[derive(Default)]
struct KitMeshes {
    built: HashMap<(BuildingKind, u8), (Handle<Mesh>, Vec3)>,
    frames: HashMap<(BuildingKind, u8), Handle<Mesh>>,
}

/// The kit's meshes of a building (see `staged`), None if the kit has no valid model for it.
fn kit_staged(cache: &mut KitMeshes, kind: BuildingKind, owner: u8, pieces: Option<(u8, u8)>, meshes: &mut Assets<Mesh>) -> Option<Staged> {
    let key = (kind, owner);
    if pieces.is_none() {
        if !cache.built.contains_key(&key) {
            let full = generated_buildings::body(kind, owner)?;
            let top = chimney(&full);
            cache.built.insert(key, (meshes.add(to_mesh(full)), top));
        }
        let (mesh, top) = cache.built[&key].clone();
        return Some(Staged { shown: Some(mesh), inner: None, frame: None, top });
    }
    let mut staged = staged(&generated_buildings::body(kind, owner)?, pieces, meshes);
    if !cache.frames.contains_key(&key) {
        let frame = meshes.add(to_mesh(generated_buildings::scaffold(kind, owner)?));
        cache.frames.insert(key, frame);
    }
    staged.frame = cache.frames.get(&key).cloned();
    Some(staged)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn respawn_buildings(
    mut commands: Commands,
    map: Res<CurrentMap>,
    levels: Res<LevelList>,
    objects: Res<OriginalObjects>,
    existing: Query<Entity, Or<(With<BuildingView>, With<BuildingLabel>, With<SiteMark>)>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut kit_skin: Local<Option<Handle<StandardMaterial>>>,
    mut kit: Local<KitMeshes>,
    mut drawn: Local<Vec<Building>>,
    mut heights: ResMut<ModelHeights>,
    mut flame_frames: ResMut<FlameFrames>,
) {
    if !map.is_changed() && *drawn == map.0.buildings {
        return;
    }
    drawn.clone_from(&map.0.buildings);
    heights.0.clear();
    for e in &existing {
        commands.entity(e).despawn();
    }
    if map.0.buildings.is_empty() {
        return;
    }
    let theme = map.0.theme.unwrap_or(DEFAULT_THEME);
    let material = original_material(&levels, theme, &mut images, &mut mats);
    let bank = objects.0.as_ref().filter(|_| material.is_some());
    let wood = mats.add(StandardMaterial { base_color: WOOD, perceptual_roughness: 0.9, ..default() });
    let smoke = mats.add(StandardMaterial { base_color: Color::srgba(0.75, 0.75, 0.75, 0.55), alpha_mode: AlphaMode::Blend, unlit: true, ..default() });
    let puff = meshes.add(Sphere::new(0.5).mesh().ico(1).unwrap());
    let mark = mats.add(mark_material());
    let cell = WORLD_UNITS_PER_CELL as f32;
    for (i, b) in map.0.buildings.iter().enumerate() {
        let Building { kind, owner, facing, .. } = *b;
        let stage = b.stage();
        if stage == Stage::Blueprint {
            let empty = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
            commands.spawn((SiteMark(i), Mesh3d(meshes.add(empty)), MeshMaterial3d(mark.clone()), NotShadowCaster, Transform::default()));
            continue;
        }
        let (x, z) = b.centre();
        let at = Vec2::new(x as f32 / cell, z as f32 / cell);
        let yaw = facing_yaw(facing);
        let tribe = if owner < catalog::TRIBES { owner } else { 0 };
        let original = bank.zip(material.as_ref()).and_then(|(bank, mat)| Some((bank.get(building_object(kind, owner)?)?, mat.clone())));
        let layers = kind.wood_cost().max(1);
        let pieces = match stage {
            Stage::UnderConstruction { used, of } | Stage::Dismantling { used, of } => Some((used, of)),
            _ => None,
        };
        let generated = original.is_none().then(|| kit_staged(&mut kit, kind, owner, pieces, &mut meshes)).flatten();
        let stand_in = original.is_none() && generated.is_none();
        let (look, skin, frame_skin, flames) = match (original, generated) {
            (Some((obj, mat)), _) => {
                let solid = solid_part(obj);
                let mut look = staged(&object_mesh(&solid, tribe), pieces, &mut meshes);
                look.frame = pieces.map(|_| meshes.add(to_mesh(beams(&object_edges(&solid), BEAM))));
                (look, mat, wood.clone(), flame::flame_mesh(obj))
            }
            (_, Some(look)) => {
                let skin = kit_skin.get_or_insert_with(|| generated_skin(&mut images, &mut mats)).clone();
                (look, skin.clone(), skin, flame::crossed_boards_at(generated_buildings::flame_bases(kind), BRAZIER_FLAME))
            }
            _ => {
                let mut look = staged(&layered_box(BOX, layers), pieces, &mut meshes);
                look.frame = pieces.map(|_| meshes.add(to_mesh(beams(&box_frame(BOX, layers), BEAM))));
                (look, mats.add(StandardMaterial { base_color: tribe_color(owner), perceptual_roughness: 0.9, ..default() }), wood.clone(), MeshData::default())
            }
        };
        let flames = (stage == Stage::Built && !flames.indices.is_empty()).then(|| {
            let offset = (b.x as usize + b.z as usize) * 3 % flame::FRAMES;
            let frame = flame_frames.get(&levels, theme, &mut images, &mut mats)[offset].clone();
            (Flame { offset }, Mesh3d(meshes.add(to_mesh(flames))), MeshMaterial3d(frame), NotShadowCaster, crate::hover::NoOutline)
        });
        let mut view = commands.spawn((
            BuildingView(i),
            Grounded { at, half: 0.0 },
            Tilted { half: FOOTPRINT_HALF, yaw },
            Transform::from_rotation(Quat::from_rotation_y(yaw)),
            Visibility::Hidden,
        ));
        view.insert(crate::hover::Hoverable::default());
        let id = view.id();
        if let Some(frame) = look.frame {
            view.with_child((Mesh3d(frame), MeshMaterial3d(frame_skin)));
        }
        // Open while under construction: its faces' outer side as usual, their inner side in the
        // tribe colour, so the inside of the building is not seen through the structure.
        let open = !matches!(stage, Stage::Built);
        let skin = match mats.get(&skin).filter(|_| open).cloned() {
            Some(m) => mats.add(StandardMaterial { cull_mode: Some(Face::Back), double_sided: false, ..m }),
            None => skin,
        };
        let inner = look.inner.map(|mesh| (mesh, mats.add(StandardMaterial { base_color: tribe_color(owner), perceptual_roughness: 0.9, cull_mode: Some(Face::Front), double_sided: true, ..default() })));
        let top = look.top;
        heights.0.insert((b.x, b.z), top.y);
        view.with_children(|v| {
            let mut body = v.spawn((Transform::default(), Visibility::Inherited));
            if let Some(shown) = look.shown {
                body.with_child((Mesh3d(shown), MeshMaterial3d(skin)));
                if let Some((mesh, inner)) = inner {
                    body.with_child((Mesh3d(mesh), MeshMaterial3d(inner)));
                }
            }
            if let Some(flames) = flames {
                body.with_child(flames);
            }
            if b.shaking > 0 {
                body.insert(Shaking);
            }
            if stage == Stage::Built && b.inside > 0 && kind.capacity() > 0 {
                for k in 0..PUFFS {
                    v.spawn((Puff { chimney: top, phase: k as f32 / PUFFS as f32 }, Mesh3d(puff.clone()), MeshMaterial3d(smoke.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::from_translation(top)));
                }
            }
        });
        if stand_in {
            commands.spawn((
                BuildingLabel(id),
                Text::new(kind.name()),
                TextFont { font_size: FontSize::Px(12.0), ..default() },
                TextColor(Color::WHITE),
                TextShadow::default(),
                Node { position_type: PositionType::Absolute, ..default() },
                Pickable::IGNORE,
                Visibility::Hidden,
            ));
        }
    }
}

#[derive(Component)]
struct BuildingTooltip;

fn spawn_tooltip(mut commands: Commands) {
    commands.spawn((BuildingTooltip, crate::nature::tooltip()));
}

/// What the tooltip says about building `i`: its name, and for the player's buildings that take
/// wood one `Braves` line, those working on it out of the most it takes while it is built, those
/// inside out of its room once built (huts only), and its wood: brought out of its cost while it is
/// built, the wood in it once built (what taking it apart gives back).
pub fn building_label(map: &game_core::map::GameMap, i: usize) -> String {
    let b = &map.buildings[i];
    let mut label = b.kind.name();
    let (site, cost) = ((b.x, b.z), b.kind.wood_cost());
    if b.owner != crate::units::PLAYER || cost == 0 {
        return label;
    }
    let built = b.stage() == Stage::Built;
    if !built {
        label += &format!("\nBraves: {}/{}", map.workers(site).count(), b.kind.max_braves());
    } else if b.kind.capacity() > 0 {
        label += &format!("\nBraves: {}/{}", map.people_inside(site), b.kind.capacity());
    }
    label += &if built { format!("\nWood: {}", b.used) } else { format!("\nWood: {}/{cost}", b.delivered().min(cost)) };
    label
}

/// Shows the hovered building's tooltip once the cursor rested on it for `HOVER_SECS`.
fn building_tooltip(
    time: Res<Time>,
    map: Res<CurrentMap>,
    windows: Query<&Window>,
    hovered: Res<crate::hover::Hovered>,
    mut since: Local<(Option<usize>, f32)>,
    mut tooltip: Query<(&mut Text, &mut Node, &mut Visibility), With<BuildingTooltip>>,
) {
    let Ok((mut text, mut node, mut vis)) = tooltip.single_mut() else { return };
    let building = match hovered.0 {
        Some(crate::hover::HoverTarget::Building(i)) if i < map.0.buildings.len() => Some(i),
        _ => None,
    };
    let now = time.elapsed_secs();
    if building != since.0 {
        *since = (building, now);
    }
    let cursor = windows.iter().next().and_then(Window::cursor_position);
    match (building.filter(|_| now - since.1 >= crate::nature::HOVER_SECS), cursor) {
        (Some(i), Some(c)) => {
            text.0 = building_label(&map.0, i);
            (node.left, node.top) = (px(c.x + 16.0), px(c.y + 18.0));
            vis.set_if_neq(Visibility::Inherited);
        }
        _ => {
            vis.set_if_neq(Visibility::Hidden);
        }
    }
}

/// Redraws each blueprint's white mark draped over the ground as drawn around the camera.
fn draw_site_marks(
    map: Res<CurrentMap>,
    params: Res<CurveParamsRes>,
    rig: Res<CameraRig>,
    marks: Query<(&SiteMark, &Mesh3d)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let terrain = &map.0.terrain;
    let size = terrain.size() as f32;
    let wrap = |d: f32| (d + size / 2.0).rem_euclid(size) - size / 2.0;
    let cell = WORLD_UNITS_PER_CELL as f32;
    for (mark, mesh) in &marks {
        let Some(b) = map.0.buildings.get(mark.0) else { continue };
        let (cx, cz) = b.centre();
        let vertex = |at: Vec2| {
            let (dx, dz) = (wrap(at.x - rig.focus.x), wrap(at.y - rig.focus.y));
            ([dx, ground_y(terrain, rig.focus, &params.0, dx, dz) + MARK_LIFT, dz], [1.0, 1.0, 1.0, 0.55])
        };
        if let Some(mut m) = meshes.get_mut(&mesh.0) {
            set_mark(&mut m, mark_mesh(b.kind, b.facing, Vec2::new(cx as f32, cz as f32) / cell, vertex));
        }
    }
}

/// An attacked building rocks about its base (the walls move, the base stays): one blow every
/// `BLOW` seconds, each building its own rhythm.
fn shake_walls(time: Res<Time>, mut bodies: Query<(Entity, &mut Transform), With<Shaking>>) {
    for (e, mut tf) in &mut bodies {
        let o = (e.index_u32() % 7) as f32 * 0.13;
        let (rx, rz) = blow_tilt((time.elapsed_secs() + o) / BLOW);
        tf.rotation = Quat::from_rotation_x(rx * SHAKE) * Quat::from_rotation_z(rz * SHAKE);
    }
}

/// Tilt (-1..1 on two axes) at `t` blows: a quick wobble right after each blow, dying out before
/// the next, its direction changing from blow to blow.
pub fn blow_tilt(t: f32) -> (f32, f32) {
    let p = t.fract();
    let fade = (1.0 - p).powi(3);
    let wobble = (p * std::f32::consts::TAU * 4.0).sin() * fade;
    let dir = t.floor() * 2.4;
    (wobble * dir.cos(), wobble * dir.sin())
}

/// Puffs rise from the chimney, growing and drifting, then shrink away and start again.
fn rise_smoke(time: Res<Time>, mut puffs: Query<(&Puff, &mut Transform)>) {
    let t = time.elapsed_secs() / PUFF_LIFE;
    for (puff, mut tf) in &mut puffs {
        let (rise, drift, size) = puff_at((t + puff.phase).fract());
        tf.translation = puff.chimney + Vec3::new(drift, rise, drift * 0.4);
        tf.scale = Vec3::splat(size);
    }
}

/// A puff at `phase` (0 just out, 1 gone): height above the chimney, sideways drift and size (cells).
pub fn puff_at(phase: f32) -> (f32, f32, f32) {
    let fade = if phase > 0.75 { (1.0 - phase) / 0.25 } else { 1.0 };
    (phase * PUFF_RISE, phase * phase * 0.35, PUFF_SIZE * (0.4 + phase) * fade)
}

/// Each stand-in's name centred over its box on screen, hidden when the box is.
fn place_labels(
    cams: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    views: Query<(&GlobalTransform, &InheritedVisibility), With<BuildingView>>,
    mut labels: Query<(&BuildingLabel, &mut Node, &ComputedNode, &mut Visibility)>,
) {
    let Some((cam, cam_t)) = cams.iter().next() else { return };
    for (label, mut node, size, mut vis) in &mut labels {
        let spot = views.get(label.0).ok().filter(|(_, shown)| shown.get()).and_then(|(gt, _)| cam.world_to_viewport(cam_t, gt.translation() + Vec3::Y * (BOX.y + 0.3)).ok());
        match spot {
            Some(p) => {
                let half = size.size() * size.inverse_scale_factor() / 2.0;
                (node.left, node.top) = (px(p.x - half.x), px(p.y - half.y));
                vis.set_if_neq(Visibility::Inherited);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tower_lookout_is_on_its_platform() {
        let tower = |facing| Building::new(BuildingKind::DrumTower, 0, 20 * 512, 20 * 512, facing);
        let (at, lift) = perch(&tower(0), 2.45).unwrap();
        assert!((at - Vec2::new(20.5, 20.5)).length() < 1e-5 && (lift - 1.372).abs() < 1e-3);
        assert!(perch(&Building::new(BuildingKind::Hut { size: 1 }, 0, 0, 0, 0), 2.0).is_none(), "huts hide their people");
    }

    #[test]
    fn tooltip_counts_braves_and_wood() {
        let mut map = game_core::map::GameMap::sandbox_buildings();
        let site = map.place_building(crate::units::PLAYER, BuildingKind::Hut { size: 1 }, (88 * 512, 64 * 512), 0).unwrap();
        let brave = map.units.iter().find(|u| u.kind == game_core::unit::UnitKind::Brave).unwrap().id;
        map.apply(&game_core::command::Command::OrderUnit { player: 0, unit: brave, order: game_core::unit::Order::Build { site: (88 * 512, 64 * 512) } });
        (map.buildings[site].stock, map.buildings[site].used) = (1, 1);
        assert_eq!(building_label(&map, site), "Hut 1\nBraves: 1/6\nWood: 2/3");
        let tower = map.buildings.iter().position(|b| b.owner == crate::units::PLAYER && b.kind == BuildingKind::DrumTower && b.stage() == Stage::Built && b.inside == 0).unwrap();
        assert_eq!(building_label(&map, tower), "Drum tower\nBraves: 0/1\nWood: 5", "a tower holds one");
        let temple = map.buildings.iter().position(|b| b.owner == crate::units::PLAYER && b.kind == BuildingKind::Temple && b.stage() == Stage::Built && b.inside == 0).unwrap();
        assert_eq!(building_label(&map, temple), "Temple\nWood: 8", "nobody goes in: the wood in it only");
        let busy = map.buildings.iter().position(|b| b.owner == crate::units::PLAYER && b.kind == BuildingKind::Hut { size: 1 } && b.stage() == Stage::Built && b.inside > 0).unwrap();
        assert_eq!(building_label(&map, busy), "Hut 1\nBraves: 3/3\nWood: 3", "once built: the braves inside");
        let red = map.buildings.iter().position(|b| b.owner == 1).unwrap();
        assert!(!building_label(&map, red).contains("Braves"), "not theirs");
    }

    #[test]
    fn identified_buildings_have_their_tribes_object() {
        assert_eq!(building_object(BuildingKind::DrumTower, 2), Some(119));
        assert_eq!(building_object(BuildingKind::Hut { size: 3 }, 1), Some(villager_hut(0, 1, 3)));
        assert_eq!(building_object(BuildingKind::Vault, 255), Some(catalog::KNOWLEDGE_PYRAMID), "neutral");
        assert_eq!(building_object(BuildingKind::BoatHut, 255), Some(121), "neutral: the blue one");
        assert_eq!(building_object(BuildingKind::GuardPost, 0), None, "not identified: stand-in box");
    }

    #[test]
    fn kit_meshes_are_shared_per_kind_and_owner_and_made_when_shown() {
        let (mut cache, mut meshes) = (KitMeshes::default(), Assets::<Mesh>::default());
        let temple = BuildingKind::Temple;
        let built = kit_staged(&mut cache, temple, 1, None, &mut meshes).unwrap();
        assert!(built.frame.is_none() && built.inner.is_none() && cache.frames.is_empty(), "no frame painted for a built one");
        assert_eq!(kit_staged(&mut cache, temple, 1, None, &mut meshes).unwrap().shown, built.shown);
        assert_ne!(kit_staged(&mut cache, temple, 2, None, &mut meshes).unwrap().shown, built.shown, "owners differ");
        let a = kit_staged(&mut cache, temple, 1, Some((1, 3)), &mut meshes).unwrap();
        let b = kit_staged(&mut cache, temple, 1, Some((2, 3)), &mut meshes).unwrap();
        assert!(a.frame.is_some() && a.frame == b.frame, "one frame per kind and owner");
        assert!(a.inner.is_some() && a.shown != b.shown, "built parts follow the pieces");
        assert!(kit_staged(&mut cache, temple, 1, Some((0, 3)), &mut meshes).unwrap().shown.is_none());
        assert_eq!(meshes.len(), 7);
        assert!(kit_staged(&mut cache, BuildingKind::Other(12), 1, None, &mut meshes).is_none());
    }

    #[test]
    fn smoke_rises_grows_then_fades() {
        let (r0, _, s0) = puff_at(0.0);
        let (r1, d1, s1) = puff_at(0.6);
        let (_, _, end) = puff_at(0.999);
        assert!(r0 == 0.0 && r1 > r0 && d1 > 0.0);
        assert!(s1 > s0 && end < 0.01, "grows, then shrinks away before starting again");
    }

    #[test]
    fn a_blow_wobbles_then_dies_out() {
        for k in 0..200 {
            let (x, z) = blow_tilt(k as f32 * 0.037);
            assert!(x.abs() <= 1.0 && z.abs() <= 1.0);
        }
        let size = |t: f32| {
            let (x, z) = blow_tilt(t);
            (x * x + z * z).sqrt()
        };
        assert!(size(3.06) > 0.5, "right after a blow");
        assert!(size(3.95) < 0.01, "still before the next");
        assert_ne!(blow_tilt(3.06), blow_tilt(4.06), "another direction each blow");
    }

    #[test]
    fn facing_in_quarter_turns() {
        assert_eq!(facing_yaw(0), 0.0);
        assert!((facing_yaw(2) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }
}
