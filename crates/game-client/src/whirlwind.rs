//! Whirlwind (`game_core::whirlwind`): its 16 funnel segments as the simulation places them every turn
//! (camera-facing, bottom-centred, width `512 / (index + 1)` 256ths, tinted by their colour class), its dust puffs
//! (`EffectKind::WhirlwindDust`, 8 frames over their 8 turns) and the debris it carries or throws
//! (`GameMap::debris`). With the original files: `hfx0-0.dat`'s alpha sprites, tint 2 in the gold `data/alpha.dat`
//! gives it (pop3-rev-analysis.md "Whirlwind mechanics"). Without them (`--no-original`): our own rings, puffs and
//! twigs (`[ours]`).

use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::{render_pos, Grounded};
use crate::sim_time::{seed, SimNow, TICK_SECS};
use crate::units::art::{alpha_picture_frame, Frame};
use crate::units::{toward_eye, upload_frames, FrameAsset, PULL_TO_EYE};
use crate::views::{sync_views, WorldView};
use crate::world::{CurrentMap, LevelList};
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::time::Tick;
use game_core::whirlwind::{Debris, Tint, SEGMENTS};
use pop3_format::blend::{AlphaTable, STRENGTHS};
use pop3_format::catalog::{EFFECT_SPRITE_FILE, WHIRLWIND_BRANCH_SPRITES, WHIRLWIND_DUST_SPRITES, WHIRLWIND_FUNNEL_SPRITES, WHIRLWIND_GOLD, WHIRLWIND_ORANGE, WHIRLWIND_RED};
use pop3_format::{Picture, SpriteBank, Theme, WORLD_UNITS_PER_CELL};
use std::f32::consts::TAU;
use std::path::Path;

/// The alpha sprites' tint drawn gold.
const GOLD_TINT: usize = 2;
/// Our own colours without the original files (`[ours]`).
const OUR_GOLD: [u8; 3] = [226, 178, 70];
const OUR_RED: Color = Color::srgb(1.0, 0.3, 0.1);
const OUR_ORANGE: Color = Color::srgb(0.9, 0.55, 0.1);
/// Our own sprites' sizes: a funnel ring and a dust puff like the original's, a twig.
const RING: (usize, usize) = (80, 50);
const PUFF: (usize, usize) = (80, 41);
const TWIG: (usize, usize) = (60, 40);
const DUST_FRAMES: usize = 8;
const BRANCH_FRAMES: usize = 9;

/// Width scale of the segment at `index` (0 top) of a full funnel: 2× at the top down to 1/8 at the bottom [exe].
pub fn width_scale(index: u8) -> f32 {
    512.0 / (index as f32 + 1.0) / 256.0
}

/// Which of a dust puff's `frames` frames shows `secs` into its life: one a turn, the last held.
pub fn dust_frame(secs: f32, frames: usize) -> usize {
    ((secs / TICK_SECS).max(0.0) as usize).min(frames.saturating_sub(1))
}

/// `AlphaTable::rgba`, with tint 2 in `gold` instead of the level's own cream.
fn gold_lut(mut rgba: [[u8; 4]; 256], gold: [u8; 3]) -> [[u8; 4]; 256] {
    for (s, px) in rgba[GOLD_TINT * STRENGTHS..(GOLD_TINT + 1) * STRENGTHS].iter_mut().enumerate() {
        *px = [gold[0], gold[1], gold[2], (s * 255 / (STRENGTHS - 1)) as u8];
    }
    rgba
}

/// The original's frames (funnel, dust, branch) and the class tints (red-orange, orange).
fn original_frames(data_dir: &Path) -> Result<([Vec<Frame>; 3], [Color; 2]), String> {
    let err = |e: pop3_format::LevelError| e.to_string();
    let bank = SpriteBank::load(data_dir, EFFECT_SPRITE_FILE).map_err(err)?;
    let palette = Theme::load(data_dir, 0).map_err(err)?.palette;
    let colour = |i: usize| palette.get(i).copied().ok_or(format!("no palette entry {i}"));
    let lut = gold_lut(AlphaTable::load(data_dir, 0).map_err(err)?.rgba(&palette), colour(WHIRLWIND_GOLD)?);
    let frames = |range: std::ops::RangeInclusive<usize>, centred: bool| {
        range
            .map(|i| {
                let sprite = bank.sprites.get(i).ok_or(format!("no sprite {i} in {EFFECT_SPRITE_FILE}"))?;
                let origin = (sprite.width / 2, if centred { sprite.height / 2 } else { sprite.height.saturating_sub(1) });
                Ok(alpha_picture_frame(&Picture { sprite: sprite.clone(), origin }, &lut))
            })
            .collect::<Result<Vec<_>, String>>()
    };
    let tint = |i: usize| colour(i).map(|[r, g, b]| Color::srgb_u8(r, g, b));
    Ok(([frames(WHIRLWIND_FUNNEL_SPRITES, false)?, frames(WHIRLWIND_DUST_SPRITES, false)?, frames(WHIRLWIND_BRANCH_SPRITES, true)?], [tint(WHIRLWIND_RED)?, tint(WHIRLWIND_ORANGE)?]))
}

fn frame_of((w, h): (usize, usize), origin: (usize, usize), pixel: impl Fn(f32, f32) -> Option<[u8; 4]>) -> Frame {
    let rgba = (0..w * h).flat_map(|i| pixel((i % w) as f32 + 0.5, (i / w) as f32 + 0.5).unwrap_or([0, 0, 0, 0])).collect();
    Frame { width: w, height: h, origin, rgba, scale: 1 }
}

/// Our funnel ring at `phase` (0..16): a thin swirling ellipse, brighter where the swirl's streaks pass.
pub fn our_ring(phase: usize) -> Frame {
    let (w, h) = RING;
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    frame_of(RING, (w / 2, h - 1), |x, y| {
        let (dx, dy) = ((x - cx) / (cx - 2.0), (y - cy) / (cy - 2.0));
        let r = (dx * dx + dy * dy).sqrt();
        let band = (1.0 - ((r - 0.8) / 0.2).abs()).max(0.0);
        let swirl = 0.5 + 0.5 * (dy.atan2(dx) * 3.0 + phase as f32 / 16.0 * TAU).sin();
        let a = band * (0.25 + 0.75 * swirl);
        (a > 0.02).then(|| [OUR_GOLD[0], OUR_GOLD[1], OUR_GOLD[2], (a * 200.0) as u8])
    })
}

/// Our dust puff `k` of `DUST_FRAMES`: a soft cloud spreading and thinning.
pub fn our_puff(k: usize) -> Frame {
    let (w, h) = PUFF;
    let t = k as f32 / (DUST_FRAMES - 1) as f32;
    let (cx, cy, r) = (w as f32 / 2.0, h as f32 * 0.7, (0.25 + 0.7 * t) * h as f32 / 2.0);
    frame_of(PUFF, (w / 2, h - 1), |x, y| {
        let d = (((x - cx) / 1.6).powi(2) + (y - cy).powi(2)).sqrt() / r;
        let a = (1.0 - d).max(0.0) * (1.0 - t * 0.8);
        (a > 0.02).then(|| [OUR_GOLD[0], OUR_GOLD[1], OUR_GOLD[2], (a * 140.0) as u8])
    })
}

/// Our twig `k` of `BRANCH_FRAMES`: a dark stick with a fork, turning.
pub fn our_twig(k: usize) -> Frame {
    let (w, h) = TWIG;
    let angle = k as f32 / BRANCH_FRAMES as f32 * TAU;
    let (c, s) = (angle.cos(), angle.sin());
    let segment = |(x, y): (f32, f32), (ax, ay): (f32, f32), (bx, by): (f32, f32)| {
        let (vx, vy, px, py) = (bx - ax, by - ay, x - ax, y - ay);
        let t = ((px * vx + py * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
        ((px - vx * t).powi(2) + (py - vy * t).powi(2)).sqrt()
    };
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let at = |u: f32, v: f32| (cx + u * c - v * s, cy + u * s + v * c);
    frame_of(TWIG, (w / 2, h / 2), |x, y| {
        let d = segment((x, y), at(-22.0, 0.0), at(22.0, 0.0)).min(segment((x, y), at(6.0, 0.0), at(18.0, -10.0)) + 0.6);
        (d < 1.8).then_some([62, 40, 22, 255])
    })
}

fn our_frames() -> [Vec<Frame>; 3] {
    [(0..SEGMENTS).map(our_ring).collect(), (0..DUST_FRAMES).map(our_puff).collect(), (0..BRANCH_FRAMES).map(our_twig).collect()]
}

#[derive(Resource)]
pub struct WhirlwindAssets {
    funnel: Vec<FrameAsset>,
    /// The funnel's material per tint (gold, red-orange, orange).
    funnel_tints: [Handle<StandardMaterial>; 3],
    dust: Vec<FrameAsset>,
    dust_red: Handle<StandardMaterial>,
    branch: Vec<FrameAsset>,
}

fn tinted(mats: &mut Assets<StandardMaterial>, base: &Handle<StandardMaterial>, tint: Color) -> Handle<StandardMaterial> {
    let material = mats.get(base).cloned().map(|m| StandardMaterial { base_color: tint, ..m }).unwrap_or_default();
    mats.add(material)
}

fn load_assets(levels: Res<LevelList>, mut commands: Commands, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let original = levels.original.then(|| original_frames(&levels.data_dir).map_err(|e| warn!("original whirlwind sprites: {e}")).ok()).flatten();
    let ([funnel, dust, branch], [red, orange]) = original.unwrap_or_else(|| (our_frames(), [OUR_RED, OUR_ORANGE]));
    let funnel = upload_frames(&funnel, &mut images, &mut meshes, &mut mats);
    let dust = upload_frames(&dust, &mut images, &mut meshes, &mut mats);
    let branch = upload_frames(&branch, &mut images, &mut meshes, &mut mats);
    let (Some(f), Some(d)) = (funnel.first().map(|f| f.material.clone()), dust.first().map(|d| d.material.clone())) else { return };
    let funnel_tints = [f.clone(), tinted(&mut mats, &f, red), tinted(&mut mats, &f, orange)];
    let dust_red = tinted(&mut mats, &d, red);
    commands.insert_resource(WhirlwindAssets { funnel, funnel_tints, dust, dust_red, branch });
}

fn tint_index(t: Tint) -> usize {
    match t {
        Tint::Gold => 0,
        Tint::RedOrange => 1,
        Tint::Orange => 2,
    }
}

type Parts<'w, 's> = Query<'w, 's, (&'static mut Transform, &'static mut Visibility, &'static mut Mesh3d, &'static mut MeshMaterial3d<StandardMaterial>), Without<GameCamera>>;
type Eye = Query<'static, 'static, &'static Transform, With<GameCamera>>;

/// A sprite at `at` (world units), `lift` terrain units above the ground there: camera-facing, pulled towards the eye
/// like the units, `width` times as wide; None off the drawn planet.
fn place(map: &GameMap, at: (u16, u16), lift: f32, width: f32, rig: &CameraRig, curve: &CurveParamsRes, eye: &Transform) -> Option<Transform> {
    let cells = Vec2::new(at.0 as f32, at.1 as f32) / WORLD_UNITS_PER_CELL as f32;
    let feet = render_pos(&map.terrain, &Grounded { at: cells, half: 0.0 }, rig.focus, &curve.0)? + Vec3::Y * lift * curve.0.height_scale;
    let (pull, scale) = toward_eye(feet, eye.translation, PULL_TO_EYE);
    let facing = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
    Some(Transform { translation: feet + pull, rotation: facing, scale: Vec3::new(width * scale, scale, scale) })
}

fn ground(map: &GameMap, at: (u16, u16)) -> f32 {
    map.terrain.height_at(at.0 as u32, at.1 as u32, WORLD_UNITS_PER_CELL) as f32
}

fn show(parts: &mut Parts<'_, '_>, part: Entity, pose: Option<Transform>, frame: &FrameAsset, material: &Handle<StandardMaterial>) {
    let Ok((mut t, mut vis, mut mesh, mut mat)) = parts.get_mut(part) else { return };
    let Some(pose) = pose else {
        *vis = Visibility::Hidden;
        return;
    };
    (*t, *vis) = (pose, Visibility::Inherited);
    if mesh.0 != frame.mesh {
        mesh.0 = frame.mesh.clone();
    }
    if mat.0 != *material {
        mat.0 = material.clone();
    }
}

fn spawn_parts(view: &mut EntityCommands, frames: &[FrameAsset], n: usize) -> Option<Vec<Entity>> {
    let first = frames.first()?;
    let mut parts = Vec::with_capacity(n);
    view.insert((Transform::default(), Visibility::Visible)).with_children(|v| {
        for _ in 0..n {
            parts.push(v.spawn((Mesh3d(first.mesh.clone()), MeshMaterial3d(first.material.clone()), NotShadowCaster, crate::hover::NoOutline, Transform::default(), Visibility::Hidden)).id());
        }
    });
    Some(parts)
}

/// One whirlwind's funnel: a part per segment of the full column, by its index.
#[derive(Component)]
pub struct FunnelView {
    parts: Vec<Entity>,
}

impl WorldView for FunnelView {
    type Model = (Id<Effect>, Effect);
    type Params = (Res<'static, CurrentMap>, Option<Res<'static, WhirlwindAssets>>, Parts<'static, 'static>, Eye, Res<'static, CameraRig>, Res<'static, CurveParamsRes>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::Whirlwind(_)))
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn(_: &(Id<Effect>, Effect), view: &mut EntityCommands, (_, assets, ..): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let assets = assets.as_ref()?;
        Some(FunnelView { parts: spawn_parts(view, &assets.funnel, SEGMENTS)? })
    }

    fn update(&self, (_, e): &(Id<Effect>, Effect), _: SimNow, (map, assets, parts, eye, rig, curve): &mut SystemParamItem<Self::Params>) {
        let (Some(assets), Ok(eye), EffectKind::Whirlwind(w)) = (assets.as_ref(), eye.single(), &e.kind) else { return };
        let eye = *eye;
        for (index, &part) in self.parts.iter().enumerate() {
            let segment = w.segments().iter().find(|s| s.index as usize == index);
            let pose = segment.and_then(|s| place(&map.0, (s.x, s.z), s.height as f32 - ground(&map.0, (s.x, s.z)), width_scale(s.index), rig, curve, &eye));
            let frame = segment.map_or(0, |s| s.frame as usize % assets.funnel.len());
            let material = &assets.funnel_tints[segment.map_or(0, |s| tint_index(s.tint))];
            show(parts, part, pose, &assets.funnel[frame], material);
        }
    }
}

/// One dust puff.
#[derive(Component)]
pub struct DustView {
    part: Entity,
}

impl WorldView for DustView {
    type Model = (Id<Effect>, Effect);
    type Params = (Res<'static, CurrentMap>, Option<Res<'static, WhirlwindAssets>>, Parts<'static, 'static>, Eye, Res<'static, CameraRig>, Res<'static, CurveParamsRes>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Effect>, Effect)> {
        map.effects.entries().filter(|(_, e)| matches!(e.kind, EffectKind::WhirlwindDust(_)))
    }

    fn id((id, _): &(Id<Effect>, Effect)) -> u32 {
        id.raw()
    }

    fn spawn(_: &(Id<Effect>, Effect), view: &mut EntityCommands, (_, assets, ..): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let assets = assets.as_ref()?;
        Some(DustView { part: spawn_parts(view, &assets.dust, 1)?[0] })
    }

    fn update(&self, (_, e): &(Id<Effect>, Effect), now: SimNow, (map, assets, parts, eye, rig, curve): &mut SystemParamItem<Self::Params>) {
        let (Some(assets), Ok(eye), EffectKind::WhirlwindDust(tint)) = (assets.as_ref(), eye.single(), &e.kind) else { return };
        let start = Tick::from_wire(map.0.now.to_wire().wrapping_sub(e.left.done().get()));
        let secs = now.secs_since(start);
        let pose = (secs >= 0.0).then(|| place(&map.0, (e.x, e.z), 0.0, 1.0, rig, curve, eye)).flatten();
        let frame = &assets.dust[dust_frame(secs, assets.dust.len())];
        let material = if *tint == Tint::Gold { frame.material.clone() } else { assets.dust_red.clone() };
        show(parts, self.part, pose, frame, &material);
    }
}

/// A branch or wood piece carried or thrown.
#[derive(Component)]
pub struct DebrisView {
    part: Entity,
}

impl WorldView for DebrisView {
    type Model = (Id<Debris>, Debris);
    type Params = (Res<'static, CurrentMap>, Option<Res<'static, WhirlwindAssets>>, Parts<'static, 'static>, Eye, Res<'static, CameraRig>, Res<'static, CurveParamsRes>);

    fn models(map: &GameMap) -> impl Iterator<Item = &(Id<Debris>, Debris)> {
        map.debris.entries()
    }

    fn id((id, _): &(Id<Debris>, Debris)) -> u32 {
        id.raw()
    }

    fn spawn(_: &(Id<Debris>, Debris), view: &mut EntityCommands, (_, assets, ..): &mut SystemParamItem<Self::Params>) -> Option<Self> {
        let assets = assets.as_ref()?;
        Some(DebrisView { part: spawn_parts(view, &assets.branch, 1)?[0] })
    }

    fn update(&self, (id, d): &(Id<Debris>, Debris), now: SimNow, (map, assets, parts, eye, rig, curve): &mut SystemParamItem<Self::Params>) {
        let (Some(assets), Ok(eye)) = (assets.as_ref(), eye.single()) else { return };
        let lift = d.motion.height.map_or(0.0, |h| h as f32 - ground(&map.0, (d.x, d.z)));
        let pose = place(&map.0, (d.x, d.z), lift.max(0.0), 1.0, rig, curve, eye);
        let k = (now.tick.to_wire() as usize + (seed(id.raw()) * 9.0) as usize) % assets.branch.len();
        let frame = &assets.branch[k];
        show(parts, self.part, pose, frame, &frame.material.clone());
    }
}

pub struct WhirlwindPlugin;

impl Plugin for WhirlwindPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets).add_systems(
            Update,
            (sync_views::<FunnelView>, sync_views::<DustView>, sync_views::<DebrisView>).chain().after(crate::units::run_ticks),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_top_is_widest_and_the_bottom_an_eighth() {
        assert_eq!(width_scale(0), 2.0);
        assert_eq!(width_scale(1), 1.0);
        assert_eq!(width_scale(15), 0.125);
    }

    #[test]
    fn dust_plays_one_frame_a_turn_then_holds() {
        assert_eq!(dust_frame(0.0, 8), 0);
        assert_eq!(dust_frame(TICK_SECS * 3.5, 8), 3);
        assert_eq!(dust_frame(TICK_SECS * 20.0, 8), 7);
        assert_eq!(dust_frame(-1.0, 8), 0);
    }

    #[test]
    fn tint_2_is_drawn_gold_by_its_strength_the_rest_untouched() {
        let lut = gold_lut([[1, 2, 3, 4]; 256], [200, 150, 50]);
        assert_eq!(lut[0x2F], [200, 150, 50, 255]);
        assert_eq!(lut[0x20], [200, 150, 50, 0]);
        assert_eq!(lut[0x1F], [1, 2, 3, 4]);
        assert_eq!(lut[0x30], [1, 2, 3, 4]);
    }

    #[test]
    fn our_sprites_have_the_originals_sizes_and_something_drawn() {
        for f in (0..SEGMENTS).map(our_ring).chain((0..DUST_FRAMES).map(our_puff)).chain((0..BRANCH_FRAMES).map(our_twig)) {
            assert!(f.rgba.chunks_exact(4).any(|px| px[3] > 0));
        }
        let ring = our_ring(0);
        assert_eq!((ring.width, ring.height, ring.origin), (80, 50, (40, 49)), "bottom-centred");
        assert_ne!(our_ring(0).rgba, our_ring(4).rgba, "the phases differ");
    }
}
