//! Spell effects (pop3-rev-analysis.md "Components"): what a spell leaves on the map once cast, each ticking on
//! its own rhythm for its own length. An effect reads the map and changes only itself: what it does to others
//! comes out as `WorldEvent`s, which `GameMap::tend_effects` applies in order (architecture.md "The tick").

use crate::event::{PushKind, WorldEvent};
use crate::map::{GameMap, Lcg};
use crate::spell::Cell;
use crate::terrain::Heightmap;
use crate::time::{Countdown, Tick};
use crate::unit::Unit;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effect {
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
    SimpleBlast(crate::blast::SimpleBlast),
    /// A blast that also panics everyone it hits (`panicking_blast`): a fireball, Firestorm's or Volcano's.
    PanickingBlast(crate::panicking_blast::PanickingBlast),
    LandBridge(crate::land_bridge::LandBridge),
    Lightning(crate::lightning::Strike),
    Flatten(crate::flatten::Flatten),
    /// Only seen (`smoke`).
    Smoke,
    /// Only seen (`ground_fire`).
    GroundFire,
    /// Its own kill tally (`swamp`); the killing itself is `GameMap::tend_swamps`, not its own turn: unlike every
    /// other effect it must see every other swamp at once, to settle whose claim an overlapping cell counts for.
    Swamp(crate::swamp::Swamp),
    /// Its own turns-to-next-chance (`convert`): the nearest wildman in range becomes a brave of the caster, every
    /// few turns, for a fixed lifetime regardless of success.
    Convert(crate::convert::Convert),
    /// A one-turn effect (`hypnotism`): the nearest eligible people of other tribes around it join the caster's.
    Hypnotism(crate::hypnotism::Hypnotism),
    /// A fixed set of hot cells (`lava`), shared by Earthquake's fissure and Volcano's flow.
    Lava(crate::lava::Lava),
    /// The multi-phase controller (`volcano`): rumbles, rises, destroys, then spawns lava and fireballs.
    Volcano(crate::volcano::Volcano),
    /// A pure scheduler (`firestorm`): one fireball every few turns for its whole lifetime.
    Firestorm(crate::firestorm::Firestorm),
    /// A timed controller (`earthquake`): shakes, then sinks a half-disc and opens a lava fissure once.
    Earthquake(crate::earthquake::Earthquake),
    #[cfg(test)]
    Probe(tests::Probe),
}

/// What hurts a unit, for its immunities (`hurts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageKind {
    Blast,
    /// Kills whatever the health (`lightning`).
    Lightning,
    /// A blow in a fight: the only damage a shield does not stop (combat.md).
    Melee,
    /// A firewarrior's bolt: a magical ranged attack, so the shield stops it too (combat.md "Firebolts"; reported,
    /// not traced).
    Fire,
    /// A swamp's kill (`swamp`): outright, any tribe (even the caster's own), shield or no shield — `hurts` never
    /// gates it (TODO.md "Mana": unlike blast, lightning, swarm or hypnotise, a shielded unit still drowns).
    Swamp,
    /// Not an outright kill (confirmed, pop3-rev-analysis.md "Volcano lava flow"): used only as `hurts`'s own
    /// eligibility check for who a hot lava cell (`lava`) or Volcano's own turn-110 destruction catches fire
    /// (`WorldEvent::Ignite`) — any tribe (even the caster's own) and shield or no shield, sparing only the
    /// caster's own shaman.
    Lava,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainChange {
    Raise { at: Cell, radius: i32, by: i32 },
    /// One turn of a flatten (`flatten::apply`).
    Flatten { at: Cell, height: u16, turn: u32 },
    /// One turn of a land bridge (`land_bridge::apply`).
    Bridge { from: Cell, to: Cell, heights: (u16, u16), turn: u32 },
    /// One turn of a volcano's cone rising (`volcano::apply`): `ground`, the centre's own height when cast.
    Volcano { at: Cell, ground: u16, turn: u32 },
}

impl Effect {
    pub fn new(owner: u8, (x, z): (u16, u16), left: Countdown, kind: EffectKind) -> Self {
        Effect { owner, x, z, left, kind }
    }

    /// One turn at `now`: its work on its own rhythm, then one turn less to go.
    #[allow(unused_variables)]
    pub fn tick(&mut self, now: Tick, world: &GameMap, rng: &mut Lcg) -> Vec<WorldEvent> {
        let events = match self.kind {
            EffectKind::SimpleBlast(ref mut blast) => blast.turn((self.x, self.z), self.owner, &world.units, &world.vehicles),
            EffectKind::PanickingBlast(ref mut blast) => blast.turn((self.x, self.z), self.owner, &world.units, &world.vehicles),
            EffectKind::LandBridge(ref mut bridge) => bridge.turn(),
            EffectKind::Lightning(ref mut strike) => strike.turn((self.x, self.z), self.owner, world),
            EffectKind::Flatten(ref mut flatten) => flatten.turn(),
            EffectKind::Convert(ref mut convert) => convert.turn((self.x, self.z), self.owner, world),
            EffectKind::Hypnotism(ref mut hypnotism) => hypnotism.turn((self.x, self.z), self.owner, world),
            EffectKind::Lava(ref mut lava) => lava.turn(self.owner, world),
            EffectKind::Volcano(ref mut volcano) => volcano.turn((self.x, self.z), self.owner, world, rng),
            EffectKind::Firestorm(ref mut firestorm) => firestorm.turn((self.x, self.z), self.owner, world, rng),
            EffectKind::Earthquake(ref mut earthquake) => earthquake.turn((self.x, self.z), self.owner, world),
            EffectKind::Smoke | EffectKind::GroundFire | EffectKind::Swamp(_) => Vec::new(),
            #[cfg(test)]
            EffectKind::Probe(ref mut probe) => probe.turn(now, (self.x, self.z), self.owner, world),
        };
        self.left.tick();
        events
    }

    pub fn is_over(&self) -> bool {
        self.left.left().get() == 0
    }

    /// The height points (cells, unwrapped) of `terrain` it is still reshaping: the terrain spells' sparkles.
    pub fn reshaping(&self, terrain: &Heightmap) -> Vec<Cell> {
        match self.kind {
            EffectKind::LandBridge(ref bridge) => bridge.reshaping(terrain),
            EffectKind::Flatten(ref flatten) => flatten.reshaping(terrain),
            EffectKind::Volcano(ref volcano) => volcano.reshaping((self.x, self.z), terrain),
            EffectKind::SimpleBlast(_)
            | EffectKind::PanickingBlast(_)
            | EffectKind::Lightning(_)
            | EffectKind::Smoke
            | EffectKind::GroundFire
            | EffectKind::Swamp(_)
            | EffectKind::Convert(_)
            | EffectKind::Hypnotism(_)
            | EffectKind::Lava(_)
            | EffectKind::Firestorm(_)
            | EffectKind::Earthquake(_) => Vec::new(),
            #[cfg(test)]
            EffectKind::Probe(_) => Vec::new(),
        }
    }
}

/// Whether `kind` damage from tribe `from` hurts `u`: never the dead; a shielded unit only by a blow, a swamp or
/// lava (those still drown/burn: TODO.md "Mana"), a firewarrior's bolt stopped like any other magical attack
/// (pop3-rev-analysis.md "Per spell", Shield; reported, not traced); a blast spares the caster's own tribe (no
/// alliances yet), lightning and lava only the caster's own shaman, a swamp spares no one, not even its own caster.
pub fn hurts(u: &Unit, kind: DamageKind, from: Option<u8>) -> bool {
    if !u.is_alive() || (u.statuses.shield.is_on() && !matches!(kind, DamageKind::Melee | DamageKind::Swamp | DamageKind::Lava)) {
        return false;
    }
    match kind {
        DamageKind::Melee | DamageKind::Fire | DamageKind::Swamp => true,
        DamageKind::Blast => from != Some(u.owner),
        DamageKind::Lightning | DamageKind::Lava => !(from == Some(u.owner) && u.kind == crate::unit::UnitKind::Shaman),
    }
}

/// Whether a push of `kind` moves `u`: a spell's not a shielded unit.
pub fn pushes(u: &Unit, kind: PushKind) -> bool {
    kind == PushKind::Melee || !u.statuses.shield.is_on()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::motion::Velocity;
    use crate::time::{Every, Ticks};
    use crate::id::Id;
    use crate::unit::{torus_delta, UnitId, UnitKind};

    /// A test effect: on its rhythm, pushes up and hurts by 1 every unit within a cell; on its first turn it may
    /// spawn a copy of itself (that does not spawn).
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Probe {
        pub every: Every,
        pub spawn: bool,
    }

    impl Probe {
        pub(super) fn turn(&mut self, now: Tick, at: (u16, u16), owner: u8, world: &GameMap) -> Vec<WorldEvent> {
            let mut events = Vec::new();
            if std::mem::take(&mut self.spawn) {
                let copy = Probe { every: self.every, spawn: false };
                events.push(WorldEvent::SpawnEffect(Effect::new(0, at, Countdown::new(Ticks::new(5)), EffectKind::Probe(copy))));
            }
            if self.every.fires(now, 0) {
                let near = |u: &&Unit| torus_delta(at.0, u.x).abs().max(torus_delta(at.1, u.z).abs()) <= 512;
                for u in world.units.iter().filter(near) {
                    events.push(WorldEvent::Push { unit: u.id, vel: Velocity::new(0, 60, 0), kind: PushKind::Spell });
                    events.push(WorldEvent::Damage { unit: u.id, amount: 1, kind: DamageKind::Blast, from: Some(owner) });
                }
            }
            events
        }
    }

    fn probe(owner: u8, at: (u16, u16), every: u32, turns: u32) -> Effect {
        Effect::new(owner, at, Countdown::new(Ticks::new(turns)), EffectKind::Probe(Probe { every: Every::new(every), spawn: false }))
    }

    /// The walk sandbox with an enemy brave next to the shaman; the brave's id and position.
    fn with_enemy() -> (GameMap, UnitId, (u16, u16)) {
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
        assert_eq!(map.effects.ids().collect::<Vec<_>>(), [long, other]);
        map.tick();
        assert_eq!(map.effects.ids().collect::<Vec<_>>(), [long]);
        map.tick();
        assert!(map.effects.is_empty());
    }

    #[test]
    fn an_id_is_never_given_again_after_its_effect_ends() {
        let (mut map, _, at) = with_enemy();
        let first = map.add_effect(probe(0, at, 1, 1));
        map.tick();
        assert!(map.effects.is_empty());
        let second = map.add_effect(probe(0, at, 1, 1));
        let other: crate::id::Id<Unit> = map.new_id_of();
        assert!(first < second && second.raw() < other.raw(), "one counter for every kind");
    }

    #[test]
    fn a_spawned_effect_ticks_from_the_next_turn() {
        let (mut map, _, at) = with_enemy();
        let parent = map.add_effect(Effect { kind: EffectKind::Probe(Probe { every: Every::new(1), spawn: true }), ..probe(0, at, 1, 3) });
        map.tick();
        let (child, effect) = map.effects.iter().find(|(id, _)| *id != parent).expect("spawned");
        assert!(child > parent);
        assert_eq!(effect.left.left(), Ticks::new(5), "not ticked yet");
        map.tick();
        assert_eq!(map.effects.get(child).unwrap().left.left(), Ticks::new(4));
    }

    #[test]
    fn events_for_things_gone_are_skipped() {
        let (mut map, _, _) = with_enemy();
        let before = map.units.clone();
        for event in [
            WorldEvent::Damage { unit: Id::from_raw(9999), amount: 50, kind: DamageKind::Blast, from: Some(1) },
            WorldEvent::Push { unit: Id::from_raw(9999), vel: Velocity::new(0, 60, 0), kind: PushKind::Spell },
            WorldEvent::Raid { building: Id::from_raw(9999) },
        ] {
            assert_eq!(map.apply_event(event), (None, Vec::new()));
        }
        assert_eq!(map.units, before);
    }

    #[test]
    fn damage_spares_the_shielded_and_the_casters_tribe() {
        let (mut map, enemy, _) = with_enemy();
        let shaman = map.units[0].id;
        let max = map.unit(enemy).unwrap().health.max();
        map.damage(shaman, 50, DamageKind::Blast, Some(0));
        assert!(map.unit(shaman).unwrap().health.is_full(), "own tribe");
        map.unit_mut(enemy).unwrap().statuses.shield = crate::status::Timed::steps(crate::status::SHIELD_STEPS);
        map.damage(enemy, 50, DamageKind::Blast, Some(0));
        assert_eq!(map.unit(enemy).unwrap().health.current(), max, "shielded");
        map.unit_mut(enemy).unwrap().statuses.shield = crate::status::Timed::default();
        map.damage(enemy, 50, DamageKind::Blast, Some(0));
        assert_eq!(map.unit(enemy).unwrap().health.current(), max - 50);
        map.damage(enemy, max, DamageKind::Blast, Some(0));
        assert!(!map.unit(enemy).unwrap().is_alive(), "dies at 0");
    }

    #[test]
    fn a_shield_stops_a_firewarriors_bolt() {
        let (mut map, enemy, _) = with_enemy();
        map.unit_mut(enemy).unwrap().statuses.shield = crate::status::Timed::steps(crate::status::SHIELD_STEPS);
        let max = map.unit(enemy).unwrap().health.max();
        map.damage(enemy, 50, DamageKind::Fire, Some(0));
        assert_eq!(map.unit(enemy).unwrap().health.current(), max, "a magical ranged attack, stopped like a spell");
    }

    #[test]
    fn a_shield_does_not_stop_a_blow() {
        let (mut map, enemy, _) = with_enemy();
        map.unit_mut(enemy).unwrap().statuses.shield = crate::status::Timed::steps(crate::status::SHIELD_STEPS);
        let max = map.unit(enemy).unwrap().health.max();
        map.damage(enemy, 60, DamageKind::Melee, Some(0));
        assert_eq!(map.unit(enemy).unwrap().health.current(), max - 60);
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
