//! The swamp's ground (`game_core::swamp`), a world view (`views::WorldView`) of each effect: the original marks
//! only the aerial map (pop3-rev-analysis.md "Swamp: drawing..."), no ground sprite at all. Ours instead stacks
//! `FOG_LAYERS` flat, transparent planes over its 3 × 3 cells, all within one unit's height (`FOG_TOP_Y`,
//! `units::selection::UNIT_HEIGHT`) so they sit close together, each running its own drifting, domain-warped
//! noise (seeded and phased per layer, `SwampFog`, a custom material, `swamp_fog.wgsl`) with a soft edge and a
//! height-based fade, so the layers never line up and the lot reads as a grounded volume, not a flat sticker.
//! Our own look, no original to trace. The ground itself is left alone, the real terrain showing through.

use crate::grounded::Grounded;
use crate::sim_time::{seed, SimNow};
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::asset::embedded_asset;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::time::Tick;
use pop3_format::WORLD_UNITS_PER_CELL;

/// Inside the swamp (Chebyshev distance from its centre cell): its 3 × 3 cells.
const SWAMP_REACH: i32 = 1;

/// Fog planes stacked from `FOG_BASE_Y` to `FOG_TOP_Y` (render units, close together: about a unit's own
/// height, `units::selection::UNIT_HEIGHT`), and their footprint (cells, centred).
const FOG_LAYERS: u32 = 5;
const FOG_BASE_Y: f32 = 0.02;
const FOG_TOP_Y: f32 = crate::units::selection::UNIT_HEIGHT;
/// Exactly the swamp's own 3 × 3 cells, no further.
const FOG_SIZE: f32 = (2 * SWAMP_REACH + 1) as f32;
/// How thick the fog reads, and how fast it drifts: both tuned low, matched against a live prototype.
const DENSITY: f32 = 0.3;
const DRIFT_SCALE: f32 = 0.3;
const FOG_COLOUR: Vec4 = Vec4::new(0.93, 0.95, 0.92, 1.0);
/// Where the tooltip anchors above a swamp (`tooltip::show_tooltip`), clear of the fog stack.
pub const TOOLTIP_HEIGHT: f32 = FOG_TOP_Y + 0.3;

/// Marks the swamp's ground anchor (its `Grounded` point) by the effect's id, for the tooltip to anchor above
/// (`tooltip::show_tooltip`): only the player's own swamp shows one (`tooltip::swamp_model`).
#[derive(Component)]
pub struct SwampAnchor(pub u32);

/// This layer's noise seed (cells) and drift (cells/second, already at `DRIFT_SCALE`): its own stream per swamp
/// and layer, so no two layers (or swamps) line up.
fn layer_seed(id: u32, layer: u32) -> Vec2 {
    let s = |salt: u32| seed(id.wrapping_mul(71).wrapping_add(layer.wrapping_mul(151)).wrapping_add(salt));
    Vec2::new(s(1) * 40.0, s(2) * 40.0)
}
fn layer_drift(id: u32, layer: u32) -> Vec2 {
    let s = |salt: u32| seed(id.wrapping_mul(71).wrapping_add(layer.wrapping_mul(151)).wrapping_add(salt));
    Vec2::new(0.03 + s(3) * 0.02, 0.02 + s(4) * 0.02) * DRIFT_SCALE
}
/// Layer `layer`'s opacity multiplier (0 at the top): the bottom layers stay thick, the top ones thin out, so the
/// fog hugs the ground instead of reading as a cloud.
fn layer_fade(layer: u32) -> f32 {
    1.0 - (layer as f32 / FOG_LAYERS as f32).powf(1.4) * 0.75
}

/// One fog layer: drifting, domain-warped noise (`swamp_fog.wgsl`) with a soft edge and this layer's own fade.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SwampFog {
    /// x: time (seconds since the swamp was cast), y: density, z: this layer's fade, w: unused.
    #[uniform(0)]
    time_density: Vec4,
    /// xy: this layer's noise seed, zw: its drift.
    #[uniform(1)]
    seed_drift: Vec4,
    #[uniform(2)]
    fog_colour: Vec4,
}

impl Material for SwampFog {
    fn fragment_shader() -> ShaderRef {
        "embedded://game_client/swamp_fog.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    /// Seen from either side, like a blueprint's mark.
    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[derive(Resource)]
pub struct SwampAssets {
    fog_quad: Handle<Mesh>,
}

/// One swamp's view: its stack of fog layers (a fixed shape, posed once at spawn; only their `time` moves, each
/// frame).
#[derive(Component)]
pub struct SwampView {
    fog: Vec<Handle<SwampFog>>,
}

impl WorldView for SwampView {
    type Model = (Id<Effect>, Effect);
    type Params = (Res<'static, CurrentMap>, Res<'static, SwampAssets>, ResMut<'static, Assets<SwampFog>>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::Swamp(_)))
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((id, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (_, assets, fog_mats): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let id = id.raw();
        let centre = Vec2::new(e.x as f32, e.z as f32) / WORLD_UNITS_PER_CELL as f32;
        view.insert((SwampAnchor(id), Grounded { at: centre, half: 0.0 }, Transform::default(), Visibility::Visible));

        let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        let mut fog = Vec::with_capacity(FOG_LAYERS as usize);
        view.with_children(|v| {
            for layer in 0..FOG_LAYERS {
                let (ls, ld) = (layer_seed(id, layer), layer_drift(id, layer));
                let material = fog_mats.add(SwampFog {
                    time_density: Vec4::new(0.0, DENSITY, layer_fade(layer), 0.0),
                    seed_drift: Vec4::new(ls.x, ls.y, ld.x, ld.y),
                    fog_colour: FOG_COLOUR,
                });
                let height = FOG_BASE_Y + (FOG_TOP_Y - FOG_BASE_Y) * (layer as f32 / (FOG_LAYERS - 1) as f32);
                v.spawn((
                    Mesh3d(assets.fog_quad.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform { translation: Vec3::Y * height, rotation: flat, ..default() },
                    Visibility::Inherited,
                    NotShadowCaster,
                    crate::hover::NoOutline,
                ));
                fog.push(material);
            }
        });
        Some(SwampView { fog })
    }

    fn update(&self, (_, e): &(Id<Effect>, Effect), now: SimNow, (map, _, fog_mats): &mut SystemParamItem<Self::Params>) {
        let start = Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get()));
        let secs = now.secs_since(start);
        for handle in &self.fog {
            if let Some(mut m) = fog_mats.get_mut(handle) {
                m.time_density.x = secs;
            }
        }
    }
}

/// Right click + Ctrl on the player's own swamp ends it early (`GameMap::clear_swamp`), like a blueprint put
/// away; never another tribe's.
fn clear_on_right_click(mouse: Res<ButtonInput<MouseButton>>, keys: crate::keymap::Shortcuts, hovered: Res<crate::hover::Hovered>, map: Res<CurrentMap>, mut schedule: ResMut<crate::units::GameSchedule>) {
    if !(mouse.just_pressed(MouseButton::Right) && keys.mods().ctrl) {
        return;
    }
    let Some(crate::hover::HoverTarget::Swamp(id)) = hovered.0 else { return };
    if map.0.effects.get(id).is_some_and(|e| e.owner == crate::units::PLAYER) {
        schedule.issue(game_core::command::Command::ClearSwamp { player: crate::units::PLAYER, swamp: id });
    }
}

pub struct SwampPlugin;

impl Plugin for SwampPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "swamp_fog.wgsl");
        app.add_plugins(MaterialPlugin::<SwampFog>::default())
            .add_systems(Startup, load_assets)
            .add_systems(Update, sync_views::<SwampView>.after(crate::units::run_ticks))
            .add_systems(Update, clear_on_right_click.after(crate::hover::HoverSystems).in_set(crate::menu::Gameplay));
    }
}

fn load_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(SwampAssets { fog_quad: meshes.add(Rectangle::new(FOG_SIZE, FOG_SIZE)) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fog_fades_toward_the_top_and_each_layer_drifts_its_own_way() {
        assert_eq!(layer_fade(0), 1.0, "the bottom layer at full strength");
        assert!(layer_fade(FOG_LAYERS - 1) < layer_fade(0), "thinner near the top");
        assert!((0..FOG_LAYERS - 1).all(|l| layer_fade(l) >= layer_fade(l + 1)), "fades monotonically");
        assert_ne!(layer_seed(5, 0), layer_seed(5, 1), "each layer its own seed");
        assert_ne!(layer_drift(5, 0), layer_drift(6, 0), "and different swamps drift differently");
    }
}
