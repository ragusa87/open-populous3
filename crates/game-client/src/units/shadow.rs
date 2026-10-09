//! Unit shadows: a soft dark ellipse lying on the ground under each unit, leaning with the slope
//! (`Tilted`), the size of the original person shadow (`HSPR0-0.DAT` 22, a 21 px wide black ellipse
//! the animations draw under every person with element flag 0x4). Drawn for every kind of art.

use super::{SimClock, PIXEL};
use crate::game_frame::GameYaw;
use crate::grounded::{Grounded, Tilted};
use crate::world::CurrentMap;
use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use game_core::unit::{Action, Unit};

/// Width of the original shadow sprite (pixels) and the ellipse's depth over its width on the
/// ground (the sprite's 21 x 4 is that ellipse seen from the original's tilted camera).
const SHADOW_PX: f32 = 21.0;
const DEPTH: f32 = 0.6;
/// Darkness in the middle (0..1).
const OPACITY: f32 = 0.6;
/// Fraction of the radius that fades out at the edge.
const SOFT_EDGE: f32 = 0.35;
/// Lift off the ground (cells) and the half size the slope is measured over.
const LIFT: f32 = 0.01;
const SLOPE_HALF: f32 = 0.25;
const TEXTURE: usize = 32;

#[derive(Component)]
struct UnitShadow(usize);

#[derive(Component)]
struct ShadowBody;

pub struct ShadowPlugin;

impl Plugin for ShadowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (respawn_shadows, follow_units).chain().after(super::run_ticks).in_set(crate::menu::Gameplay));
    }
}

/// Black ellipse filling the texture: an even core, its rim fading out (RGBA).
pub fn shadow_texture(size: usize) -> Vec<u8> {
    let c = (size as f32 - 1.0) / 2.0;
    (0..size * size)
        .flat_map(|i| {
            let (x, y) = ((i % size) as f32 - c, (i / size) as f32 - c);
            let r = ((x * x + y * y).sqrt() / c).min(1.0);
            [0, 0, 0, (OPACITY * ((1.0 - r) / SOFT_EDGE).min(1.0) * 255.0) as u8]
        })
        .collect()
}

/// Ground size of the shadow (cells): width across, depth along.
pub fn shadow_size() -> Vec2 {
    let w = SHADOW_PX * PIXEL;
    Vec2::new(w, w * DEPTH)
}

/// Whether a unit casts a shadow: not once dead, nor while sinking in the sea.
pub fn casts_shadow(unit: &Unit) -> bool {
    !matches!(unit.action, Action::Dead { .. } | Action::Drowning)
}

fn respawn_shadows(
    mut commands: Commands,
    map: Res<CurrentMap>,
    existing: Query<Entity, With<UnitShadow>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut assets: Local<Option<(Handle<Mesh>, Handle<StandardMaterial>)>>,
) {
    if !map.is_changed() {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn();
    }
    let (mesh, material) = assets
        .get_or_insert_with(|| {
            let image = Image::new(
                Extent3d { width: TEXTURE as u32, height: TEXTURE as u32, depth_or_array_layers: 1 },
                TextureDimension::D2,
                shadow_texture(TEXTURE),
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            let material = StandardMaterial {
                base_color_texture: Some(images.add(image)),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                depth_bias: 10.0,
                ..default()
            };
            let size = shadow_size();
            (meshes.add(Plane3d::new(Vec3::Y, size / 2.0)), mats.add(material))
        })
        .clone();
    for i in 0..map.0.units.len() {
        commands
            .spawn((UnitShadow(i), crate::object_scale::Scaled(crate::object_scale::ObjectGroup::Units), Grounded { at: Vec2::ZERO, half: 0.0 }, Tilted { half: SLOPE_HALF, yaw: GameYaw::default() }, Transform::default(), Visibility::Hidden))
            .with_child((ShadowBody, Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), NotShadowCaster, Transform::from_xyz(0.0, LIFT, 0.0)));
    }
}

fn follow_units(
    map: Res<CurrentMap>,
    clock: Res<SimClock>,
    mut shadows: Query<(&UnitShadow, &mut Grounded, &Children)>,
    mut bodies: Query<&mut Visibility, With<ShadowBody>>,
) {
    for (shadow, mut ground, children) in &mut shadows {
        let unit = map.0.units.get(shadow.0);
        if let Some(u) = unit {
            ground.at = clock.cell_pos(shadow.0, u);
        }
        let shown = unit.is_some_and(casts_shadow);
        for &child in children {
            if let Ok(mut vis) = bodies.get_mut(child) {
                *vis = if shown { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::unit::UnitKind;

    #[test]
    fn soft_dark_ellipse() {
        let tex = shadow_texture(32);
        let px = |x: usize, y: usize| &tex[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
        assert_eq!(&px(16, 16)[..3], &[0, 0, 0]);
        assert!(px(16, 16)[3] == px(18, 16)[3], "even core");
        assert!(px(16, 16)[3] > px(29, 16)[3] && px(29, 16)[3] > 0 && px(0, 0)[3] == 0, "soft rim");
        assert!(px(16, 16)[3] as f32 <= OPACITY * 255.0);
    }

    #[test]
    fn shadow_is_the_original_width_and_flatter() {
        let s = shadow_size();
        assert!((s.x - 21.0 / 88.0).abs() < 1e-6 && s.y < s.x);
    }

    #[test]
    fn no_shadow_once_dead_or_drowning() {
        let mut u = Unit::new(1, 0, UnitKind::Brave, (512, 512));
        assert!(casts_shadow(&u));
        u.action = Action::Drowning;
        assert!(!casts_shadow(&u));
    }
}
