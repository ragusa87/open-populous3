//! Shaman sprite art: per tribe, pose and view direction, a loop of RGBA frames anchored at the
//! feet. From the original animations when allowed (see docs/specs/animations.md), else generated
//! (`procedural`). Which frame to show is decided here from the simulated action.

use game_core::unit::{Action, CAST_TICKS, DYING_TICKS};
use pop3_format::anim::{AnimBank, SPRITE_FILE};
use pop3_format::catalog::{ShamanAnim, TRIBES};
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
}

impl Pose {
    pub const ALL: [Pose; 6] = [Pose::Idle, Pose::Walk, Pose::Pray, Pose::Cast, Pose::Fall, Pose::Drown];

    /// Frames per second of looping poses.
    fn fps(self) -> f32 {
        match self {
            Pose::Walk => 10.0,
            Pose::Drown => 8.0,
            Pose::Pray => 4.0,
            _ => 6.0,
        }
    }

    fn original(self) -> ShamanAnim {
        match self {
            Pose::Idle => ShamanAnim::Idle,
            Pose::Walk => ShamanAnim::Walk,
            Pose::Pray => ShamanAnim::Kneel,
            Pose::Cast => ShamanAnim::Cast,
            Pose::Fall => ShamanAnim::Fall,
            Pose::Drown => ShamanAnim::Flung,
        }
    }
}

pub fn pose_for(action: &Action) -> Pose {
    match action {
        Action::Idle => Pose::Idle,
        Action::Walking { .. } => Pose::Walk,
        Action::Praying => Pose::Pray,
        Action::Casting { .. } => Pose::Cast,
        Action::Drowning => Pose::Drown,
        Action::Dying { .. } | Action::Dead { .. } => Pose::Fall,
    }
}

/// RGBA pixels with the feet at `origin` (pixels from the top-left corner).
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub origin: (usize, usize),
    pub rgba: Vec<u8>,
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
    Frame { width: w2, height: h2, origin: (f.origin.0 * 2, f.origin.1 * 2), rgba }
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
    Frame { width: p.sprite.width, height: p.sprite.height, origin: p.origin, rgba }
}

/// Every tribe's shaman from the original animations (sprites in palette `pal0-0`).
pub fn original_art(data_dir: &Path) -> Result<Vec<TribeArt>, String> {
    let err = |e: LevelError| e.to_string();
    let bank = AnimBank::load(data_dir).map_err(err)?;
    let sprites = SpriteBank::load(data_dir, SPRITE_FILE).map_err(err)?;
    let palette = Theme::load(data_dir, 0).map_err(err)?.palette;
    let art = (0..TRIBES)
        .map(|tribe| TribeArt {
            poses: Pose::ALL
                .iter()
                .map(|pose| {
                    let anim = pose.original().anim(tribe);
                    (0..DIRS)
                        .map(|dir| {
                            let mirrored = bank.start(anim, dir).is_some_and(|s| s.mirrored);
                            let frames = bank.frame_loop(anim, dir);
                            frames.iter().map(|&f| picture_frame(&bank.compose(&sprites, f, mirrored), &palette)).collect()
                        })
                        .collect()
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let complete = art.iter().all(|t| Pose::ALL.iter().all(|&p| (0..DIRS).all(|d| !t.frames(p, d).is_empty())));
    if complete { Ok(art) } else { Err(format!("shaman animations missing ({} in the file)", bank.starts.len())) }
}

pub fn generated_art() -> Vec<TribeArt> {
    (0..TRIBES)
        .map(|tribe| TribeArt {
            poses: Pose::ALL
                .iter()
                .map(|&pose| (0..DIRS).map(|dir| super::procedural::frames(tribe, pose, dir)).collect())
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn generated_art_covers_every_pose_and_direction() {
        let art = generated_art();
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
        Frame { width: w, height: h, origin: (1, 2), rgba }
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
