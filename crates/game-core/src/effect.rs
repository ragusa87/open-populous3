//! Spell effects (pop3-rev-analysis.md "Components"): what a spell leaves on the map once cast, each ticking on
//! its own rhythm for its own length. An effect reads the map and changes only itself: what it does to others
//! comes out as `EffectEvent`s, which `GameMap::tend_effects` applies in order (architecture.md "The tick").

use crate::map::{GameMap, Lcg};
use crate::motion::Velocity;
use crate::spell::Cell;
use crate::time::{Countdown, Tick};
use crate::unit::Unit;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effect {
    /// From `GameMap::new_id`, given when the effect is added (`GameMap::add_effect`).
    pub id: u32,
    /// The caster's tribe.
    pub owner: u8,
    /// Where it stands, world units.
    pub x: u16,
    pub z: u16,
    /// Turns to go; removed the turn it reaches 0.
    pub left: Countdown,
    pub kind: EffectKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffectKind {
    #[cfg(test)]
    Probe(tests::Probe),
}

/// What hurts a unit, for its immunities (`hurts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageKind {
    Blast,
}

/// An effect's change to something else, applied by the map in order. A unit or building gone meanwhile (its id
/// missing) is skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffectEvent {
    Damage { unit: u32, amount: u16, kind: DamageKind },
    Push { unit: u32, vel: Velocity },
    Panic { unit: u32 },
    Raid { building: u32 },
    SetFire { building: u32 },
    /// A new effect, ticking from the next turn; its id is given then.
    Spawn(Effect),
    Terrain(TerrainChange),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainChange {
    Raise { at: Cell, radius: i32, by: i32 },
    Flatten { at: Cell, radius: i32 },
}

impl Effect {
    pub fn new(owner: u8, (x, z): (u16, u16), left: Countdown, kind: EffectKind) -> Self {
        Effect { id: 0, owner, x, z, left, kind }
    }

    /// One turn at `now`: its work on its own rhythm, then one turn less to go.
    #[allow(unused_variables, unreachable_code)]
    pub fn tick(&mut self, now: Tick, world: &GameMap, rng: &mut Lcg) -> Vec<EffectEvent> {
        let events = match self.kind {
            #[cfg(test)]
            EffectKind::Probe(ref mut probe) => probe.turn(now, (self.x, self.z), world),
        };
        self.left.tick();
        events
    }

    pub fn is_over(&self) -> bool {
        self.left.left().get() == 0
    }
}

/// Whether `kind` damage from tribe `from` hurts `u`: never a shielded unit (pop3-rev-analysis.md "Per spell",
/// Shield), and a blast spares the caster's own tribe (no alliances yet).
pub fn hurts(u: &Unit, kind: DamageKind, from: u8) -> bool {
    if u.statuses.shielded || !u.is_alive() {
        return false;
    }
    match kind {
        DamageKind::Blast => u.owner != from,
    }
}

/// Whether a push moves `u`: not a shielded unit.
pub fn pushes(u: &Unit) -> bool {
    !u.statuses.shielded
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::time::{Every, Ticks};
    use crate::unit::{torus_delta, UnitKind};

    /// A test effect: on its rhythm, pushes up and hurts by 1 every unit within a cell; on its first turn it may
    /// spawn a copy of itself (that does not spawn).
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Probe {
        pub every: Every,
        pub spawn: bool,
    }

    impl Probe {
        pub(super) fn turn(&mut self, now: Tick, at: (u16, u16), world: &GameMap) -> Vec<EffectEvent> {
            let mut events = Vec::new();
            if std::mem::take(&mut self.spawn) {
                let copy = Probe { every: self.every, spawn: false };
                events.push(EffectEvent::Spawn(Effect::new(0, at, Countdown::new(Ticks::new(5)), EffectKind::Probe(copy))));
            }
            if self.every.fires(now, 0) {
                let near = |u: &&Unit| torus_delta(at.0, u.x).abs().max(torus_delta(at.1, u.z).abs()) <= 512;
                for u in world.units.iter().filter(near) {
                    events.push(EffectEvent::Push { unit: u.id, vel: Velocity::new(0, 60, 0) });
                    events.push(EffectEvent::Damage { unit: u.id, amount: 1, kind: DamageKind::Blast });
                }
            }
            events
        }
    }

    fn probe(owner: u8, at: (u16, u16), every: u32, turns: u32) -> Effect {
        Effect::new(owner, at, Countdown::new(Ticks::new(turns)), EffectKind::Probe(Probe { every: Every::new(every), spawn: false }))
    }

    /// The walk sandbox with an enemy brave next to the shaman; the brave's id and position.
    fn with_enemy() -> (GameMap, u32, (u16, u16)) {
        let mut map = GameMap::sandbox_walk();
        let s = &map.units[0];
        let at = (s.x.wrapping_add(1024), s.z);
        let id = map.spawn(1, UnitKind::Brave, at).id;
        (map, id, at)
    }

    #[test]
    fn works_on_its_rhythm_and_ends_after_its_length() {
        let (map, _, at) = with_enemy();
        let mut e = probe(0, at, 4, 10);
        let mut rng = map.rng;
        let fired: Vec<u32> = (1..=10).filter(|&n| !e.tick(Tick::from_wire(n), &map, &mut rng).is_empty()).collect();
        assert_eq!(fired, [4, 8]);
        assert!(e.is_over());
    }

    #[test]
    fn effects_end_in_order_and_hit_before_the_units_move() {
        let (mut map, enemy, at) = with_enemy();
        let long = map.add_effect(probe(0, at, 1, 3));
        map.add_effect(probe(0, (0, 0), 1, 1));
        let other = map.add_effect(probe(0, (0, 0), 1, 2));
        let max = map.unit(enemy).unwrap().health.max();
        map.tick();
        assert!(map.unit(enemy).unwrap().motion.airborne(), "pushed, then its own tick flew it");
        assert_eq!(map.unit(enemy).unwrap().health.current(), max - 1);
        assert_eq!(map.effects.iter().map(|e| e.id).collect::<Vec<_>>(), [long, other]);
        map.tick();
        assert_eq!(map.effects.iter().map(|e| e.id).collect::<Vec<_>>(), [long]);
        map.tick();
        assert!(map.effects.is_empty());
    }

    #[test]
    fn a_spawned_effect_ticks_from_the_next_turn() {
        let (mut map, _, at) = with_enemy();
        let parent = map.add_effect(Effect { kind: EffectKind::Probe(Probe { every: Every::new(1), spawn: true }), ..probe(0, at, 1, 3) });
        map.tick();
        let child = map.effects.iter().find(|e| e.id != parent).expect("spawned");
        assert!(child.id > parent);
        assert_eq!(child.left.left(), Ticks::new(5), "not ticked yet");
        map.tick();
        assert_eq!(map.effects.iter().find(|e| e.id != parent).unwrap().left.left(), Ticks::new(4));
    }

    #[test]
    fn events_for_things_gone_are_skipped() {
        let (mut map, _, _) = with_enemy();
        let before = map.units.clone();
        for event in [
            EffectEvent::Damage { unit: 9999, amount: 50, kind: DamageKind::Blast },
            EffectEvent::Push { unit: 9999, vel: Velocity::new(0, 60, 0) },
            EffectEvent::Raid { building: 9999 },
        ] {
            assert_eq!(map.apply_effect(1, event), None);
        }
        assert_eq!(map.units, before);
    }

    #[test]
    fn damage_spares_the_shielded_and_the_casters_tribe() {
        let (mut map, enemy, _) = with_enemy();
        let shaman = map.units[0].id;
        let max = map.unit(enemy).unwrap().health.max();
        map.damage(shaman, 50, DamageKind::Blast, 0);
        assert!(map.unit(shaman).unwrap().health.is_full(), "own tribe");
        map.unit_mut(enemy).unwrap().statuses.shielded = true;
        map.damage(enemy, 50, DamageKind::Blast, 0);
        assert_eq!(map.unit(enemy).unwrap().health.current(), max, "shielded");
        map.unit_mut(enemy).unwrap().statuses.shielded = false;
        map.damage(enemy, 50, DamageKind::Blast, 0);
        assert_eq!(map.unit(enemy).unwrap().health.current(), max - 50);
        map.damage(enemy, max, DamageKind::Blast, 0);
        assert!(!map.unit(enemy).unwrap().is_alive(), "dies at 0");
    }

    #[test]
    fn the_same_turns_give_the_same_map() {
        let run = || {
            let (mut map, _, at) = with_enemy();
            map.add_effect(probe(0, at, 2, 20));
            map.run(Ticks::new(40));
            (map.units, map.effects, map.rng)
        };
        assert_eq!(run(), run());
    }
}
