//! Totems on the map (`GameMap::totems`): the original objects of their look, textured from the
//! level theme's atlas and seen from the front only like buildings, their flames animated; otherwise
//! (or with `--no-original`) a plain stone pillar. Animated (docs/specs/worship.md):
//! - the stone totem, once it gave, turns every slab's bottom ring by `TOTAL_TURN` (`twisted`, from
//!   object 1 towards object 3); exhausted, it sinks under the ground with smoke and is gone;
//! - the stone head nods at all times (`nod_angle`), its head split from its pedestal (`split_head`).
//!
//! The generated stand-ins do the same with blocks. Redrawn when the map changes.

use crate::flame::{self, Flame, FlameFrames};
use crate::grounded::Grounded;
use crate::original_models::{atlas_image, object_mesh, solid_part, to_mesh, OriginalObjects};
use crate::units::SimClock;
use crate::world::{CurrentMap, LevelList};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use game_core::totem::{Totem, TotemKind, HOLD_TICKS, SINK_TICKS, TURN_TICKS};
use pop3_format::catalog::{PRAYER_TOTEM, STONE_HEAD, TOTEM, TOTEM_ANIMATED, TOTEM_POLES, WINGED_DEATH_PERCHED, WINGED_DEATH_TOTEM};
use pop3_format::{Atlas, Object, Theme, WORLD_UNITS_PER_CELL};
use std::f32::consts::TAU;

/// Theme whose atlas textures totems on maps without one.
const DEFAULT_THEME: u8 = 0;
/// Footprint half size (cells): the totem rests on the lowest ground under it.
const HALF: f32 = 0.4;
/// The generated stand-in pillar (cells) and its colour.
const PILLAR: Vec3 = Vec3::new(0.45, 1.3, 0.45);
const PILLAR_STONE: Color = Color::srgb(0.5, 0.48, 0.44);
/// The generated stone totem: blocks stacked from the ground, each this high, narrowing upwards.
const BLOCKS: usize = 5;
const BLOCK_HIGH: f32 = 0.3;
/// How far every slab's bottom ring turns once the stone totem gave: 64° and a full turn.
pub const TOTAL_TURN: f32 = (64.0 + 360.0) * std::f32::consts::PI / 180.0;
/// A point whose angle differs more than this (radians) between objects 1 and 3 is on a ring that turns.
const TURNING: f32 = 0.05;
/// The stone head: its head is every face at or above this height (world units, its pedestal's top),
/// nodding about a horizontal axis there by `NOD_DEG` each way, one nod every `NOD_SECS`.
const NECK: i16 = 324;
const NOD_DEG: f32 = 6.0;
const NOD_SECS: f32 = 1.6;
/// Smoke while sinking: puffs, one every `PUFF_GAP` seconds, each living `PUFF_LIFE` seconds.
const PUFFS: usize = 14;
const PUFF_GAP: f32 = 0.25;
const PUFF_LIFE: f32 = 1.4;

/// The view of the totem at this index of `GameMap::totems`, and the height of its top (cells).
#[derive(Component)]
pub struct TotemView {
    pub index: usize,
    pub top: f32,
}

/// What sinks of a totem: its meshes (not its smoke).
#[derive(Component)]
struct TotemBody;

/// The stone totem's rocks, posed from objects 1 and 3 at the turn `shown` (thousandths).
#[derive(Component)]
struct StoneRocks {
    shown: Option<u16>,
}

/// A block of the generated stone totem, `0` the base (it never turns).
#[derive(Component)]
struct RockBlock(usize);

/// The stone head's head, turning about its neck.
#[derive(Component)]
struct NodPivot;

/// A puff of smoke while the totem sinks, `seed` setting its place and start.
#[derive(Component)]
struct SmokePuff(usize);

/// The queries of the parts `animate_totems` moves, kept apart (Bevy needs them disjoint).
type Bodies<'w, 's> = Query<'w, 's, (&'static mut Transform, &'static mut Visibility), (With<TotemBody>, Without<SmokePuff>, Without<NodPivot>, Without<RockBlock>)>;
type Blocks<'w, 's> = Query<'w, 's, (&'static RockBlock, &'static mut Transform), (Without<TotemBody>, Without<SmokePuff>, Without<NodPivot>)>;
type Pivots<'w, 's> = Query<'w, 's, &'static mut Transform, (With<NodPivot>, Without<TotemBody>, Without<SmokePuff>)>;
type Puffs<'w, 's> = Query<'w, 's, (&'static SmokePuff, &'static mut Transform, &'static mut Visibility, &'static MeshMaterial3d<StandardMaterial>), (Without<TotemBody>, Without<NodPivot>, Without<RockBlock>)>;

pub struct TotemsPlugin;

impl Plugin for TotemsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (respawn_totems, animate_totems).chain());
    }
}

/// The original objects drawing a totem of `kind`, at the same origin.
pub fn totem_objects(kind: TotemKind) -> Vec<usize> {
    match kind {
        TotemKind::Totem => vec![TOTEM],
        TotemKind::WingedDeath => vec![WINGED_DEATH_TOTEM, WINGED_DEATH_PERCHED],
        TotemKind::Prayer => vec![PRAYER_TOTEM],
        TotemKind::StoneHead => vec![STONE_HEAD],
        TotemKind::Pole(k) => vec![TOTEM_POLES[k as usize % TOTEM_POLES.len()]],
    }
}

fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    0.5 - 0.5 * (std::f32::consts::PI * x).cos()
}

/// How far the stone totem's rings have turned (0 to 1), `alpha` into the current tick: eased over
/// `TURN_TICKS` after its first gift, then turned for good.
pub fn turn_fraction(t: &Totem, alpha: f32) -> f32 {
    match t.given {
        0 => 0.0,
        1 => ease((t.since_given as f32 + alpha) / TURN_TICKS as f32),
        _ => 1.0,
    }
}

/// How far an exhausted totem has sunk (0 to 1): eased over `SINK_TICKS`, after its turn and a hold.
pub fn sink_fraction(t: &Totem, alpha: f32) -> f32 {
    if !t.is_exhausted() {
        return 0.0;
    }
    ease((t.since_given as f32 + alpha - (TURN_TICKS + HOLD_TICKS) as f32) / SINK_TICKS as f32)
}

/// The stone totem with its rings turned `fraction` of the way: each point on a ring that turns
/// between object 1 (`stored`) and object 3 (`turned`) goes round the centre axis by `TOTAL_TURN`
/// times `fraction` (the way object 3 turns it), its radius towards object 3's.
pub fn twisted(stored: &Object, turned: &Object, fraction: f32) -> Object {
    let points = stored
        .points
        .iter()
        .zip(&turned.points)
        .map(|(p, q)| {
            let (a0, a1) = ((p[2] as f32).atan2(p[0] as f32), (q[2] as f32).atan2(q[0] as f32));
            let delta = (a1 - a0 + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI;
            if delta.abs() <= TURNING {
                return *p;
            }
            let (r0, r1) = ((p[0] as f32).hypot(p[2] as f32), (q[0] as f32).hypot(q[2] as f32));
            let (r, a) = (r0 + (r1 - r0) * fraction.clamp(0.0, 1.0), a0 + delta.signum() * TOTAL_TURN * fraction);
            [(r * a.cos()).round() as i16, p[1], (r * a.sin()).round() as i16]
        })
        .collect();
    Object { points, faces: stored.faces.clone() }
}

/// The stone head split at its neck: its pedestal, and its head (every face with all its points at
/// `NECK` or above) moved so the neck is its origin.
pub fn split_head(obj: &Object) -> (Object, Object) {
    let (head, pedestal): (Vec<_>, Vec<_>) = obj.faces.iter().cloned().partition(|f| f.points.iter().all(|&i| obj.points[i as usize][1] >= NECK));
    let lowered = obj.points.iter().map(|p| [p[0], p[1] - NECK, p[2]]).collect();
    (Object { points: obj.points.clone(), faces: pedestal }, Object { points: lowered, faces: head })
}

/// The stone head's nod (radians) at `secs`: `NOD_DEG` each way, one nod every `NOD_SECS`.
pub fn nod_angle(secs: f32) -> f32 {
    NOD_DEG.to_radians() * (TAU * secs / NOD_SECS).sin()
}

/// Puff `seed` of the smoke `secs` into the sinking: where it is (cells, about the totem's foot), its
/// size and its opacity; None when it is not out.
pub fn puff(seed: usize, secs: f32) -> Option<(Vec3, f32, f32)> {
    let since = secs - seed as f32 * PUFF_GAP;
    if since < 0.0 || secs > SINK_TICKS as f32 / 10.0 + PUFF_LIFE / 2.0 {
        return None;
    }
    let p = since.rem_euclid(PUFF_LIFE) / PUFF_LIFE;
    let a = seed as f32 * 2.4;
    let out = 0.45 + 0.12 * p;
    Some((Vec3::new(a.cos() * out, 0.08 + 0.8 * p, a.sin() * out), 0.12 + 0.24 * p, 0.55 * (1.0 - p)))
}

/// The theme atlas as a front-faced material, None without original files.
fn original_material(levels: &LevelList, theme: u8, images: &mut Assets<Image>, mats: &mut Assets<StandardMaterial>) -> Option<Handle<StandardMaterial>> {
    if !levels.original {
        return None;
    }
    let load = || -> Result<_, pop3_format::LevelError> { Ok((Atlas::load(&levels.data_dir, theme)?, Theme::load(&levels.data_dir, theme)?)) };
    let (atlas, palette) = load().map_err(|e| warn!("totem atlas for theme {theme}: {e}")).ok()?;
    Some(mats.add(StandardMaterial {
        base_color_texture: Some(images.add(atlas_image(&atlas, &palette.palette))),
        alpha_mode: AlphaMode::Mask(0.5),
        perceptual_roughness: 0.95,
        cull_mode: Some(Face::Back),
        ..default()
    }))
}

#[allow(clippy::too_many_arguments)]
fn respawn_totems(
    mut commands: Commands,
    map: Res<CurrentMap>,
    levels: Res<LevelList>,
    objects: Res<OriginalObjects>,
    existing: Query<Entity, With<TotemView>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut flame_frames: ResMut<FlameFrames>,
) {
    if !map.is_changed() {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn();
    }
    if map.0.totems.is_empty() {
        return;
    }
    let theme = map.0.theme.unwrap_or(DEFAULT_THEME);
    let material = original_material(&levels, theme, &mut images, &mut mats);
    let bank = objects.0.as_ref().filter(|_| material.is_some());
    let stone = mats.add(StandardMaterial { base_color: PILLAR_STONE, perceptual_roughness: 0.95, ..default() });
    let smoke = StandardMaterial { base_color: Color::srgba(0.75, 0.75, 0.75, 0.55), alpha_mode: AlphaMode::Blend, unlit: true, ..default() };
    let ball = meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap());
    let cell = WORLD_UNITS_PER_CELL as f32;
    for (index, t) in map.0.totems.iter().enumerate() {
        let at = Vec2::new(t.x as f32, t.z as f32) / cell;
        let parts: Vec<_> = bank.map(|bank| totem_objects(t.kind).into_iter().filter_map(|i| bank.get(i)).collect()).unwrap_or_default();
        let top = parts.iter().flat_map(|o| o.points.iter().map(|p| p[1])).max().map_or(PILLAR.y, |y| y as f32 / cell);
        let mut view = commands.spawn((TotemView { index, top }, crate::hover::Hoverable::default(), Grounded { at, half: HALF }, Transform::default(), Visibility::Hidden));
        if t.kind == TotemKind::Totem {
            view.with_children(|v| {
                for k in 0..PUFFS {
                    v.spawn((SmokePuff(k), Mesh3d(ball.clone()), MeshMaterial3d(mats.add(smoke.clone())), NotShadowCaster, crate::hover::NoOutline, Transform::default(), Visibility::Hidden));
                }
            });
        }
        let mut body = Vec::new();
        let (Some(mat), false) = (material.clone(), parts.is_empty()) else {
            view.with_children(|v| {
                let mut b = v.spawn((TotemBody, Transform::default(), Visibility::Inherited));
                match t.kind {
                    TotemKind::Totem => {
                        for k in 0..BLOCKS {
                            let side = PILLAR.x * 1.6 * (1.0 - 0.15 * k as f32);
                            b.with_child((RockBlock(k), Mesh3d(meshes.add(Cuboid::new(side, BLOCK_HIGH, side))), MeshMaterial3d(stone.clone()), Transform::from_xyz(0.0, BLOCK_HIGH * (k as f32 + 0.5), 0.0)));
                        }
                    }
                    TotemKind::StoneHead => {
                        let neck = NECK as f32 / cell;
                        b.with_child((Mesh3d(meshes.add(Cuboid::new(PILLAR.x * 1.3, neck, PILLAR.x * 1.3))), MeshMaterial3d(stone.clone()), Transform::from_xyz(0.0, neck / 2.0, 0.0)));
                        b.with_children(|p| {
                            p.spawn((NodPivot, Transform::from_xyz(0.0, neck, 0.0), Visibility::Inherited)).with_child((
                                Mesh3d(meshes.add(Cuboid::new(PILLAR.x * 1.1, PILLAR.y - neck, PILLAR.x))),
                                MeshMaterial3d(stone.clone()),
                                Transform::from_xyz(0.0, (PILLAR.y - neck) / 2.0, 0.0),
                            ));
                        });
                    }
                    _ => {
                        b.with_child((Mesh3d(meshes.add(Cuboid::from_size(PILLAR))), MeshMaterial3d(stone.clone()), Transform::from_xyz(0.0, PILLAR.y / 2.0, 0.0)));
                    }
                }
            });
            continue;
        };
        let offset = (t.x as usize + t.z as usize) * 3 % flame::FRAMES;
        for obj in parts {
            let solid = solid_part(obj);
            match t.kind {
                TotemKind::Totem => body.push((Mesh3d(meshes.add(to_mesh(object_mesh(&solid, 0)))), Some(StoneRocks { shown: None }), None)),
                TotemKind::StoneHead => {
                    let (pedestal, head) = split_head(&solid);
                    body.push((Mesh3d(meshes.add(to_mesh(object_mesh(&pedestal, 0)))), None, None));
                    body.push((Mesh3d(meshes.add(to_mesh(object_mesh(&head, 0)))), None, Some(NECK as f32 / cell)));
                }
                _ => body.push((Mesh3d(meshes.add(to_mesh(object_mesh(&solid, 0)))), None, None)),
            }
            let flames = flame::flame_mesh(obj);
            if !flames.indices.is_empty() {
                let frame = flame_frames.get(&levels, theme, &mut images, &mut mats)[offset].clone();
                view.with_child((Flame { offset }, Mesh3d(meshes.add(to_mesh(flames))), MeshMaterial3d(frame), NotShadowCaster, crate::hover::NoOutline));
            }
        }
        view.with_children(|v| {
            v.spawn((TotemBody, Transform::default(), Visibility::Inherited)).with_children(|b| {
                for (mesh, rocks, neck) in body {
                    match (rocks, neck) {
                        (Some(rocks), _) => {
                            b.spawn((rocks, mesh, MeshMaterial3d(mat.clone())));
                        }
                        (_, Some(neck)) => {
                            b.spawn((NodPivot, Transform::from_xyz(0.0, neck, 0.0), Visibility::Inherited)).with_child((mesh, MeshMaterial3d(mat.clone())));
                        }
                        _ => {
                            b.spawn((mesh, MeshMaterial3d(mat.clone())));
                        }
                    }
                }
            });
        });
    }
}

/// Each frame: the stone totems' rings turn and their bodies sink (smoke while sinking, hidden once
/// gone), the stone heads nod.
#[allow(clippy::too_many_arguments)]
fn animate_totems(
    time: Res<Time>,
    clock: Res<SimClock>,
    map: Res<CurrentMap>,
    objects: Res<OriginalObjects>,
    views: Query<(&TotemView, &Children)>,
    children: Query<&Children>,
    mut bodies: Bodies,
    mut rocks: Query<(&mut StoneRocks, &Mesh3d)>,
    mut blocks: Blocks,
    mut pivots: Pivots,
    mut puffs: Puffs,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let alpha = clock.alpha();
    let nod = nod_angle(time.elapsed_secs());
    let frames = objects.0.as_ref().and_then(|b| b.get(TOTEM).zip(b.get(TOTEM_ANIMATED)));
    for (view, kids) in &views {
        let Some(t) = map.0.totems.get(view.index) else { continue };
        let (turn, sink) = (turn_fraction(t, alpha), sink_fraction(t, alpha));
        let sinking = (t.since_given as f32 + alpha - (TURN_TICKS + HOLD_TICKS) as f32) / 10.0;
        for e in std::iter::once(kids.iter().collect::<Vec<_>>()).flatten().flat_map(|c| std::iter::once(c).chain(children.iter_descendants(c))) {
            if let Ok((mut tf, mut vis)) = bodies.get_mut(e) {
                tf.translation.y = -view.top * 1.05 * sink;
                vis.set_if_neq(if t.is_gone() { Visibility::Hidden } else { Visibility::Inherited });
            }
            if let (Ok((mut r, mesh)), Some((stored, turned))) = (rocks.get_mut(e), frames) {
                let shown = (turn * 1000.0) as u16;
                if r.shown != Some(shown) {
                    r.shown = Some(shown);
                    meshes.insert(&mesh.0, to_mesh(object_mesh(&twisted(&solid_part(stored), &solid_part(turned), turn), 0))).ok();
                }
            }
            if let Ok((block, mut tf)) = blocks.get_mut(e) {
                tf.rotation = Quat::from_rotation_y(if block.0 == 0 { 0.0 } else { TOTAL_TURN * turn });
            }
            if let Ok(mut tf) = pivots.get_mut(e) {
                tf.rotation = Quat::from_rotation_x(nod);
            }
            if let Ok((p, mut tf, mut vis, mat)) = puffs.get_mut(e) {
                match puff(p.0, sinking).filter(|_| t.is_exhausted() && !t.is_gone()) {
                    Some((at, size, opacity)) => {
                        (tf.translation, tf.scale) = (at, Vec3::splat(size));
                        vis.set_if_neq(Visibility::Inherited);
                        if let Some(mut m) = mats.get_mut(&mat.0) {
                            m.base_color.set_alpha(opacity);
                        }
                    }
                    None => {
                        vis.set_if_neq(Visibility::Hidden);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pop3_format::Face;

    #[test]
    fn every_totem_look_has_its_objects() {
        for kind in TotemKind::ALL {
            assert!(!totem_objects(kind).is_empty(), "{kind:?}");
        }
        assert_eq!(totem_objects(TotemKind::WingedDeath), vec![WINGED_DEATH_TOTEM, WINGED_DEATH_PERCHED], "with its bird");
        assert_eq!(totem_objects(TotemKind::Pole(2)), vec![TOTEM_POLES[2]]);
    }

    fn gave(given: u8, since: u16, occurrences: u8) -> Totem {
        Totem { given, since_given: since, occurrences, ..Totem::new(TotemKind::Totem, (0, 0)) }
    }

    #[test]
    fn it_turns_after_its_first_gift_and_sinks_once_exhausted() {
        assert_eq!(turn_fraction(&gave(0, 0, 0), 0.0), 0.0, "not yet");
        assert!((turn_fraction(&gave(1, TURN_TICKS / 2, 0), 0.0) - 0.5).abs() < 1e-6, "half way, eased");
        assert_eq!(turn_fraction(&gave(1, TURN_TICKS, 0), 0.0), 1.0);
        assert_eq!(turn_fraction(&gave(3, 0, 0), 0.0), 1.0, "turned for good");
        assert_eq!(sink_fraction(&gave(5, 999, 0), 0.0), 0.0, "no limit: never sinks");
        assert_eq!(sink_fraction(&gave(1, TURN_TICKS + HOLD_TICKS, 1), 0.0), 0.0, "turn, then the hold");
        assert_eq!(sink_fraction(&gave(1, TURN_TICKS + HOLD_TICKS + SINK_TICKS, 1), 0.0), 1.0);
    }

    fn ring(y: i16, angle: f32, r: f32) -> [i16; 3] {
        [(r * angle.cos()).round() as i16, y, (r * angle.sin()).round() as i16]
    }

    #[test]
    fn only_the_turning_rings_go_round_by_the_whole_turn() {
        let still = ring(160, 0.3, 212.0);
        let (from, to) = (ring(160, 0.0, 196.0), ring(160, 0.34, 214.0));
        let stored = Object { points: vec![still, from], faces: Vec::new() };
        let turned = Object { points: vec![still, to], faces: Vec::new() };
        assert_eq!(twisted(&stored, &turned, 0.0).points, stored.points, "as stored");
        let end = twisted(&stored, &turned, 1.0);
        assert_eq!(end.points[0], still, "a ring object 3 leaves alone");
        let p = end.points[1];
        assert!(((p[2] as f32).atan2(p[0] as f32).rem_euclid(TAU) - TOTAL_TURN.rem_euclid(TAU)).abs() < 0.02, "64° and a turn: {p:?}");
        assert!(((p[0] as f32).hypot(p[2] as f32) - 214.0).abs() < 1.5, "object 3's radius");
    }

    #[test]
    fn the_stone_head_splits_at_its_neck_and_nods() {
        let quad = |a: u16, b: u16, c: u16, d: u16| Face { tile: Some(1), colour: 0, points: vec![a, b, c, d], uv: vec![(0, 0); 4], flags: 6 };
        let points = vec![[0, 0, 0], [10, 0, 0], [10, NECK, 0], [0, NECK, 0], [10, 900, 0], [0, 900, 0]];
        let (pedestal, head) = split_head(&Object { points, faces: vec![quad(0, 1, 2, 3), quad(3, 2, 4, 5)] });
        assert_eq!((pedestal.faces.len(), head.faces.len()), (1, 1));
        assert_eq!(head.points[3], [0, 0, 0], "the neck is the head's origin");
        assert_eq!(nod_angle(0.0), 0.0);
        assert!((nod_angle(NOD_SECS / 4.0) - NOD_DEG.to_radians()).abs() < 1e-5, "6° at most");
        assert!((nod_angle(NOD_SECS) - nod_angle(0.0)).abs() < 1e-5, "one nod every 1.6 s");
    }

    #[test]
    fn smoke_rises_and_fades_while_it_sinks() {
        assert!(puff(0, -0.1).is_none(), "not before the sinking");
        assert!(puff(3, 0.5).is_none(), "puff 3 comes out at 0.75 s");
        let (low, _, thick) = puff(0, 0.1).unwrap();
        let (high, _, thin) = puff(0, 1.2).unwrap();
        assert!(high.y > low.y && thin < thick);
        assert!(puff(0, 10.0).is_none(), "over once sunk");
    }
}
