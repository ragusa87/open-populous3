//! Open-source unit art from rendered sheets (docs/specs/unit-art.md, "Rendered sheets"): one PNG per
//! pose, 8 rows (directions, see `art::sprite_dir`) of 320 x 288 cells at 4 pixels per base pixel, feet
//! at (160, 256), tribe-coloured parts in magenta. Made from a rigged model by the `render_sprites`
//! example. Each tribe swaps the magenta hue for its colour.

use super::art::{Frame, Pose, TribeArt, DIRS};
use super::procedural::tribe_rgb;
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, Image, ImageSampler, ImageType};
use bevy::prelude::{Color, Hsva};
use game_core::unit::UnitKind;
use pop3_format::catalog::TRIBES;

/// Image pixels per base pixel (1 base pixel = 1/88 cell).
pub const SCALE: usize = 4;
pub const CELL: (usize, usize) = (320, 288);
const FEET: (usize, usize) = (160, 256);
const MAGENTA_HUE: f32 = 300.0;

macro_rules! sheet {
    ($kind:literal, $pose:literal) => {
        include_bytes!(concat!("../../../../assets/units/", $kind, "/", $pose, ".png")).as_slice()
    };
}

/// The bundled sheet (PNG) of a kind's pose, if it has art.
pub fn bundled(kind: UnitKind, pose: Pose) -> Option<&'static [u8]> {
    match kind {
        UnitKind::Shaman => Some(match pose {
            Pose::Idle => sheet!("shaman", "idle"),
            Pose::Walk => sheet!("shaman", "walk"),
            Pose::Pray => sheet!("shaman", "pray"),
            Pose::Cast => sheet!("shaman", "cast"),
            Pose::Fall => sheet!("shaman", "fall"),
            Pose::Drown => sheet!("shaman", "drown"),
        }),
        _ => None,
    }
}

/// PNG bytes to RGBA (width, height, pixels).
pub fn decode(png: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let image = Image::from_buffer(png, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::Default, RenderAssetUsages::default()).ok()?;
    let rgba = image.try_into_dynamic().ok()?.to_rgba8();
    Some((rgba.width() as usize, rgba.height() as usize, rgba.into_raw()))
}

/// Cell `(col, row)` of a sheet `width` pixels wide, cropped to its pixels (feet kept inside).
pub fn cell(width: usize, rgba: &[u8], col: usize, row: usize) -> Frame {
    let (x0, y0) = (col * CELL.0, row * CELL.1);
    let opaque = |x: usize, y: usize| rgba[((y0 + y) * width + x0 + x) * 4 + 3] != 0;
    let (mut l, mut t, mut r, mut b) = (FEET.0, FEET.1, FEET.0 + 1, FEET.1 + 1);
    for y in 0..CELL.1 {
        for x in 0..CELL.0 {
            if opaque(x, y) {
                (l, t, r, b) = (l.min(x), t.min(y), r.max(x + 1), b.max(y + 1));
            }
        }
    }
    let mut out = Vec::with_capacity((r - l) * (b - t) * 4);
    for y in t..b {
        let row = ((y0 + y) * width + x0) * 4;
        out.extend_from_slice(&rgba[row + l * 4..row + r * 4]);
    }
    Frame { width: r - l, height: b - t, origin: (FEET.0 - l, FEET.1 - t), rgba: out, scale: SCALE }
}

/// Every direction's frame loop of a sheet: `[dir][frame]`; None if it is not 8 rows of cells.
pub fn split(png: &[u8]) -> Option<Vec<Vec<Frame>>> {
    let (w, h, rgba) = decode(png)?;
    if h != CELL.1 * DIRS || w % CELL.0 != 0 || w == 0 {
        return None;
    }
    Some((0..DIRS).map(|dir| (0..w / CELL.0).map(|col| cell(w, &rgba, col, dir)).collect()).collect())
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

/// Every tribe's art for a kind from its bundled sheets; None if it has none (or one is malformed).
pub fn sheet_art(kind: UnitKind) -> Option<Vec<TribeArt>> {
    let poses: Vec<Vec<Vec<Frame>>> = Pose::ALL.iter().map(|&pose| split(bundled(kind, pose)?)).collect::<Option<_>>()?;
    Some(
        (0..TRIBES)
            .map(|tribe| {
                let rgb = tribe_rgb(tribe);
                TribeArt { poses: poses.iter().map(|dirs| dirs.iter().map(|frames| frames.iter().map(|f| recolour(f, rgb)).collect()).collect()).collect() }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_sheets_are_8_rows_of_cells_with_feet_on_the_ground() {
        for pose in Pose::ALL {
            let dirs = split(bundled(UnitKind::Shaman, pose).unwrap()).unwrap_or_else(|| panic!("{pose:?} sheet"));
            assert_eq!(dirs.len(), DIRS);
            for frames in &dirs {
                assert!(!frames.is_empty());
                for f in frames {
                    assert_eq!(f.scale, SCALE);
                    assert!(f.rgba.chunks_exact(4).all(|p| p[3] == 0 || p[3] == 255), "{pose:?}: hard edges");
                }
            }
        }
        let idle = &split(bundled(UnitKind::Shaman, Pose::Idle).unwrap()).unwrap()[0][0];
        let height = idle.origin.1 as f32 / SCALE as f32;
        assert!((40.0..=55.0).contains(&height), "standing, hat included: {height} base px");
        assert!(bundled(UnitKind::Brave, Pose::Idle).is_none());
    }

    #[test]
    fn cells_crop_around_the_feet() {
        let mut rgba = vec![0u8; CELL.0 * 2 * CELL.1 * 4];
        let w = CELL.0 * 2;
        for (x, y) in [(CELL.0 + 150, 200), (CELL.0 + 170, 255)] {
            rgba[(y * w + x) * 4 + 3] = 255;
        }
        let f = cell(w, &rgba, 1, 0);
        assert_eq!((f.width, f.height, f.origin), (21, 57, (10, 56)));
        assert_eq!(cell(w, &rgba, 0, 0).origin, (0, 0), "empty cell: just the feet");
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
        let art = sheet_art(UnitKind::Shaman).unwrap();
        assert_eq!(art.len(), TRIBES as usize);
        assert_ne!(art[0].frames(Pose::Idle, 0)[0].rgba, art[1].frames(Pose::Idle, 0)[0].rgba);
        assert!(Pose::ALL.iter().all(|&p| (0..DIRS).all(|d| !art[2].frames(p, d).is_empty())));
    }
}
