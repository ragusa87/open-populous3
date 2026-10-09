//! The theme's sky (`sky0-X.dat`, a tiling cloud layer) on a dome around the eye, fading out to
//! space when zooming out to the planet. The layer is projected like a flat ceiling of clouds
//! (`dome_uv`): no pinch overhead, denser towards the horizon. Maps without an original theme keep
//! the plain sky colour.

use crate::camera::{space_fade, CameraRig, GameCamera};
use crate::world::{CurrentMap, LevelList};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use pop3_format::theme::Sky;
use pop3_format::Theme;

/// Dome radius (render units): past the drawn terrain, inside the camera's far plane.
const RADIUS: f32 = 600.0;
/// Cloud layer tiles per unit of ceiling, and how far below the eye the ceiling's horizon sits
/// (keeps the tiles finite at the horizon).
const TILES: f32 = 0.8;
const HORIZON: f32 = 0.2;

#[derive(Component)]
struct SkyDome;

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, respawn_dome).add_systems(PostUpdate, follow_eye.before(TransformSystems::Propagate));
    }
}

/// Texture coordinates of the cloud ceiling seen along `dir` (unit vector): straight up is the
/// origin, tiles shrink towards the horizon; below it the ceiling is mirrored (seen past the
/// planet's edge), so the layer has no seam.
pub fn dome_uv(dir: Vec3) -> [f32; 2] {
    let k = TILES / (dir.y.abs() + HORIZON);
    [dir.x * k, dir.z * k]
}

/// A sphere around the eye with the cloud ceiling's coordinates (`dome_uv`).
fn dome_mesh() -> Mesh {
    let mut mesh = Sphere::new(RADIUS).mesh().uv(64, 32);
    let uvs: Option<Vec<[f32; 2]>> = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|p| p.as_float3())
        .map(|p| p.iter().map(|&v| dome_uv(Vec3::from(v).normalize_or(Vec3::Y))).collect());
    if let Some(uvs) = uvs {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    }
    mesh
}

/// RGBA8 pixels of a sky, opaque.
pub fn sky_pixels(sky: &Sky) -> Vec<u8> {
    sky.rgb.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect()
}

fn load_sky(levels: &LevelList, theme: u8) -> Option<Sky> {
    let palette = Theme::load(&levels.data_dir, theme).map_err(|e| warn!("sky theme {theme}: {e}")).ok()?.palette;
    Sky::load(&levels.data_dir, theme, &palette).map_err(|e| warn!("sky {theme}: {e}")).ok()
}

fn respawn_dome(
    mut commands: Commands,
    map: Res<CurrentMap>,
    levels: Res<LevelList>,
    existing: Query<Entity, With<SkyDome>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut theme: Local<Option<Option<u8>>>,
) {
    if *theme == Some(map.0.theme) {
        return;
    }
    *theme = Some(map.0.theme);
    for e in &existing {
        commands.entity(e).despawn();
    }
    let Some(sky) = map.0.theme.filter(|_| levels.original).and_then(|t| load_sky(&levels, t)) else { return };
    let mut image = Image::new(
        Extent3d { width: sky.width as u32, height: sky.height as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        sky_pixels(&sky),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    let material = StandardMaterial {
        base_color_texture: Some(images.add(image)),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        double_sided: true,
        fog_enabled: false,
        ..default()
    };
    commands.spawn((SkyDome, Mesh3d(meshes.add(dome_mesh())), MeshMaterial3d(mats.add(material)), NotShadowCaster, Transform::default()));
}

/// Centred on the eye, opaque near the ground and gone when looking at the planet from space.
fn follow_eye(
    rig: Res<CameraRig>,
    eye: Query<&Transform, (With<GameCamera>, Without<SkyDome>)>,
    mut dome: Query<(&mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>), With<SkyDome>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(eye) = eye.single() else { return };
    let alpha = 1.0 - space_fade(rig.distance);
    for (mut t, mut vis, material) in &mut dome {
        t.translation = eye.translation;
        *vis = if alpha > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        if let Some(mut m) = mats.get_mut(&material.0) && m.base_color.alpha() != alpha {
            m.base_color = Color::WHITE.with_alpha(alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ceiling_is_flat_overhead_and_seamless() {
        assert_eq!(dome_uv(Vec3::Y), [0.0, 0.0], "overhead");
        let near = dome_uv(Vec3::new(0.3, 0.95, 0.0).normalize());
        let low = dome_uv(Vec3::new(0.95, 0.3, 0.0).normalize());
        assert!(low[0] > near[0] * 3.0, "denser towards the horizon");
        let (above, below) = (dome_uv(Vec3::new(1.0, 0.01, 0.0).normalize()), dome_uv(Vec3::new(1.0, -0.01, 0.0).normalize()));
        assert!((above[0] - below[0]).abs() < 1e-4, "no seam at the horizon");
        assert!(dome_uv(Vec3::X)[0].is_finite());
    }

    #[test]
    fn sky_pixels_are_opaque_rgba() {
        let sky = Sky { width: 2, height: 1, rgb: vec![[1, 2, 3], [4, 5, 6]] };
        assert_eq!(sky_pixels(&sky), vec![1, 2, 3, 255, 4, 5, 6, 255]);
    }
}
