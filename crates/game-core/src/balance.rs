//! Game-balance numbers (docs/specs/huts-and-training.md "Balance and game turns"): plain data, grouped by
//! topic, read from `GameMap` instead of scattered module constants - one place to tune, and one place to
//! fill from a real install's `constant.dat` (`Balance::apply_constants`, reconciled one group at a time,
//! TODO.md "Computer players (AI)" - the way `game_core::spell_book::SpellStats` already was). Lockstep
//! peers must agree on it: part of the game setup, not a global.

use crate::time::Every;
use crate::unit::UnitKind;

/// A duration in the original's own game turns, as `constant.dat` stores it (pop3-rev-analysis.md "Turns and
/// timing": the original runs 12 turns a second, same as our own ticks). `Balance::ticks` is the identity
/// today - a seam for a correction, not a guessed ratio, if a session that actually dumps `constant.dat` finds
/// its own encoding does not match ticks one for one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Turns(pub u32);

impl Turns {
    pub const fn new(n: u32) -> Turns {
        Turns(n)
    }
}

/// Houses: breeding and growth (huts-and-training.md "Huts" rules 5-6, pop3-rev-analysis.md "Huts: breeding and
/// growth").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Huts {
    /// Breeding's own rhythm and step (`BREED_EVERY`, `BREED_STEP` [exe]).
    pub breed_every: Every,
    pub breed_step: u16,
    /// Growth's own rhythm, step and target (`GROW_EVERY`, `GROW_STEP` [exe]; `GROW_TARGET`, exe-only, no
    /// `constant.dat` entry [exe]).
    pub grow_every: Every,
    pub grow_step: u16,
    pub grow_target: u16,
    /// `HUT1..3_SPROG_TIME` by level (1-3) [files].
    pub sprog_time: [u16; 3],
    /// `SPROG%_POP_BAND_00_04%` (band 0) and `_95_99%` (band 19) [files]; the 18 between are `[ours]`, see
    /// `hut_growth`'s own module docs.
    pub sprog_pct_first: u32,
    pub sprog_pct_last: u32,
    pub sprog_bands: u32,
    /// A freshly completed hut's own breed head start: not quite full, "the first brave comes quickly" [exe].
    pub fresh_breed_head_start: u16,
}

/// Training huts (huts-and-training.md "Training huts", pop3-rev-analysis.md "Training buildings").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Training {
    /// The cost is recomputed on this rhythm while training ("Cost" [exe]).
    pub recost_every: Every,
    /// Turns before the next is let in after one went in, and after one came out ("The queue at the door"
    /// [exe]).
    pub admit_gap: Turns,
    pub left_gap: Turns,
    /// Percent of the cost by the number of that kind the tribe already has, up to each bound
    /// (`TRAIN_MANA_BAND_*`); above the last, `top_band`.
    pub bands: [(usize, u32); 5],
    pub top_band: u32,
    /// Mana to train one (`HUMAN_TRAIN_MANA_*`; the computer players' 1000 once tribes have a controller,
    /// `headcount::mana_income`'s own gap).
    pub warrior_preacher_mana: u32,
    pub spy_firewarrior_mana: u32,
}

/// Mana income (huts-and-training.md "Huts" rule 7, pop3-rev-analysis.md "Income").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Income {
    /// `MANA_UPDATE_FREQ` + 1: the income's own rhythm [exe].
    pub update_every: Every,
    /// `HUMAN_MANA_ADJUST` (a fraction, ×1.25 default): always used today, no AI/controller type exists yet
    /// to pick `COMPUTER_MANA_ADJUST` instead.
    pub human_adjust_num: u32,
    pub human_adjust_den: u32,
    /// Mana per busy/idle person (`MANA_BUSY_BRAVES`/`_IDLE_BRAVES`, `_BUSY_SPECIALS`/`_IDLE_SPECIALS`,
    /// `_F_SHAMEN`): the shaman's own rate applies whether she is busy or not.
    pub brave_busy: u32,
    pub brave_idle: u32,
    pub specialist_busy: u32,
    pub specialist_idle: u32,
    pub shaman: u32,
}

/// Per-kind life (hit points; `kind_stats.rs`'s own `KindStats::life`, constants.md "Units" `LIFE_<P>"):
/// our own defaults already are the traced real numbers, so this only matters for a modded install with
/// different ones. No entry for a wildman, which "has none, a brave's stands in" (`kind_stats.rs`) - it
/// always mirrors `brave` here too, even after a real file overrides it. Not Angel of Death either: it has
/// no `constant.dat` entry (exe-only, `kind_stats.rs`) and never changes kind into or out of at runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Units {
    pub shaman: u16,
    pub brave: u16,
    pub warrior: u16,
    pub preacher: u16,
    pub spy: u16,
    pub firewarrior: u16,
}

impl Units {
    /// `life`'s value for `kind` (`GameMap::spawn`/`with_shamans`/`change_owner`'s own correction over
    /// `Unit::new`/`Unit::change_owner`'s static `KindStats::life` default). Angel of Death and anything
    /// else not in `UnitKind::ALL` falls back to its own `KindStats::life` unchanged - not a kind a real
    /// install's file ever touches.
    pub fn life(&self, kind: UnitKind) -> u16 {
        match kind {
            UnitKind::Shaman => self.shaman,
            UnitKind::Brave | UnitKind::Wildman => self.brave,
            UnitKind::Warrior => self.warrior,
            UnitKind::Preacher => self.preacher,
            UnitKind::Spy => self.spy,
            UnitKind::Firewarrior => self.firewarrior,
            other => other.stats().life,
        }
    }
}

/// Every game-balance number, grouped by topic (huts-and-training.md "Balance and game turns").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Balance {
    pub huts: Huts,
    pub training: Training,
    pub income: Income,
    pub units: Units,
}

impl Balance {
    /// Our own defaults: the original's own values (pop3-rev-analysis.md, constants.md), written by hand, used
    /// in generated mode and the sandboxes - to tune later. `Balance::from_constants` will fill one from
    /// `constant.dat` instead, once `pop3_format::constants` exists, when an install is present.
    pub const fn ours() -> Balance {
        Balance {
            huts: Huts {
                breed_every: Every::new(4),
                breed_step: 2,
                grow_every: Every::new(16),
                grow_step: 8,
                grow_target: 2400,
                sprog_time: [4000, 3000, 2000],
                sprog_pct_first: 30,
                sprog_pct_last: 200,
                sprog_bands: 20,
                fresh_breed_head_start: 54,
            },
            training: Training {
                recost_every: Every::new(16),
                admit_gap: Turns::new(16),
                left_gap: Turns::new(12),
                bands: [(3, 100), (7, 125), (11, 150), (15, 175), (20, 200)],
                top_band: 250,
                warrior_preacher_mana: 3500,
                spy_firewarrior_mana: 4000,
            },
            income: Income {
                update_every: Every::new(16),
                human_adjust_num: 5,
                human_adjust_den: 4,
                brave_busy: 15,
                brave_idle: 4,
                specialist_busy: 5,
                specialist_idle: 4,
                shaman: 30,
            },
            units: Units { shaman: 2000, brave: 1000, warrior: 1800, preacher: 1100, spy: 600, firewarrior: 700 },
        }
    }

    /// `t` as ticks: identity today, see `Turns`.
    pub fn ticks(self, t: Turns) -> u32 {
        t.0
    }

    /// Corrects each group `constant.dat` actually backs, one at a time (TODO.md "Computer players (AI)"):
    /// only `units.life` (`LIFE_<P>`) so far, the way `SpellBook::apply_constants` already does for spells.
    /// A missing or unreadable key leaves that kind's own default untouched, same lenient spirit.
    /// **`LIFE_<P>`'s own per-kind suffixes are `[ours]`, a guess**: constants.md only confirms the group
    /// exists and gives two example values (brave, warrior), not every kind's exact spelling - worth
    /// checking against a real decoded file or the community's own dump before trusting this for a modded
    /// install (a wrong guess is harmless either way: that kind's default, already the traced real number,
    /// just stays).
    pub fn apply_constants(&mut self, raw: &pop3_format::Constants) {
        let u = &mut self.units;
        if let Some(v) = raw.get("LIFE_SHAMAN") {
            u.shaman = v as u16;
        }
        if let Some(v) = raw.get("LIFE_BRAVE") {
            u.brave = v as u16;
        }
        if let Some(v) = raw.get("LIFE_WARRIOR") {
            u.warrior = v as u16;
        }
        if let Some(v) = raw.get("LIFE_PREACHER") {
            u.preacher = v as u16;
        }
        if let Some(v) = raw.get("LIFE_SPY") {
            u.spy = v as u16;
        }
        if let Some(v) = raw.get("LIFE_FIREWARRIOR") {
            u.firewarrior = v as u16;
        }
    }
}

impl Default for Balance {
    fn default() -> Balance {
        Balance::ours()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_are_ticks_today() {
        assert_eq!(Balance::ours().ticks(Turns::new(16)), 16);
    }

    #[test]
    fn units_life_mirrors_brave_for_a_wildman() {
        let units = Balance::ours().units;
        assert_eq!(units.life(UnitKind::Wildman), units.life(UnitKind::Brave));
    }

    #[test]
    fn units_life_falls_back_to_kind_stats_off_the_table() {
        assert_eq!(Balance::ours().units.life(UnitKind::AngelOfDeath), UnitKind::AngelOfDeath.stats().life);
    }

    #[test]
    fn apply_constants_overrides_only_the_keys_a_file_actually_has() {
        let mut balance = Balance::ours();
        let raw = pop3_format::Constants::parse(b"P3CONST_LIFE_BRAVE = 4242\n").unwrap();
        balance.apply_constants(&raw);
        assert_eq!(balance.units.brave, 4242, "overridden by the file");
        assert_eq!(balance.units.warrior, Balance::ours().units.warrior, "no LIFE_WARRIOR key: default kept");
        assert_eq!(balance.units.life(UnitKind::Wildman), 4242, "mirrors the now-overridden brave");
    }
}
