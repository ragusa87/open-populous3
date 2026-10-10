//! Open-source unit art baked by `unit-baker` (docs/specs/unit-art.md, "Baked atlases") from CC0
//! Quaternius characters (see assets/CREDITS.md): per kind, an atlas PNG of frames cropped around
//! their pixels at 4 pixels per base pixel, tribe-coloured parts in magenta, and its index (where
//! each pose, direction and frame sits, and its feet). Each tribe swaps the magenta hue for its colour.

use super::art::{Frame, Pose, TribeArt, DIRS};
use super::procedural::tribe_rgb;
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, Image, ImageSampler, ImageType};
use bevy::prelude::{Color, Hsva};
use game_core::unit::UnitKind;
use pop3_format::catalog::TRIBES;

/// Image pixels per base pixel (1 base pixel = 1/88 cell).
pub const SCALE: usize = 4;
const MAGENTA_HUE: f32 = 300.0;
/// Wildmen wear the brave's clothes in this colour (no tribe).
const WILD_HIDE: [u8; 3] = [150, 112, 70];

macro_rules! baked {
    ($kind:literal) => {
        (include_bytes!(concat!("../../../../assets/units/", $kind, ".png")).as_slice(), include_str!(concat!("../../../../assets/units/", $kind, ".txt")))
    };
}

/// The baked atlas (PNG) and index of a kind (`assets/units/<kind>.png` and `.txt`).
pub fn bundled(kind: UnitKind) -> (&'static [u8], &'static str) {
    match kind {
        UnitKind::Shaman => baked!("shaman"),
        UnitKind::Brave => baked!("brave"),
        UnitKind::Warrior => baked!("warrior"),
        UnitKind::Preacher => baked!("preacher"),
        UnitKind::Spy => baked!("spy"),
        UnitKind::Firewarrior => baked!("firewarrior"),
        // No model of their own: the brave's, in a neutral hide colour (`sheet_art`).
        UnitKind::Wildman => baked!("brave"),
    }
}

/// Pose name in the index.
pub fn pose_name(pose: Pose) -> &'static str {
    match pose {
        Pose::Idle => "idle",
        Pose::Walk => "walk",
        Pose::Pray => "pray",
        Pose::Cast => "cast",
        Pose::Fall => "fall",
        Pose::Drown => "drown",
        Pose::Stranded => "stranded",
        Pose::Chop => "chop",
        Pose::CarryWalk => "carry_walk",
        Pose::CarryIdle => "carry_idle",
        Pose::Jump => "jump",
        Pose::Hammer => "hammer",
        Pose::Flung => "flung",
        Pose::Tumble => "tumble",
    }
}

/// Whether a kind's sheets have a pose: only the shaman casts, and she is never stranded; the wood
/// jump and tumbling poses are not rendered yet (`Pose::fallback` stands in).
pub fn plays(kind: UnitKind, pose: Pose) -> bool {
    match pose {
        Pose::Cast => kind == UnitKind::Shaman,
        Pose::Stranded => kind != UnitKind::Shaman,
        Pose::Chop | Pose::CarryWalk | Pose::CarryIdle | Pose::Jump | Pose::Hammer | Pose::Flung | Pose::Tumble => false,
        _ => true,
    }
}

/// PNG bytes to RGBA (width, height, pixels).
pub fn decode(png: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let image = Image::from_buffer(png, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::Default, RenderAssetUsages::default()).ok()?;
    let rgba = image.try_into_dynamic().ok()?.to_rgba8();
    Some((rgba.width() as usize, rgba.height() as usize, rgba.into_raw()))
}

/// The frame an index entry points at in an atlas `width` x `height`; None if it lies outside.
pub fn frame_at(width: usize, height: usize, rgba: &[u8], entry: &unit_atlas::Entry) -> Option<Frame> {
    let [x, y, w, h] = entry.rect.map(|v| v as usize);
    if w == 0 || h == 0 || x + w > width || y + h > height {
        return None;
    }
    let pixels = (y..y + h).flat_map(|row| &rgba[(row * width + x) * 4..(row * width + x + w) * 4]).copied().collect();
    Some(Frame { width: w, height: h, origin: (entry.origin.0 as usize, entry.origin.1 as usize), rgba: pixels, scale: SCALE })
}

/// Every pose's frame loops, `[pose][dir][frame]` in `Pose::ALL` order (empty if not baked); an error
/// if the atlas does not decode, an entry falls outside it, or frames are missing or out of order.
pub fn frames(png: &[u8], index: &str) -> Result<Vec<Vec<Vec<Frame>>>, String> {
    let (width, height, rgba) = decode(png).ok_or("atlas does not decode")?;
    let mut poses: Vec<Vec<Vec<Frame>>> = Pose::ALL.iter().map(|_| vec![Vec::new(); DIRS]).collect();
    for e in unit_atlas::parse(index)? {
        let pose = Pose::ALL.iter().position(|&p| pose_name(p) == e.pose).ok_or_else(|| format!("unknown pose {}", e.pose))?;
        let loop_ = poses[pose].get_mut(e.dir).ok_or_else(|| format!("{} direction {}", e.pose, e.dir))?;
        if e.frame != loop_.len() {
            return Err(format!("{} direction {}: frame {} out of order", e.pose, e.dir, e.frame));
        }
        loop_.push(frame_at(width, height, &rgba, &e).ok_or_else(|| format!("{} {} {}: outside the atlas", e.pose, e.dir, e.frame))?);
    }
    Ok(poses)
}

/// Magenta parts take the tribe's hue (shading kept); everything else is untouched.
pub fn recolour(frame: &Frame, tribe: [u8; 3]) -> Frame {
    let target = Hsva::from(Color::srgb_u8(tribe[0], tribe[1], tribe[2])).hue;
    let mut out = frame.clone();
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

/// Every tribe's art for a kind from its baked atlas; None if a pose it plays is missing (or the
/// atlas is malformed). Poses it never plays show their fallback's loop (`Pose::fallback`), else its idle.
pub fn sheet_art(kind: UnitKind) -> Option<Vec<TribeArt>> {
    let (png, index) = bundled(kind);
    let mut poses = frames(png, index).map_err(|e| bevy::log::warn!("{kind:?} atlas: {e}")).ok()?;
    for &pose in &Pose::ALL {
        if poses[pose as usize].iter().any(Vec::is_empty) {
            if plays(kind, pose) {
                return None;
            }
            let stand_in = if pose.fallback() != pose { pose.fallback() } else { Pose::Idle };
            poses[pose as usize] = poses[stand_in as usize].clone();
        }
    }
    Some(
        (0..TRIBES)
            .map(|tribe| {
                let rgb = if kind == UnitKind::Wildman { WILD_HIDE } else { tribe_rgb(tribe) };
                TribeArt { poses: poses.iter().map(|dirs| dirs.iter().map(|frames| frames.iter().map(|f| recolour(f, rgb)).collect()).collect()).collect() }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baked(kind: UnitKind) -> Vec<Vec<Vec<Frame>>> {
        let (png, index) = bundled(kind);
        frames(png, index).unwrap_or_else(|e| panic!("{kind:?}: {e}"))
    }

    #[test]
    fn every_kind_pose_and_direction_is_baked_with_hard_edges() {
        for kind in UnitKind::ALL {
            let poses = baked(kind);
            for &pose in &Pose::ALL {
                for frames in &poses[pose as usize] {
                    assert_eq!(frames.is_empty(), !plays(kind, pose), "{kind:?} {pose:?}");
                    for f in frames {
                        assert_eq!(f.scale, SCALE);
                        assert!(f.rgba.chunks_exact(4).all(|p| p[3] == 0 || p[3] == 255), "{kind:?} {pose:?}: hard edges");
                    }
                }
            }
        }
    }

    #[test]
    fn feet_on_the_ground() {
        for kind in UnitKind::ALL {
            let poses = baked(kind);
            for frames in &poses[Pose::Idle as usize] {
                let f = &frames[0];
                let height = f.origin.1 as f32 / SCALE as f32;
                assert!((30.0..=55.0).contains(&height), "{kind:?} standing, headgear included: {height} base px");
                let below = f.height - f.origin.1;
                assert!(below <= 3 * SCALE, "{kind:?}: {below} px under the feet (outline and soles only)");
                assert!(f.origin.0 < f.width);
            }
        }
    }

    #[test]
    fn bundled_index_round_trips() {
        for kind in UnitKind::ALL {
            let (_, index) = bundled(kind);
            let entries = unit_atlas::parse(index).unwrap();
            assert_eq!(unit_atlas::write(&entries), index, "{kind:?}");
        }
    }

    #[test]
    fn frames_are_cut_from_the_index() {
        let rgba: Vec<u8> = (0..4 * 3).flat_map(|i| [i as u8, 0, 0, 255]).collect();
        let entry = unit_atlas::Entry { pose: "idle".into(), dir: 0, frame: 0, rect: [1, 1, 2, 2], origin: (1, 1) };
        let f = frame_at(4, 3, &rgba, &entry).unwrap();
        assert_eq!((f.width, f.height, f.origin), (2, 2, (1, 1)));
        assert_eq!(f.rgba.chunks(4).map(|p| p[0]).collect::<Vec<_>>(), [5, 6, 9, 10]);
        assert!(frame_at(4, 3, &rgba, &unit_atlas::Entry { rect: [3, 0, 2, 1], ..entry }).is_none());
    }

    #[test]
    fn recolour_swaps_only_magenta() {
        let mut f = Frame { width: 2, height: 1, origin: (0, 0), rgba: vec![200, 0, 200, 255, 214, 150, 100, 255], scale: 1 };
        f = recolour(&f, tribe_rgb(1));
        assert!(f.rgba[0] > 150 && f.rgba[1] < 40 && f.rgba[2] < 40, "magenta to red: {:?}", &f.rgba[..3]);
        assert_eq!(&f.rgba[4..8], &[214, 150, 100, 255], "skin untouched");
    }

    #[test]
    fn every_tribe_and_pose() {
        for kind in UnitKind::ALL {
            let art = sheet_art(kind).unwrap();
            assert_eq!(art.len(), TRIBES as usize);
            assert!(Pose::ALL.iter().all(|&p| (0..DIRS).all(|d| !art[2].frames(p, d).is_empty())), "{kind:?}");
        }
        let art = sheet_art(UnitKind::Shaman).unwrap();
        assert_ne!(art[0].frames(Pose::Idle, 0)[0].rgba, art[1].frames(Pose::Idle, 0)[0].rgba);
    }
}
