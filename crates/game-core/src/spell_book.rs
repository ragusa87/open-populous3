//! A tribe's spells: which ones it knows, their charges and mana recharge.
//! Deterministic integer state, ticked by the simulation.

use pop3_format::level::{ThingData, KIND_SPELL};
use pop3_format::{Level, LevelHeader};

pub const MAX_CHARGES: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpellKind {
    Blast,
    Convert,
    Swarm,
    Invisibility,
    Hypnotism,
    Whirlwind,
    LandBridge,
    Lightning,
    Flatten,
    Erosion,
    Swamp,
    Earthquake,
    Firestorm,
    AngelOfDeath,
    Volcano,
    Armageddon,
    GhostArmy,
    MagicalShield,
    /// Sandbox spell: moves the shaman to any walkable spot. The original has a spell model 21
    /// teleport, never on the player's panel (`from_model` leaves it out).
    Teleport,
}

impl SpellKind {
    pub const ALL: [SpellKind; 19] = [
        SpellKind::Blast,
        SpellKind::Convert,
        SpellKind::Swarm,
        SpellKind::Invisibility,
        SpellKind::Hypnotism,
        SpellKind::Whirlwind,
        SpellKind::LandBridge,
        SpellKind::Lightning,
        SpellKind::Flatten,
        SpellKind::Erosion,
        SpellKind::Swamp,
        SpellKind::Earthquake,
        SpellKind::Firestorm,
        SpellKind::AngelOfDeath,
        SpellKind::Volcano,
        SpellKind::Armageddon,
        SpellKind::GhostArmy,
        SpellKind::MagicalShield,
        SpellKind::Teleport,
    ];

    /// The original spell model number (level header masks, discovery things, AI scripts).
    pub fn model(self) -> u8 {
        match self {
            SpellKind::Blast => 2,
            SpellKind::Lightning => 3,
            SpellKind::Whirlwind => 4,
            SpellKind::Swarm => 5,
            SpellKind::Invisibility => 6,
            SpellKind::Hypnotism => 7,
            SpellKind::Firestorm => 8,
            SpellKind::GhostArmy => 9,
            SpellKind::Erosion => 10,
            SpellKind::Swamp => 11,
            SpellKind::LandBridge => 12,
            SpellKind::AngelOfDeath => 13,
            SpellKind::Earthquake => 14,
            SpellKind::Flatten => 15,
            SpellKind::Volcano => 16,
            SpellKind::Convert => 17,
            SpellKind::Armageddon => 18,
            SpellKind::MagicalShield => 19,
            SpellKind::Teleport => 21,
        }
    }

    /// The player's spell for an original model (2-19). Burn (1), bloodlust (20) and teleport (21)
    /// are not on the original panel: None, like unknown models.
    pub fn from_model(model: u8) -> Option<SpellKind> {
        SpellKind::ALL.iter().copied().find(|k| k.model() == model && *k != SpellKind::Teleport)
    }

    pub fn name(self) -> &'static str {
        match self {
            SpellKind::Blast => "Blast",
            SpellKind::Convert => "Convert",
            SpellKind::Swarm => "Swarm",
            SpellKind::Invisibility => "Invisibility",
            SpellKind::Hypnotism => "Hypnotism",
            SpellKind::Whirlwind => "Whirlwind",
            SpellKind::LandBridge => "Land Bridge",
            SpellKind::Lightning => "Lightning",
            SpellKind::Flatten => "Flatten",
            SpellKind::Erosion => "Erosion",
            SpellKind::Swamp => "Swamp",
            SpellKind::Earthquake => "Earthquake",
            SpellKind::Firestorm => "Firestorm",
            SpellKind::AngelOfDeath => "Angel of Death",
            SpellKind::Volcano => "Volcano",
            SpellKind::Armageddon => "Armageddon",
            SpellKind::GhostArmy => "Ghost Army",
            SpellKind::MagicalShield => "Magical Shield",
            SpellKind::Teleport => "Teleportation",
        }
    }

    /// Mana needed to regain one charge.
    pub fn cost(self) -> u32 {
        match self {
            SpellKind::Teleport => 1,
            SpellKind::Blast | SpellKind::Convert => 40,
            SpellKind::Swarm | SpellKind::Invisibility | SpellKind::LandBridge | SpellKind::GhostArmy => 120,
            SpellKind::MagicalShield => 200,
            SpellKind::Hypnotism | SpellKind::Whirlwind | SpellKind::Lightning | SpellKind::Flatten => 200,
            SpellKind::Erosion | SpellKind::Swamp => 300,
            SpellKind::Earthquake | SpellKind::Firestorm => 500,
            SpellKind::AngelOfDeath | SpellKind::Volcano => 800,
            SpellKind::Armageddon => 1500,
        }
    }

    /// The most charges this spell can hold (big spells hold fewer).
    pub fn max_charges(self) -> u8 {
        match self.cost() {
            0..=200 => MAX_CHARGES,
            201..=500 => 2,
            _ => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    /// Not in this level at all.
    Hidden,
    /// Shown as "?": must be discovered (e.g. by worshipping a totem) before use.
    Discoverable,
    /// Gray tile: a fixed number of free uses, no recharge, gone when used up.
    Provided { shots: u8 },
    /// Known spell: charges refill with mana.
    Known,
    /// Cast at will, nothing used up (sandbox spells).
    Unlimited,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpellSlot {
    pub kind: SpellKind,
    pub availability: Availability,
    pub charges: u8,
    /// Mana accumulated toward the next charge, `0..kind.cost()`.
    pub recharge: u32,
    /// Switched off by the player: it takes no mana and keeps its charges (known spells only).
    pub paused: bool,
}

impl SpellSlot {
    pub fn new(kind: SpellKind, availability: Availability) -> Self {
        SpellSlot { kind, availability, charges: 0, recharge: 0, paused: false }
    }

    pub fn can_cast(&self) -> bool {
        match self.availability {
            Availability::Known => self.charges > 0,
            Availability::Provided { shots } => shots > 0,
            Availability::Unlimited => true,
            _ => false,
        }
    }

    /// Known and not full: it would take mana, unless paused (`takes_mana`).
    pub fn is_recharging(&self) -> bool {
        self.availability == Availability::Known && self.charges < self.kind.max_charges()
    }

    pub fn takes_mana(&self) -> bool {
        self.is_recharging() && !self.paused
    }
}

/// Whether the header's `SpellsAvailable` gives `kind` from the start. Armageddon only counts next to
/// Convert: it is the campaign's last spell (discovered in levels 17 and 18, in the masks from 19 on,
/// always with Convert), while levels 1 and 2 carry bit 18 among leftover bits (burn, bloodlust,
/// teleport) and no Convert.
pub fn known_in_header(header: &LevelHeader, kind: SpellKind) -> bool {
    let set = |k: SpellKind| header.spell_available(k.model());
    set(kind) && (kind != SpellKind::Armageddon || set(SpellKind::Convert))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpellBook {
    pub slots: Vec<SpellSlot>,
}

impl SpellBook {
    /// Every spell, all hidden: the level then reveals some.
    pub fn new() -> Self {
        SpellBook { slots: SpellKind::ALL.iter().map(|&k| SpellSlot::new(k, Availability::Hidden)).collect() }
    }

    /// An original level's loadout: the header's `SpellsAvailable` spells are known (`known_in_header`),
    /// with full charges (unverified: how charged they start); the spells of its discovery things that
    /// are not known yet are "?".
    pub fn from_level(header: &LevelHeader, level: &Level) -> Self {
        let mut book = SpellBook::new();
        let discoveries = level.things.iter().filter_map(|t| match t.data() {
            ThingData::Discovery(d) if d.kind == KIND_SPELL => SpellKind::from_model(d.model),
            _ => None,
        });
        for kind in discoveries {
            book.set(kind, Availability::Discoverable);
        }
        for kind in SpellKind::ALL.iter().copied().filter(|k| SpellKind::from_model(k.model()) == Some(*k)) {
            if known_in_header(header, kind) {
                book.set(kind, Availability::Known);
            }
        }
        book.fill();
        book
    }

    /// Every known spell at full charges.
    pub fn fill(&mut self) {
        for s in self.slots.iter_mut().filter(|s| s.availability == Availability::Known) {
            s.charges = s.kind.max_charges();
            s.recharge = 0;
        }
    }

    pub fn slot(&self, kind: SpellKind) -> Option<&SpellSlot> {
        self.slots.iter().find(|s| s.kind == kind)
    }

    pub fn set(&mut self, kind: SpellKind, availability: Availability) {
        if let Some(s) = self.slots.iter_mut().find(|s| s.kind == kind) {
            s.availability = availability;
        }
    }

    /// Switches a known spell's recharge off or on (the player's right click on its tile).
    pub fn toggle_pause(&mut self, kind: SpellKind) {
        if let Some(s) = self.slots.iter_mut().find(|s| s.kind == kind && s.availability == Availability::Known) {
            s.paused = !s.paused;
        }
    }

    pub fn discover(&mut self, kind: SpellKind) {
        if self.slot(kind).is_some_and(|s| s.availability == Availability::Discoverable) {
            self.set(kind, Availability::Known);
        }
    }

    /// Give `mana` to every recharging spell that is not paused; full charges keep the rest at 0.
    pub fn tick(&mut self, mana: u32) {
        for s in self.slots.iter_mut().filter(|s| s.takes_mana()) {
            s.recharge += mana;
            while s.recharge >= s.kind.cost() && s.charges < s.kind.max_charges() {
                s.recharge -= s.kind.cost();
                s.charges += 1;
            }
            if s.charges == s.kind.max_charges() {
                s.recharge = 0;
            }
        }
    }

    /// Consume one use. Returns false if the spell cannot be cast right now.
    pub fn cast(&mut self, kind: SpellKind) -> bool {
        let Some(s) = self.slots.iter_mut().find(|s| s.kind == kind) else { return false };
        match s.availability {
            Availability::Known if s.charges > 0 => s.charges -= 1,
            Availability::Provided { shots } if shots > 0 => {
                s.availability = if shots == 1 { Availability::Hidden } else { Availability::Provided { shots: shots - 1 } };
            }
            Availability::Unlimited => {}
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_spells_recharge_up_to_max() {
        let mut b = SpellBook::new();
        b.set(SpellKind::Blast, Availability::Known);
        b.tick(39);
        assert_eq!(b.slot(SpellKind::Blast).unwrap().charges, 0);
        b.tick(1);
        assert_eq!(b.slot(SpellKind::Blast).unwrap().charges, 1);
        b.tick(10_000);
        let s = b.slot(SpellKind::Blast).unwrap();
        assert_eq!((s.charges, s.recharge), (MAX_CHARGES, 0));
    }

    #[test]
    fn original_model_numbers() {
        assert_eq!(SpellKind::from_model(2), Some(SpellKind::Blast));
        assert_eq!(SpellKind::from_model(17), Some(SpellKind::Convert));
        assert_eq!(SpellKind::from_model(19), Some(SpellKind::MagicalShield));
        for model in [0, 1, 20, 21, 22] {
            assert_eq!(SpellKind::from_model(model), None, "model {model} is not a panel spell");
        }
        for k in SpellKind::ALL.iter().filter(|&&k| k != SpellKind::Teleport) {
            assert_eq!(SpellKind::from_model(k.model()), Some(*k));
        }
    }

    #[test]
    fn level_loadout_from_header_and_discoveries() {
        let mut hdr = vec![0u8; 616];
        let mask: u32 = 0xffc0_0001 | 1 << 2 | 1 << 5 | 1 << 20 | 1 << 21;
        hdr[0..4].copy_from_slice(&mask.to_le_bytes());
        let header = LevelHeader::parse(&hdr);
        let mut dat = vec![0u8; pop3_format::level::DAT_SIZE];
        let thing = |dat: &mut Vec<u8>, slot: usize, kind: u8, model: u8| {
            let t = 81_987 + slot * 55;
            dat[t..t + 2].copy_from_slice(&[pop3_format::level::GENERAL_DISCOVERY, pop3_format::level::KIND_GENERAL]);
            dat[t + 7..t + 10].copy_from_slice(&[kind, model, 3]);
        };
        thing(&mut dat, 0, KIND_SPELL, 17);
        thing(&mut dat, 1, KIND_SPELL, 5);
        thing(&mut dat, 2, 2, 4);
        let book = SpellBook::from_level(&header, &Level::parse(&dat).unwrap());
        let avail = |k| book.slot(k).unwrap().availability;
        assert_eq!(avail(SpellKind::Blast), Availability::Known);
        assert_eq!(book.slot(SpellKind::Blast).unwrap().charges, MAX_CHARGES, "starts charged");
        assert_eq!(avail(SpellKind::Swarm), Availability::Known, "known wins over its discovery");
        assert_eq!(avail(SpellKind::Convert), Availability::Discoverable);
        assert_eq!(avail(SpellKind::Whirlwind), Availability::Hidden, "a building discovery");
        assert_eq!(avail(SpellKind::Teleport), Availability::Hidden, "model 21 is not the sandbox teleport");
        let known = book.slots.iter().filter(|s| s.availability == Availability::Known).count();
        assert_eq!(known, 2, "the always-set mask bits and bloodlust give nothing");
    }

    #[test]
    fn armageddon_only_with_convert() {
        let header = |models: &[u8]| {
            let mask = models.iter().fold(0xffc0_0001u32, |m, &b| m | 1 << b);
            let mut hdr = vec![0u8; 616];
            hdr[0..4].copy_from_slice(&mask.to_le_bytes());
            LevelHeader::parse(&hdr)
        };
        let level_1 = header(&[1, 2, 18, 20, 21]);
        assert!(known_in_header(&level_1, SpellKind::Blast));
        assert!(!known_in_header(&level_1, SpellKind::Armageddon), "leftover bit on level 1");
        let level_19 = header(&[2, 17, 18]);
        assert!(known_in_header(&level_19, SpellKind::Armageddon));
    }

    #[test]
    fn paused_spells_take_no_mana_and_keep_their_charges() {
        let mut b = SpellBook::new();
        b.set(SpellKind::Blast, Availability::Known);
        b.tick(40);
        b.toggle_pause(SpellKind::Blast);
        b.tick(1000);
        let s = b.slot(SpellKind::Blast).unwrap();
        assert_eq!((s.charges, s.recharge, s.paused), (1, 0, true));
        assert!(b.cast(SpellKind::Blast), "a paused spell can still be cast");
        b.toggle_pause(SpellKind::Blast);
        b.tick(40);
        assert_eq!(b.slot(SpellKind::Blast).unwrap().charges, 1, "recharging again");
        b.set(SpellKind::Volcano, Availability::Provided { shots: 1 });
        b.toggle_pause(SpellKind::Volcano);
        assert!(!b.slot(SpellKind::Volcano).unwrap().paused, "only known spells recharge");
    }

    #[test]
    fn big_spells_hold_fewer_charges() {
        assert_eq!(SpellKind::Armageddon.max_charges(), 1);
        assert_eq!(SpellKind::Swamp.max_charges(), 2);
    }

    #[test]
    fn provided_shots_run_out_and_never_recharge() {
        let mut b = SpellBook::new();
        b.set(SpellKind::Volcano, Availability::Provided { shots: 2 });
        b.tick(10_000);
        assert!(b.cast(SpellKind::Volcano));
        assert!(b.cast(SpellKind::Volcano));
        assert_eq!(b.slot(SpellKind::Volcano).unwrap().availability, Availability::Hidden);
        assert!(!b.cast(SpellKind::Volcano));
    }

    #[test]
    fn discoverable_must_be_discovered_first() {
        let mut b = SpellBook::new();
        b.set(SpellKind::Flatten, Availability::Discoverable);
        b.tick(10_000);
        assert!(!b.cast(SpellKind::Flatten));
        b.discover(SpellKind::Flatten);
        b.tick(200);
        assert!(b.cast(SpellKind::Flatten));
    }

    #[test]
    fn unlimited_spells_never_run_out() {
        let mut b = SpellBook::new();
        b.set(SpellKind::Teleport, Availability::Unlimited);
        for _ in 0..100 {
            assert!(b.cast(SpellKind::Teleport));
        }
        assert!(!b.slot(SpellKind::Teleport).unwrap().is_recharging());
    }

    #[test]
    fn casting_uses_a_charge() {
        let mut b = SpellBook::new();
        b.set(SpellKind::Convert, Availability::Known);
        b.tick(80);
        assert!(b.cast(SpellKind::Convert));
        assert_eq!(b.slot(SpellKind::Convert).unwrap().charges, 1);
    }
}
