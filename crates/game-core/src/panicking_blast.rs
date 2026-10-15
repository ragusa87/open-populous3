//! A panicking blast (pop3-rev-analysis.md "Components" "Fireball shots": "a simple blast (firestorm's also panics
//! everyone it hits)"), an effect (`effect::EffectKind::PanickingBlast`): the same 3-turn, radius 2/4/5 shape as
//! `blast::SimpleBlast` (`blast::RADII`, `blast::DAMAGE_PERSON`, the same push), but everyone it hurts also panics
//! (`WorldEvent::Panic`) unless shielded (inferred, not explicitly confirmed, consistent with Shield blocking most
//! offensive spells: docs/plans/spells.md "Firestorm"). What a fireball is, for Firestorm and Volcano alike
//! (`blast::SimpleBlast` itself stays pure, no panic flag, by the user's own call) — kept a separate module rather
//! than a flag on `SimpleBlast` so the two can diverge later without an awkward shared knob.

use crate::blast::{push, DAMAGE_PERSON, PUSH_RANGE, RADII};
use crate::effect::{DamageKind, Effect, EffectKind};
use crate::event::{PushKind, WorldEvent};
use crate::id::Registry;
use crate::time::{Countdown, Ticks};
use crate::unit::{Unit, UnitId, UnitKind};
use crate::vehicle::Vehicle;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PanickingBlast {
    /// Turns done.
    turn: u8,
    /// Units pushed so far (ids): each only once.
    pushed: Vec<UnitId>,
}

/// A panicking blast cast by tribe `owner` at `at` (world units).
pub fn effect(owner: u8, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(Ticks::new(RADII.len() as u32)), EffectKind::PanickingBlast(PanickingBlast::default()))
}

impl PanickingBlast {
    /// One turn of the blast at `at` cast by `owner`: the pushes, damage and panic it deals now.
    pub fn turn(&mut self, at: (u16, u16), owner: u8, units: &[Unit], vehicles: &Registry<Vehicle>) -> Vec<WorldEvent> {
        let Some(&radius) = RADII.get(self.turn as usize) else { return Vec::new() };
        self.turn += 1;
        let reach = radius.min(PUSH_RANGE);
        let mut events = Vec::new();
        for u in units.iter().filter(|u| u.is_alive() && u.in_the_open()) {
            let pushable = !(u.owner == owner && u.kind == UnitKind::Shaman) && !self.pushed.contains(&u.id);
            if let Some(vel) = push(at, (u.x, u.z), reach).filter(|_| pushable) {
                self.pushed.push(u.id);
                events.push(WorldEvent::Push { unit: u.id, vel, kind: PushKind::Spell });
            }
            if push(at, (u.x, u.z), radius).is_some() {
                events.push(WorldEvent::Damage { unit: u.id, amount: DAMAGE_PERSON, kind: DamageKind::Blast, from: Some(owner) });
                if !u.statuses.shield.is_on() {
                    events.push(WorldEvent::Panic { unit: u.id });
                }
            }
        }
        for (vehicle, _) in vehicles.iter().filter(|(_, v)| push(at, (v.x, v.z), radius).is_some()) {
            events.push(WorldEvent::DamageVehicle { vehicle, amount: DAMAGE_PERSON, from: owner });
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::Id;

    #[test]
    fn hurts_and_panics_everyone_in_reach_but_spares_a_shielded_ones_panic() {
        let shielded_id = Id::from_raw(2);
        let mut shielded = Unit::new(shielded_id, 1, UnitKind::Brave, (5000, 5000));
        shielded.statuses.shield = crate::status::Timed::steps(crate::status::SHIELD_STEPS);
        let units = [Unit::new(Id::from_raw(1), 1, UnitKind::Brave, (5000, 5000)), shielded];
        let vehicles = Registry::default();
        let events = PanickingBlast::default().turn((5000, 5000), 0, &units, &vehicles);
        let panicked: Vec<_> = events.iter().filter_map(|e| if let WorldEvent::Panic { unit } = e { Some(*unit) } else { None }).collect();
        let damaged: Vec<_> = events.iter().filter_map(|e| if let WorldEvent::Damage { unit, .. } = e { Some(*unit) } else { None }).collect();
        assert_eq!(panicked, [Id::from_raw(1)], "only the unshielded one panics");
        assert!(damaged.contains(&Id::from_raw(1)) && damaged.contains(&shielded_id), "the shield doesn't block the push-radius damage itself here: GameMap::damage gates that");
    }

    #[test]
    fn three_turns_then_over() {
        let units = [Unit::new(Id::from_raw(1), 1, UnitKind::Brave, (5000, 5000))];
        let vehicles = Registry::default();
        let mut blast = PanickingBlast::default();
        for _ in 0..RADII.len() {
            assert!(!blast.turn((5000, 5000), 0, &units, &vehicles).is_empty());
        }
        assert!(blast.turn((5000, 5000), 0, &units, &vehicles).is_empty(), "over");
    }
}
