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
    /// `SPROG%_POP_BAND_<lo>_<hi>%` by population band (`00_04` .. `95_99`) [files].
    pub sprog_pct: [u32; SPROG_BANDS],
    /// A freshly completed hut's own breed head start: not quite full, "the first brave comes quickly" [exe].
    pub fresh_breed_head_start: u16,
}

/// Population bands of breeding's `SPROG%` table.
pub const SPROG_BANDS: usize = 20;

/// The `constant.dat` name of `SPROG%` band `band`: a 5 % share of 200 people each, `00_04` for band 0.
pub fn sprog_key(band: usize) -> String {
    format!("SPROG%_POP_BAND_{:02}_{:02}%", band * 5, band * 5 + 4)
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
    /// Mana to train one (`HUMAN_TRAIN_MANA_WARR`/`_PREACH`/`_SPY`/`_SWARR`; the computer players' 1000 once
    /// tribes have a controller, `headcount::mana_income`'s own gap).
    pub warrior_mana: u32,
    pub preacher_mana: u32,
    pub spy_mana: u32,
    pub firewarrior_mana: u32,
}

/// Mana income (huts-and-training.md "Huts" rule 7, pop3-rev-analysis.md "Income").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Income {
    /// `MANA_UPDATE_FREQ` + 1: the income's own rhythm [exe].
    pub update_every: Every,
    /// `HUMAN_MANA_ADJUST` (a fraction, ×1.25 default; the file's percent over 100): always used today, no AI/controller type exists yet
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
/// always mirrors `brave` here too, even after a real file overrides it. The Angel of Death's is `LIFE_AOD`
/// (spells.md "Original balance"), 10 000 like the exe's own per-type table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Units {
    pub shaman: u16,
    pub brave: u16,
    pub warrior: u16,
    pub preacher: u16,
    pub spy: u16,
    pub firewarrior: u16,
    pub angel: u16,
}

impl Units {
    /// `life`'s value for `kind` (`GameMap::spawn`/`with_shamans`/`change_owner`'s own correction over
    /// `Unit::new`/`Unit::change_owner`'s static `KindStats::life` default).
    pub fn life(&self, kind: UnitKind) -> u16 {
        match kind {
            UnitKind::Shaman => self.shaman,
            UnitKind::Brave | UnitKind::Wildman => self.brave,
            UnitKind::Warrior => self.warrior,
            UnitKind::Preacher => self.preacher,
            UnitKind::Spy => self.spy,
            UnitKind::Firewarrior => self.firewarrior,
            UnitKind::AngelOfDeath => self.angel,
        }
    }
}

/// The Angel of Death's own end (spells.md "Original balance"): it dies once it has lived `duration` or made
/// `kill_count` kills, whichever comes first (angel_of_death.rs). Timings and counts, so the real numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Angels {
    /// `AOD_DURATION`.
    pub duration: Turns,
    /// `AOD_KILL_COUNT`.
    pub kill_count: u8,
}

/// Spies (docs/specs/units.md "Spies").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spies {
    /// `SPY_DISGUISE_DELAY`: turns a fresh disguise takes to settle (`disguise.rs`).
    pub disguise_delay: Turns,
}

/// Every game-balance number, grouped by topic (huts-and-training.md "Balance and game turns").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Balance {
    pub huts: Huts,
    pub training: Training,
    pub income: Income,
    pub units: Units,
    pub spies: Spies,
    pub angels: Angels,
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
                sprog_pct: [30, 35, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 195, 200],
                fresh_breed_head_start: 54,
            },
            training: Training {
                recost_every: Every::new(16),
                admit_gap: Turns::new(16),
                left_gap: Turns::new(12),
                bands: [(3, 100), (7, 125), (11, 150), (15, 175), (20, 200)],
                top_band: 250,
                warrior_mana: 3500,
                preacher_mana: 3500,
                spy_mana: 4000,
                firewarrior_mana: 4000,
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
            units: Units { shaman: 2000, brave: 1000, warrior: 1800, preacher: 1100, spy: 600, firewarrior: 700, angel: 10_000 },
            spies: Spies { disguise_delay: Turns::new(crate::disguise::DISGUISE_DELAY.get()) },
            angels: Angels { duration: Turns::new(2500), kill_count: 40 },
        }
    }

    /// `t` as ticks: identity today, see `Turns`.
    pub fn ticks(self, t: Turns) -> u32 {
        t.0
    }

    /// Corrects each group `constant.dat` actually backs, one at a time (TODO.md "Computer players (AI)"):
    /// `units.life` (`LIFE_<P>`), `spies`, `angels`, the huts' breeding times and bands, the training costs and
    /// top band, and the income's rhythm, human adjustment and shaman rate so far, the way `SpellBook::apply_constants` already does for spells.
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
        if let Some(v) = raw.get("LIFE_AOD") {
            u.angel = v as u16;
        }
        if let Some(v) = raw.get("SPY_DISGUISE_DELAY") {
            self.spies.disguise_delay = Turns::new(v);
        }
        if let Some(v) = raw.get("AOD_DURATION") {
            self.angels.duration = Turns::new(v);
        }
        if let Some(v) = raw.get("AOD_KILL_COUNT") {
            self.angels.kill_count = v.min(u8::MAX as u32) as u8;
        }
        self.apply_economy(raw);
    }

    /// The huts, training and income keys of `apply_constants`.
    fn apply_economy(&mut self, raw: &pop3_format::Constants) {
        for (level, time) in self.huts.sprog_time.iter_mut().enumerate() {
            if let Some(v) = raw.get(&format!("HUT{}_SPROG_TIME", level + 1)) {
                *time = v.min(u16::MAX as u32) as u16;
            }
        }
        for (band, pct) in self.huts.sprog_pct.iter_mut().enumerate() {
            if let Some(v) = raw.get(&sprog_key(band)) {
                *pct = v;
            }
        }
        let t = &mut self.training;
        for (key, mana) in [("WARR", &mut t.warrior_mana), ("PREACH", &mut t.preacher_mana), ("SPY", &mut t.spy_mana), ("SWARR", &mut t.firewarrior_mana)] {
            if let Some(v) = raw.get(&format!("HUMAN_TRAIN_MANA_{key}")) {
                *mana = v;
            }
        }
        if let Some(v) = raw.get("TRAIN_MANA_BAND_21+") {
            t.top_band = v;
        }
        let i = &mut self.income;
        if let Some(period) = raw.get("MANA_UPDATE_FREQ").map(|v| v.saturating_add(1)).filter(|p| p.is_power_of_two()) {
            i.update_every = Every::new(period);
        }
        if let Some(v) = raw.get("HUMAN_MANA_ADJUST") {
            (i.human_adjust_num, i.human_adjust_den) = (v, 100);
        }
        if let Some(v) = raw.get("MANA_F_SHAMEN") {
            i.shaman = v;
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
    fn the_angels_own_life_defaults_to_its_kind_stats() {
        assert_eq!(Balance::ours().units.life(UnitKind::AngelOfDeath), UnitKind::AngelOfDeath.stats().life);
    }

    #[test]
    fn apply_constants_reads_the_angels_own_life_lifetime_and_kill_count() {
        let mut balance = Balance::ours();
        let raw = pop3_format::Constants::parse(b"P3CONST_LIFE_AOD = 900\nP3CONST_AOD_DURATION = 77\nP3CONST_AOD_KILL_COUNT = 3\n").unwrap();
        balance.apply_constants(&raw);
        assert_eq!(balance.units.life(UnitKind::AngelOfDeath), 900);
        assert_eq!(balance.angels, Angels { duration: Turns::new(77), kill_count: 3 });
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

    #[test]
    fn the_sprog_keys_name_each_five_percent_share() {
        assert_eq!(sprog_key(0), "SPROG%_POP_BAND_00_04%");
        assert_eq!(sprog_key(SPROG_BANDS - 1), "SPROG%_POP_BAND_95_99%");
    }

    #[test]
    fn apply_constants_reads_the_huts_training_and_income_groups() {
        let mut balance = Balance::ours();
        let raw = pop3_format::Constants::parse(
            b"P3CONST_HUT2_SPROG_TIME = 2500\nP3CONST_SPROG%_POP_BAND_10_14% = 44\nP3CONST_HUMAN_TRAIN_MANA_SWARR = 4100\nP3CONST_HUMAN_TRAIN_MANA_PREACH = 3300\nP3CONST_TRAIN_MANA_BAND_21+ = 260\nP3CONST_MANA_UPDATE_FREQ = 31\nP3CONST_HUMAN_MANA_ADJUST = 150\nP3CONST_MANA_F_SHAMEN = 33\n",
        )
        .unwrap();
        balance.apply_constants(&raw);
        let ours = Balance::ours();
        assert_eq!(balance.huts.sprog_time, [4000, 2500, 2000]);
        assert_eq!((balance.huts.sprog_pct[2], balance.huts.sprog_pct[3]), (44, ours.huts.sprog_pct[3]));
        let t = balance.training;
        assert_eq!((t.warrior_mana, t.preacher_mana, t.spy_mana, t.firewarrior_mana, t.top_band), (3500, 3300, 4000, 4100, 260));
        let i = balance.income;
        assert_eq!((i.update_every, i.human_adjust_num, i.human_adjust_den, i.shaman), (Every::new(32), 150, 100, 33));
    }

    #[test]
    fn a_mana_update_freq_that_is_not_a_mask_is_ignored() {
        let mut balance = Balance::ours();
        balance.apply_constants(&pop3_format::Constants::parse(b"P3CONST_MANA_UPDATE_FREQ = 20\n").unwrap());
        assert_eq!(balance.income.update_every, Balance::ours().income.update_every);
    }
}
