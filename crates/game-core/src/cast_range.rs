//! How far a shaman casts (pop3-rev-analysis.md "Range"): a spell's `SP_W_RANGE`, scaled by the
//! caster's altitude band, a third more from a finished drum tower. Checked when aiming, like the
//! original, not when the command runs.

use crate::map::torus_dist2;

/// `ALT_BAND_0..7_SPELL_INCR` (constant.dat), percent of the range per altitude band.
pub const ALT_BAND_PERCENT: [u32; 8] = [80, 90, 100, 110, 120, 130, 140, 150];
/// Height units per altitude band (`z >> 7`).
pub const BAND_HEIGHT: u32 = 128;

/// The altitude percent at `height`, times `BAND_HEIGHT`: the band's, interpolated linearly to the next.
pub fn altitude_scale(height: u16) -> u32 {
    let band = (height as u32 / BAND_HEIGHT).min(7) as usize;
    let next = (band + 1).min(7);
    let frac = if band == 7 { 0 } else { height as u32 % BAND_HEIGHT };
    ALT_BAND_PERCENT[band] * BAND_HEIGHT + (ALT_BAND_PERCENT[next] - ALT_BAND_PERCENT[band]) * frac
}

/// The range (world units) of a spell reaching `base` at the default altitude, cast from `height`.
pub fn range(base: u32, height: u16, in_tower: bool) -> u32 {
    let r = base * altitude_scale(height) / (100 * BAND_HEIGHT);
    if in_tower { r + r / 3 } else { r }
}

/// Whether `to` is within `range` of `from` (world units), the short way around the torus.
pub fn within(from: (u16, u16), to: (u16, u16), range: u32) -> bool {
    torus_dist2(from, to) <= (range as i64).pow(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_of_128_interpolated() {
        assert_eq!(altitude_scale(0), 80 * 128);
        assert_eq!(altitude_scale(64), 85 * 128, "halfway to band 1");
        assert_eq!(altitude_scale(256), 100 * 128);
        assert_eq!(altitude_scale(7 * 128), 150 * 128);
        assert_eq!(altitude_scale(1024), 150 * 128, "capped at band 7");
    }

    #[test]
    fn range_scales_with_altitude_and_the_tower() {
        assert_eq!(range(3072, 0, false), 2457);
        assert_eq!(range(3072, 256, false), 3072);
        assert_eq!(range(3072, 1024, false), 4608);
        assert_eq!(range(3072, 256, true), 4096);
    }

    #[test]
    fn within_wraps_around_the_torus() {
        assert!(within((100, 100), (100 + 3072, 100), 3072));
        assert!(!within((100, 100), (100 + 3073, 100), 3072));
        assert!(within((65000, 0), (1000, 0), 3072), "1536 the short way");
    }
}
