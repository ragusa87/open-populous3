//! A tribe's spells: which ones it knows, their charges and mana recharge.
//! Deterministic integer state, ticked by the simulation.

use crate::mana::Mana;
use pop3_format::level::{ThingData, KIND_SPELL};
use pop3_format::{Constants, Level, LevelHeader};

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
    /// A hidden spell some levels give (model 20): the nearest followers fight harder for a while (no effect yet).
    /// It takes Ghost Army's place on the panel when given.
    Bloodlust,
}

impl SpellKind {
    pub const ALL: [SpellKind; 20] = [
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
        SpellKind::Bloodlust,
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
            SpellKind::Bloodlust => 20,
            SpellKind::Teleport => 21,
        }
    }

    /// The player's spell for an original model (2-20). Burn (1) and teleport (21) are not on the original
    /// panel: None, like unknown models.
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
            SpellKind::Bloodlust => "Bloodlust",
        }
    }

    /// Spells that start at the caster and reach towards the target (land bridge): their origin is the
    /// shaman, or the view on a level where the player is a god (`GameMap::shaman_omni`).
    pub fn has_origin(self) -> bool {
        self == SpellKind::LandBridge
    }

    /// The constant.dat suffix for this spell (`SPELL_<suffix>`, `SP_1_OFF_MAX_<suffix>`,
    /// `SPELL_<suffix>_OPT_S`, `SP_W_RANGE_<suffix>`; spells.md "Original balance") — except Convert's own
    /// cost, which the file keys under `SPELL_CONVERT_WILD` instead (`SpellStats::from_constants`).
    fn file_suffix(self) -> &'static str {
        match self {
            SpellKind::Blast => "BLAST",
            SpellKind::Lightning => "BOLT",
            SpellKind::Whirlwind => "WWIND",
            SpellKind::Swarm => "PLAGUE",
            SpellKind::Invisibility => "INVIS",
            SpellKind::Hypnotism => "HYPNO",
            SpellKind::Firestorm => "FIREST",
            SpellKind::GhostArmy => "GARMY",
            SpellKind::Erosion => "EROSION",
            SpellKind::Swamp => "SWAMP",
            SpellKind::LandBridge => "LBRIDGE",
            SpellKind::AngelOfDeath => "AOD",
            SpellKind::Earthquake => "QUAKE",
            SpellKind::Flatten => "FLATTEN",
            SpellKind::Volcano => "VOLCANO",
            SpellKind::Convert => "CONVERT",
            SpellKind::Armageddon => "ARMAGEDDON",
            SpellKind::MagicalShield => "SHIELD",
            SpellKind::Bloodlust => "BLOODLUST",
            SpellKind::Teleport => "TELEPORT",
        }
    }
}

/// A spell's balance: the same for every player all match long (`constant.dat` is per-install, not
/// per-tribe, "Use here"). Resolved once when its `SpellSlot` is built (`default_for`), possibly corrected
/// from a real install's file straight after (`from_constants`, `SpellBook::apply_constants`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellStats {
    /// Mana needed to regain one charge. `[ours]`: kept on our own placeholder scale even when a real file
    /// loads (docs/specs/constants.md "Use here": generated mode keeps its own defaults, written by us, not
    /// copied from the file).
    pub cost: u32,
    /// The most charges this spell can hold (`SP_1_OFF_MAX_<X>`; big spells hold fewer).
    pub max_charges: u8,
    /// `SPELL_<X>_OPT_S` (spells.md "Original balance"): the original's own "advisor" pacing, in seconds, for
    /// this spell to reach full charge (`pop3-rev-analysis.md` "The mana bar (HUD)").
    pub advisor_seconds: u32,
    /// How far the shaman casts it at the middle altitude band (`SP_W_RANGE`, world units, `cast_range`);
    /// None: no limit (Teleport's 65 536 is the whole torus, Armageddon has none).
    pub range: Option<u32>,
}

impl SpellStats {
    /// Our own numbers, used until (or unless) a real `constant.dat` overrides them.
    pub fn default_for(kind: SpellKind) -> SpellStats {
        SpellStats { cost: default_cost(kind), max_charges: default_max_charges(kind), advisor_seconds: default_advisor_seconds(kind), range: default_range(kind) }
    }

    /// `default_for(kind)`, with each field replaced by `raw`'s own value where `kind`'s constant.dat key is
    /// present — so a partial or modded file only overrides what it actually defines (Armageddon and
    /// Bloodlust have no `SPELL_<X>` cost entry in the file at all: they simply keep their default cost).
    pub fn from_constants(kind: SpellKind, raw: &Constants) -> SpellStats {
        let mut stats = SpellStats::default_for(kind);
        let suffix = kind.file_suffix();
        let cost_suffix = if kind == SpellKind::Convert { "CONVERT_WILD" } else { suffix };
        if let Some(v) = raw.get(&format!("SPELL_{cost_suffix}")) {
            stats.cost = v;
        }
        if let Some(v) = raw.get(&format!("SP_1_OFF_MAX_{suffix}")) {
            stats.max_charges = v as u8;
        }
        if let Some(v) = raw.get(&format!("SPELL_{suffix}_OPT_S")) {
            stats.advisor_seconds = v;
        }
        if let Some(v) = raw.get(&format!("SP_W_RANGE_{suffix}")) {
            stats.range = Some(v);
        }
        stats
    }
}

/// `[ours]`: a placeholder scale, kept as the default for generated/no-original mode (`SpellStats::cost`).
fn default_cost(kind: SpellKind) -> u32 {
    match kind {
        SpellKind::Teleport => 1,
        SpellKind::Blast | SpellKind::Convert => 40,
        SpellKind::Swarm | SpellKind::Invisibility | SpellKind::LandBridge | SpellKind::GhostArmy => 120,
        SpellKind::MagicalShield | SpellKind::Bloodlust => 200,
        SpellKind::Hypnotism | SpellKind::Whirlwind | SpellKind::Lightning | SpellKind::Flatten => 200,
        SpellKind::Erosion | SpellKind::Swamp => 300,
        SpellKind::Earthquake | SpellKind::Firestorm => 500,
        SpellKind::AngelOfDeath | SpellKind::Volcano => 800,
        SpellKind::Armageddon => 1500,
    }
}

/// `SP_1_OFF_MAX_<X>` (spells.md "Original balance"), unconditionally: unlike `cost`, a charge count has no
/// "our own economy" interpretation, so there's no reason to keep the old cost-banded derivation (already
/// documented wrong for Whirlwind/Hypnotism/Swamp/Flatten) as a separate "no install" default.
fn default_max_charges(kind: SpellKind) -> u8 {
    match kind {
        SpellKind::Blast
        | SpellKind::Lightning
        | SpellKind::Swarm
        | SpellKind::Invisibility
        | SpellKind::LandBridge
        | SpellKind::Convert
        | SpellKind::MagicalShield
        | SpellKind::Bloodlust
        | SpellKind::Teleport
        | SpellKind::GhostArmy => MAX_CHARGES,
        SpellKind::Whirlwind | SpellKind::Hypnotism | SpellKind::Swamp | SpellKind::Flatten => 3,
        SpellKind::Firestorm | SpellKind::Erosion | SpellKind::Earthquake => 2,
        SpellKind::AngelOfDeath | SpellKind::Volcano | SpellKind::Armageddon => 1,
    }
}

fn default_advisor_seconds(kind: SpellKind) -> u32 {
    match kind {
        SpellKind::Blast => 30,
        SpellKind::GhostArmy | SpellKind::Armageddon => 60,
        SpellKind::Swarm => 45,
        SpellKind::Invisibility | SpellKind::MagicalShield | SpellKind::Bloodlust | SpellKind::Teleport => 120,
        SpellKind::Lightning | SpellKind::Whirlwind | SpellKind::Firestorm | SpellKind::Swamp | SpellKind::LandBridge => 180,
        SpellKind::Hypnotism | SpellKind::Erosion | SpellKind::Earthquake | SpellKind::Flatten => 240,
        SpellKind::AngelOfDeath | SpellKind::Volcano | SpellKind::Convert => 300,
    }
}

fn default_range(kind: SpellKind) -> Option<u32> {
    match kind {
        SpellKind::Teleport | SpellKind::Armageddon => None,
        SpellKind::Blast | SpellKind::AngelOfDeath | SpellKind::Volcano => Some(3072),
        SpellKind::LandBridge => Some(5120),
        SpellKind::Lightning | SpellKind::Swarm => Some(6144),
        SpellKind::Convert => Some(8192),
        SpellKind::Whirlwind
        | SpellKind::Invisibility
        | SpellKind::Hypnotism
        | SpellKind::Firestorm
        | SpellKind::GhostArmy
        | SpellKind::Erosion
        | SpellKind::Swamp
        | SpellKind::Earthquake
        | SpellKind::Flatten
        | SpellKind::MagicalShield
        | SpellKind::Bloodlust => Some(4096),
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
    /// Mana accumulated toward the next charge, `0..stats.cost`.
    pub recharge: u32,
    /// Switched off by the player: it takes no mana and keeps its charges (known spells only).
    pub paused: bool,
    /// This spell's balance, resolved once at construction (`SpellStats::default_for`), possibly corrected
    /// afterwards by `SpellBook::apply_constants` from a real install.
    pub stats: SpellStats,
    /// What a level's own discovery thing said this spell becomes once found (permanent / this level / once);
    /// `None` outside `from_level` (`demo()`, `sandbox()`, ...), which keeps today's "discovering always gives
    /// `Known`" behaviour. Read only by `discover` (spells.md "Spell book").
    pub discovery_availability: Option<pop3_format::level::Availability>,
}

impl SpellSlot {
    pub fn new(kind: SpellKind, availability: Availability) -> Self {
        SpellSlot {
            kind,
            availability,
            charges: 0,
            recharge: 0,
            paused: false,
            stats: SpellStats::default_for(kind),
            discovery_availability: None,
        }
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
        self.availability == Availability::Known && self.charges < self.stats.max_charges
    }

    pub fn takes_mana(&self) -> bool {
        self.is_recharging() && !self.paused
    }

    /// Adds `mana` toward the next charges; returns what is left once full.
    fn give(&mut self, mana: u32) -> u32 {
        self.recharge += mana;
        while self.recharge >= self.stats.cost && self.charges < self.stats.max_charges {
            self.recharge -= self.stats.cost;
            self.charges += 1;
        }
        if self.charges < self.stats.max_charges {
            return 0;
        }
        std::mem::take(&mut self.recharge)
    }
}

/// Whether the header's `SpellsAvailable` gives `kind` from the start. Armageddon only counts next to
/// Convert: it is the campaign's last spell (discovered in levels 17 and 18, in the masks from 19 on,
/// always with Convert), while levels 1 and 2 carry bit 18 among leftover bits (burn, bloodlust,
/// teleport) and no Convert.
pub fn known_in_header(header: &LevelHeader, kind: SpellKind) -> bool {
    let set = |k: SpellKind| header.spell_available(k.model());
    // Bit 20 is among the leftovers of levels 1 and 2 too: bloodlust only comes from discoveries (to check).
    kind != SpellKind::Bloodlust && set(kind) && (kind != SpellKind::Armageddon || set(SpellKind::Convert))
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
        // Keyed by `SpellKind`, not file order: a level with two discovery things for the same spell (not seen
        // in the campaign) would otherwise depend on which one `things` lists first.
        let discoveries: std::collections::HashMap<SpellKind, pop3_format::level::Availability> = level
            .things
            .iter()
            .filter_map(|t| match t.data() {
                ThingData::Discovery(d) if d.kind == KIND_SPELL => {
                    SpellKind::from_model(d.model).map(|k| (k, d.availability))
                }
                _ => None,
            })
            .collect();
        for (&kind, &availability) in &discoveries {
            book.set(kind, Availability::Discoverable);
            if let Some(s) = book.slots.iter_mut().find(|s| s.kind == kind) {
                s.discovery_availability = Some(availability);
            }
        }
        for kind in SpellKind::ALL.iter().copied().filter(|k| SpellKind::from_model(k.model()) == Some(*k)) {
            if known_in_header(header, kind) {
                book.set(kind, Availability::Known);
            }
        }
        book.fill();
        book
    }

    /// Loadout showing every tile state, for maps without one of their own.
    pub fn demo() -> Self {
        let mut b = SpellBook::new();
        for k in [SpellKind::Blast, SpellKind::Convert, SpellKind::Swarm, SpellKind::LandBridge, SpellKind::Erosion] {
            b.set(k, Availability::Known);
        }
        b.set(SpellKind::Invisibility, Availability::Provided { shots: 3 });
        b.set(SpellKind::Volcano, Availability::Provided { shots: 1 });
        b.set(SpellKind::Flatten, Availability::Discoverable);
        b.set(SpellKind::Lightning, Availability::Discoverable);
        b.charge(Mana(500));
        b
    }

    /// The sandboxes': the demo loadout plus Teleport, Flatten and Lightning, free.
    pub fn sandbox() -> Self {
        let mut b = SpellBook::demo();
        for k in [SpellKind::Teleport, SpellKind::Flatten, SpellKind::Lightning] {
            b.set(k, Availability::Unlimited);
        }
        b
    }

    /// Every spell, Teleport too, cast at will (Sandbox > Spells); not Bloodlust, which would hide Ghost Army's tile
    /// (`STAND_INS`, hud/spells.rs) in a loadout meant to show every tile at once.
    pub fn unlimited() -> Self {
        let mut b = SpellBook::new();
        for k in SpellKind::ALL.into_iter().filter(|&k| k != SpellKind::Bloodlust) {
            b.set(k, Availability::Unlimited);
        }
        b
    }

    /// Every known spell at full charges.
    pub fn fill(&mut self) {
        for s in self.slots.iter_mut().filter(|s| s.availability == Availability::Known) {
            s.charges = s.stats.max_charges;
            s.recharge = 0;
        }
    }

    /// Corrects every slot's `stats` from a real install's `constant.dat` (`SpellStats::from_constants`),
    /// called once right after construction (`GameMap::load_original`); callers should `fill()` again
    /// afterwards so `charges` reflects any corrected `max_charges` before play starts.
    pub fn apply_constants(&mut self, raw: &Constants) {
        for s in self.slots.iter_mut() {
            s.stats = SpellStats::from_constants(s.kind, raw);
        }
    }

    /// A deterministic hash of every spell's resolved `cost`/`max_charges` (spells.md "Mana"): the two
    /// `SpellStats` fields that actually feed simulation state (`SpellBook::charge`/`cast`, lockstep-replayed),
    /// unlike `advisor_seconds` (HUD mana-bar pacing only) or `range` (checked locally by the caster, never
    /// compared across peers) - a mismatch in either of those can't desync a match, so they're left out to
    /// avoid a false "mismatch" refusal over a difference that wouldn't actually diverge anything. Two peers
    /// with a different install (or one with none) resolve `SpellStats` independently
    /// (`GameMap::load_original`'s `apply_constants`, or the shared code-level defaults): comparing this at
    /// game start (`game_net::Message::Handshake`) is meant to catch that before it silently desyncs mid-match
    /// (TODO.md "Mana"). Iterates `SpellKind::ALL` in its own fixed declaration order, not `self.slots`' own
    /// order or any `HashMap`, so it never depends on construction order (AGENTS.md's determinism rule).
    /// `DefaultHasher::new()` always starts from the same fixed keys (unlike `RandomState`, which is
    /// per-process-random for `HashMap`), so this value is stable across peers and runs, not just within one.
    pub fn constants_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for kind in SpellKind::ALL {
            let stats = self.slot(kind).map_or_else(|| SpellStats::default_for(kind), |s| s.stats);
            stats.cost.hash(&mut hasher);
            stats.max_charges.hash(&mut hasher);
        }
        hasher.finish()
    }

    pub fn slot(&self, kind: SpellKind) -> Option<&SpellSlot> {
        self.slots.iter().find(|s| s.kind == kind)
    }

    pub fn set(&mut self, kind: SpellKind, availability: Availability) {
        if let Some(s) = self.slots.iter_mut().find(|s| s.kind == kind) {
            s.availability = availability;
        }
    }

    /// Stops or resumes a known spell's recharge (`Command::PauseSpell`).
    pub fn set_paused(&mut self, kind: SpellKind, paused: bool) {
        if let Some(s) = self.slots.iter_mut().find(|s| s.kind == kind && s.availability == Availability::Known) {
            s.paused = paused;
        }
    }

    /// Applies the discovery's own availability (spells.md "Spell book"): a `Once` discovery is a single free
    /// use (`Provided { shots: 1 }`); `Permanent`/`ThisLevel`/an unseen byte/no discovery data at all (`demo()`,
    /// `sandbox()`, ...) all give `Known` — Permanent and ThisLevel are indistinguishable today, with no
    /// cross-level save/progression system to tell them apart.
    pub fn discover(&mut self, kind: SpellKind) {
        let Some(s) = self.slots.iter_mut().find(|s| s.kind == kind) else { return };
        if s.availability != Availability::Discoverable {
            return;
        }
        s.availability = match s.discovery_availability {
            Some(pop3_format::level::Availability::Once) => Availability::Provided { shots: 1 },
            Some(pop3_format::level::Availability::Permanent)
            | Some(pop3_format::level::Availability::ThisLevel)
            | Some(pop3_format::level::Availability::Unknown(_))
            | None => Availability::Known,
        };
    }

    /// The spell the mana bar's "advisor" pacing estimate is read from: the first charging spell
    /// (`SpellSlot::takes_mana`), in book order — the same set `charge` splits mana between
    /// (`pop3-rev-analysis.md` "The mana bar (HUD)": "pick a candidate spell the same way the distribution
    /// loop itself does").
    pub fn charging_candidate(&self) -> Option<SpellStats> {
        self.slots.iter().find(|s| s.takes_mana()).map(|s| s.stats)
    }

    /// Share `mana` evenly between the spells that take it (`SpellSlot::takes_mana`); a spell filled up passes
    /// the rest of its share on. Returns what no spell took.
    pub fn charge(&mut self, mana: Mana) -> Mana {
        let mut left = mana.0;
        loop {
            let takers: Vec<usize> = (0..self.slots.len()).filter(|&i| self.slots[i].takes_mana()).collect();
            if left == 0 || takers.is_empty() {
                return Mana(left);
            }
            let share = (left / takers.len() as u32).max(1);
            for i in takers {
                let given = share.min(left);
                left = left - given + self.slots[i].give(given);
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
        b.charge(Mana(39));
        assert_eq!(b.slot(SpellKind::Blast).unwrap().charges, 0);
        b.charge(Mana(1));
        assert_eq!(b.slot(SpellKind::Blast).unwrap().charges, 1);
        b.charge(Mana(10_000));
        let s = b.slot(SpellKind::Blast).unwrap();
        assert_eq!((s.charges, s.recharge), (MAX_CHARGES, 0));
    }

    #[test]
    fn ranges_from_the_original_table() {
        let range = |k| SpellStats::default_for(k).range;
        assert_eq!(range(SpellKind::Blast), Some(3072));
        assert_eq!(range(SpellKind::LandBridge), Some(5120));
        assert_eq!(range(SpellKind::Convert), Some(8192));
        assert_eq!(range(SpellKind::Teleport), None);
        assert!(SpellKind::LandBridge.has_origin() && !SpellKind::Blast.has_origin());
    }

    #[test]
    fn original_model_numbers() {
        assert_eq!(SpellKind::from_model(2), Some(SpellKind::Blast));
        assert_eq!(SpellKind::from_model(17), Some(SpellKind::Convert));
        assert_eq!(SpellKind::from_model(19), Some(SpellKind::MagicalShield));
        assert_eq!(SpellKind::from_model(20), Some(SpellKind::Bloodlust), "a level's discovery may give it");
        for model in [0, 1, 21, 22] {
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
        let thing = |dat: &mut Vec<u8>, slot: usize, kind: u8, model: u8, availability_byte: u8| {
            let t = 81_987 + slot * 55;
            dat[t..t + 2].copy_from_slice(&[pop3_format::level::GENERAL_DISCOVERY, pop3_format::level::KIND_GENERAL]);
            dat[t + 7..t + 10].copy_from_slice(&[kind, model, availability_byte]);
        };
        thing(&mut dat, 0, KIND_SPELL, 17, 3);
        thing(&mut dat, 1, KIND_SPELL, 5, 1);
        thing(&mut dat, 2, 2, 4, 3);
        let book = SpellBook::from_level(&header, &Level::parse(&dat).unwrap());
        let avail = |k| book.slot(k).unwrap().availability;
        assert_eq!(avail(SpellKind::Blast), Availability::Known);
        assert_eq!(book.slot(SpellKind::Blast).unwrap().charges, MAX_CHARGES, "starts charged");
        assert_eq!(avail(SpellKind::Swarm), Availability::Known, "known wins over its discovery");
        assert_eq!(avail(SpellKind::Convert), Availability::Discoverable);
        assert_eq!(
            book.slot(SpellKind::Convert).unwrap().discovery_availability,
            Some(pop3_format::level::Availability::Once),
            "availability byte 3, carried through even though the slot is still Discoverable"
        );
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
        b.charge(Mana(40));
        b.set_paused(SpellKind::Blast, true);
        b.charge(Mana(1000));
        let s = b.slot(SpellKind::Blast).unwrap();
        assert_eq!((s.charges, s.recharge, s.paused), (1, 0, true));
        assert!(b.cast(SpellKind::Blast), "a paused spell can still be cast");
        b.set_paused(SpellKind::Blast, false);
        b.charge(Mana(40));
        assert_eq!(b.slot(SpellKind::Blast).unwrap().charges, 1, "recharging again");
        b.set(SpellKind::Volcano, Availability::Provided { shots: 1 });
        b.set_paused(SpellKind::Volcano, true);
        assert!(!b.slot(SpellKind::Volcano).unwrap().paused, "only known spells recharge");
    }

    #[test]
    fn apply_constants_updates_every_slot_in_the_book() {
        let mut b = SpellBook::new();
        let raw = Constants::parse(b"P3CONST_SPELL_BLAST = 55555\n").unwrap();
        b.apply_constants(&raw);
        assert_eq!(b.slot(SpellKind::Blast).unwrap().stats.cost, 55_555);
        assert_eq!(b.slot(SpellKind::Swamp).unwrap().stats, SpellStats::default_for(SpellKind::Swamp), "no key for it: default kept");
    }

    #[test]
    fn constants_hash_agrees_for_two_books_built_the_same_way() {
        assert_eq!(SpellBook::new().constants_hash(), SpellBook::new().constants_hash());
        assert_eq!(SpellBook::demo().constants_hash(), SpellBook::demo().constants_hash());
    }

    #[test]
    fn constants_hash_changes_when_a_cost_or_a_charge_count_does_but_not_otherwise() {
        let base = SpellBook::new().constants_hash();
        let mut cost_changed = SpellBook::new();
        cost_changed.apply_constants(&Constants::parse(b"P3CONST_SPELL_BLAST = 55555\n").unwrap());
        assert_ne!(cost_changed.constants_hash(), base, "a changed cost must be caught");

        let mut charges_changed = SpellBook::new();
        charges_changed.apply_constants(&Constants::parse(b"P3CONST_SP_1_OFF_MAX_BLAST = 1\n").unwrap());
        assert_ne!(charges_changed.constants_hash(), base, "a changed charge count must be caught");

        let mut advisor_changed = SpellBook::new();
        advisor_changed.apply_constants(&Constants::parse(b"P3CONST_SPELL_BLAST_OPT_S = 1\n").unwrap());
        assert_eq!(advisor_changed.constants_hash(), base, "advisor pacing alone can't desync, left out of the hash");

        let mut range_changed = SpellBook::new();
        range_changed.apply_constants(&Constants::parse(b"P3CONST_SP_W_RANGE_BLAST = 1\n").unwrap());
        assert_eq!(range_changed.constants_hash(), base, "cast range alone can't desync, left out of the hash");
    }

    #[test]
    fn constants_hash_does_not_depend_on_the_order_apply_constants_is_called_in() {
        let raw = Constants::parse(b"P3CONST_SPELL_BLAST = 55555\nP3CONST_SPELL_SWARM = 1\n").unwrap();
        let mut a = SpellBook::new();
        a.apply_constants(&raw);
        // A book with the same resolved stats, reached by a different path (two separate corrections instead
        // of one), must hash the same: the hash is keyed by `SpellKind`, not by how `slots` got built.
        let mut b = SpellBook::new();
        b.apply_constants(&Constants::parse(b"P3CONST_SPELL_SWARM = 1\n").unwrap());
        b.apply_constants(&raw);
        assert_eq!(a.constants_hash(), b.constants_hash());
    }

    #[test]
    fn mana_is_shared_and_a_full_spell_passes_its_share_on() {
        let mut b = SpellBook::new();
        b.set(SpellKind::Blast, Availability::Known);
        b.set(SpellKind::Swarm, Availability::Known);
        assert_eq!(b.charge(Mana(81)), Mana::ZERO);
        let (blast, swarm) = (b.slot(SpellKind::Blast).unwrap(), b.slot(SpellKind::Swarm).unwrap());
        assert_eq!((blast.charges, blast.recharge, swarm.recharge), (1, 1, 40), "41 and 40, the odd one to the first");
        assert_eq!(b.charge(Mana(1000)), Mana(1000 - 119 - 440), "both full: 4 x 40 and 4 x 120");
        assert!(b.slots.iter().all(|s| s.recharge == 0));
    }

    #[test]
    fn big_spells_hold_fewer_charges() {
        let max_charges = |k| SpellStats::default_for(k).max_charges;
        assert_eq!(max_charges(SpellKind::Armageddon), 1);
        assert_eq!(max_charges(SpellKind::Swamp), 3, "SP_1_OFF_MAX_SWAMP, not the old cost-banded guess of 2");
    }

    #[test]
    fn from_constants_overrides_only_the_keys_a_file_actually_has() {
        let raw = Constants::parse(b"P3CONST_SPELL_BLAST = 99999\nP3CONST_SP_1_OFF_MAX_BLAST = 2\n").unwrap();
        let stats = SpellStats::from_constants(SpellKind::Blast, &raw);
        let default = SpellStats::default_for(SpellKind::Blast);
        assert_eq!((stats.cost, stats.max_charges), (99_999, 2), "both keys present, both overridden");
        assert_eq!((stats.advisor_seconds, stats.range), (default.advisor_seconds, default.range), "no key: kept default");
    }

    #[test]
    fn convert_reads_its_cost_under_its_own_wild_suffix() {
        let raw = Constants::parse(b"P3CONST_SPELL_CONVERT_WILD = 42\nP3CONST_SPELL_CONVERT = 7777\n").unwrap();
        assert_eq!(SpellStats::from_constants(SpellKind::Convert, &raw).cost, 42, "SPELL_CONVERT_WILD, not SPELL_CONVERT");
    }

    #[test]
    fn a_spell_absent_from_the_file_keeps_every_default() {
        let raw = Constants::parse(b"P3CONST_SPELL_BLAST = 1\n").unwrap();
        assert_eq!(SpellStats::from_constants(SpellKind::Armageddon, &raw), SpellStats::default_for(SpellKind::Armageddon));
    }

    #[test]
    fn provided_shots_run_out_and_never_recharge() {
        let mut b = SpellBook::new();
        b.set(SpellKind::Volcano, Availability::Provided { shots: 2 });
        b.charge(Mana(10_000));
        assert!(b.cast(SpellKind::Volcano));
        assert!(b.cast(SpellKind::Volcano));
        assert_eq!(b.slot(SpellKind::Volcano).unwrap().availability, Availability::Hidden);
        assert!(!b.cast(SpellKind::Volcano));
    }

    #[test]
    fn discoverable_must_be_discovered_first() {
        let mut b = SpellBook::new();
        b.set(SpellKind::Flatten, Availability::Discoverable);
        b.charge(Mana(10_000));
        assert!(!b.cast(SpellKind::Flatten));
        b.discover(SpellKind::Flatten);
        b.charge(Mana(200));
        assert!(b.cast(SpellKind::Flatten));
    }

    #[test]
    fn discover_applies_the_level_s_own_discovery_availability() {
        let discoverable_with = |kind, availability| {
            let mut b = SpellBook::new();
            b.set(kind, Availability::Discoverable);
            b.slots.iter_mut().find(|s| s.kind == kind).unwrap().discovery_availability = availability;
            b
        };
        let mut once = discoverable_with(SpellKind::Flatten, Some(pop3_format::level::Availability::Once));
        once.discover(SpellKind::Flatten);
        assert_eq!(once.slot(SpellKind::Flatten).unwrap().availability, Availability::Provided { shots: 1 });

        let mut permanent = discoverable_with(SpellKind::Lightning, Some(pop3_format::level::Availability::Permanent));
        permanent.discover(SpellKind::Lightning);
        assert_eq!(permanent.slot(SpellKind::Lightning).unwrap().availability, Availability::Known);

        let mut this_level = discoverable_with(SpellKind::Swarm, Some(pop3_format::level::Availability::ThisLevel));
        this_level.discover(SpellKind::Swarm);
        assert_eq!(
            this_level.slot(SpellKind::Swarm).unwrap().availability,
            Availability::Known,
            "Permanent and ThisLevel are indistinguishable today: no cross-level state to tell them apart"
        );
    }

    #[test]
    fn teleport_is_free_in_the_sandboxes_only() {
        let teleport = SpellBook::sandbox().slot(SpellKind::Teleport).cloned().unwrap();
        assert!(teleport.can_cast() && teleport.availability == Availability::Unlimited);
        assert_eq!(SpellBook::demo().slot(SpellKind::Teleport).unwrap().availability, Availability::Hidden);
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
        b.charge(Mana(80));
        assert!(b.cast(SpellKind::Convert));
        assert_eq!(b.slot(SpellKind::Convert).unwrap().charges, 1);
    }

    #[test]
    fn unlimited_casts_every_spell_at_will() {
        let mut b = SpellBook::unlimited();
        for k in SpellKind::ALL.into_iter().filter(|&k| k != SpellKind::Bloodlust) {
            assert!(b.cast(k) && b.cast(k), "{k:?}");
        }
        assert!(!b.cast(SpellKind::Bloodlust), "left out: it would hide Ghost Army's tile");
    }
}
