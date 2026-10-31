//! Smoke (pop3-rev-analysis.md "Components": smoke 7/0x03, the building smokes 55 and 74-76), an effect only seen:
//! it does nothing to the world, the client draws it for its length. A burning building's structural hit gives off
//! damage smoke (`crate::fire`).

use crate::effect::{Effect, EffectKind};
use crate::time::{Countdown, Ticks};
use crate::tribes::Owner;

/// How long a building's damage smoke lasts (a guess: the original's building smokes are not traced).
pub const DAMAGE_TICKS: Ticks = Ticks::new(36);

/// Damage smoke of a building of tribe `owner` standing at `at` (world units).
pub fn damage(owner: impl Into<Owner>, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(DAMAGE_TICKS), EffectKind::Smoke)
}
