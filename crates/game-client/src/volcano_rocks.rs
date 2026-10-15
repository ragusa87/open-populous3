//! Volcano's decorative rolling rocks (pop3-rev-analysis.md "Volcano rocks"): one born each turn `FIRST_TURN` to
//! `FIRST_TURN + COUNT` of a Volcano effect (`game_core::volcano`'s own doc: "drawn client-side only ... nothing
//! for the simulation to do" — no `EffectKind` data of its own, everything here is derived from the effect's id
//! and elapsed turns alone, so every client draws the same rocks). Drawn like wood (`wood.rs`): a camera-facing
//! sprite, the original's rolling rock (`hfx0-0.dat`, `VOLCANO_ROCK_SPRITES`, 4 frames picked by position so it
//! "seems to roll") when allowed, else our own small procedural stand-in (4 seeded variants, no original art to
//! trace).
//!
//! Motion is our own invented stand-in, not the original's: the trace has no launch at all ("no initial
//! velocity ... it falls", already created at rest at its landing point) — ours instead visibly ejects each rock
//! from the crater's own centre, arcing out to that same landing point under gravity (`FLIGHT_SECS`, a `peak *
//! 4t(1-t)` parabolic rise added on top of the actual ground it flies over, scaled by how far it's thrown), then
//! rolls downhill (`downhill_at`, a finite difference on the terrain under its landing point — steeper ground
//! rolls it farther, flat ground barely at all, the same spirit as the trace's own "slides while the slope is
//! above 24" without its exact per-triangle physics) before settling and fading after `REST_SECS` ("about 10.7 s",
//! the trace's own rest-before-deletion timer). `Grounded` re-samples the real terrain height under it at every
//! step of both, so it genuinely falls and rolls over the land's own shape, not a flat plane.

use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::{mesh_height, render_pos, Grounded};
use crate::sim_time::{seed, SimNow};
use crate::units::art::Frame;
use crate::units::{toward_eye, upload_frame, FrameAsset};
use crate::views::{sync_views, WorldView};
use crate::world::{CurrentMap, LevelList};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::terrain::Heightmap;
use game_core::time::{Tick, TICKS_PER_SECOND};
use pop3_format::catalog::{EFFECT_SPRITE_FILE, VOLCANO_ROCK_SPRITES};
use pop3_format::{Picture, SpriteBank, Theme, WORLD_UNITS_PER_CELL};
use std::f32::consts::TAU;
use std::path::Path;

/// One rock born per turn in this window (pop3-rev-analysis.md "Volcano rocks": turns 105-130, 26 in all).
const FIRST_TURN: u32 = 105;
const COUNT: u32 = 26;
/// Distance from the centre, world units (pop3-rev-analysis.md: `(rand & 0x1FF) * 2 + 0x600`, 1536-2558, 3-5 cells).
const MIN_DIST: f32 = 1536.0;
const MAX_DIST: f32 = 2558.0;
/// Our own invented ejection (see module doc): rises then falls over this long, arcing from the crater centre to
/// its landing point; how high above the ground it gets at the arc's midpoint (render units) scales with how far
/// it's thrown, between `FLIGHT_PEAK_MIN` (the closest rocks) and `FLIGHT_PEAK_MAX` (the farthest).
const FLIGHT_SECS: f32 = 0.9;
const FLIGHT_PEAK_MIN: f32 = 0.6;
const FLIGHT_PEAK_MAX: f32 = 1.6;
/// A settle-roll once it lands, before coming fully to rest: downhill, by `downhill_at`'s slope at the landing
/// point, scaled between `ROLL_DIST_MIN` (near-flat ground) and `ROLL_DIST_MAX` (as steep as the volcano's own
/// slopes, `SLOPE_REF`, height units per cell) — our own feel, same spirit as the trace's own "slides while the
/// slope is above 24", not its exact per-triangle physics. `ROLL_DECAY` shapes the friction-like ease-out.
const ROLL_SECS: f32 = 1.1;
const ROLL_DIST_MIN: f32 = 0.03;
const ROLL_DIST_MAX: f32 = 0.55;
const SLOPE_REF: f32 = 300.0;
const ROLL_DECAY: f32 = 4.0;
/// At rest this long before it vanishes (pop3-rev-analysis.md: "128 turns to live", 10.7 s).
const REST_SECS: f32 = 128.0 / TICKS_PER_SECOND as f32;
/// How far the sprite is pulled towards the camera so nearby ground does not cut it (`wood.rs`).
const PULL_TO_EYE: f32 = 0.25;

/// One rock's fixed-at-spawn placement, timing and landing physics (flight peak and roll both
/// derived once at spawn from how far it's thrown and the real terrain slope under where it lands).
#[derive(Component, Clone, Copy)]
pub struct Rock {
    /// The volcano effect it belongs to: looked up each frame for its own start tick.
    effect: Id<Effect>,
    /// Seconds into the volcano's own life that it's born.
    born: f32,
    /// Where it's ejected from (the crater centre) and where it lands, both in cells.
    from: Vec2,
    at: Vec2,
    /// How high its flight arcs above the ground (render units), scaled by throw distance.
    peak: f32,
    /// Downhill direction and distance (cells) it settle-rolls after landing (`downhill_at`).
    roll_dir: Vec2,
    roll_dist: f32,
}

/// Steepest-descent direction (unit vector, zero on flat ground) and slope magnitude (height units
/// dropped per cell) at `at` (cell coords), sampled the same way the terrain mesh draws it
/// (`grounded::mesh_height`) so a rock's roll follows the ground it's actually rendered on.
fn downhill_at(terrain: &Heightmap, at: Vec2) -> (Vec2, f32) {
    const H: f32 = 0.5;
    let y = |ox: f32, oz: f32| mesh_height(terrain, at.x + ox, at.y + oz);
    let grad = Vec2::new((y(H, 0.0) - y(-H, 0.0)) / (2.0 * H), (y(0.0, H) - y(0.0, -H)) / (2.0 * H));
    let mag = grad.length();
    if mag < 1e-3 {
        (Vec2::ZERO, 0.0)
    } else {
        (-grad / mag, mag)
    }
}

#[derive(Component)]
struct RockBody;

#[derive(Resource)]
pub struct RockAssets {
    frames: [FrameAsset; 4],
}

/// One volcano's rocks: a marker only — `pose_rocks` drives every rock by its own `Rock` component, not through
/// this view's children (it needs the camera and the effect's live state together with a `Query` on the rocks in
/// the same system, which `WorldView::update`'s `Params` tuple does not give it).
#[derive(Component)]
pub struct VolcanoRocks;

impl WorldView for VolcanoRocks {
    type Model = (Id<Effect>, Effect);
    type Params = (Res<'static, CurrentMap>, Res<'static, RockAssets>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::Volcano(_)))
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn((id, e): &(Id<Effect>, Effect), view: &mut EntityCommands, (map, assets): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let raw = id.raw();
        let centre = Vec2::new(e.x as f32, e.z as f32) / WORLD_UNITS_PER_CELL as f32;
        let (min_dist, max_dist) = (MIN_DIST / WORLD_UNITS_PER_CELL as f32, MAX_DIST / WORLD_UNITS_PER_CELL as f32);
        view.insert((Transform::default(), Visibility::Hidden));
        view.with_children(|v| {
            for i in 0..COUNT {
                let s = |salt: u32| seed(raw.wrapping_mul(71).wrapping_add(i.wrapping_mul(151)).wrapping_add(salt));
                let angle = s(1) * TAU;
                let dist = (MIN_DIST + s(2) * (MAX_DIST - MIN_DIST)) / WORLD_UNITS_PER_CELL as f32;
                let dir = Vec2::new(angle.cos(), angle.sin());
                let at = centre + dir * dist;
                let thrown = ((dist - min_dist) / (max_dist - min_dist)).clamp(0.0, 1.0);
                let (downhill, slope) = downhill_at(&map.0.terrain, at);
                let roll_dir = if downhill == Vec2::ZERO { Vec2::from_angle(s(4) * TAU) } else { downhill };
                let rock = Rock {
                    effect: *id,
                    born: (FIRST_TURN + i) as f32 / TICKS_PER_SECOND as f32,
                    from: centre,
                    at,
                    peak: FLIGHT_PEAK_MIN + (FLIGHT_PEAK_MAX - FLIGHT_PEAK_MIN) * thrown,
                    roll_dir,
                    roll_dist: ROLL_DIST_MIN + (ROLL_DIST_MAX - ROLL_DIST_MIN) * (slope / SLOPE_REF).min(1.0),
                };
                let frame = &assets.frames[0];
                v.spawn((rock, Grounded { at: rock.at, half: 0.0 }, Transform::default(), Visibility::Hidden, crate::object_scale::Scaled(crate::object_scale::ObjectGroup::Scenery), crate::hover::NoOutline))
                    .with_child((RockBody, Mesh3d(frame.mesh.clone()), MeshMaterial3d(frame.material.clone()), Transform::default()));
            }
        });
        Some(VolcanoRocks)
    }

    /// Nothing to do here: `pose_rocks` (`PostUpdate`, after the ground is placed) does all the per-frame motion,
    /// reading `Rock`/`Grounded` directly — it needs the camera and the effect's own current state, neither of
    /// which this trait's `Params` carries alongside a `Query` on the children in the same pass.
    fn update(&self, _model: &(Id<Effect>, Effect), _now: SimNow, _params: &mut SystemParamItem<Self::Params>) {}
}

/// The original's rolling rock, feet at its bottom centre, one `Frame` per `VOLCANO_ROCK_SPRITES` frame.
fn original_frames(data_dir: &Path) -> Result<[Frame; 4], String> {
    let err = |e: pop3_format::LevelError| e.to_string();
    let bank = SpriteBank::load(data_dir, EFFECT_SPRITE_FILE).map_err(err)?;
    let palette = Theme::load(data_dir, 0).map_err(err)?.palette;
    let mut frames = Vec::with_capacity(4);
    for i in VOLCANO_ROCK_SPRITES {
        let sprite = bank.sprites.get(i).ok_or(format!("no sprite {i} in {EFFECT_SPRITE_FILE}"))?;
        let origin = (sprite.width / 2, sprite.height.saturating_sub(1));
        frames.push(crate::units::art::picture_frame(&Picture { sprite: sprite.clone(), origin }, &palette));
    }
    frames.try_into().map_err(|_| "wrong frame count".to_string())
}

/// Our own small stand-in, 4 seeded variants for the same "rolling" look: a rough grey-brown blob, no original
/// art to trace.
fn generated_frames() -> [Frame; 4] {
    std::array::from_fn(|i| generated_frame(i as u32))
}

const GEN_SIZE: usize = 14;

fn generated_frame(variant: u32) -> Frame {
    let mut rgba = vec![0u8; GEN_SIZE * GEN_SIZE * 4];
    let s = |salt: u32| seed(variant.wrapping_mul(2_654_435_761).wrapping_add(salt));
    let (cx, cy) = (GEN_SIZE as f32 / 2.0, GEN_SIZE as f32 - 3.5);
    let base_r = GEN_SIZE as f32 * 0.34;
    let wobble_phase = s(1) * TAU;
    for y in 0..GEN_SIZE {
        for x in 0..GEN_SIZE {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let angle = dy.atan2(dx);
            let r = base_r * (1.0 + 0.14 * (angle * 3.0 + wobble_phase).sin());
            if dx * dx + dy * dy > r * r {
                continue;
            }
            let shade = (0.55 - 0.25 * (dx / base_r).clamp(-1.0, 1.0) + 0.1 * (dy / base_r)).clamp(0.25, 0.85);
            let (cr, cg, cb) = (shade * 110.0 + 30.0, shade * 96.0 + 26.0, shade * 88.0 + 24.0);
            let i = (y * GEN_SIZE + x) * 4;
            rgba[i..i + 4].copy_from_slice(&[cr as u8, cg as u8, cb as u8, 255]);
        }
    }
    Frame { width: GEN_SIZE, height: GEN_SIZE, origin: (GEN_SIZE / 2, GEN_SIZE - 1), rgba, scale: 1 }
}

fn load_assets(levels: Res<LevelList>, mut commands: Commands, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let original = levels.original.then(|| original_frames(&levels.data_dir).map_err(|e| warn!("original volcano rock sprite: {e}")).ok()).flatten();
    let frames = original.unwrap_or_else(generated_frames);
    let uploaded: Vec<FrameAsset> = frames.iter().map(|f| upload_frame(f, &mut images, &mut meshes, &mut mats)).collect();
    let Ok(frames) = uploaded.try_into() else { return };
    commands.insert_resource(RockAssets { frames });
}

/// The rolling frame to show at world position `(x, z)` (pop3-rev-analysis.md: `346 + ((x >> 6) + (z >> 6)) & 3`).
fn frame_at((x, z): (i32, i32)) -> usize {
    (((x >> 6) + (z >> 6)) & 3) as usize
}

/// Every rock's motion, each frame, after the ground is placed under it (`grounded::place_grounded`): the
/// ejection arc, the settle-roll, fading out, which rolling frame to show, and facing the camera — all of it
/// read fresh from `Rock`/`CurrentMap`/`SimNow` rather than carried state, so it survives a save/load or a replay
/// the same way every other view does.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn pose_rocks(
    map: Res<CurrentMap>,
    now: Res<SimNow>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    assets: Res<RockAssets>,
    eye: Query<&Transform, (With<GameCamera>, Without<Rock>, Without<RockBody>)>,
    mut rocks: Query<(&Rock, &mut Grounded, &mut Transform, &mut Visibility, &Children), Without<RockBody>>,
    mut bodies: Query<(&mut Transform, &mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>), With<RockBody>>,
) {
    let Ok(eye) = eye.single() else { return };
    let facing = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
    for (rock, mut grounded, mut root, mut vis, children) in &mut rocks {
        let Some(e) = map.0.effects.get(rock.effect) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let start = Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get()));
        let age = now.secs_since(start) - rock.born;
        let gone = FLIGHT_SECS + ROLL_SECS + REST_SECS;
        if age < 0.0 || age > gone {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Visible;
        let (xz, height) = if age < FLIGHT_SECS {
            let t = age / FLIGHT_SECS;
            (rock.from.lerp(rock.at, t), rock.peak * 4.0 * t * (1.0 - t))
        } else {
            let roll_t = ((age - FLIGHT_SECS) / ROLL_SECS).clamp(0.0, 1.0);
            let eased = 1.0 - (-ROLL_DECAY * roll_t).exp();
            (rock.at + rock.roll_dir * rock.roll_dist * eased, 0.0)
        };
        grounded.at = xz;
        let Some(feet) = render_pos(&map.0.terrain, &grounded, rig.focus, &params.0) else {
            *vis = Visibility::Hidden;
            continue;
        };
        root.translation = feet;
        let (offset, scale) = toward_eye(feet + Vec3::Y * height, eye.translation, PULL_TO_EYE);
        let world = (xz * WORLD_UNITS_PER_CELL as f32).as_ivec2();
        let frame = &assets.frames[frame_at((world.x, world.y))];
        for &child in children {
            if let Ok((mut t, mut mesh, mut mat)) = bodies.get_mut(child) {
                t.translation = facing.inverse() * offset;
                t.rotation = facing;
                t.scale = Vec3::splat(scale);
                if mesh.0 != frame.mesh {
                    mesh.0 = frame.mesh.clone();
                }
                if mat.0 != frame.material {
                    mat.0 = frame.material.clone();
                }
            }
        }
    }
}

pub struct VolcanoRocksPlugin;

impl Plugin for VolcanoRocksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets)
            .add_systems(Update, sync_views::<VolcanoRocks>.after(crate::units::run_ticks))
            .add_systems(PostUpdate, pose_rocks.after(crate::grounded::place_grounded).before(TransformSystems::Propagate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_frames_are_opaque_blobs_feet_at_bottom_centre() {
        for f in generated_frames() {
            assert_eq!((f.width, f.height, f.scale), (GEN_SIZE, GEN_SIZE, 1));
            assert_eq!(f.origin, (GEN_SIZE / 2, GEN_SIZE - 1));
            assert!(f.rgba.chunks_exact(4).any(|px| px[3] == 255), "something drawn");
            assert!(f.rgba.chunks_exact(4).all(|px| px[3] == 0 || px[3] == 255), "opaque, like the original");
        }
    }

    #[test]
    fn the_four_variants_differ() {
        let [a, b, _, _] = generated_frames();
        assert_ne!(a.rgba, b.rgba);
    }

    #[test]
    fn frame_at_cycles_every_eighth_of_a_cell() {
        let frames: Vec<usize> = (0..4).map(|k| frame_at((k * 64, 0))).collect();
        assert_eq!(frames, [0, 1, 2, 3]);
        assert_eq!(frame_at((256, 0)), frame_at((0, 0)), "wraps every 4 steps");
    }

    #[test]
    fn downhill_at_points_away_from_rising_ground() {
        let size = 8;
        let heights: Vec<u16> = (0..size * size).map(|i| (i % size) as u16 * 100).collect();
        let terrain = Heightmap::from_heights(size, heights);
        let (dir, slope) = downhill_at(&terrain, Vec2::new(4.0, 4.0));
        assert!(dir.x < 0.0, "rolls towards lower x, where the ground is lower: {dir}");
        assert!(slope > 0.0);
    }

    #[test]
    fn downhill_at_is_zero_on_flat_ground() {
        let terrain = Heightmap::new(8);
        let (dir, slope) = downhill_at(&terrain, Vec2::new(4.0, 4.0));
        assert_eq!(dir, Vec2::ZERO);
        assert_eq!(slope, 0.0);
    }
}
