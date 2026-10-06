//! A playable map: terrain + metadata, built from an original level or generated.

use crate::command::Command;
use crate::site::{generated_sites, sites_from_level, ReincarnationSite};
use crate::terrain::{DirtyRect, Heightmap, MAX_HEIGHT};
use crate::unit::{Order, Unit, UnitEvent};
use pop3_format::{Level, LevelHeader, MAP_SIZE};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct GameMap {
    pub name: String,
    /// Original landscape theme index (`pop3_format::theme_char`), None for generated maps.
    pub theme: Option<u8>,
    pub terrain: Heightmap,
    /// One per tribe, sorted by owner.
    pub sites: Vec<ReincarnationSite>,
    /// One shaman per site for now, in site order.
    pub units: Vec<Unit>,
}

impl GameMap {
    pub fn from_level(level: &Level, name: impl Into<String>, theme: Option<u8>) -> Self {
        GameMap {
            name: name.into(),
            theme,
            terrain: Heightmap::from_heights(MAP_SIZE, level.heights.clone()),
            sites: sites_from_level(level),
            units: Vec::new(),
        }
        .with_shamans()
    }

    /// Shamans spawn at game start: each site levels its ground (see `flatten_for_spawn`).
    fn with_shamans(mut self) -> Self {
        for (i, site) in self.sites.iter().enumerate() {
            site.flatten_for_spawn(&mut self.terrain);
            self.units.push(Unit::shaman(i as u32 + 1, site));
        }
        self
    }

    /// Load `levlXXXX.dat`, picking the name from the sibling `.hdr` if present.
    pub fn load_original(dat: &Path) -> Result<Self, pop3_format::LevelError> {
        let level = Level::load(dat)?;
        let header = LevelHeader::load(dat.with_extension("hdr")).ok();
        let name = header
            .as_ref()
            .map(|h| h.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| dat.file_stem().unwrap_or_default().to_string_lossy().into_owned());
        Ok(Self::from_level(&level, name, header.map(|h| h.theme)))
    }

    /// Deterministic island map from a seed (fallback when no original data exists).
    pub fn generate(seed: u32) -> Self {
        let mut rng = Lcg(seed.max(1));
        let mut terrain = Heightmap::new(MAP_SIZE);
        for _ in 0..40 {
            let c = ((rng.next() % MAP_SIZE as u32) as i32, (rng.next() % MAP_SIZE as u32) as i32);
            let radius = 4 + (rng.next() % 14) as i32;
            let amount = 150 + (rng.next() % 350) as i32;
            terrain.raise(c, radius, amount);
        }
        for h in 0..MAP_SIZE * MAP_SIZE {
            let (x, z) = ((h % MAP_SIZE) as i32, (h / MAP_SIZE) as i32);
            let v = terrain.get(x, z).saturating_sub(120).min(MAX_HEIGHT);
            terrain.set(x, z, v);
        }
        let sites = generated_sites(&terrain);
        GameMap { name: format!("Generated #{seed}"), theme: None, terrain, sites, units: Vec::new() }.with_shamans()
    }
}

impl GameMap {
    pub fn site_of(&self, owner: u8) -> Option<&ReincarnationSite> {
        self.sites.iter().find(|s| s.owner == owner)
    }

    pub fn shaman_of(&self, owner: u8) -> Option<&Unit> {
        self.units.iter().find(|u| u.owner == owner)
    }

    /// Apply a player's command; returns the terrain region it changed, if any.
    /// Casting a spell also makes the caster's shaman do her cast jump.
    pub fn apply(&mut self, command: &Command) -> Option<DirtyRect> {
        match *command {
            Command::Cast { player, spell } => {
                self.order(player, Order::Cast);
                Some(spell.cast(&mut self.terrain))
            }
            Command::Order { player, order } => {
                self.order(player, order);
                None
            }
            Command::OrderUnit { player, unit, order } => {
                if let Some(u) = self.units.iter_mut().find(|u| u.id == unit && u.owner == player) {
                    u.order(order);
                }
                None
            }
        }
    }

    fn order(&mut self, player: u8, order: Order) {
        if let Some(u) = self.units.iter_mut().find(|u| u.owner == player) {
            u.order(order);
        }
    }

    /// One simulation tick for every unit, in order; returns the ground levelled by reincarnations.
    pub fn tick(&mut self) -> Vec<DirtyRect> {
        let mut dirty = Vec::new();
        for unit in &mut self.units {
            let site = self.sites.iter().find(|s| s.owner == unit.owner);
            if unit.tick(&self.terrain, site) == Some(UnitEvent::Reincarnated) {
                dirty.extend(site.map(|s| s.flatten_for_spawn(&mut self.terrain)));
            }
        }
        dirty
    }
}

/// Tiny deterministic PRNG, same sequence on every platform.
pub struct Lcg(pub u32);

impl Lcg {
    pub fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0 >> 8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic_and_has_land_and_sea() {
        let a = GameMap::generate(42);
        assert_eq!(a.terrain, GameMap::generate(42).terrain);
        let hs = a.terrain.heights();
        assert!(hs.iter().any(|&h| h == 0));
        assert!(hs.iter().any(|&h| h > 100));
        assert_eq!(a.sites, GameMap::generate(42).sites);
        assert!(a.site_of(0).is_some());
        for s in &a.sites {
            assert!(!a.terrain.is_water(s.cell().0, s.cell().1), "spawn ground is land");
        }
    }

    #[test]
    fn a_shaman_stands_on_each_site() {
        let m = GameMap::generate(7);
        assert_eq!(m.units.len(), m.sites.len());
        for s in &m.sites {
            let u = m.shaman_of(s.owner).unwrap();
            assert_eq!((u.x, u.z), s.spawn_point());
        }
    }

    #[test]
    fn commands_drive_the_shaman_and_ticks_replay_identically() {
        use crate::unit::Action;
        let run = || {
            let mut m = GameMap::generate(7);
            let (x, z) = (m.units[0].x.wrapping_add(512), m.units[0].z);
            m.apply(&Command::Order { player: 0, order: Order::MoveTo { x, z } });
            for _ in 0..3 {
                m.tick();
            }
            m.apply(&Command::Order { player: 0, order: Order::Pray });
            m.tick();
            m
        };
        let (a, b) = (run(), run());
        assert_eq!(a.units, b.units);
        assert_eq!(a.units[0].action, Action::Praying);
        assert!(a.units[0].x != a.sites[0].x, "she moved");
    }

    #[test]
    fn unit_orders_only_reach_the_owners_units() {
        use crate::unit::Action;
        let mut m = GameMap::generate(7);
        let (mine, theirs) = (m.units[0].id, m.units[1].id);
        m.apply(&Command::OrderUnit { player: 0, unit: theirs, order: Order::Pray });
        assert_eq!(m.units[1].action, Action::Idle);
        m.apply(&Command::OrderUnit { player: 0, unit: mine, order: Order::Pray });
        assert_eq!(m.units[0].action, Action::Praying);
    }
}
