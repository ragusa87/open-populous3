//! A tribe's spells: which ones it knows, their charges and mana recharge.
//! Deterministic integer state, ticked by the simulation.

pub const MAX_CHARGES: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// Sandbox spell (not in the original): moves the shaman to any walkable spot.
    Teleport,
}

impl SpellKind {
    pub const ALL: [SpellKind; 17] = [
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
        SpellKind::Teleport,
    ];

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
            SpellKind::Teleport => "Teleportation",
        }
    }

    /// Mana needed to regain one charge.
    pub fn cost(self) -> u32 {
        match self {
            SpellKind::Teleport => 1,
            SpellKind::Blast | SpellKind::Convert => 40,
            SpellKind::Swarm | SpellKind::Invisibility | SpellKind::LandBridge => 120,
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
}

impl SpellSlot {
    pub fn new(kind: SpellKind, availability: Availability) -> Self {
        SpellSlot { kind, availability, charges: 0, recharge: 0 }
    }

    pub fn can_cast(&self) -> bool {
        match self.availability {
            Availability::Known => self.charges > 0,
            Availability::Provided { shots } => shots > 0,
            Availability::Unlimited => true,
            _ => false,
        }
    }

    pub fn is_recharging(&self) -> bool {
        self.availability == Availability::Known && self.charges < self.kind.max_charges()
    }
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

    pub fn slot(&self, kind: SpellKind) -> Option<&SpellSlot> {
        self.slots.iter().find(|s| s.kind == kind)
    }

    pub fn set(&mut self, kind: SpellKind, availability: Availability) {
        if let Some(s) = self.slots.iter_mut().find(|s| s.kind == kind) {
            s.availability = availability;
        }
    }

    pub fn discover(&mut self, kind: SpellKind) {
        if self.slot(kind).is_some_and(|s| s.availability == Availability::Discoverable) {
            self.set(kind, Availability::Known);
        }
    }

    /// Give `mana` to every recharging spell; full charges keep the rest at 0.
    pub fn tick(&mut self, mana: u32) {
        for s in self.slots.iter_mut().filter(|s| s.is_recharging()) {
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
