//! The drum loops chained under the music (sound.md "Music and drums"): each time a game track starts a random
//! drum bank is chosen, then one loop after the other, each picked from how much the local player is fighting.
//! Pure; `audio.rs` plays them.

use game_core::tribes::Owner;
use game_core::unit::{Action, Unit};

/// How much the local player is fighting, set by the game every turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Intensity {
    /// No drums (ours: the original plays its calm loop).
    #[default]
    Calm,
    Attacking,
    Melee,
}

/// Engaged units from which the drums go to the melee loops. A guess (ours), as is scaling by the number of the
/// player's people engaged rather than the original's attacking/in-a-melee flag (TODO.md "Sound").
pub const MELEE_ARMY: usize = 5;

/// From how many of `player`'s living people attack, challenge or are in a melee: none calm, a few attacking,
/// `MELEE_ARMY` or more melee.
pub fn fight_intensity<'a>(units: impl IntoIterator<Item = &'a Unit>, player: Owner) -> Intensity {
    let engaged = units
        .into_iter()
        .filter(|u| u.owner == player && u.is_alive() && matches!(u.action, Action::Attacking { .. } | Action::Challenge { .. } | Action::Melee { .. }))
        .count();
    match engaged {
        0 => Intensity::Calm,
        n if n < MELEE_ARMY => Intensity::Attacking,
        _ => Intensity::Melee,
    }
}

/// What the next loop depends on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrumState {
    /// Loops played in a row at `Melee`.
    melee_loops: u32,
}

impl DrumState {
    /// The bank entry of the next loop, for a bank of `entries`: none calm, 1 attacking, 2 in a melee and every
    /// 4th melee loop 3 when the bank has one. Which loop counts as the 4th is our reading.
    pub fn next(&mut self, now: Intensity, entries: usize) -> Option<usize> {
        let entry = match now {
            Intensity::Calm => None,
            Intensity::Attacking => Some(1),
            Intensity::Melee => {
                self.melee_loops += 1;
                Some(if self.melee_loops.is_multiple_of(4) && entries > 3 { 3 } else { 2 })
            }
        };
        if now != Intensity::Melee {
            self.melee_loops = 0;
        }
        entry.filter(|_| entries > 0).map(|e| e.min(entries - 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Intensity::*;

    fn run(steps: &[Intensity], entries: usize) -> Vec<Option<usize>> {
        let mut s = DrumState::default();
        steps.iter().map(|&i| s.next(i, entries)).collect()
    }

    #[test]
    fn calm_plays_no_drums() {
        assert_eq!(run(&[Calm, Attacking, Calm], 4), vec![None, Some(1), None]);
    }

    #[test]
    fn a_melee_plays_its_loop_and_every_fourth_the_extra_one() {
        let m = |v: &[usize]| v.iter().map(|&e| Some(e)).collect::<Vec<_>>();
        assert_eq!(run(&[Melee; 8], 4), m(&[2, 2, 2, 3, 2, 2, 2, 3]));
        assert_eq!(run(&[Melee; 4], 3), m(&[2, 2, 2, 2]), "a bank without a fourth loop");
        assert_eq!(run(&[Melee, Melee, Attacking, Melee, Melee, Melee, Melee], 4), m(&[2, 2, 1, 2, 2, 2, 3]), "the count restarts");
    }

    #[test]
    fn a_short_bank_never_gives_an_entry_it_lacks() {
        assert_eq!(run(&[Melee], 2), vec![Some(1)]);
        assert_eq!(run(&[Attacking], 0), vec![None]);
    }

    #[test]
    fn the_drums_follow_how_many_of_the_player_s_people_attack() {
        use game_core::id::Id;
        use game_core::tribes::TribeId;
        use game_core::unit::{Target, UnitKind};
        let unit = |id: u32, owner: TribeId, attacking: bool| {
            let mut u = Unit::new(Id::from_raw(id), owner, UnitKind::Brave, (100, 100));
            if attacking {
                u.action = Action::Attacking { target: Target::Unit(Id::from_raw(999)), to: (0, 0) };
            }
            u
        };
        let blue = Owner::Tribe(TribeId::Blue);
        let enemies: Vec<Unit> = (0..9).map(|i| unit(i, TribeId::Red, true)).collect();
        assert_eq!(fight_intensity(&enemies, blue), Calm, "only the player's people count");
        let few: Vec<Unit> = (0..MELEE_ARMY as u32).map(|i| unit(i, TribeId::Blue, i > 0)).collect();
        assert_eq!(fight_intensity(&few, blue), Attacking);
        let army: Vec<Unit> = (0..MELEE_ARMY as u32).map(|i| unit(i, TribeId::Blue, true)).collect();
        assert_eq!(fight_intensity(&army, blue), Melee);
    }
}
