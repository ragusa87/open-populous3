//! Open-source unit art from a single still picture (docs/specs/unit-art.md, "Still"): a 256 x 256
//! RGBA image at 4 pixels per base pixel, feet at (128, 232), tribe-coloured parts in magenta. Every
//! pose is made by moving the still around its feet (walk bob, cast jump, fall, sinking...), each
//! tribe by swapping the magenta hue for its colour. Left-hand views are mirrored.

use super::art::{Frame, Pose, TribeArt, DIRS};
use super::procedural::{frame_count, tribe_rgb};
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, Image, ImageSampler, ImageType};
use bevy::prelude::{Color, Hsva};
use game_core::unit::UnitKind;
use pop3_format::catalog::TRIBES;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Image pixels per base pixel (1 base pixel = 1/88 cell).
pub const SCALE: usize = 4;
const FEET: (f32, f32) = (128.0, 232.0);
/// Output canvas before cropping: room for the fall (lying ~40 base px from the feet) and the jump.
const OUT: (usize, usize) = (420, 320);
const OUT_FEET: (f32, f32) = (210.0, 296.0);
const MAGENTA_HUE: f32 = 300.0;
const WATER: [u8; 4] = [170, 210, 255, 255];

static SHAMAN: &[u8] = include_bytes!("../../../../assets/units/shaman/still.png");

/// The bundled still of a kind, if any.
pub fn bundled(kind: UnitKind) -> Option<&'static [u8]> {
    match kind {
        UnitKind::Shaman => Some(SHAMAN),
        _ => None,
    }
}

/// PNG bytes to a frame anchored at the still's feet.
pub fn decode(png: &[u8]) -> Option<Frame> {
    let image = Image::from_buffer(png, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::Default, RenderAssetUsages::default()).ok()?;
    let rgba = image.try_into_dynamic().ok()?.to_rgba8();
    let (width, height) = (rgba.width() as usize, rgba.height() as usize);
    Some(Frame { width, height, origin: (FEET.0 as usize, FEET.1 as usize), rgba: rgba.into_raw(), scale: SCALE })
}

/// Magenta parts take the tribe's hue (shading kept); everything else is untouched.
pub fn recolour(still: &Frame, tribe: [u8; 3]) -> Frame {
    let target = Hsva::from(Color::srgb_u8(tribe[0], tribe[1], tribe[2])).hue;
    let mut out = still.clone();
    for px in out.rgba.chunks_exact_mut(4).filter(|p| p[3] != 0) {
        let hsva = Hsva::from(Color::srgb_u8(px[0], px[1], px[2]));
        let off = (hsva.hue - MAGENTA_HUE).abs();
        if off.min(360.0 - off) <= 30.0 && hsva.saturation > 0.3 {
            let c = Color::from(Hsva { hue: target, ..hsva }).to_srgba();
            px[..3].copy_from_slice(&[c.red, c.green, c.blue].map(|v| (v * 255.0).round() as u8));
        }
    }
    out
}

/// How a frame moves the still: up (base px), turned about the feet (radians, positive = head to
/// the right), squashed vertically, and under water below the feet line (ripples drawn there).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Placement {
    lift: f32,
    angle: f32,
    squash: f32,
    water: bool,
}

fn placement(pose: Pose, f: usize, n: usize) -> Placement {
    let base = Placement { squash: 1.0, ..Default::default() };
    let t = f as f32 / n as f32;
    match pose {
        Pose::Idle => Placement { squash: 1.0 - 0.01 * (f % 2) as f32, ..base },
        Pose::Walk => Placement { lift: 1.5 * (t * TAU).sin().abs(), angle: 0.04 * (t * TAU).sin(), ..base },
        Pose::Pray => Placement { squash: 0.8, angle: 0.12, ..base },
        Pose::Cast => Placement { lift: 12.0 * (f as f32 / (n - 1) as f32 * PI).sin(), ..base },
        Pose::Fall => Placement { angle: -FRAC_PI_2 * (f as f32 / (n - 2) as f32).min(1.0), ..base },
        Pose::Drown => Placement { lift: -10.0 - (f % 2) as f32, angle: 0.08 * if f % 2 == 0 { 1.0 } else { -1.0 }, water: true, ..base },
    }
}

/// The still moved by `p`, cropped to its pixels (feet kept inside), mirrored if asked.
fn place(still: &Frame, p: Placement, mirrored: bool) -> Frame {
    let (sin, cos) = p.angle.sin_cos();
    let mut rgba = vec![0u8; OUT.0 * OUT.1 * 4];
    for oy in 0..OUT.1 {
        for ox in 0..OUT.0 {
            if p.water && oy as f32 >= OUT_FEET.1 {
                continue;
            }
            // Figure space (x right, y up, from the feet), then undo lift, turn and squash.
            let (x, y) = (ox as f32 + 0.5 - OUT_FEET.0, OUT_FEET.1 - oy as f32 - 0.5 - p.lift * SCALE as f32);
            let (x, y) = (x * cos + y * sin, -x * sin + y * cos);
            let (sx, sy) = ((FEET.0 + x).floor(), (FEET.1 - y / p.squash).floor());
            if sx < 0.0 || sy < 0.0 || sx >= still.width as f32 || sy >= still.height as f32 {
                continue;
            }
            let i = (sy as usize * still.width + sx as usize) * 4;
            rgba[(oy * OUT.0 + ox) * 4..][..4].copy_from_slice(&still.rgba[i..i + 4]);
        }
    }
    if p.water {
        let y = OUT_FEET.1 as usize;
        for x in (OUT_FEET.0 as usize - 40)..(OUT_FEET.0 as usize + 40) {
            if (x / 6) % 3 != 2 {
                rgba[(y * OUT.0 + x) * 4..][..4].copy_from_slice(&WATER);
            }
        }
    }
    crop(&rgba, mirrored)
}

fn crop(rgba: &[u8], mirrored: bool) -> Frame {
    let feet = (OUT_FEET.0 as usize, OUT_FEET.1 as usize);
    let (mut x0, mut y0, mut x1, mut y1) = (feet.0, feet.1, feet.0 + 1, feet.1 + 1);
    for y in 0..OUT.1 {
        for x in 0..OUT.0 {
            if rgba[(y * OUT.0 + x) * 4 + 3] != 0 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let mut out = Vec::with_capacity(w * h * 4);
    for y in y0..y1 {
        let row = &rgba[(y * OUT.0 + x0) * 4..(y * OUT.0 + x1) * 4];
        if mirrored {
            out.extend(row.chunks_exact(4).rev().flatten());
        } else {
            out.extend_from_slice(row);
        }
    }
    let ox = feet.0 - x0;
    Frame { width: w, height: h, origin: (if mirrored { w - ox } else { ox }, feet.1 - y0), rgba: out, scale: SCALE }
}

/// The frame loop of `pose` seen from `dir`: the still faces down-right, so directions 0-4 show it
/// as is and 5-7 mirrored.
pub fn frames(still: &Frame, pose: Pose, dir: usize) -> Vec<Frame> {
    let n = frame_count(pose);
    (0..n).map(|f| place(still, placement(pose, f, n), dir % DIRS >= 5)).collect()
}

/// Every tribe's art for a kind from its bundled still; None if it has none (or it does not decode).
pub fn still_art(kind: UnitKind) -> Option<Vec<TribeArt>> {
    let still = decode(bundled(kind)?)?;
    Some(
        (0..TRIBES)
            .map(|tribe| {
                let coloured = recolour(&still, tribe_rgb(tribe));
                TribeArt { poses: Pose::ALL.iter().map(|&pose| (0..DIRS).map(|dir| frames(&coloured, pose, dir)).collect()).collect() }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shaman() -> Frame {
        decode(SHAMAN).expect("bundled still decodes")
    }

    fn opaque(f: &Frame) -> usize {
        f.rgba.chunks_exact(4).filter(|p| p[3] != 0).count()
    }

    #[test]
    fn bundled_still_is_normalised() {
        let s = shaman();
        assert_eq!((s.width, s.height, s.origin, s.scale), (256, 256, (128, 232), SCALE));
        assert!(s.rgba.chunks_exact(4).all(|p| p[3] == 0 || p[3] == 255), "hard edges");
        let magenta = s.rgba.chunks_exact(4).filter(|p| p[3] != 0 && p[0] as u16 > p[1] as u16 + 60 && p[2] as u16 > p[1] as u16 + 60).count();
        assert!(magenta > 1000, "tribe-coloured robe: {magenta} px");
        assert!(bundled(UnitKind::Brave).is_none());
    }

    #[test]
    fn recolour_swaps_only_magenta() {
        let mut f = Frame { width: 2, height: 1, origin: (0, 0), rgba: vec![200, 0, 200, 255, 214, 150, 100, 255], scale: 1 };
        f = recolour(&f, tribe_rgb(1));
        assert!(f.rgba[0] > 150 && f.rgba[1] < 40 && f.rgba[2] < 40, "magenta to red: {:?}", &f.rgba[..3]);
        assert_eq!(&f.rgba[4..8], &[214, 150, 100, 255], "skin untouched");
    }

    #[test]
    fn poses_move_the_still_around_its_feet() {
        let s = shaman();
        let idle = &frames(&s, Pose::Idle, 1)[0];
        assert_eq!(opaque(idle), opaque(&s), "idle is the still");
        let cast = frames(&s, Pose::Cast, 1);
        assert!(cast[6].origin.1 > idle.origin.1 + 40, "jumps up: feet {} px under the top", cast[6].origin.1);
        let fall = frames(&s, Pose::Fall, 1);
        assert!(fall[6].width > fall[6].height, "lying: {}x{}", fall[6].width, fall[6].height);
        let drown = &frames(&s, Pose::Drown, 1)[0];
        assert!(opaque(drown) < opaque(idle) * 3 / 4, "sunk into the water");
        assert_eq!(frames(&s, Pose::Walk, 1).len(), 8);
    }

    #[test]
    fn left_views_are_mirrored() {
        let s = shaman();
        let (r, l) = (&frames(&s, Pose::Idle, 1)[0], &frames(&s, Pose::Idle, 7)[0]);
        assert_eq!((r.width, r.height), (l.width, l.height));
        assert_eq!(l.origin.0, r.width - r.origin.0);
    }

    #[test]
    fn every_tribe_and_pose() {
        let art = still_art(UnitKind::Shaman).unwrap();
        assert_eq!(art.len(), TRIBES as usize);
        assert_ne!(art[0].frames(Pose::Idle, 0)[0].rgba, art[1].frames(Pose::Idle, 0)[0].rgba);
        assert!(Pose::ALL.iter().all(|&p| !art[2].frames(p, 3).is_empty()));
    }
}
