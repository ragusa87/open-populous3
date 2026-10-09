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
    /// The gauge, 0 to `pray_time`.
    Praying { progress: u16 },
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

    /// One tick of the closing after the reward, whatever happens to the shaman.
    pub fn tick(&mut self) {
        if let VaultPhase::Granted { ticks } = self.phase {
            self.phase = if ticks + 1 >= OPEN_TICKS + CLOSE_TICKS { VaultPhase::Spent } else { VaultPhase::Granted { ticks: ticks + 1 } };
        }
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
                let band = (self.pray_time / DOOR_BAND).max(1);
                let into = progress.min(self.pray_time).saturating_sub(self.pray_time - band);
                ((into as u32 * FULL as u32 / band as u32) as u16, FULL)
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

    fn praying(progress: u16) -> Vault {
        Vault { phase: VaultPhase::Praying { progress }, ..Vault::new(None, 100) }
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
        v.phase = VaultPhase::Praying { progress: 100 };
        assert_eq!(v.grant(), Some(Reward::Building(BuildingKind::Temple)));
        assert_eq!(v.grant(), None, "only once");
        assert!(v.is_spent(), "nobody goes in once granted");
        for _ in 0..OPEN_TICKS {
            assert_eq!(v.door_and_top(), (FULL, FULL), "open while she walks out");
            v.tick();
        }
        for _ in 0..CLOSE_TICKS / 2 {
            v.tick();
        }
        assert_eq!(v.door_and_top(), (500, 500), "door and top close together");
        for _ in 0..CLOSE_TICKS {
            v.tick();
        }
        assert_eq!((v.phase, v.door_and_top()), (VaultPhase::Spent, (0, 0)));
    }

    #[test]
    fn a_tiny_gauge_still_opens_its_door() {
        let v = Vault { phase: VaultPhase::Praying { progress: 5 }, ..Vault::new(None, 5) };
        assert_eq!(v.door_and_top(), (FULL, FULL));
        assert_eq!(Vault::new(None, 0).pray_time, 1);
    }
}
