//! A short fire on the ground (pop3-rev-analysis.md "Components": the fire 7/0x0A lightning lights where it burns no
//! tree), an effect only seen: it does nothing to the world, the client draws it for its length.

use crate::effect::{Effect, EffectKind};
use crate::time::{Countdown, Ticks};

/// How long it burns (a guess: the original's 7/0x0A length is not traced).
pub const TICKS: Ticks = Ticks::new(48);

/// A fire lit by tribe `owner` at `at` (world units).
pub fn effect(owner: u8, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(TICKS), EffectKind::GroundFire)
}
