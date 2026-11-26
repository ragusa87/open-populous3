//! Totems: the places units pray at for a reward (docs/specs/worship.md). A level's totem is
//! scenery 9; who may pray at it, how many count and how long it takes come from the trigger on its
//! cell. Which original model the levels' totems use is not known: they are drawn as stone heads,
//! and the Worship sandbox shows every candidate.

use crate::building::{BuildingKind, Reward};
use crate::id::Id;
use crate::spell_book::SpellKind;
use crate::time::Ticks;
use crate::unit::UnitId;
use pop3_format::level::{Availability, Thing, Trigger, KIND_BUILDING, KIND_SCENERY, KIND_SPELL};
use pop3_format::{Level, ThingData, WORLD_UNITS_PER_CELL};

/// Effect 88, "teleport": its trigger's tribe's shaman teleports to it.
const EFFECT_TELEPORT: u8 = 88;

/// The things some trigger targets (their 0-based slots): created when it fires, not at level load
/// (pop3-rev-analysis.md "Fog of war": "Totems and other level hooks").
pub fn targeted(level: &Level) -> std::collections::BTreeSet<u16> {
    level
        .things
        .iter()
        .filter_map(|t| match t.data() {
            ThingData::Trigger(trigger) => Some(trigger.targets),
            _ => None,
        })
        .flatten()
        .filter_map(|slot| slot.checked_sub(1))
        .collect()
}

/// Scenery model of the levels' totems.
pub const SCENERY_TOTEM: u8 = 9;
/// Trigger types: any of the tribe's units pray; only the shaman; only the shaman, and an Angel of
/// Death appears when it fires.
const TRIGGER_UNITS: u8 = 0;
const TRIGGER_SHAMAN: u8 = 3;
const TRIGGER_SHAMAN_ANGEL: u8 = 5;
/// Gauge length of a totem without a trigger (sandboxes), in ticks of full-speed praying.
pub const DEFAULT_PRAY_TIME: u16 = 100;
/// Discovery kind of mana (`DiscoveryType` 6).
const DISCOVERY_MANA: u8 = 6;
/// Tribes with a gauge of their own on each totem.
pub const TRIBES: usize = 4;
/// After its first gift the stone totem turns its rocks for `TURN_TICKS`; once it gave its last, it
/// holds `HOLD_TICKS` more, then sinks under the ground for `SINK_TICKS` and is gone (worship.md).
pub const TURN_TICKS: Ticks = Ticks::millis(2000);
pub const HOLD_TICKS: Ticks = Ticks::millis(1000);
pub const SINK_TICKS: Ticks = Ticks::millis(4000);

/// The original objects that look like something to pray at (objects.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TotemKind {
    /// Totem with rotating rocks.
    Totem,
    /// Totem of the winged death, a bird perched on it.
    WingedDeath,
    /// Stone prayer totem.
    Prayer,
    StoneHead,
    /// Totem pole, three variants.
    Pole(u8),
}

impl TotemKind {
    pub const ALL: [TotemKind; 7] = [TotemKind::Totem, TotemKind::WingedDeath, TotemKind::Prayer, TotemKind::StoneHead, TotemKind::Pole(0), TotemKind::Pole(1), TotemKind::Pole(2)];

    pub fn name(self) -> &'static str {
        match self {
            TotemKind::Totem => "Totem",
            TotemKind::WingedDeath => "Winged death totem",
            TotemKind::Prayer => "Prayer totem",
            TotemKind::StoneHead => "Stone head",
            TotemKind::Pole(_) => "Totem pole",
        }
    }
}

/// A totem's stable id (`GameMap::new_id_of`).
pub type TotemId = Id<Totem>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Totem {
    /// Given by `GameMap::add_totem`, never twice on a map (0 until then).
    pub id: TotemId,
    pub kind: TotemKind,
    /// Centre, world units (512 per cell).
    pub x: u16,
    pub z: u16,
    /// Units counted while praying (`TriggerCount`): the gauge is at full speed with that many.
    pub prayers: u16,
    /// Only the shaman may pray at it.
    pub shaman_only: bool,
    /// An Angel of Death appears when it fires.
    pub summons_angel: bool,
    /// Gauge length (the trigger's `PrayTime`).
    pub pray_time: u16,
    /// What it gives each time a tribe fills its gauge (the trigger's targets).
    pub gifts: Vec<Reward>,
    /// How many times it can give (the trigger's `NumOccurences`; 0: no limit, a guess), and how many
    /// times it gave.
    pub occurrences: u8,
    pub given: u8,
    /// Each tribe's gauge, in `gauge::STEP`s.
    pub gauges: [u32; TRIBES],
    /// The units praying at it, in the order they started: each tribe's first `prayers` count.
    pub queue: Vec<UnitId>,
    /// Ticks since it last gave (counting once it gave at all).
    pub since_given: u16,
    /// Not risen yet: a trigger creates it (`Reward::WakeTotem`); nothing to see, hover or pray at till then.
    pub dormant: bool,
}

impl Totem {
    /// A totem any one unit fills in `DEFAULT_PRAY_TIME`.
    pub fn new(kind: TotemKind, (x, z): (u16, u16)) -> Self {
        Totem { id: Id::from_raw(0), kind, x, z, prayers: 1, shaman_only: false, summons_angel: false, pray_time: DEFAULT_PRAY_TIME, gifts: Vec::new(), occurrences: 0, given: 0, gauges: [0; TRIBES], queue: Vec::new(), since_given: 0, dormant: false }
    }

    /// The totem of `thing` with what `trigger` asks for and gives (its targets in `level`); the
    /// defaults of `new` without one.
    pub fn from_trigger(level: &Level, thing: &Thing, trigger: Option<&Trigger>) -> Self {
        let totem = Totem::new(TotemKind::StoneHead, (thing.x, thing.z));
        let Some(t) = trigger else { return totem };
        Totem {
            prayers: t.count.max(1),
            shaman_only: matches!(t.trigger_type, TRIGGER_SHAMAN | TRIGGER_SHAMAN_ANGEL),
            summons_angel: t.trigger_type == TRIGGER_SHAMAN_ANGEL,
            pray_time: t.pray_time.max(1) as u16,
            gifts: t.targets.iter().filter_map(|&slot| level.slot(slot)).map(|target| gift(target, t.create_player_owned != 0)).collect(),
            occurrences: t.occurrences.max(0) as u8,
            ..totem
        }
    }

    /// Its gauge's steps when full.
    pub fn full(&self) -> u32 {
        self.pray_time as u32 * crate::gauge::STEP
    }

    /// It gave as many times as it can.
    pub fn is_exhausted(&self) -> bool {
        self.occurrences > 0 && self.given >= self.occurrences
    }

    /// Not risen yet, or exhausted and sunk under the ground: nothing to see, hover or pray at.
    pub fn is_gone(&self) -> bool {
        self.dormant || self.is_exhausted() && self.since_given as u32 >= (TURN_TICKS + HOLD_TICKS + SINK_TICKS).get()
    }
}

/// What a trigger target gives: a discovered spell (one cast for a "once" discovery), building or
/// mana; anything else is not handled yet.
pub fn gift(target: &Thing, player_owned: bool) -> Reward {
    use crate::spell::Spell;
    use pop3_format::level::{KIND_EFFECT, KIND_PERSON, KIND_VEHICLE};
    let at = (target.x, target.z);
    let point = |(x, z): (u16, u16)| (((x as u32 + WORLD_UNITS_PER_CELL / 2) / WORLD_UNITS_PER_CELL) as i32, ((z as u32 + WORLD_UNITS_PER_CELL / 2) / WORLD_UNITS_PER_CELL) as i32);
    let cell = |(x, z): (u16, u16)| ((x as u32 / WORLD_UNITS_PER_CELL) as i32, (z as u32 / WORLD_UNITS_PER_CELL) as i32);
    let owner = (!player_owned).then(|| crate::tribes::Owner::from_u8(target.owner));
    let aimed = match target.data() {
        ThingData::EffectTarget { x, z } => (x, z),
        _ => at,
    };
    if target.kind == KIND_EFFECT {
        let cast = match target.model {
            12 => Some(Spell::GhostArmy { at }),
            13 => Some(Spell::Invisibility { at }),
            15 => Some(Spell::Volcano { at }),
            16 => Some(Spell::Hypnotism { at }),
            pop3_format::level::EFFECT_LIGHTNING_BOLT => Some(Spell::Lightning { at: aimed }),
            18 => Some(Spell::Swamp { at }),
            19 => Some(Spell::AngelOfDeath { at }),
            20 => Some(Spell::Whirlwind { at }),
            21 => Some(Spell::Swarm { at }),
            22 => Some(Spell::Firestorm { at }),
            23 => Some(Spell::Erode { at: point(at) }),
            pop3_format::level::EFFECT_LAND_BRIDGE => Some(Spell::LandBridge { from: cell(at), to: cell(aimed) }),
            26 => Some(Spell::Earthquake { at }),
            30 => Some(Spell::Lightning { at }),
            31 => Some(Spell::Flatten { at: point(at) }),
            73 => Some(Spell::Convert { at }),
            86 => Some(Spell::Armageddon { at }),
            _ => None,
        };
        if let Some(spell) = cast {
            return Reward::Cast(spell);
        }
        if target.model == EFFECT_TELEPORT {
            return Reward::Teleport { to: at };
        }
    }
    match (target.kind, target.model) {
        (KIND_PERSON, model) => {
            if let Some(kind) = crate::unit::UnitKind::from_person_model(model) {
                return Reward::Person { kind, owner, at };
            }
        }
        (KIND_BUILDING, model) => return Reward::PlaceBuilding { kind: BuildingKind::from_model(model), owner, at, facing: target.facing() },
        (KIND_VEHICLE, 1 | 2) => return Reward::Vehicle { kind: crate::vehicle::VehicleKind::Boat, owner, at },
        (KIND_VEHICLE, 3 | 4) => return Reward::Vehicle { kind: crate::vehicle::VehicleKind::Balloon, owner, at },
        (KIND_SCENERY, SCENERY_TOTEM) => return Reward::WakeTotem { at },
        _ => {}
    }
    if matches!(target.data(), ThingData::Trigger(_)) {
        return Reward::WakeTotem { at };
    }
    match target.data() {
        ThingData::Discovery(d) if d.kind == KIND_SPELL => match SpellKind::from_model(d.model) {
            Some(kind) if d.availability == Availability::Once => Reward::OneShot(kind),
            Some(kind) => Reward::Spell(kind),
            None => Reward::Unhandled { kind: d.kind, model: d.model },
        },
        ThingData::Discovery(d) if d.kind == KIND_BUILDING => Reward::Building(BuildingKind::from_model(d.model)),
        ThingData::Discovery(d) if d.kind == DISCOVERY_MANA => Reward::Mana(d.mana.max(0) as u32),
        _ if target.kind == pop3_format::level::KIND_EFFECT && target.model == pop3_format::level::EFFECT_REVEAL_FOG => {
            Reward::RevealFog { cell: ((target.x as u32 / WORLD_UNITS_PER_CELL) as i32, (target.z as u32 / WORLD_UNITS_PER_CELL) as i32) }
        }
        _ => Reward::Unhandled { kind: target.kind, model: target.model },
    }
}

/// The triggers standing on `thing`'s cell, in slot order.
pub fn triggers_on_cell(level: &Level, thing: &Thing) -> Vec<Trigger> {
    let cell = |t: &Thing| (t.x as u32 / WORLD_UNITS_PER_CELL, t.z as u32 / WORLD_UNITS_PER_CELL);
    level
        .things
        .iter()
        .filter(|t| cell(t) == cell(thing))
        .filter_map(|t| match t.data() {
            ThingData::Trigger(trigger) => Some(trigger),
            _ => None,
        })
        .collect()
}

/// A level's totems (scenery 9), each with the prayer trigger on its cell; several totems on one
/// cell take its triggers in slot order (level 23).
pub fn totems_from_level(level: &Level) -> Vec<Totem> {
    let targeted = targeted(level);
    let is_totem = |t: &&Thing| t.kind == KIND_SCENERY && t.model == SCENERY_TOTEM;
    let totems: Vec<&Thing> = level.things.iter().filter(is_totem).collect();
    totems
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let before = totems[..i].iter().filter(|o| (o.x / 512, o.z / 512) == (t.x / 512, t.z / 512)).count();
            let prayer = triggers_on_cell(level, t).into_iter().filter(|tr| matches!(tr.trigger_type, TRIGGER_UNITS | TRIGGER_SHAMAN | TRIGGER_SHAMAN_ANGEL)).nth(before);
            let cell = |o: &Thing| (o.x / 512, o.z / 512);
            let its_trigger_waits = level.things.iter().any(|o| cell(o) == cell(t) && matches!(o.data(), ThingData::Trigger(_)) && targeted.contains(&o.slot));
            Totem { dormant: targeted.contains(&t.slot) || its_trigger_waits, ..Totem::from_trigger(level, t, prayer.as_ref()) }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pop3_format::level::{DAT_SIZE, KIND_GENERAL};

    fn level_with(things: &[[u8; 55]]) -> Level {
        let mut d = vec![0u8; DAT_SIZE];
        for (i, t) in things.iter().enumerate() {
            d[81_987 + i * 55..][..55].copy_from_slice(t);
        }
        Level::parse(&d).unwrap()
    }

    fn thing(model: u8, kind: u8, (cx, cz): (u16, u16), extra: &[u8]) -> [u8; 55] {
        let mut t = [0u8; 55];
        let (x, z) = ((cx * 512 + 256).to_le_bytes(), (cz * 512 + 256).to_le_bytes());
        t[..7].copy_from_slice(&[model, kind, 0, x[0], x[1], z[0], z[1]]);
        t[7..7 + extra.len()].copy_from_slice(extra);
        t
    }

    fn trigger(cell: (u16, u16), trigger_type: u8, count: u8, pray_time: u16) -> [u8; 55] {
        let mut t = thing(6, KIND_GENERAL, cell, &[trigger_type, 1, 0, 1, count]);
        t[33..35].copy_from_slice(&pray_time.to_le_bytes());
        t
    }

    fn totem(cell: (u16, u16)) -> [u8; 55] {
        thing(SCENERY_TOTEM, KIND_SCENERY, cell, &[])
    }

    #[test]
    fn effects_people_vehicles_and_heads_given_map_to_their_rewards() {
        use crate::spell::Spell;
        use pop3_format::level::{KIND_EFFECT, KIND_PERSON, KIND_VEHICLE};
        let at = (30 * 512 + 256, 40 * 512 + 256);
        let level = level_with(&[
            thing(23, KIND_EFFECT, (30, 40), &[]),
            thing(15, KIND_EFFECT, (30, 40), &[]),
            thing(88, KIND_EFFECT, (30, 40), &[]),
            thing(3, KIND_PERSON, (30, 40), &[]),
            thing(1, KIND_VEHICLE, (30, 40), &[]),
            totem((30, 40)),
            thing(5, KIND_EFFECT, (30, 40), &[]),
        ]);
        let g: Vec<Reward> = level.things.iter().map(|t| gift(t, false)).collect();
        assert_eq!(g[0], Reward::Cast(Spell::Erode { at: (31, 41) }), "erosion on the nearest height point");
        assert_eq!(g[1], Reward::Cast(Spell::Volcano { at }));
        assert_eq!(g[2], Reward::Teleport { to: at });
        assert_eq!(g[3], Reward::Person { kind: crate::unit::UnitKind::Warrior, owner: Some(crate::tribes::Owner::from_u8(0)), at });
        assert_eq!(g[4], Reward::Vehicle { kind: crate::vehicle::VehicleKind::Boat, owner: Some(crate::tribes::Owner::from_u8(0)), at });
        assert_eq!(g[5], Reward::WakeTotem { at });
        assert_eq!(g[6], Reward::Unhandled { kind: KIND_EFFECT, model: 5 }, "meaning not in the docs");
        assert_eq!(gift(&level.things[3], true), Reward::Person { kind: crate::unit::UnitKind::Warrior, owner: None, at }, "CreatePlayerOwned");
    }

    #[test]
    fn a_head_a_trigger_creates_starts_dormant() {
        let mut fire = trigger((5, 5), 0, 1, 10);
        fire[13..15].copy_from_slice(&3u16.to_le_bytes());
        let level = level_with(&[totem((5, 5)), fire, totem((20, 20)), trigger((20, 20), 0, 1, 10)]);
        let t = totems_from_level(&level);
        assert!(!t[0].dormant);
        assert!(t[1].dormant, "slot 3 (1-based) is the second head");
        assert_eq!(t[0].gifts, vec![Reward::WakeTotem { at: (20 * 512 + 256, 20 * 512 + 256) }]);
    }

    #[test]
    fn a_land_bridge_effect_gives_a_bridge_from_its_cell_to_its_target() {
        let (x, z) = ((101 * 512 + 256u16).to_le_bytes(), (48 * 512 + 256u16).to_le_bytes());
        let level = level_with(&[thing(pop3_format::level::EFFECT_LAND_BRIDGE, pop3_format::level::KIND_EFFECT, (93, 48), &[x[0], x[1], 0, 0, z[0], z[1], 0, 0])]);
        assert_eq!(gift(&level.things[0], false), Reward::Cast(crate::spell::Spell::LandBridge { from: (93, 48), to: (101, 48) }));
    }

    #[test]
    fn a_totem_takes_who_how_many_and_how_long_from_its_trigger() {
        let level = level_with(&[totem((9, 123)), trigger((9, 123), 0, 6, 420), totem((51, 9)), trigger((51, 9), 3, 1, 32), totem((83, 65)), trigger((83, 65), 5, 1, 10)]);
        let t = totems_from_level(&level);
        assert_eq!((t[0].prayers, t[0].shaman_only, t[0].pray_time), (6, false, 420), "any 6 units");
        assert_eq!((t[1].prayers, t[1].shaman_only, t[1].summons_angel), (1, true, false), "the shaman only");
        assert_eq!((t[2].shaman_only, t[2].summons_angel), (true, true), "the shaman, then an Angel of Death");
        assert_eq!((t[0].x, t[0].z), (9 * 512 + 256, 123 * 512 + 256));
    }

    #[test]
    fn totems_on_one_cell_take_its_triggers_in_order_and_a_lone_one_the_defaults() {
        let level = level_with(&[totem((30, 84)), trigger((30, 84), 0, 6, 1000), totem((30, 84)), trigger((30, 84), 0, 1, 100), totem((5, 5)), trigger((5, 6), 0, 4, 50)]);
        let t = totems_from_level(&level);
        assert_eq!((t[0].prayers, t[0].pray_time), (6, 1000));
        assert_eq!((t[1].prayers, t[1].pray_time), (1, 100));
        assert_eq!((t[2].prayers, t[2].pray_time), (1, DEFAULT_PRAY_TIME), "its trigger is on the next cell");
    }

    #[test]
    fn its_gifts_and_occurrences_come_from_the_trigger_targets() {
        let mut trig = trigger((9, 9), 0, 4, 50);
        trig[13..19].copy_from_slice(&[3, 0, 4, 0, 5, 0]);
        trig[10] = 2;
        let once = thing(2, KIND_GENERAL, (0, 0), &[KIND_SPELL, SpellKind::Swarm.model(), 3, 1]);
        let mut mana = thing(2, KIND_GENERAL, (0, 0), &[DISCOVERY_MANA, 3, 3, 1]);
        mana[11..15].copy_from_slice(&50_000i32.to_le_bytes());
        let effect = thing(91, 7, (0, 0), &[]);
        let level = level_with(&[totem((9, 9)), trig, once, mana, effect]);
        let t = &totems_from_level(&level)[0];
        assert_eq!(t.gifts, vec![Reward::OneShot(SpellKind::Swarm), Reward::Mana(50_000), Reward::Unhandled { kind: 7, model: 91 }]);
        assert_eq!(t.occurrences, 2);
    }

    #[test]
    fn a_library_trigger_is_not_a_prayer_trigger() {
        let level = level_with(&[totem((1, 1)), trigger((1, 1), 4, 5, 77)]);
        assert_eq!(totems_from_level(&level)[0].pray_time, DEFAULT_PRAY_TIME);
    }
}
