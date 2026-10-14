//! The swamp's ground (`game_core::swamp`), a world view (`views::WorldView`) of each effect: the original marks
//! only the aerial map (pop3-rev-analysis.md "Swamp: drawing..."), no ground sprite at all; ours instead shows the
//! ground gone marshy, a pale misty patch a few centimetres over it (`LIFT`), draped over its 3 cells like a
//! blueprint's footprint (`blueprint::grid_mark`), with a scatter of flat reed-like leaves lying on it, pseudo-random
//! and seeded from the effect's id (`leaf_offset`, `leaf_yaw`) so every client draws the same ones. Our own look.

use crate::blueprint::{grid_mark, mark_material, set_mark};
use crate::camera::{CameraRig, CurveParamsRes};
use crate::grounded::{ground_point, Grounded};
use crate::sim_time::{seed, SimNow};
use crate::views::{sync_views, WorldView};
use crate::world::CurrentMap;
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use pop3_format::WORLD_UNITS_PER_CELL;

/// Height over the ground (render units).
const LIFT: f32 = 0.05;
/// The patch's pale, misty colour (RGBA).
const COLOUR: [f32; 4] = [0.82, 0.86, 0.80, 0.5];
/// Leaves scattered on the patch, and how far from its centre (cells).
const LEAVES: u32 = 7;
const SCATTER_RADIUS: f32 = 1.15;
/// A leaf's size (cells): tapered to a point at both ends along its length.
const LEAF: (f32, f32) = (0.3, 0.5);
const TEXTURE: usize = 24;

/// A pseudo-random offset (cells, within `SCATTER_RADIUS` of the centre) for the `k`th leaf of swamp `id`.
fn leaf_offset(id: u32, k: u32) -> Vec2 {
    let s = |salt: u32| seed(id.wrapping_mul(31).wrapping_add(k.wrapping_mul(97)).wrapping_add(salt)) * 2.0 - 1.0;
    Vec2::new(s(1), s(2)) * SCATTER_RADIUS
}

/// Lobes the patch's edge is built from (`blob_alpha`): the first covers the centre, the rest are its bumps.
const LOBES: u32 = 6;

/// The patch's alpha at offset `p` (cells from its centre), 0 past its edge: several overlapping round lobes,
/// seeded from the effect's `id` (a different stream from the leaves'), so it reads as an irregular, cloud-like
/// blob rather than the grid it is built on, the same every frame and for every client.
fn blob_alpha(id: u32, p: Vec2) -> f32 {
    (0..LOBES)
        .map(|k| {
            let s = |salt: u32| seed(id.wrapping_mul(53).wrapping_add(k.wrapping_mul(131)).wrapping_add(salt));
            let centre = if k == 0 { Vec2::ZERO } else { Vec2::new(s(11) * 2.0 - 1.0, s(12) * 2.0 - 1.0) * 0.85 };
            let radius = 0.9 + s(13) * 0.55;
            (1.0 - (p - centre).length() / radius).clamp(0.0, 1.0).powf(1.3)
        })
        .fold(0.0f32, f32::max)
}

/// A pseudo-random yaw (radians) for the `k`th leaf of swamp `id`.
fn leaf_yaw(id: u32, k: u32) -> f32 {
    seed(id.wrapping_mul(31).wrapping_add(k.wrapping_mul(97)).wrapping_add(3)) * std::f32::consts::TAU
}

/// A leaf, green, tapering to a point at both ends along its length (the texture's y), opaque in the middle
/// (RGBA, `size` square).
fn leaf_texture(size: usize) -> Vec<u8> {
    let c = (size as f32 - 1.0) / 2.0;
    (0..size * size)
        .flat_map(|i| {
            let x = ((i % size) as f32 - c) / c;
            let y = ((i / size) as f32 - c) / c;
            let width = (1.0 - y * y).max(0.0).sqrt();
            let d = if width > 1e-3 { x.abs() / width } else { 2.0 };
            let a = (1.0 - d).clamp(0.0, 1.0);
            [70, 110, 45, (a.powf(0.4) * 255.0) as u8]
        })
        .collect()
}

#[derive(Component)]
struct Leaf;

#[derive(Resource)]
pub struct SwampAssets {
    leaf: Handle<Mesh>,
    leaf_material: Handle<StandardMaterial>,
}

/// One swamp's view: the patch over its 3 × 3 cells, rebuilt every frame as the camera moves (it spans several
/// cells, like a blueprint's footprint, not one grounded point); its leaves, scattered once at spawn, are plain
/// `Grounded` points and need no further updates.
#[derive(Component)]
pub struct SwampView {
    mesh: Handle<Mesh>,
}

/// The patch's mesh over the 3 cells centred on `centre` (cells), draped on the ground at `rig`'s focus, its edge
/// shaped by `blob_alpha` instead of the square it is built on.
fn patch_mesh(terrain: &game_core::terrain::Heightmap, rig: &CameraRig, params: &CurveParamsRes, id: u32, centre: Vec2) -> crate::blueprint::MarkMesh {
    let on_ground = |at: Vec2| <[f32; 3]>::from(ground_point(terrain, rig.focus, &params.0, at) + Vec3::Y * LIFT);
    grid_mark(centre - Vec2::splat(1.5), centre + Vec2::splat(1.5), |at| {
        let a = COLOUR[3] * blob_alpha(id, at - centre);
        (on_ground(at), [COLOUR[0], COLOUR[1], COLOUR[2], a])
    })
}

impl WorldView for SwampView {
    type Model = (Id<Effect>, Effect);
    type Params = (Res<'static, CurrentMap>, Res<'static, CameraRig>, Res<'static, CurveParamsRes>, Res<'static, SwampAssets>, ResMut<'static, Assets<Mesh>>, ResMut<'static, Assets<StandardMaterial>>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::Swamp(_)))
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((id, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (map, rig, params, assets, meshes, mats): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let id = id.raw();
        let centre = Vec2::new(e.x as f32, e.z as f32) / WORLD_UNITS_PER_CELL as f32;
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        set_mark(&mut mesh, patch_mesh(&map.0.terrain, rig, params, id, centre));
        let mesh = meshes.add(mesh);
        view.insert((Mesh3d(mesh.clone()), MeshMaterial3d(mats.add(mark_material())), NotShadowCaster, crate::hover::NoOutline, Transform::default(), Visibility::Visible));
        view.with_children(|v| {
            for k in 0..LEAVES {
                let lying_flat = Quat::from_rotation_y(leaf_yaw(id, k)) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
                v.spawn((
                    Leaf,
                    Mesh3d(assets.leaf.clone()),
                    MeshMaterial3d(assets.leaf_material.clone()),
                    Grounded { at: centre + leaf_offset(id, k), half: 0.0 },
                    Transform::from_rotation(lying_flat),
                    Visibility::Inherited,
                    NotShadowCaster,
                    crate::hover::NoOutline,
                ));
            }
        });
        Some(SwampView { mesh })
    }

    fn update(&self, (id, e): &(Id<Effect>, Effect), _now: SimNow, (map, rig, params, _, meshes, _): &mut SystemParamItem<Self::Params>) {
        let Some(mut mesh) = meshes.get_mut(&self.mesh) else { return };
        let centre = Vec2::new(e.x as f32, e.z as f32) / WORLD_UNITS_PER_CELL as f32;
        set_mark(&mut mesh, patch_mesh(&map.0.terrain, rig, params, id.raw(), centre));
    }
}

pub struct SwampPlugin;

impl Plugin for SwampPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets).add_systems(Update, sync_views::<SwampView>.after(crate::units::run_ticks));
    }
}

fn load_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let texture = Image::new(
        Extent3d { width: TEXTURE as u32, height: TEXTURE as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        leaf_texture(TEXTURE),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let leaf_material = mats.add(StandardMaterial {
        base_color: Color::srgb(0.3, 0.46, 0.2),
        base_color_texture: Some(images.add(texture)),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        // Over the patch's own depth bias (`mark_material`, 20.0), so the leaves draw over it, not under.
        depth_bias: 25.0,
        ..default()
    });
    commands.insert_resource(SwampAssets { leaf: meshes.add(Rectangle::new(LEAF.0, LEAF.1)), leaf_material });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_scatter_within_radius_each_its_own_spot() {
        let offsets: Vec<Vec2> = (0..LEAVES).map(|k| leaf_offset(5, k)).collect();
        assert!(offsets.iter().all(|o| o.length() <= SCATTER_RADIUS * 2f32.sqrt() + 1e-4));
        assert!(offsets.windows(2).all(|w| w[0] != w[1]), "each leaf its own spot");
        assert_eq!(leaf_offset(5, 2), leaf_offset(5, 2), "the same swamp looks the same");
        assert_ne!(leaf_offset(5, 2), leaf_offset(6, 2), "so do different swamps");
    }

    #[test]
    fn the_blob_is_full_at_the_centre_and_gone_well_past_the_patch() {
        assert_eq!(blob_alpha(9, Vec2::ZERO), 1.0, "the first lobe always covers the centre");
        assert_eq!(blob_alpha(9, Vec2::splat(5.0)), 0.0, "long gone far out");
        assert_eq!(blob_alpha(9, Vec2::new(0.3, -0.2)), blob_alpha(9, Vec2::new(0.3, -0.2)), "the same swamp looks the same");
        assert_ne!(
            (0..20).map(|k| blob_alpha(9, Vec2::new(k as f32 * 0.1, 0.0))).collect::<Vec<_>>(),
            (0..20).map(|k| blob_alpha(10, Vec2::new(k as f32 * 0.1, 0.0))).collect::<Vec<_>>(),
            "so do different swamps"
        );
    }

    #[test]
    fn the_leaf_is_opaque_along_its_middle_and_clear_at_the_tips() {
        let px = leaf_texture(TEXTURE);
        let alpha = |x: usize, y: usize| px[(y * TEXTURE + x) * 4 + 3];
        let mid = TEXTURE / 2;
        assert!(alpha(mid, mid) > 150);
        assert_eq!((alpha(mid, 0), alpha(mid, TEXTURE - 1)), (0, 0));
    }
}
