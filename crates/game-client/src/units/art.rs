//! Unit sprite art: per kind, tribe, pose and view direction, a loop of RGBA frames anchored at
//! the feet. The shaman from the original animations when allowed (see docs/specs/animations.md),
//! everything else generated (`procedural`). Which frame to show is decided here from the simulated action.

use game_core::unit::{Action, UnitKind, CAST_TICKS, DYING_TICKS, JUMP_TICKS};
use pop3_format::anim::{AnimBank, Outfit, SPRITE_FILE};
use pop3_format::catalog::{PersonAnim, ARMS_UP_FRAME, ShamanAnim, WildmanAnim, OUTFIT_FIREWARRIOR, OUTFIT_PREACHER, OUTFIT_SPY, OUTFIT_WARRIOR, TRIBES};
use pop3_format::{LevelError, Picture, SpriteBank, Theme};
use std::f32::consts::FRAC_PI_4;
use std::path::Path;

/// View directions: 0 faces the viewer, 2 screen right, 4 shows the back.
pub const DIRS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    Idle,
    Walk,
    Pray,
    Cast,
    Fall,
    Drown,
    /// Arms up, not moving: the target cannot be reached.
    Stranded,
    /// A brave cutting wood.
    Chop,
    /// A brave walking with a piece of wood.
    CarryWalk,
    /// A brave standing with a piece of wood.
    CarryIdle,
    /// A brave jumping to flatten a plan's ground.
    Jump,
    /// A brave hammering inside a building under construction.
    Hammer,
}

impl Pose {
    pub const ALL: [Pose; 12] =
        [Pose::Idle, Pose::Walk, Pose::Pray, Pose::Cast, Pose::Fall, Pose::Drown, Pose::Stranded, Pose::Chop, Pose::CarryWalk, Pose::CarryIdle, Pose::Jump, Pose::Hammer];

    /// The pose drawn instead where a pose has no art of its own (wood poses: braves only, and not in
    /// the open-source sheets yet).
    pub fn fallback(self) -> Pose {
        match self {
            Pose::Chop | Pose::CarryIdle | Pose::Hammer => Pose::Idle,
            Pose::CarryWalk => Pose::Walk,
            Pose::Jump => Pose::Stranded,
            pose => pose,
        }
    }

    /// Frames per second of looping poses.
    fn fps(self) -> f32 {
        match self {
            Pose::Walk | Pose::CarryWalk => 10.0,
            Pose::Drown => 8.0,
            Pose::Pray | Pose::Stranded => 4.0,
            _ => 6.0,
        }
    }

    fn original(self) -> ShamanAnim {
        match self {
            Pose::Idle | Pose::Stranded | Pose::Chop | Pose::CarryIdle | Pose::Hammer => ShamanAnim::Idle,
            Pose::Walk | Pose::CarryWalk => ShamanAnim::Walk,
            Pose::Pray => ShamanAnim::Kneel,
            Pose::Cast | Pose::Jump => ShamanAnim::Cast,
            Pose::Fall => ShamanAnim::Fall,
            Pose::Drown => ShamanAnim::Flung,
        }
    }
}

/// The pose for an action; `carrying` wood changes standing and walking.
pub fn pose_for(action: &Action, carrying: bool) -> Pose {
    match action {
        Action::Idle | Action::Landing { .. } if carrying => Pose::CarryIdle,
        Action::Idle | Action::Landing { .. } => Pose::Idle,
        Action::Walking { .. } | Action::AroundFire { .. } if carrying => Pose::CarryWalk,
        Action::Walking { .. } | Action::AroundFire { .. } | Action::Entering { .. } => Pose::Walk,
        Action::Chopping { .. } => Pose::Chop,
        Action::Building { .. } | Action::Hammering => Pose::Hammer,
        Action::Flattening { .. } => Pose::Jump,
        Action::Holding { .. } => Pose::CarryIdle,
        Action::Stranded { .. } => Pose::Stranded,
        Action::Worshipping { .. } => Pose::Pray,
        Action::Casting { .. } => Pose::Cast,
        Action::Drowning => Pose::Drown,
        Action::Dying { .. } | Action::Dead { .. } => Pose::Fall,
    }
}

/// RGBA pixels with the feet at `origin` (pixels from the top-left corner), `scale` pixels per base
/// pixel (1 for pixel art, upscaled when uploaded; more for detailed art, drawn as is).
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub origin: (usize, usize),
    pub rgba: Vec<u8>,
    pub scale: usize,
}

/// `poses[pose as usize][dir]`: the frame loop for that pose seen from that direction.
#[derive(Clone, Debug, Default)]
pub struct TribeArt {
    pub poses: Vec<Vec<Vec<Frame>>>,
}

impl TribeArt {
    pub fn frames(&self, pose: Pose, dir: usize) -> &[Frame] {
        self.poses.get(pose as usize).and_then(|d| d.get(dir % DIRS)).map_or(&[], Vec::as_slice)
    }
}

/// Which of the 8 drawn directions shows a unit with heading `facing` (eighths of a turn,
/// 0 = +z, 2 = +x) to a camera at `yaw` (the eye sits towards `(sin yaw, cos yaw)`).
pub fn sprite_dir(facing: u8, yaw: f32) -> usize {
    let a = facing as f32 * FRAC_PI_4 - yaw;
    ((a / FRAC_PI_4).round() as i32).rem_euclid(DIRS as i32) as usize
}

/// Frame to show: timed actions play once in step with the simulation (`alpha` = fraction of
/// the current tick), the fall holds the lying frame (second to last); others loop on `anim_secs`.
pub fn frame_index(pose: Pose, frames: usize, action: &Action, anim_secs: f32, alpha: f32) -> usize {
    if frames == 0 {
        return 0;
    }
    let lying = frames.saturating_sub(2);
    let progress = |ticks: u16| (action.elapsed().unwrap_or(0) as f32 + alpha) / ticks as f32;
    match action {
        Action::Casting { .. } => ((progress(CAST_TICKS) * frames as f32) as usize).min(frames - 1),
        Action::Flattening { .. } => ((progress(JUMP_TICKS) * frames as f32) as usize).min(frames - 1),
        Action::Dying { .. } => ((progress(DYING_TICKS) * (lying + 1) as f32) as usize).min(lying),
        Action::Dead { .. } => lying,
        _ => (anim_secs * pose.fps()) as usize % frames,
    }
}

/// Scale2x (EPX) pixel-art upscale: doubles the size, rounding diagonal staircases instead of
/// making blocks. Origin doubles too.
pub fn scale2x(f: &Frame) -> Frame {
    let (w, h) = (f.width as i32, f.height as i32);
    let px = |x: i32, y: i32| -> [u8; 4] {
        if (0..w).contains(&x) && (0..h).contains(&y) {
            let i = (y * w + x) as usize * 4;
            [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2], f.rgba[i + 3]]
        } else {
            [0; 4]
        }
    };
    let (w2, h2) = (f.width * 2, f.height * 2);
    let mut rgba = vec![0u8; w2 * h2 * 4];
    for y in 0..h {
        for x in 0..w {
            let (p, a, b, c, d) = (px(x, y), px(x, y - 1), px(x + 1, y), px(x - 1, y), px(x, y + 1));
            let pick = |n1: [u8; 4], n2: [u8; 4], o1: [u8; 4], o2: [u8; 4]| if n1 == n2 && n1 != o1 && n2 != o2 { n1 } else { p };
            let out = [pick(c, a, d, b), pick(a, b, c, d), pick(d, c, b, a), pick(b, d, a, c)];
            for (k, v) in out.iter().enumerate() {
                let (ox, oy) = (x as usize * 2 + k % 2, y as usize * 2 + k / 2);
                rgba[(oy * w2 + ox) * 4..][..4].copy_from_slice(v);
            }
        }
    }
    Frame { width: w2, height: h2, origin: (f.origin.0 * 2, f.origin.1 * 2), rgba, scale: f.scale * 2 }
}

/// Give transparent pixels the colour of an opaque neighbour (alpha stays 0), so linear
/// filtering does not darken the edges.
pub fn bleed_edges(f: &mut Frame) {
    let (w, h) = (f.width, f.height);
    let src = f.rgba.clone();
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            if src[i + 3] != 0 {
                continue;
            }
            let near = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)];
            if let Some(j) = near.iter().filter(|&&(nx, ny)| nx < w && ny < h).map(|&(nx, ny)| (ny * w + nx) * 4).find(|&j| src[j + 3] != 0) {
                f.rgba[i..i + 3].copy_from_slice(&src[j..j + 3]);
            }
        }
    }
}

pub fn picture_frame(p: &Picture, palette: &[[u8; 3]]) -> Frame {
    let rgba = p
        .sprite
        .pixels
        .iter()
        .flat_map(|px| match px.and_then(|i| palette.get(i as usize)) {
            Some(&[r, g, b]) => [r, g, b, 255],
            None => [0, 0, 0, 0],
        })
        .collect();
    Frame { width: p.sprite.width, height: p.sprite.height, origin: p.origin, rgba, scale: 1 }
}

/// A blended sprite (`pop3_format::blend`): each pixel's RGBA from the theme's alpha table
/// (`AlphaTable::rgba`), so it is blended over whatever is behind it instead of drawn as colours.
pub fn alpha_picture_frame(p: &Picture, rgba: &[[u8; 4]; 256]) -> Frame {
    let rgba = p.sprite.pixels.iter().flat_map(|px| px.map_or([0, 0, 0, 0], |i| rgba[i as usize])).collect();
    Frame { width: p.sprite.width, height: p.sprite.height, origin: p.origin, rgba, scale: 1 }
}

/// Whether a frame has see-through pixels that are not fully transparent (drawn blended).
pub fn has_partial_alpha(f: &Frame) -> bool {
    f.rgba.chunks_exact(4).any(|px| px[3] != 0 && px[3] != 255)
}

/// Which original animation shows `kind` in `pose` for `tribe`, with which outfit layer, and
/// which single frame to hold if any. The shaman has an animation per tribe; the others are
/// coloured by layers (docs/specs/animations.md).
pub fn original_anim(kind: UnitKind, pose: Pose, tribe: u8) -> (usize, Option<Outfit>, Option<usize>) {
    match kind {
        UnitKind::Shaman => (pose.original().anim(tribe), None, None),
        UnitKind::Wildman => {
            let anim = match pose {
                Pose::Idle | Pose::Cast | Pose::Stranded | Pose::Chop | Pose::CarryIdle | Pose::Jump | Pose::Hammer => WildmanAnim::Stand,
                Pose::Walk | Pose::CarryWalk => WildmanAnim::Walk,
                Pose::Pray => WildmanAnim::Sit,
                Pose::Fall => WildmanAnim::Down,
                Pose::Drown => WildmanAnim::Flung,
            };
            (anim.anim(), None, None)
        }
        _ => {
            let outfit = match kind {
                UnitKind::Warrior => Some(OUTFIT_WARRIOR),
                UnitKind::Firewarrior => Some(OUTFIT_FIREWARRIOR),
                UnitKind::Spy => Some(OUTFIT_SPY),
                UnitKind::Preacher => Some(OUTFIT_PREACHER),
                _ => None,
            };
            let pose = if kind == UnitKind::Brave { pose } else { pose.fallback() };
            let anim = match pose {
                Pose::Idle | Pose::Cast => PersonAnim::Stand,
                Pose::Walk => PersonAnim::Walk,
                Pose::Pray => PersonAnim::Kneel,
                Pose::Fall => PersonAnim::Fall,
                Pose::Drown => PersonAnim::Drown,
                Pose::Stranded => PersonAnim::ArmsUp,
                Pose::Chop => PersonAnim::Chop,
                Pose::CarryWalk => PersonAnim::CarryWalk,
                Pose::CarryIdle => PersonAnim::CarryStand,
                Pose::Jump => PersonAnim::ArmsUp,
                // No hammering anim identified yet: the axe swing stands in.
                Pose::Hammer => PersonAnim::Chop,
            };
            (anim.anim(), outfit, (pose == Pose::Stranded).then_some(ARMS_UP_FRAME))
        }
    }
}

/// The original person animations and sprites (palette `pal0-0`), loaded once.
pub struct Originals {
    bank: AnimBank,
    sprites: SpriteBank,
    palette: Vec<[u8; 3]>,
}

impl Originals {
    pub fn load(data_dir: &Path) -> Result<Self, String> {
        let err = |e: LevelError| e.to_string();
        Ok(Originals {
            bank: AnimBank::load(data_dir).map_err(err)?,
            sprites: SpriteBank::load(data_dir, SPRITE_FILE).map_err(err)?,
            palette: Theme::load(data_dir, 0).map_err(err)?.palette,
        })
    }

    /// Every tribe's art for a unit kind.
    pub fn art(&self, kind: UnitKind) -> Result<Vec<TribeArt>, String> {
        let bank = &self.bank;
        let art = (0..TRIBES)
            .map(|tribe| TribeArt {
                poses: Pose::ALL
                    .iter()
                    .map(|&pose| {
                        let (anim, outfit, held) = original_anim(kind, pose, tribe);
                        let layer_tribe = if matches!(kind, UnitKind::Shaman | UnitKind::Wildman) { 0 } else { tribe };
                        (0..DIRS)
                            .map(|dir| {
                                let mirrored = bank.start(anim, dir).is_some_and(|s| s.mirrored);
                                let frames = bank.frame_loop(anim, dir);
                                let frames = match held {
                                    Some(i) => frames.get(i).map_or(&[][..], std::slice::from_ref),
                                    None => &frames[..],
                                };
                                frames.iter().map(|&f| picture_frame(&bank.compose_as(&self.sprites, f, mirrored, layer_tribe, outfit), &self.palette)).collect()
                            })
                            .collect()
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        let complete = art.iter().all(|t| Pose::ALL.iter().all(|&p| (0..DIRS).all(|d| !t.frames(p, d).is_empty())));
        if complete { Ok(art) } else { Err(format!("{kind:?} animations missing ({} in the file)", bank.starts.len())) }
    }
}

pub fn generated_art(kind: UnitKind) -> Vec<TribeArt> {
    (0..TRIBES)
        .map(|tribe| TribeArt {
            poses: Pose::ALL
                .iter()
                .map(|&pose| (0..DIRS).map(|dir| super::procedural::frames(kind, tribe, pose, dir)).collect())
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_sprites_keep_their_strength() {
        let mut lut = [[0u8; 4]; 256];
        lut[0x18] = [10, 20, 30, 136];
        let sprite = pop3_format::Sprite { width: 2, height: 1, pixels: vec![Some(0x18), None] };
        let f = alpha_picture_frame(&Picture { sprite, origin: (1, 0) }, &lut);
        assert_eq!(f.rgba, vec![10, 20, 30, 136, 0, 0, 0, 0]);
        assert!(has_partial_alpha(&f));
        let opaque = Frame { rgba: vec![1, 2, 3, 255, 0, 0, 0, 0], ..f };
        assert!(!has_partial_alpha(&opaque));
    }
    use std::f32::consts::PI;

    #[test]
    fn direction_relative_to_the_camera() {
        assert_eq!(sprite_dir(4, 0.0), 4, "walking away from a camera on +z: back");
        assert_eq!(sprite_dir(0, 0.0), 0, "towards the camera: front");
        assert_eq!(sprite_dir(2, 0.0), 2, "+x is screen right at yaw 0");
        assert_eq!(sprite_dir(2, PI / 2.0), 0, "camera moved to +x: she faces it");
        assert_eq!(sprite_dir(0, PI / 4.0), 7);
    }

    #[test]
    fn timed_actions_play_once_in_step_with_ticks() {
        let cast = |left| Action::Casting { left };
        assert_eq!(frame_index(Pose::Cast, 12, &cast(CAST_TICKS), 5.0, 0.0), 0);
        assert_eq!(frame_index(Pose::Cast, 12, &cast(6), 0.0, 0.0), 6);
        assert_eq!(frame_index(Pose::Cast, 12, &cast(1), 0.0, 0.99), 11);
        assert_eq!(frame_index(Pose::Fall, 8, &Action::Dying { left: 1 }, 0.0, 0.99), 6);
        assert_eq!(frame_index(Pose::Fall, 8, &Action::Dead { left: 3 }, 0.0, 0.0), 6, "lies still");
    }

    #[test]
    fn open_actions_loop() {
        assert_eq!(frame_index(Pose::Walk, 8, &Action::Walking { to: (0, 0) }, 0.95, 0.0), 1);
        assert_eq!(frame_index(Pose::Walk, 8, &Action::Walking { to: (0, 0) }, 1.0, 0.0), 2);
        assert_eq!(frame_index(Pose::Idle, 0, &Action::Idle, 1.0, 0.0), 0);
    }

    #[test]
    fn stranded_tribesmen_hold_their_arms_up() {
        assert_eq!(pose_for(&Action::Stranded { to: (0, 0) }, false), Pose::Stranded);
        assert_eq!(pose_for(&Action::Chopping { tree: (0, 0), left: 3 }, false), Pose::Chop);
        assert_eq!(pose_for(&Action::Walking { to: (0, 0) }, true), Pose::CarryWalk);
        assert_eq!(pose_for(&Action::Idle, true), Pose::CarryIdle);
        assert_eq!(pose_for(&Action::Holding { left: 5 }, true), Pose::CarryIdle);
        assert_eq!(original_anim(UnitKind::Brave, Pose::Chop, 1), (11, None, None), "cutting wood");
        assert_eq!(original_anim(UnitKind::Brave, Pose::CarryWalk, 0), (9, None, None));
        assert_eq!(original_anim(UnitKind::Brave, Pose::CarryIdle, 0), (10, None, None));
        assert_eq!(original_anim(UnitKind::Warrior, Pose::Chop, 0), original_anim(UnitKind::Warrior, Pose::Idle, 0), "braves only");
        assert_eq!(original_anim(UnitKind::Shaman, Pose::CarryWalk, 2), original_anim(UnitKind::Shaman, Pose::Walk, 2));
        assert_eq!(original_anim(UnitKind::Brave, Pose::Stranded, 2), (12, None, Some(ARMS_UP_FRAME)));
        assert_eq!(original_anim(UnitKind::Brave, Pose::Jump, 1), (12, None, None), "the whole flattening jump");
        assert_eq!(pose_for(&Action::Flattening { at: (0, 0), left: 3 }, false), Pose::Jump);
        assert_eq!(pose_for(&Action::Hammering, false), Pose::Hammer);
        assert_eq!(original_anim(UnitKind::Brave, Pose::Hammer, 0).0, 11, "the axe swing stands in");
        assert_eq!(original_anim(UnitKind::Warrior, Pose::Stranded, 0), (12, Some(OUTFIT_WARRIOR), Some(ARMS_UP_FRAME)));
        assert_eq!(original_anim(UnitKind::Preacher, Pose::Pray, 1), (8, Some(OUTFIT_PREACHER), None), "the monk on the tribesman body");
        assert_eq!(original_anim(UnitKind::Spy, Pose::Drown, 0), (40, Some(OUTFIT_SPY), None), "lying, spirit rising");
        assert_eq!(original_anim(UnitKind::Shaman, Pose::Stranded, 1), original_anim(UnitKind::Shaman, Pose::Idle, 1));
    }

    #[test]
    fn generated_art_covers_every_pose_and_direction() {
        let art = generated_art(UnitKind::Warrior);
        assert_eq!(art.len(), TRIBES as usize);
        for pose in Pose::ALL {
            for dir in 0..DIRS {
                let frames = art[1].frames(pose, dir);
                assert!(!frames.is_empty(), "{pose:?} {dir}");
                assert!(frames.iter().all(|f| f.rgba.len() == f.width * f.height * 4));
            }
        }
        assert_ne!(art[0].frames(Pose::Idle, 0)[0].rgba, art[1].frames(Pose::Idle, 0)[0].rgba, "tribe colour");
    }

    fn frame(w: usize, h: usize, on: &[(usize, usize)]) -> Frame {
        let mut rgba = vec![0u8; w * h * 4];
        for &(x, y) in on {
            rgba[(y * w + x) * 4..][..4].copy_from_slice(&[255, 255, 255, 255]);
        }
        Frame { width: w, height: h, origin: (1, 2), rgba, scale: 1 }
    }

    #[test]
    fn scale2x_rounds_diagonals() {
        let f = scale2x(&frame(2, 2, &[(0, 0), (1, 1)]));
        assert_eq!((f.width, f.height, f.origin), (4, 4, (2, 4)));
        let on = |x: usize, y: usize| f.rgba[(y * 4 + x) * 4 + 3] != 0;
        assert!(on(0, 0) && on(1, 1) && on(2, 2) && on(3, 3));
        assert!(on(2, 1) && on(1, 2), "the staircase gets filled in");
        assert!(!on(3, 0) && !on(0, 3));
        let single = scale2x(&frame(1, 1, &[(0, 0)]));
        assert!(single.rgba.chunks(4).all(|p| p[3] == 255), "a lone pixel becomes a block");
    }

    #[test]
    fn bleeding_colours_keeps_transparency() {
        let mut f = frame(2, 1, &[(0, 0)]);
        f.rgba[0..3].copy_from_slice(&[10, 20, 30]);
        bleed_edges(&mut f);
        assert_eq!(&f.rgba[4..8], &[10, 20, 30, 0]);
    }

    #[test]
    fn picture_uses_palette_and_transparency() {
        let p = Picture { sprite: pop3_format::Sprite { width: 2, height: 1, pixels: vec![Some(1), None] }, origin: (1, 1) };
        let f = picture_frame(&p, &[[0, 0, 0], [9, 8, 7]]);
        assert_eq!(f.rgba, vec![9, 8, 7, 255, 0, 0, 0, 0]);
    }
}
