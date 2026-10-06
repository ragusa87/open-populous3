//! A puff of dust where a unit touches down after floating (teleport landing): soft motes spread
//! out around the feet, rise a little, grow and fade. Cosmetic only, on the real-time clock.

use super::{toward_eye, UnitView, PULL_TO_EYE};
use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::{render_pos, Grounded};
use crate::world::CurrentMap;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use game_core::unit::Action;
use std::f32::consts::TAU;

/// Seconds a puff lasts.
pub const LIFE: f32 = 0.8;
const MOTES: usize = 10;
/// How far the motes spread from the feet, and how high they rise (cells).
const SPREAD: f32 = 0.4;
const RISE: f32 = 0.1;
/// Mote size (cells) at birth and at the end.
const SIZE: (f32, f32) = (0.12, 0.3);
const START_ALPHA: f32 = 0.9;
const DUST: [u8; 3] = [222, 206, 172];
const TEXTURE: usize = 32;

/// One mote `k` of `n`, `age` seconds after touchdown: offset from the feet (world axes, cells),
/// size (cells) and opacity. Motes spread evenly around, fast at first then slowing down.
pub fn mote(k: usize, n: usize, age: f32) -> (Vec3, f32, f32) {
    let t = (age / LIFE).clamp(0.0, 1.0);
    let out = 1.0 - (1.0 - t) * (1.0 - t);
    let angle = k as f32 / n as f32 * TAU + 0.3;
    let r = 0.05 + (SPREAD - 0.05) * out;
    let offset = Vec3::new(angle.cos() * r, 0.02 + RISE * out, angle.sin() * r);
    (offset, SIZE.0 + (SIZE.1 - SIZE.0) * out, START_ALPHA * (1.0 - t))
}

/// Soft round dot, opaque in the middle and fading to the edge (RGBA, `size` x `size`).
pub fn dust_texture(size: usize) -> Vec<u8> {
    let c = (size as f32 - 1.0) / 2.0;
    (0..size * size)
        .flat_map(|i| {
            let (x, y) = ((i % size) as f32 - c, (i / size) as f32 - c);
            let r = ((x * x + y * y).sqrt() / c).min(1.0);
            [DUST[0], DUST[1], DUST[2], ((1.0 - r) * (1.0 - r) * 255.0) as u8]
        })
        .collect()
}

/// Whether each unit touched down this frame: it was landing and no longer is (alive).
pub fn touchdowns(was_landing: &[bool], actions: &[Action]) -> Vec<bool> {
    actions
        .iter()
        .enumerate()
        .map(|(i, a)| was_landing.get(i).copied().unwrap_or(false) && !matches!(a, Action::Landing { .. } | Action::Dying { .. } | Action::Dead { .. }))
        .collect()
}

#[derive(Resource)]
struct DustAssets {
    quad: Handle<Mesh>,
    texture: Handle<Image>,
}

#[derive(Component)]
struct Puff {
    born: f32,
}

#[derive(Component)]
struct Mote(usize);

pub struct DustPlugin;

impl Plugin for DustPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets)
            .add_systems(Update, spawn_puffs.after(super::run_ticks).in_set(crate::menu::Gameplay))
            .add_systems(PostUpdate, animate_puffs.before(TransformSystems::Propagate));
    }
}

fn load_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>) {
    let texture = Image::new(
        Extent3d { width: TEXTURE as u32, height: TEXTURE as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        dust_texture(TEXTURE),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    commands.insert_resource(DustAssets { quad: meshes.add(Rectangle::new(1.0, 1.0)), texture: images.add(texture) });
}

/// A puff at each unit that touched down since the last frame.
fn spawn_puffs(
    mut commands: Commands,
    map: Res<CurrentMap>,
    time: Res<Time>,
    clock: Res<super::SimClock>,
    assets: Res<DustAssets>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut was_landing: Local<Vec<bool>>,
) {
    let actions: Vec<Action> = map.0.units.iter().map(|u| u.action).collect();
    for (i, down) in touchdowns(&was_landing, &actions).into_iter().enumerate() {
        if !down {
            continue;
        }
        let material = mats.add(StandardMaterial {
            base_color_texture: Some(assets.texture.clone()),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });
        let at = clock.cell_pos(i, &map.0.units[i]);
        commands.spawn((Puff { born: time.elapsed_secs() }, Grounded { at, half: 0.0 }, Transform::default(), Visibility::Hidden)).with_children(|p| {
            for k in 0..MOTES {
                p.spawn((Mote(k), Mesh3d(assets.quad.clone()), MeshMaterial3d(material.clone()), Transform::default()));
            }
        });
    }
    *was_landing = actions.iter().map(|a| matches!(a, Action::Landing { .. })).collect();
}

/// Moves, grows and fades the motes, facing the camera and pulled towards it like the units
/// (`toward_eye`); a puff is removed once faded.
#[allow(clippy::too_many_arguments)]
fn animate_puffs(
    mut commands: Commands,
    time: Res<Time>,
    map: Res<CurrentMap>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    eye: Query<&Transform, (With<GameCamera>, Without<Puff>, Without<Mote>)>,
    mut puffs: Query<(Entity, &Puff, &Grounded, &mut Transform, &Children), (Without<Mote>, Without<UnitView>)>,
    mut motes: Query<(&Mote, &mut Transform, &MeshMaterial3d<StandardMaterial>), Without<Puff>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(eye) = eye.single() else { return };
    let facing = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
    for (entity, puff, ground, mut t, children) in &mut puffs {
        let age = time.elapsed_secs() - puff.born;
        if age > LIFE {
            commands.entity(entity).despawn();
            continue;
        }
        t.rotation = facing;
        let Some(feet) = render_pos(&map.0.terrain, ground, rig.focus, &params.0) else { continue };
        let (pull, scale) = toward_eye(feet, eye.translation, PULL_TO_EYE);
        for &child in children {
            let Ok((m, mut mt, material)) = motes.get_mut(child) else { continue };
            let (offset, size, alpha) = mote(m.0, MOTES, age);
            mt.translation = facing.inverse() * (pull + offset * scale);
            mt.scale = Vec3::splat(size * scale);
            if let Some(mut mat) = mats.get_mut(&material.0) {
                mat.base_color = Color::srgba(1.0, 1.0, 1.0, alpha);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motes_spread_rise_grow_and_fade() {
        let (start, s0, a0) = mote(0, 8, 0.0);
        let (mid, s1, a1) = mote(0, 8, LIFE / 2.0);
        let (end, s2, a2) = mote(0, 8, LIFE);
        let flat = |v: Vec3| v.xz().length();
        assert!(flat(start) < flat(mid) && flat(mid) < flat(end) && (flat(end) - SPREAD).abs() < 1e-5);
        assert!(start.y < mid.y && mid.y <= end.y);
        assert!(s0 < s1 && s1 < s2);
        assert!(a0 > a1 && a1 > a2 && a2 == 0.0);
        assert!(flat(mid) - flat(start) > flat(end) - flat(mid), "fast first, then slowing");
        let around: Vec3 = (0..8).map(|k| mote(k, 8, LIFE).0).sum();
        assert!(around.xz().length() < 1e-4, "evenly spread around the feet");
    }

    #[test]
    fn dust_dot_is_soft() {
        let tex = dust_texture(32);
        let alpha = |x: usize, y: usize| tex[(y * 32 + x) * 4 + 3];
        assert!(alpha(16, 16) > 200 && alpha(0, 0) == 0 && alpha(16, 16) > alpha(24, 16) && alpha(24, 16) > 0);
    }

    #[test]
    fn puffs_where_landing_ends() {
        let landing = Action::Landing { left: 1 };
        let was = [true, true, false, true];
        let now = [Action::Idle, landing, Action::Idle, Action::Dying { left: 8 }];
        assert_eq!(touchdowns(&was, &now), vec![true, false, false, false]);
        assert_eq!(touchdowns(&[], &[Action::Idle]), vec![false], "new units");
    }
}
