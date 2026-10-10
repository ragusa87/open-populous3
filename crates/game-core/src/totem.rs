//! Totems: the places units pray at for a reward (docs/specs/worship.md). A level's totem is
//! scenery 9; who may pray at it, how many count and how long it takes come from the trigger on its
//! cell. Which original model the levels' totems use is not known: they are drawn as stone heads,
//! and the Worship sandbox shows every candidate.

use crate::building::{BuildingKind, Reward};
use crate::spell_book::SpellKind;
use crate::time::Ticks;
use pop3_format::level::{Availability, Thing, Trigger, KIND_BUILDING, KIND_SCENERY, KIND_SPELL};
use pop3_format::{Level, ThingData, WORLD_UNITS_PER_CELL};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Totem {
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
    pub queue: Vec<u32>,
    /// Ticks since it last gave (counting once it gave at all).
    pub since_given: u16,
}

impl Totem {
    /// A totem any one unit fills in `DEFAULT_PRAY_TIME`.
    pub fn new(kind: TotemKind, (x, z): (u16, u16)) -> Self {
        Totem { kind, x, z, prayers: 1, shaman_only: false, summons_angel: false, pray_time: DEFAULT_PRAY_TIME, gifts: Vec::new(), occurrences: 0, given: 0, gauges: [0; TRIBES], queue: Vec::new(), since_given: 0 }
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
            gifts: t.targets.iter().filter_map(|&slot| level.slot(slot)).map(gift).collect(),
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

    /// Exhausted and sunk under the ground: nothing to see, hover or pray at any more.
    pub fn is_gone(&self) -> bool {
        self.is_exhausted() && self.since_given as u32 >= (TURN_TICKS + HOLD_TICKS + SINK_TICKS).get()
    }
}

/// What a trigger target gives: a discovered spell (one cast for a "once" discovery), building or
/// mana; anything else is not handled yet.
pub fn gift(target: &Thing) -> Reward {
    match target.data() {
        ThingData::Discovery(d) if d.kind == KIND_SPELL => match SpellKind::from_model(d.model) {
            Some(kind) if d.availability == Availability::Once => Reward::OneShot(kind),
            Some(kind) => Reward::Spell(kind),
            None => Reward::Unhandled { kind: d.kind, model: d.model },
        },
        ThingData::Discovery(d) if d.kind == KIND_BUILDING => Reward::Building(BuildingKind::from_model(d.model)),
        ThingData::Discovery(d) if d.kind == DISCOVERY_MANA => Reward::Mana(d.mana.max(0) as u32),
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
    let is_totem = |t: &&Thing| t.kind == KIND_SCENERY && t.model == SCENERY_TOTEM;
    let totems: Vec<&Thing> = level.things.iter().filter(is_totem).collect();
    totems
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let before = totems[..i].iter().filter(|o| (o.x / 512, o.z / 512) == (t.x / 512, t.z / 512)).count();
            let prayer = triggers_on_cell(level, t).into_iter().filter(|tr| matches!(tr.trigger_type, TRIGGER_UNITS | TRIGGER_SHAMAN | TRIGGER_SHAMAN_ANGEL)).nth(before);
            Totem::from_trigger(level, t, prayer.as_ref())
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
        let effect = thing(24, 7, (0, 0), &[]);
        let level = level_with(&[totem((9, 9)), trig, once, mana, effect]);
        let t = &totems_from_level(&level)[0];
        assert_eq!(t.gifts, vec![Reward::OneShot(SpellKind::Swarm), Reward::Mana(50_000), Reward::Unhandled { kind: 7, model: 24 }]);
        assert_eq!(t.occurrences, 2);
    }

    #[test]
    fn a_library_trigger_is_not_a_prayer_trigger() {
        let level = level_with(&[totem((1, 1)), trigger((1, 1), 4, 5, 77)]);
        assert_eq!(totems_from_level(&level)[0].pray_time, DEFAULT_PRAY_TIME);
    }
}
