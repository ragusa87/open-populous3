//! Pyramids of knowledge (vaults, docs/specs/worship.md): what one teaches and how far its tribe has
//! got. Its phases only go forward: Praying (a gauge) -> Granted (the reward given, the door still
//! open while the shaman walks out, then closing as the top folds) -> Spent. The door and the top
//! follow the phase (`Vault::door_and_top`).

use crate::building::{BuildingKind, Reward};
use crate::spell_book::SpellKind;
use pop3_format::level::{Thing, ThingData, KIND_BUILDING, KIND_SPELL};
use pop3_format::Level;

/// Trigger type of a vault's prayer.
const TRIGGER_LIBRARY: u8 = 4;
/// Gauge length of a vault without a trigger (sandboxes), in ticks of praying.
pub const DEFAULT_PRAY_TIME: u16 = 100;
/// After the reward: ticks the door stays open for the shaman to walk out, then ticks it takes to
/// close as the top folds.
pub const OPEN_TICKS: u16 = 20;
pub const CLOSE_TICKS: u16 = 10;
/// The door opens over the last tenth of the gauge.
const DOOR_BAND: u16 = 10;
/// Door and top positions are in thousandths: 0 closed, 1000 open.
pub const FULL: u16 = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultPhase {
    /// The gauge, in `gauge::STEP`s: full at `pray_time` units.
    Praying { progress: u32 },
    /// Ticks since the reward was granted.
    Granted { ticks: u16 },
    Spent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vault {
    /// What it teaches, from the level's library trigger; None on maps without one.
    pub reward: Option<Reward>,
    /// Gauge length (the trigger's `PrayTime`).
    pub pray_time: u16,
    pub phase: VaultPhase,
}

impl Vault {
    pub fn new(reward: Option<Reward>, pray_time: u16) -> Self {
        Vault { reward, pray_time: pray_time.max(1), phase: VaultPhase::Praying { progress: 0 } }
    }

    /// The vault of `thing`, from the library trigger standing on its cell (the building record has
    /// no link of its own, docs/specs/level-format.md "Vault of knowledge").
    pub fn from_level(level: &Level, thing: &Thing) -> Self {
        let library = crate::totem::triggers_on_cell(level, thing).into_iter().find(|t| t.trigger_type == TRIGGER_LIBRARY);
        let Some(trigger) = library else { return Vault::new(None, DEFAULT_PRAY_TIME) };
        let reward = trigger.targets.iter().filter_map(|&slot| level.slot(slot)).find_map(|target| match target.data() {
            ThingData::Discovery(d) if d.kind == KIND_SPELL => SpellKind::from_model(d.model).map(Reward::Spell),
            ThingData::Discovery(d) if d.kind == KIND_BUILDING => Some(Reward::Building(BuildingKind::from_model(d.model))),
            _ => None,
        });
        Vault::new(reward, trigger.pray_time.max(0) as u16)
    }

    /// The shaman reached the middle: the reward is given (once) and the closing starts.
    pub fn grant(&mut self) -> Option<Reward> {
        if !matches!(self.phase, VaultPhase::Praying { .. }) {
            return None;
        }
        self.phase = VaultPhase::Granted { ticks: 0 };
        self.reward
    }

    /// The gauge's steps when full.
    pub fn full(&self) -> u32 {
        self.pray_time as u32 * crate::gauge::STEP
    }

    /// The gauge in thousandths of full (0 once granted).
    pub fn progress_permille(&self) -> u16 {
        match self.phase {
            VaultPhase::Praying { progress } => (progress.min(self.full()) as u64 * FULL as u64 / self.full() as u64) as u16,
            _ => 0,
        }
    }

    /// One tick: praying, the gauge fills while the shaman holding it (`holding`: 1 when she prays at
    /// the door or is inside) prays, out of the one it needs, and drains linearly otherwise
    /// (`gauge::step`); granted, the closing goes on whatever happens to her.
    pub fn tick(&mut self, holding: u16) {
        self.phase = match self.phase {
            VaultPhase::Praying { progress } => VaultPhase::Praying { progress: crate::gauge::step(progress, holding, 1, self.full()) },
            VaultPhase::Granted { ticks } if ticks + 1 >= OPEN_TICKS + CLOSE_TICKS => VaultPhase::Spent,
            VaultPhase::Granted { ticks } => VaultPhase::Granted { ticks: ticks + 1 },
            VaultPhase::Spent => VaultPhase::Spent,
        };
    }

    /// The gauge is full: the door is open, the shaman may go in.
    pub fn is_full(&self) -> bool {
        self.phase == VaultPhase::Praying { progress: self.full() }
    }

    /// Nobody may go in any more.
    pub fn is_spent(&self) -> bool {
        !matches!(self.phase, VaultPhase::Praying { .. })
    }

    /// How open the door and the top are, in thousandths (0 closed, `FULL` open). Praying, the door
    /// follows the gauge over its last `DOOR_BAND`th, the top is open; granted, the door stays open
    /// `OPEN_TICKS`, then both close together over `CLOSE_TICKS`; spent, both closed.
    pub fn door_and_top(&self) -> (u16, u16) {
        match self.phase {
            VaultPhase::Praying { progress } => {
                let full = self.full();
                let band = (full / DOOR_BAND as u32).max(1);
                let into = progress.min(full).saturating_sub(full - band);
                ((into as u64 * FULL as u64 / band as u64) as u16, FULL)
            }
            VaultPhase::Granted { ticks } => {
                let closing = ticks.saturating_sub(OPEN_TICKS).min(CLOSE_TICKS);
                let open = FULL - (closing as u32 * FULL as u32 / CLOSE_TICKS as u32) as u16;
                (open, open)
            }
            VaultPhase::Spent => (0, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gauge::STEP;

    fn praying(units: u32) -> Vault {
        Vault { phase: VaultPhase::Praying { progress: units * STEP }, ..Vault::new(None, 100) }
    }

    #[test]
    fn the_door_follows_the_last_tenth_of_the_gauge_the_top_stays_open() {
        assert_eq!(praying(0).door_and_top(), (0, FULL), "fresh: door closed, top open");
        assert_eq!(praying(90).door_and_top(), (0, FULL));
        assert_eq!(praying(95).door_and_top(), (500, FULL));
        assert_eq!(praying(100).door_and_top(), (FULL, FULL), "full: open");
        assert_eq!(praying(95).door_and_top(), (500, FULL), "draining closes it again");
    }

    #[test]
    fn granted_once_then_open_then_closing_with_the_top_then_spent() {
        let mut v = Vault::new(Some(Reward::Building(BuildingKind::Temple)), 100);
        v.phase = VaultPhase::Praying { progress: 100 * STEP };
        assert_eq!(v.grant(), Some(Reward::Building(BuildingKind::Temple)));
        assert_eq!(v.grant(), None, "only once");
        assert!(v.is_spent(), "nobody goes in once granted");
        for _ in 0..OPEN_TICKS {
            assert_eq!(v.door_and_top(), (FULL, FULL), "open while she walks out");
            v.tick(0);
        }
        for _ in 0..CLOSE_TICKS / 2 {
            v.tick(0);
        }
        assert_eq!(v.door_and_top(), (500, 500), "door and top close together");
        for _ in 0..CLOSE_TICKS {
            v.tick(0);
        }
        assert_eq!((v.phase, v.door_and_top()), (VaultPhase::Spent, (0, 0)));
    }

    #[test]
    fn the_gauge_fills_while_held_and_drains_otherwise() {
        let mut v = praying(98);
        v.tick(1);
        v.tick(1);
        assert!(v.is_full());
        v.tick(1);
        assert_eq!(v.phase, VaultPhase::Praying { progress: 100 * STEP }, "no further");
        v.tick(0);
        assert_eq!((v.phase, v.is_full()), (VaultPhase::Praying { progress: 99 * STEP }, false), "nobody: it drains linearly");
        let mut empty = praying(0);
        empty.tick(0);
        assert_eq!(empty.phase, VaultPhase::Praying { progress: 0 });
    }

    #[test]
    fn a_tiny_gauge_still_opens_its_door() {
        let v = Vault { phase: VaultPhase::Praying { progress: 5 * STEP }, ..Vault::new(None, 5) };
        assert_eq!(v.door_and_top(), (FULL, FULL));
        assert_eq!(Vault::new(None, 0).pray_time, 1);
    }
}
