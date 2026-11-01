//! The Angel of Death itself (`game_core::unit::UnitKind::AngelOfDeath`, `game_core::angel_of_death::Flight`):
//! a world view of each living one, the original "winged death" object-bank mesh (`catalog::WINGED_DEATH_BACK`/
//! `WINGED_DEATH_ALT`, the same model family as the totem's own static monument, `totems.rs`) rather than a 2D
//! unit sprite — the companion object the original's own flight state actually draws, never the type-8 person
//! record itself (pop3-rev-analysis.md "Angel of Death mechanics" "Visuals", TODO.md "Angel of Death"). Turned
//! by its heading and lifted to its flight height like a vehicle (`vehicles.rs`); its ground shadow is the same
//! one every unit casts (`units::shadow`, already generic over kind). Cross-blended between its two flight
//! poses (diving/descending, cruising/level or climbing) over `BLEND_TICKS` whenever its vertical motion changes
//! direction, rather than swapped instantly — the original's own "0-5 ramp... 3 intermediate sub-variants" isn't
//! pinned down further, approximated here as a straight crossfade between the two meshes (`[ours]`). With
//! `--no-original`, a stand-in silhouette in the tribe's colour instead.

use crate::camera::CurveParamsRes;
use crate::game_frame::GameYaw;
use crate::grounded::Grounded;
use crate::original_models::{atlas_image, object_mesh, solid_part, to_mesh, OriginalObjects};
use crate::sim_time::SimNow;
use crate::views::{sync_views, WorldView};
use crate::world::{CurrentMap, LevelList};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use game_core::angel_of_death::Flight;
use game_core::angle::{Angle, TURN};
use game_core::map::GameMap;
use game_core::time::Tick;
use game_core::tribes::{Owner, TribeId};
use game_core::unit::{Unit, UnitKind};
use pop3_format::{catalog, Atlas, Theme, WORLD_UNITS_PER_CELL};

/// Ticks the two flight-pose meshes cross-fade over whenever its vertical motion changes direction
/// (pop3-rev-analysis.md "Angel of Death mechanics": the original's own "0-5 ramp... 3 intermediate
/// sub-variants" isn't pinned down further; approximated here as a straight crossfade, `[ours]`).
const BLEND_TICKS: f32 = 6.0;

/// Stand-in body (cells, length along the heading): a narrower, elongated silhouette for the diving pose, a
/// wide flat one for the cruising pose.
const WINGSPAN: f32 = 1.3;
const LENGTH: f32 = 0.55;
const DEPTH: f32 = 0.1;

const DEFAULT_THEME: u8 = 0;

fn radians(a: Angle) -> f32 {
    a.signed() as f32 * std::f32::consts::TAU / TURN as f32
}

/// Whether moving from `from` to `to` (terrain height units) reads as diving (descending) rather than
/// cruising (level or climbing).
fn is_diving(from: i32, to: i32) -> bool {
    to < from
}

/// How far blended (0 to 1) into whichever pose it last picked, `elapsed` ticks (fractional) after it did.
fn blend_fraction(elapsed: f32) -> f32 {
    (elapsed / BLEND_TICKS).clamp(0.0, 1.0)
}

/// How high its body is drawn over the ground under it, render units.
fn lift(map: &GameMap, u: &Unit, flight: &Flight, height_scale: f32) -> f32 {
    let ground = map.terrain.height_at(u.x as u32, u.z as u32, WORLD_UNITS_PER_CELL);
    (flight.height - ground).max(0) as f32 * height_scale
}

/// Its body's own turn: yaw from its heading alone (its pose swap, not a pitch of its own, reads as dive or
/// cruise), the same formula as a vehicle's hull (`vehicles::hull_rotation`).
fn body_rotation(flight: &Flight) -> Quat {
    GameYaw(std::f32::consts::FRAC_PI_2 - radians(flight.heading)).into()
}

/// Which flight pose it is blending towards, tracked client-side from the height it was last drawn at: the
/// simulation keeps no history of its own vertical motion (`game_core::angel_of_death::Flight` is just a
/// heading and a height).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    height: i32,
    diving: bool,
    /// When `diving` last flipped; `None` before it ever has, so a freshly spawned angel starts already
    /// settled into its pose rather than blending in from nothing.
    since: Option<Tick>,
}

impl Pose {
    fn new(height: i32) -> Self {
        Pose { height, diving: false, since: None }
    }

    /// Updates from this frame's height, flipping which pose it blends towards if its direction changed.
    fn track(&mut self, height: i32, now: Tick) {
        if height == self.height {
            return;
        }
        let diving = is_diving(self.height, height);
        if diving != self.diving {
            self.diving = diving;
            self.since = Some(now);
        }
        self.height = height;
    }

    /// Opacity of the diving mesh and the cruising mesh at `now`.
    fn weights(&self, now: SimNow) -> (f32, f32) {
        let f = match self.since {
            Some(since) => blend_fraction(now.turns_since(since).max(0.0)),
            None => 1.0,
        };
        if self.diving { (f, 1.0 - f) } else { (1.0 - f, f) }
    }
}

/// Meshes and materials made so far, for each tribe (the original objects are recoloured per tribe; the
/// atlas texture itself is theme-wide, like `buildings`/`vehicles`).
#[derive(Resource, Default)]
pub struct AngelLook {
    theme: Option<u8>,
    atlas: Option<Handle<Image>>,
    parts: Vec<(TribeId, Handle<Mesh>, Handle<Mesh>)>,
}

impl AngelLook {
    /// The diving and cruising meshes of `tribe`, built once and cached.
    fn meshes(&mut self, tribe: TribeId, theme: u8, levels: &LevelList, objects: &OriginalObjects, assets: &mut Assets3d) -> (Handle<Mesh>, Handle<Mesh>) {
        if let Some((_, dive, cruise)) = self.parts.iter().find(|(t, ..)| *t == tribe) {
            return (dive.clone(), cruise.clone());
        }
        if self.theme != Some(theme) {
            self.theme = Some(theme);
            self.atlas = angel_atlas(levels, theme, &mut assets.images);
            self.parts.clear();
        }
        let bank = objects.0.as_ref().filter(|_| self.atlas.is_some());
        let found = bank.and_then(|b| b.get(catalog::WINGED_DEATH_BACK).zip(b.get(catalog::WINGED_DEATH_ALT)));
        let (dive, cruise) = match found {
            Some((dive, cruise)) => (assets.meshes.add(to_mesh(object_mesh(&solid_part(dive), tribe))), assets.meshes.add(to_mesh(object_mesh(&solid_part(cruise), tribe)))),
            None => stand_in(assets),
        };
        self.parts.push((tribe, dive.clone(), cruise.clone()));
        (dive, cruise)
    }
}

/// The theme's atlas as a texture, None without original files (`buildings::original_material`'s own loading,
/// kept separate here since the material itself needs `AlphaMode::Blend` for the pose cross-fade).
fn angel_atlas(levels: &LevelList, theme: u8, images: &mut Assets<Image>) -> Option<Handle<Image>> {
    if !levels.original {
        return None;
    }
    let load = || -> Result<_, pop3_format::LevelError> { Ok((Atlas::load(&levels.data_dir, theme)?, Theme::load(&levels.data_dir, theme)?)) };
    let (atlas, palette) = load().map_err(|e| warn!("angel of death atlas for theme {theme}: {e}")).ok()?;
    Some(images.add(atlas_image(&atlas, &palette.palette)))
}

/// Our own stand-in body, no original art to trace for a flying model of its own: a flattened box, narrower
/// and longer swept back for the dive, wide and flat for the level cruise.
fn stand_in(assets: &mut Assets3d) -> (Handle<Mesh>, Handle<Mesh>) {
    let dive = Cuboid::new(WINGSPAN * 0.55, DEPTH, LENGTH * 1.15);
    let cruise = Cuboid::new(WINGSPAN, DEPTH, LENGTH);
    (assets.meshes.add(dive), assets.meshes.add(cruise))
}

/// One pose's own material: the theme's atlas (tribe colour baked into the mesh's own UVs) or a flat tint of
/// `owner`'s tribe colour, always alpha-blended so the cross-fade can animate its opacity.
fn material(look: &AngelLook, owner: Owner, mats: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
    let base = match &look.atlas {
        Some(tex) => StandardMaterial { base_color_texture: Some(tex.clone()), alpha_mode: AlphaMode::Blend, perceptual_roughness: 0.95, cull_mode: Some(Face::Back), ..default() },
        None => StandardMaterial { base_color: crate::sites::tribe_color(owner), alpha_mode: AlphaMode::Blend, perceptual_roughness: 0.8, cull_mode: None, ..default() },
    };
    mats.add(base)
}

/// The asset stores an angel's parts are made in.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Assets3d<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    images: ResMut<'w, Assets<Image>>,
}

/// One angel's view: the root (moved over the ground), its body (lifted and turned) and the two pose
/// materials it cross-fades between.
#[derive(Component)]
pub struct AngelOfDeathView {
    me: Entity,
    hull: Entity,
    dive_material: Handle<StandardMaterial>,
    cruise_material: Handle<StandardMaterial>,
}

impl WorldView for AngelOfDeathView {
    type Model = Unit;
    type Params = (
        (Res<'static, CurrentMap>, Res<'static, CurveParamsRes>, Res<'static, LevelList>, Res<'static, OriginalObjects>),
        ResMut<'static, AngelLook>,
        Assets3d<'static>,
        Query<'static, 'static, (&'static mut Grounded, &'static mut Pose)>,
        Query<'static, 'static, &'static mut Transform, Without<Grounded>>,
    );

    fn models(map: &GameMap) -> impl Iterator<Item = &Unit> {
        map.units.iter().filter(|u| u.kind == UnitKind::AngelOfDeath)
    }

    fn id(u: &Unit) -> u32 {
        u.id.raw()
    }

    fn spawn(u: &Unit, view: &mut EntityCommands, ((map, params, levels, objects), look, assets, ..): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let theme = map.0.theme.unwrap_or(DEFAULT_THEME);
        let tribe = u.owner.tribe().unwrap_or(TribeId::Blue);
        let (dive_mesh, cruise_mesh) = look.meshes(tribe, theme, levels, objects, assets);
        let (dive_material, cruise_material) = (material(look, u.owner, &mut assets.materials), material(look, u.owner, &mut assets.materials));
        let flight = u.flight.unwrap_or(Flight { heading: Angle::ZERO, height: 0 });
        let me = view.id();
        let at = Vec2::new(u.x as f32, u.z as f32) / WORLD_UNITS_PER_CELL as f32;
        view.insert((Grounded { at, half: 0.0 }, Pose::new(flight.height), crate::object_scale::Scaled(crate::object_scale::ObjectGroup::Scenery), crate::hover::Hoverable::default(), Transform::default(), Visibility::Hidden));
        let mut hull = Entity::PLACEHOLDER;
        let pose = Transform::from_xyz(0.0, lift(&map.0, u, &flight, params.0.height_scale), 0.0).with_rotation(body_rotation(&flight));
        view.with_children(|c| {
            hull = c
                .spawn((pose, Visibility::Inherited))
                .with_children(|h| {
                    h.spawn((Mesh3d(dive_mesh), MeshMaterial3d(dive_material.clone())));
                    h.spawn((Mesh3d(cruise_mesh), MeshMaterial3d(cruise_material.clone())));
                })
                .id();
        });
        Some(AngelOfDeathView { me, hull, dive_material, cruise_material })
    }

    fn update(&self, u: &Unit, now: SimNow, ((map, params, ..), _look, assets, grounded, transforms): &mut SystemParamItem<Self::Params>) {
        let flight = u.flight.unwrap_or(Flight { heading: Angle::ZERO, height: 0 });
        let mut weights = (0.0, 1.0);
        if let Ok((mut g, mut pose)) = grounded.get_mut(self.me) {
            g.at = Vec2::new(u.x as f32, u.z as f32) / WORLD_UNITS_PER_CELL as f32;
            pose.track(flight.height, now.tick);
            weights = pose.weights(now);
        }
        if let Ok(mut t) = transforms.get_mut(self.hull) {
            t.translation.y = lift(&map.0, u, &flight, params.0.height_scale);
            t.rotation = body_rotation(&flight);
        }
        let (dive_w, cruise_w) = weights;
        if let Some(mut m) = assets.materials.get_mut(&self.dive_material) {
            m.base_color = m.base_color.with_alpha(dive_w);
        }
        if let Some(mut m) = assets.materials.get_mut(&self.cruise_material) {
            m.base_color = m.base_color.with_alpha(cruise_w);
        }
    }
}

pub struct AngelOfDeathPlugin;

impl Plugin for AngelOfDeathPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AngelLook>().add_systems(Update, sync_views::<AngelOfDeathView>.after(crate::units::run_ticks));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::id::Id;

    #[test]
    fn diving_is_losing_height_cruising_is_level_or_climbing() {
        assert!(is_diving(1000, 900));
        assert!(!is_diving(900, 900));
        assert!(!is_diving(900, 1000));
    }

    #[test]
    fn the_blend_is_linear_over_its_ticks_then_clamped() {
        assert_eq!(blend_fraction(0.0), 0.0);
        assert!((blend_fraction(BLEND_TICKS / 2.0) - 0.5).abs() < 1e-6);
        assert_eq!(blend_fraction(BLEND_TICKS), 1.0);
        assert_eq!(blend_fraction(BLEND_TICKS * 2.0), 1.0, "clamped, not overshooting");
    }

    #[test]
    fn a_direction_change_restarts_the_blend_towards_the_new_pose() {
        let mut pose = Pose::new(1000);
        let t0 = Tick::from_wire(0);
        assert_eq!(pose.weights(SimNow { tick: t0, frac: 0.0 }), (0.0, 1.0), "starts fully cruising, as if it always had");
        pose.track(900, t0);
        assert!(pose.diving);
        assert_eq!(pose.weights(SimNow { tick: t0, frac: 0.0 }), (0.0, 1.0), "just flipped: still fully the old pose");
        let mid = Tick::from_wire(t0.to_wire() + BLEND_TICKS as u32 / 2);
        let (dive, cruise) = pose.weights(SimNow { tick: mid, frac: 0.0 });
        assert!((dive - 0.5).abs() < 1e-6 && (cruise - 0.5).abs() < 1e-6);
        pose.track(1000, mid);
        assert!(!pose.diving, "climbing again: the target flips back");
    }

    #[test]
    fn lift_never_goes_below_the_ground() {
        let map = GameMap::sandbox_units();
        let u = Unit::new(Id::from_raw(1), TribeId::Blue, UnitKind::AngelOfDeath, (1024, 1024));
        let ground = map.terrain.height_at(1024, 1024, WORLD_UNITS_PER_CELL);
        assert_eq!(lift(&map, &u, &Flight { heading: Angle::ZERO, height: ground + 600 }, 0.5), 300.0);
        assert_eq!(lift(&map, &u, &Flight { heading: Angle::ZERO, height: ground - 600 }, 0.5), 0.0);
    }

    #[test]
    fn the_body_turns_with_its_heading_not_the_unit_kinds_walking_yaw() {
        let nose = |steps| body_rotation(&Flight { heading: Angle::new(steps), height: 0 }) * Vec3::NEG_Z;
        let game = |r: Vec3| Vec2::new(r.x, -r.z);
        assert!((game(nose(0)) - Vec2::X).length() < 1e-5, "heading 0 is +x");
        assert!((game(nose(512)) - Vec2::Y).length() < 1e-5, "a quarter turn is +z");
    }
}
