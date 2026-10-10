//! What the mouse is over (`Hovered`) and its halo: a light outline around views marked `Hoverable`
//! (every unit, wood piles, trees, totems, every building; never the reincarnation site). Sprites (units, piles) get a
//! quad behind them that draws only the pixels just outside their silhouette (`hover_outline.wgsl`);
//! 3D models (trees, buildings) get an inverted hull: their meshes pushed out along the normals, front
//! faces culled, so only a rim shows around them.

use crate::buildings::BuildingView;
use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::pick_ground;
use crate::hud::PANEL_WIDTH;
use crate::nature::{HoveredTree, TreeView};
use crate::units::selection::{unit_at, OnScreen, UNIT_HEIGHT};
use crate::units::{world_units, UnitInput, UnitSprite, UnitView, PLAYER};
use crate::wood::{WoodBody, WoodView};
use crate::world::CurrentMap;
use bevy::asset::{embedded_asset, RenderAssetUsages};
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology, VertexAttributeValues};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Face, RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use game_core::building::Stage;
use game_core::map::GameMap;
use std::collections::HashMap;

/// Halo colour: warm white.
const HALO: Color = Color::srgb(1.0, 0.95, 0.7);
/// Sprite outline width and the room left for it around the frame, in sprite base pixels.
const SPRITE_WIDTH_PX: f32 = 1.5;
const SPRITE_MARGIN_PX: f32 = 2.5;
/// 3D outline width, in cells.
const HULL_WIDTH: f32 = 0.03;
/// A click this close to a building's footprint (world units) is on it.
const BUILDING_MARGIN: i32 = 128;

/// The hoverable views of buildings, trees and totems (one query: systems take at most 16 parameters).
type HoverableViews<'w, 's> = Query<'w, 's, (Option<&'static BuildingView>, Option<&'static TreeView>, Option<&'static crate::totems::TotemView>, &'static GlobalTransform), With<Hoverable>>;
/// Every tree, building and totem view, with its entity.
type ModelViews<'w, 's> = Query<'w, 's, (Entity, Option<&'static TreeView>, Option<&'static BuildingView>, Option<&'static crate::totems::TotemView>)>;

/// The thing under the mouse that gets a halo, by its index in its `GameMap` list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoverTarget {
    Unit(usize),
    Wood(usize),
    Tree(usize),
    Totem(usize),
    Building(usize),
}

#[derive(Resource, Default, Debug, PartialEq)]
pub struct Hovered(pub Option<HoverTarget>);

/// A view the mouse can hover (halo); `health`: a unit also shows its health bar then (the player's
/// own units).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Hoverable {
    pub health: bool,
}

/// Meshes never outlined (a building's smoke puffs).
#[derive(Component)]
pub struct NoOutline;

/// A halo entity, child of what it outlines.
#[derive(Component)]
struct OutlinePart;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct SpriteOutline {
    #[uniform(0)]
    color: LinearRgba,
    #[texture(1)]
    #[sampler(2)]
    sprite: Handle<Image>,
    /// x: outline width in texels.
    #[uniform(3)]
    width: Vec4,
}

impl Material for SpriteOutline {
    fn fragment_shader() -> ShaderRef {
        "embedded://game_client/hover_outline.wgsl".into()
    }

    /// Both sides, like the sprites themselves (some face the camera with their back).
    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Outline meshes and materials made so far, by the mesh they outline (and hull width).
#[derive(Resource, Default)]
struct OutlineCache {
    sprites: HashMap<AssetId<Mesh>, (Handle<Mesh>, Handle<SpriteOutline>)>,
    hulls: HashMap<(AssetId<Mesh>, u32), Option<Handle<Mesh>>>,
    hull_material: Option<Handle<StandardMaterial>>,
}

impl OutlineCache {
    /// Forgets the outlines made from mesh `id`, changed in place (a vault's door and top): the next
    /// halo is made again from its new shape.
    fn forget(&mut self, id: AssetId<Mesh>) {
        self.sprites.remove(&id);
        self.hulls.retain(|(mesh, _), _| *mesh != id);
    }
}

fn forget_changed_outlines(mut events: MessageReader<AssetEvent<Mesh>>, mut cache: ResMut<OutlineCache>) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event {
            cache.forget(*id);
        }
    }
}

/// Picks what is hovered and outlines it: read its result after this.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct HoverSystems;

pub struct HoverPlugin;

impl Plugin for HoverPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "hover_outline.wgsl");
        app.add_plugins(MaterialPlugin::<SpriteOutline>::default())
            .init_resource::<Hovered>()
            .init_resource::<OutlineCache>()
            .add_systems(Update, (forget_changed_outlines, detect, draw_halo).chain().after(UnitInput).after(crate::units::UnitViews).in_set(crate::menu::Gameplay).in_set(HoverSystems));
    }
}

/// What gets the halo when the cursor is on each of these: a unit first, then a wood pile, a tree,
/// a totem, a building.
pub fn pick(unit: Option<usize>, wood: Option<usize>, tree: Option<usize>, totem: Option<usize>, building: Option<usize>) -> Option<HoverTarget> {
    unit.map(HoverTarget::Unit)
        .or(wood.map(HoverTarget::Wood))
        .or(tree.map(HoverTarget::Tree))
        .or(totem.map(HoverTarget::Totem))
        .or(building.map(HoverTarget::Building))
}

/// The totem standing at world point `at` (`GameMap::totem_at`) if `hoverable` accepts it (by index).
pub fn totem_at(map: &GameMap, at: (u16, u16), hoverable: impl Fn(usize) -> bool) -> Option<usize> {
    map.totem_at(at).filter(|&i| hoverable(i))
}

/// A building as seen on screen: its index, the box around its model (pixels) and how far it is from
/// the eye.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildingOnScreen {
    pub index: usize,
    pub min: Vec2,
    pub max: Vec2,
    pub depth: f32,
}

/// The building under the cursor: inside its box on screen, the nearest one when several overlap. The
/// whole model counts, not only its footprint on the ground (with a low camera, the cursor on a tower's
/// top points at the ground behind it).
pub fn building_on_screen(cursor: Vec2, buildings: &[BuildingOnScreen]) -> Option<usize> {
    buildings
        .iter()
        .filter(|b| (b.min.x..=b.max.x).contains(&cursor.x) && (b.min.y..=b.max.y).contains(&cursor.y))
        .min_by(|a, b| a.depth.total_cmp(&b.depth))
        .map(|b| b.index)
}

/// The box on screen around a model standing at `gt`: its footprint (`half` cells either side along
/// its own axes) from the ground up to `height` cells.
pub fn screen_box(cam: (&Camera, &GlobalTransform), gt: &GlobalTransform, half: Vec2, height: f32) -> Option<(Vec2, Vec2)> {
    let (o, right, back, up) = (gt.translation(), gt.right() * half.x, gt.back() * half.y, gt.up() * height);
    let mut points = Vec::with_capacity(8);
    for corner in [right + back, right - back, -right + back, -right - back] {
        for lift in [Vec3::ZERO, up] {
            points.push(cam.0.world_to_viewport(cam.1, o + corner + lift).ok()?);
        }
    }
    let min = points.iter().fold(Vec2::MAX, |a, p| a.min(*p));
    let max = points.iter().fold(Vec2::MIN, |a, p| a.max(*p));
    Some((min, max))
}

/// The first building whose footprint holds world point `at` and that `hoverable` accepts (by
/// index).
pub fn building_at(map: &GameMap, at: (u16, u16), hoverable: impl Fn(usize) -> bool) -> Option<usize> {
    map.buildings.iter().enumerate().position(|(i, b)| b.covers(at, BUILDING_MARGIN) && hoverable(i))
}

/// Dev: `HOVER=unit:3` (or `wood`, `tree`, `totem`, `building` and an index in its `GameMap` list) puts the
/// halo there instead of under the mouse, for screenshots.
pub fn parse_target(v: &str) -> Option<HoverTarget> {
    let (kind, index) = v.split_once(':')?;
    let i = index.trim().parse().ok()?;
    Some(match kind.trim() {
        "unit" => HoverTarget::Unit(i),
        "wood" => HoverTarget::Wood(i),
        "tree" => HoverTarget::Tree(i),
        "building" => HoverTarget::Building(i),
        "totem" => HoverTarget::Totem(i),
        _ => return None,
    })
}

/// Index in `GameMap::wood` of the pile at world point `at` (`GameMap::wood_at`).
fn wood_index(map: &GameMap, at: (u16, u16)) -> Option<usize> {
    let pile = map.wood_at(at)?;
    map.wood.iter().position(|w| (w.x, w.z) == pile)
}

#[allow(clippy::too_many_arguments)]
fn detect(
    windows: Query<&Window>,
    ui: Query<&Interaction>,
    spell: Res<crate::hud::spells::SelectedSpell>,
    blueprint: Res<crate::blueprint::Blueprint>,
    cams: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    units: Query<(&UnitView, &GlobalTransform, &Visibility), With<Hoverable>>,
    woods: Query<&WoodView, With<Hoverable>>,
    views: HoverableViews,
    map: Res<CurrentMap>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    tree: Res<HoveredTree>,
    selection: Res<crate::units::selection::Selection>,
    heights: Res<crate::buildings::ModelHeights>,
    mut hovered: ResMut<Hovered>,
    mut forced: Local<Option<Option<HoverTarget>>>,
) {
    if let Some(target) = *forced.get_or_insert_with(|| std::env::var("HOVER").ok().and_then(|v| parse_target(&v))) {
        hovered.set_if_neq(Hovered(Some(target)));
        return;
    }
    let cursor = windows.iter().next().and_then(Window::cursor_position);
    let over_ui = ui.iter().any(|i| *i != Interaction::None);
    let on_map = cursor.filter(|c| c.x > PANEL_WIDTH && !over_ui && spell.0.is_none() && !blueprint.is_active());
    let target = on_map.zip(cams.iter().next()).and_then(|(c, cam)| {
        let people: Vec<OnScreen> = units
            .iter()
            .filter(|(_, _, vis)| **vis != Visibility::Hidden)
            .filter_map(|(v, gt, _)| {
                let u = map.0.units.get(v.0).filter(|u| u.is_alive() && !selection.contains(u.id) && !crate::units::hidden_inside(&map.0, u))?.pickable()?;
                let feet = cam.0.world_to_viewport(cam.1, gt.translation()).ok()?;
                let head = cam.0.world_to_viewport(cam.1, gt.translation() + gt.up() * UNIT_HEIGHT).ok()?;
                Some(OnScreen::new(u, feet, head))
            })
            .collect();
        let unit = unit_at(c, &people).and_then(|p| map.0.units.iter().position(|u| u.id == p.id()));
        let ground = cam.0.viewport_to_world(cam.1, c).ok().and_then(|r| pick_ground(&map.0.terrain, rig.focus, &params.0, r.origin, *r.direction)).map(world_units);
        let wood = ground.and_then(|at| wood_index(&map.0, at)).filter(|&i| woods.iter().any(|w| w.0 == i));
        // Plans have no view to outline but are hovered for their tooltip.
        let plan = |i: usize| map.0.buildings[i].owner == PLAYER && map.0.buildings[i].stage() == Stage::Blueprint;
        let standing: Vec<BuildingOnScreen> = views
            .iter()
            .filter_map(|(b, _, _, gt)| {
                let i = b?.0;
                let building = map.0.buildings.get(i)?;
                let f = building.kind.footprint();
                let half = Vec2::new(f.half.0 as f32, f.half.1 as f32) / 512.0;
                let height = heights.0.get(&(building.x, building.z)).copied().unwrap_or(1.0);
                let (min, max) = screen_box(cam, gt, half, height)?;
                Some(BuildingOnScreen { index: i, min, max, depth: cam.1.translation().distance(gt.translation()) })
            })
            .collect();
        let building = building_on_screen(c, &standing).or_else(|| ground.and_then(|at| building_at(&map.0, at, plan)));
        let tree = tree.tree.filter(|&i| views.iter().any(|(_, t, _, _)| t.is_some_and(|t| t.0 == i)));
        let totem = ground.and_then(|at| totem_at(&map.0, at, |i| views.iter().any(|(_, _, t, _)| t.is_some_and(|t| t.index == i))));
        pick(unit, wood, tree, totem, building)
    });
    hovered.set_if_neq(Hovered(target));
}

/// Keeps one halo on the hovered thing and none elsewhere.
#[allow(clippy::too_many_arguments)]
fn draw_halo(
    mut commands: Commands,
    hovered: Res<Hovered>,
    mut cache: ResMut<OutlineCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut sprite_mats: ResMut<Assets<SpriteOutline>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    images: Res<Assets<Image>>,
    unit_sprites: Query<(Entity, &UnitSprite)>,
    wood_views: Query<(&WoodView, &Children)>,
    wood_bodies: Query<Entity, With<WoodBody>>,
    models: ModelViews,
    children: Query<&Children>,
    shapes: Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>, &GlobalTransform), (Without<OutlinePart>, Without<NoOutline>)>,
    mut parts: Query<(Entity, &ChildOf, &mut Mesh3d, Option<&mut MeshMaterial3d<SpriteOutline>>), With<OutlinePart>>,
) {
    let sprite = match hovered.0 {
        Some(HoverTarget::Unit(i)) => unit_sprites.iter().find(|(_, s)| s.0 == i).map(|(e, _)| e),
        Some(HoverTarget::Wood(i)) => wood_views.iter().find(|(v, _)| v.0 == i).and_then(|(_, c)| c.iter().find(|&e| wood_bodies.contains(e))),
        _ => None,
    };
    let model = match hovered.0 {
        Some(HoverTarget::Tree(i)) => models.iter().find(|(_, t, _, _)| t.is_some_and(|t| t.0 == i)).map(|(e, ..)| e),
        Some(HoverTarget::Building(i)) => models.iter().find(|(_, _, b, _)| b.is_some_and(|b| b.0 == i)).map(|(e, ..)| e),
        Some(HoverTarget::Totem(i)) => models.iter().find(|(_, _, _, t)| t.is_some_and(|t| t.index == i)).map(|(e, ..)| e),
        _ => None,
    };
    let mut wanted: Vec<(Entity, Handle<Mesh>, Option<Handle<SpriteOutline>>)> = Vec::new();
    if let Some(e) = sprite && let Ok((mesh, mat, _)) = shapes.get(e) {
        let made = cache.sprites.get(&mesh.0.id()).cloned().or_else(|| {
            let texture = mats.get(&mat.0)?.base_color_texture.clone()?;
            let texels = images.get(&texture)?.width() as f32;
            let (outline, width) = sprite_outline_mesh(meshes.get(&mesh.0)?, texels)?;
            let made = (meshes.add(outline), sprite_mats.add(SpriteOutline { color: HALO.into(), sprite: texture, width: Vec4::new(width, 0.0, 0.0, 0.0) }));
            cache.sprites.insert(mesh.0.id(), made.clone());
            Some(made)
        });
        if let Some((m, s)) = made {
            wanted.push((e, m, Some(s)));
        }
    }
    if let Some(root) = model {
        cache.hull_material.get_or_insert_with(|| mats.add(StandardMaterial { base_color: HALO, unlit: true, cull_mode: Some(Face::Front), ..default() }));
        for e in std::iter::once(root).chain(children.iter_descendants(root)) {
            let Ok((mesh, _, gt)) = shapes.get(e) else { continue };
            let scale = gt.compute_transform().scale;
            let width = HULL_WIDTH / ((scale.x + scale.y + scale.z) / 3.0).max(1e-4);
            let key = (mesh.0.id(), (width * 10_000.0) as u32);
            let hull = match cache.hulls.get(&key) {
                Some(h) => h.clone(),
                None => {
                    let h = meshes.get(&mesh.0).and_then(|m| hull(m, width)).map(|m| meshes.add(m));
                    cache.hulls.insert(key, h.clone());
                    h
                }
            };
            if let Some(h) = hull {
                wanted.push((e, h, None));
            }
        }
    }
    let mut kept = Vec::new();
    for (part, parent, mut mesh, sprite_mat) in &mut parts {
        match wanted.iter().find(|(e, ..)| *e == parent.parent()) {
            Some((e, m, s)) if !kept.contains(e) && s.is_some() == sprite_mat.is_some() => {
                if mesh.0 != *m {
                    mesh.0 = m.clone();
                }
                if let (Some(mut mat), Some(s)) = (sprite_mat, s) && mat.0 != *s {
                    mat.0 = s.clone();
                }
                kept.push(*e);
            }
            _ => commands.entity(part).despawn(),
        }
    }
    for (e, mesh, sprite_mat) in wanted.into_iter().filter(|(e, ..)| !kept.contains(e)) {
        let mut part = commands.spawn((OutlinePart, NotShadowCaster, Mesh3d(mesh), Transform::default(), ChildOf(e)));
        match sprite_mat {
            Some(s) => part.insert(MeshMaterial3d(s)),
            None => part.insert(MeshMaterial3d(cache.hull_material.clone().unwrap_or_default())),
        };
    }
}

/// A sprite quad (`units::sprite_quad`) grown by `SPRITE_MARGIN_PX` on every side, its UVs running
/// past 0..1 so the frame keeps its place, and the outline width in texels for a texture `texels`
/// wide. None if the mesh is not such a quad.
pub fn sprite_outline_mesh(quad: &Mesh, texels: f32) -> Option<(Mesh, f32)> {
    let Some(VertexAttributeValues::Float32x3(p)) = quad.attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
    let (l, r) = p.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(v[0]), b.max(v[0])));
    let (b, t) = p.iter().fold((f32::MAX, f32::MIN), |(a, c), v| (a.min(v[1]), c.max(v[1])));
    let (w, h) = (r - l, t - b);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let m = SPRITE_MARGIN_PX * crate::units::PIXEL;
    let (l2, r2, t2, b2) = (l - m, r + m, t + m, b - m);
    let uv = |x: f32, y: f32| [(x - l) / w, (t - y) / h];
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[l2, t2, 0.0], [r2, t2, 0.0], [r2, b2, 0.0], [l2, b2, 0.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![uv(l2, t2), uv(r2, t2), uv(r2, b2), uv(l2, b2)])
        .with_inserted_indices(Indices::U32(vec![0, 2, 1, 0, 3, 2]));
    Some((mesh, SPRITE_WIDTH_PX * crate::units::PIXEL * texels / w))
}

/// The mesh pushed out by `width` (its own units) along normals averaged over vertices sharing a
/// position (so hard edges do not open), same triangles: drawn with front faces culled, its inside
/// shows as a rim around the model. None without positions and normals.
pub fn hull(mesh: &Mesh, width: f32) -> Option<Mesh> {
    let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
    let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else { return None };
    let key = |v: &[f32; 3]| v.map(|c| (c * 4096.0).round() as i32);
    let mut sums: HashMap<[i32; 3], Vec3> = HashMap::new();
    for (v, normal) in p.iter().zip(n) {
        *sums.entry(key(v)).or_default() += Vec3::from(*normal);
    }
    let pushed: Vec<[f32; 3]> = p.iter().map(|v| (Vec3::from(*v) + sums[&key(v)].normalize_or_zero() * width).to_array()).collect();
    let normals: Vec<[f32; 3]> = p.iter().map(|v| sums[&key(v)].normalize_or_zero().to_array()).collect();
    let mut out = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pushed)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    if let Some(indices) = mesh.indices() {
        out.insert_indices(indices.clone());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mesh_changed_in_place_loses_its_outlines() {
        let (changed, other) = (Handle::<Mesh>::default().id(), AssetId::<Mesh>::invalid());
        let mut cache = OutlineCache::default();
        cache.hulls.insert((changed, 300), None);
        cache.hulls.insert((changed, 150), None);
        cache.hulls.insert((other, 300), None);
        cache.forget(changed);
        assert_eq!(cache.hulls.keys().collect::<Vec<_>>(), vec![&(other, 300)]);
    }
    use crate::units::{sprite_quad, PLAYER};

    #[test]
    fn a_building_is_under_the_cursor_anywhere_on_its_model_the_nearest_first() {
        let tower = BuildingOnScreen { index: 1, min: Vec2::new(100.0, 50.0), max: Vec2::new(140.0, 200.0), depth: 20.0 };
        let hut = BuildingOnScreen { index: 2, min: Vec2::new(90.0, 150.0), max: Vec2::new(180.0, 210.0), depth: 12.0 };
        assert_eq!(building_on_screen(Vec2::new(120.0, 60.0), &[tower, hut]), Some(1), "the tower's top");
        assert_eq!(building_on_screen(Vec2::new(120.0, 180.0), &[tower, hut]), Some(2), "the hut in front of it");
        assert_eq!(building_on_screen(Vec2::new(120.0, 40.0), &[tower, hut]), None, "above them");
    }

    #[test]
    fn a_unit_wins_over_wood_trees_and_buildings() {
        assert_eq!(pick(Some(3), Some(1), Some(2), Some(5), Some(0)), Some(HoverTarget::Unit(3)));
        assert_eq!(pick(None, Some(1), Some(2), Some(5), Some(0)), Some(HoverTarget::Wood(1)));
        assert_eq!(pick(None, None, Some(2), Some(5), Some(0)), Some(HoverTarget::Tree(2)));
        assert_eq!(pick(None, None, None, Some(5), Some(0)), Some(HoverTarget::Totem(5)));
        assert_eq!(pick(None, None, None, None, Some(0)), Some(HoverTarget::Building(0)));
        assert_eq!(pick(None, None, None, None, None), None);
    }

    #[test]
    fn a_totem_is_under_the_cursor_near_its_centre_across_the_seam() {
        use game_core::totem::{Totem, TotemKind};
        let mut map = GameMap::sandbox_worship();
        map.totems = vec![Totem::new(TotemKind::StoneHead, (100, 30000)), Totem::new(TotemKind::Totem, (40000, 40000))];
        assert_eq!(totem_at(&map, (100 + game_core::worship::TOTEM_MARGIN as u16, 30000), |_| true), Some(0));
        assert_eq!(totem_at(&map, (65500, 30100), |_| true), Some(0), "across the map's seam");
        assert_eq!(totem_at(&map, (100 + game_core::worship::TOTEM_MARGIN as u16 + 1, 30000), |_| true), None);
        assert_eq!(totem_at(&map, (40000, 40000), |i| i != 1), None, "no view to hover");
    }

    #[test]
    fn dev_target_by_kind_and_index() {
        assert_eq!(parse_target("tree:4"), Some(HoverTarget::Tree(4)));
        assert_eq!(parse_target("building: 2"), Some(HoverTarget::Building(2)));
        assert_eq!(parse_target("rock:1"), None);
        assert_eq!(parse_target("unit"), None);
    }

    #[test]
    fn only_hoverable_standing_buildings_are_hovered() {
        let map = GameMap::sandbox_buildings();
        let mine = map.buildings.iter().position(|b| b.owner == PLAYER && b.stage() == Stage::Built).unwrap();
        let theirs = map.buildings.iter().position(|b| b.owner != PLAYER).unwrap();
        let plan = map.buildings.iter().position(|b| b.owner == PLAYER && b.stage() == Stage::Blueprint).unwrap();
        let players = |i: usize| map.buildings[i].owner == PLAYER;
        assert_eq!(building_at(&map, map.buildings[mine].centre(), players), Some(mine));
        assert_eq!(building_at(&map, map.buildings[theirs].centre(), players), None, "not hoverable");
        assert_eq!(building_at(&map, map.buildings[theirs].centre(), |_| true), Some(theirs));
        assert_eq!(building_at(&map, map.buildings[plan].centre(), |_| true), Some(plan), "a plan, for its tooltip");
    }

    #[test]
    fn the_sprite_outline_quad_keeps_the_frame_in_place() {
        let quad = sprite_quad(Vec2::new(20.0, 40.0), Vec2::new(10.0, 36.0));
        let (outline, width) = sprite_outline_mesh(&quad, 80.0).unwrap();
        let Some(VertexAttributeValues::Float32x2(uv)) = outline.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!() };
        let m = SPRITE_MARGIN_PX / 20.0;
        assert!((uv[0][0] + m).abs() < 1e-5 && (uv[1][0] - 1.0 - m).abs() < 1e-5, "{uv:?}");
        assert!((width - SPRITE_WIDTH_PX * 4.0).abs() < 1e-4, "80 texels over 20 px: 4 per px, {width}");
    }

    #[test]
    fn the_hull_grows_along_shared_normals() {
        let cube = Mesh::from(Cuboid::new(1.0, 1.0, 1.0));
        let h = hull(&cube, 0.1).unwrap();
        let Some(VertexAttributeValues::Float32x3(p)) = h.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        let corner = 0.5 + 0.1 / 3f32.sqrt();
        assert!(p.iter().all(|v| v.iter().all(|c| (c.abs() - corner).abs() < 1e-4)), "corners pushed out diagonally, faces stay closed: {p:?}");
        assert_eq!(h.indices().map(|i| i.len()), cube.indices().map(|i| i.len()));
    }
}
