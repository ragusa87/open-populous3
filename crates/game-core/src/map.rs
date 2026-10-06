//! A playable map: terrain + metadata, built from an original level or generated.

use crate::command::Command;
use crate::path::Mobility;
use crate::slots;
use crate::spell::Spell;
use std::collections::BTreeSet;
use crate::tree::{scatter, Tree};
use crate::site::{generated_sites, sites_from_level, ReincarnationSite};
use crate::terrain::{DirtyRect, Heightmap, MAX_HEIGHT};
use crate::unit::{Action, Order, Unit, UnitEvent, UnitKind};
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
    /// Wood on the map (none on original levels until their things are decoded).
    pub trees: Vec<Tree>,
}

impl GameMap {
    pub fn from_level(level: &Level, name: impl Into<String>, theme: Option<u8>) -> Self {
        GameMap {
            name: name.into(),
            theme,
            terrain: Heightmap::from_heights(MAP_SIZE, level.heights.clone()),
            sites: sites_from_level(level),
            units: Vec::new(),
            trees: Vec::new(),
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

    /// Groves of trees from `seed`, clear of the sites (after their ground is levelled).
    fn with_trees(mut self, seed: u32, groves: usize) -> Self {
        let sites: Vec<(i32, i32)> = self.sites.iter().map(|s| s.cell()).collect();
        self.trees = scatter(&self.terrain, seed, &sites, groves);
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
        GameMap { name: format!("Generated #{seed}"), theme: None, terrain, sites, units: Vec::new(), trees: Vec::new() }.with_shamans().with_trees(seed, 60)
    }

    /// Test ground for walking: a small flat island around the player's site at the centre, a gentle
    /// ramp to the east (+x), a steep hill to the north (-z), a lake to the west and a mesa ringed by
    /// cliffs to the south-east (clear of the camera, which starts south of the site), each a few cells past the spawn platform so they are quick to reach.
    pub fn sandbox_walk() -> Self {
        const C: i32 = MAP_SIZE as i32 / 2;
        const ISLAND: i32 = 22;
        const BASE: i32 = 64;
        let mut terrain = Heightmap::new(MAP_SIZE);
        for z in 0..MAP_SIZE as i32 {
            for x in 0..MAP_SIZE as i32 {
                let (dx, dz) = (x - C, z - C);
                if dx * dx + dz * dz > ISLAND * ISLAND {
                    continue;
                }
                let ramp = if (6..=12).contains(&dx) && dz.abs() <= 5 { (dx - 5) * 30 } else if (13..=16).contains(&dx) && dz.abs() <= 5 { 210 } else { 0 };
                let peak = (dx * dx + (dz + 10) * (dz + 10)) as u32;
                let hill = 500 - 100 * crate::unit::isqrt(peak) as i32;
                let lake = (dx + 9) * (dx + 9) + dz * dz <= 9;
                let mesa = if (8..=11).contains(&dz) && (6..=10).contains(&dx) { 400 } else { 0 };
                let h = if lake { 0 } else { BASE + ramp.max(hill).max(mesa).max(0) };
                terrain.set(x, z, h as u16);
            }
        }
        let sites = vec![ReincarnationSite::at_cell(0, (C, C))];
        GameMap { name: "Sandbox: walk".into(), theme: None, terrain, sites, units: Vec::new(), trees: Vec::new() }.with_shamans().with_trees(1, 150)
    }

    /// Test ground for units: a flat island with the player's site and shaman at the centre, three
    /// of every other kind for the player in columns to the west (one column per kind), one of each
    /// for tribe 1 (red, no shaman) in a row to the east, all in view of the starting camera (south
    /// of the site), and a pond to the north to walk around.
    pub fn sandbox_units() -> Self {
        const C: i32 = MAP_SIZE as i32 / 2;
        const ISLAND: i32 = 24;
        let mut terrain = Heightmap::new(MAP_SIZE);
        for z in 0..MAP_SIZE as i32 {
            for x in 0..MAP_SIZE as i32 {
                let (dx, dz) = (x - C, z - C);
                let pond = dx * dx + (dz + 9) * (dz + 9) <= 9;
                if dx * dx + dz * dz <= ISLAND * ISLAND && !pond {
                    terrain.set(x, z, 64);
                }
            }
        }
        let sites = vec![ReincarnationSite::at_cell(0, (C, C))];
        let mut map = GameMap { name: "Sandbox: units".into(), theme: None, terrain, sites, units: Vec::new(), trees: Vec::new() }.with_shamans().with_trees(2, 150);
        let at = |dx: i32, dz: i32| ((C + dx) as u16 * 512 + 256, (C + dz) as u16 * 512 + 256);
        for (row, &kind) in UnitKind::ALL[1..].iter().enumerate() {
            let row = row as i32;
            for n in 0..3 {
                let id = map.units.len() as u32 + 1;
                map.units.push(Unit::new(id, 0, kind, at(-5 - 2 * row, 2 * n - 2)));
            }
            let id = map.units.len() as u32 + 1;
            map.units.push(Unit::new(id, 1, kind, at(6 + 2 * row, 0)));
        }
        map
    }
}

impl GameMap {
    pub fn site_of(&self, owner: u8) -> Option<&ReincarnationSite> {
        self.sites.iter().find(|s| s.owner == owner)
    }

    pub fn shaman_of(&self, owner: u8) -> Option<&Unit> {
        self.units.iter().find(|u| u.owner == owner && u.kind == UnitKind::Shaman)
    }

    fn shaman_mut(&mut self, owner: u8) -> Option<&mut Unit> {
        self.units.iter_mut().find(|u| u.owner == owner && u.kind == UnitKind::Shaman)
    }

    /// Whether `player` can cast `spell` here and now: Teleport needs a living shaman and ground
    /// she can walk at the target; other spells always apply.
    pub fn can_cast(&self, player: u8, spell: &Spell) -> bool {
        match *spell {
            Spell::Teleport { to } => {
                let cell = ((to.0 as u32 / 512) as i32, (to.1 as u32 / 512) as i32);
                self.shaman_of(player).is_some_and(Unit::is_alive) && Mobility::Walk.passable(&self.terrain, cell)
            }
            _ => true,
        }
    }

    /// Apply a player's command; returns the terrain region it changed, if any. Casting a spell
    /// makes the caster's shaman do her cast jump (Teleport: she moves when it ends); a spell that
    /// cannot be cast (`can_cast`) does nothing.
    pub fn apply(&mut self, command: &Command) -> Option<DirtyRect> {
        match *command {
            Command::Cast { player, spell } if !self.can_cast(player, &spell) => None,
            Command::Cast { player, spell: Spell::Teleport { to } } => {
                if let Some(u) = self.shaman_mut(player) {
                    u.cast_teleport(to);
                }
                None
            }
            Command::Cast { player, spell } => {
                self.order(player, Order::Cast);
                spell.cast(&mut self.terrain)
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

    /// An order to the player's shaman.
    fn order(&mut self, player: u8, order: Order) {
        if let Some(u) = self.shaman_mut(player) {
            u.order(order);
        }
    }

    /// Spots a unit cannot stop on: every spot of a visible tree's cell, and every living unit's but
    /// `except`'s (where it stands, or where it is going).
    pub fn taken_spots(&self, except: &[u32]) -> BTreeSet<slots::Spot> {
        let trees = self.trees.iter().filter(|t| t.is_visible()).flat_map(|t| slots::cell_spots(t.cell()));
        let units = self.units.iter().filter(|u| u.is_alive() && !except.contains(&u.id)).map(|u| match u.action {
            Action::Walking { to } | Action::Stranded { to } => slots::spot_of(to),
            _ => slots::spot_of((u.x, u.z)),
        });
        trees.chain(units).collect()
    }

    /// Orders sending the player's living `units` to `to`, each to a free spot of its own around it
    /// (`slots::dispatch`).
    pub fn dispatch(&self, player: u8, units: &[u32], to: (u16, u16)) -> Vec<Command> {
        let movers: Vec<(u32, (u16, u16))> =
            self.units.iter().filter(|u| u.owner == player && u.is_alive() && units.contains(&u.id)).map(|u| (u.id, (u.x, u.z))).collect();
        let ids: Vec<u32> = movers.iter().map(|m| m.0).collect();
        slots::dispatch(&self.terrain, &movers, to, &self.taken_spots(&ids))
            .into_iter()
            .map(|(unit, (x, z))| Command::OrderUnit { player, unit, order: Order::MoveTo { x, z } })
            .collect()
    }

    /// One simulation tick for every unit, in order; returns the ground levelled by reincarnations.
    /// A unit that arrives (walking, or landing from a teleport) on a taken spot moves on to the
    /// nearest free one.
    pub fn tick(&mut self) -> Vec<DirtyRect> {
        let mut dirty = Vec::new();
        self.trees.iter_mut().for_each(Tree::tick);
        let arriving: Vec<bool> = self.units.iter().map(|u| matches!(u.action, Action::Walking { .. } | Action::Landing { .. })).collect();
        for unit in &mut self.units {
            let site = self.sites.iter().find(|s| s.owner == unit.owner);
            if unit.tick(&self.terrain, site) == Some(UnitEvent::Reincarnated) {
                dirty.extend(site.map(|s| s.flatten_for_spawn(&mut self.terrain)));
            }
        }
        for i in 0..self.units.len() {
            if arriving[i] && self.units[i].action == Action::Idle {
                let u = &self.units[i];
                let (taken, spot) = (self.taken_spots(&[u.id]), slots::spot_of((u.x, u.z)));
                if taken.contains(&spot) {
                    if let Some(&free) = slots::free_spots_near(&self.terrain, spot, &taken, 1).first() {
                        let (x, z) = slots::spot_centre(free);
                        self.units[i].order(Order::MoveTo { x, z });
                    }
                }
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
    fn sandbox_walk_has_a_ramp_a_hill_a_lake_and_a_mesa() {
        let m = GameMap::sandbox_walk();
        let c = MAP_SIZE as i32 / 2;
        let h = |dx: i32, dz: i32| m.terrain.get(c + dx, c + dz) as i32;
        assert_eq!(m.units.len(), 1);
        assert!(m.units[0].owner == 0 && !m.terrain.is_water(c, c));
        assert_eq!(h(8, 0) - h(7, 0), 30, "gentle ramp");
        assert_eq!(h(0, -8) - h(0, -7), 100, "steep hill");
        assert!(m.terrain.is_water(c - 9, c), "lake");
        assert!(crate::path::is_cliff(&m.terrain, (c + 8, c + 7)) && !crate::path::is_cliff(&m.terrain, (c + 8, c + 9)), "mesa");
        assert!(!(-14..0).any(|dz| crate::path::is_cliff(&m.terrain, (c, c + dz))), "the hill is no cliff");
        assert!(m.terrain.is_water(c + 25, c), "sea around the island");
    }

    #[test]
    fn sandbox_walk_shaman_walks_around_the_lake() {
        let mut m = GameMap::sandbox_walk();
        let c = MAP_SIZE as i32 / 2;
        let at = |dx: i32| ((c + dx) as u16 * 512 + 256, c as u16 * 512 + 256);
        let order = |m: &mut GameMap, to: (u16, u16)| {
            let unit = m.units[0].id;
            m.apply(&Command::OrderUnit { player: 0, unit, order: Order::MoveTo { x: to.0, z: to.1 } });
        };
        let walk = |m: &mut GameMap| (0..400).find(|_| {
            m.tick();
            m.units[0].action == crate::unit::Action::Idle
        });
        order(&mut m, at(-14));
        assert!(walk(&mut m).is_some());
        order(&mut m, at(-4));
        assert!(walk(&mut m).is_some(), "goes around the lake");
        assert_eq!((m.units[0].x, m.units[0].z), at(-4));
    }

    #[test]
    fn sandbox_walk_shaman_goes_around_the_hill_faster_than_over_it() {
        let m = GameMap::sandbox_walk();
        let c = MAP_SIZE as i32 / 2;
        let at = |dz: i32| (c as u16 * 512 + 256, (c + dz) as u16 * 512 + 256);
        let ticks = |hops: &[(u16, u16)]| {
            let mut u = m.units[0].clone();
            (u.x, u.z) = at(-4);
            let mut ticks = 0;
            for &(x, z) in hops {
                u.order(Order::MoveTo { x, z });
                while u.action != crate::unit::Action::Idle {
                    u.tick(&m.terrain, None);
                    ticks += 1;
                }
            }
            (ticks, u)
        };
        let (fastest, u) = ticks(&[at(-17)]);
        assert_eq!((u.x, u.z), at(-17));
        let over: Vec<_> = (5..=17).map(|d| at(-d)).collect();
        let (straight, _) = ticks(&over);
        assert!(fastest < straight, "around {fastest} ticks, over the top {straight}");
    }

    #[test]
    fn teleport_only_onto_walkable_ground() {
        let mut m = GameMap::sandbox_walk();
        let c = MAP_SIZE as i32 / 2;
        let at = |dx: i32, dz: i32| ((c + dx) as u16 * 512 + 256, (c + dz) as u16 * 512 + 256);
        let cast = |to| Command::Cast { player: 0, spell: Spell::Teleport { to } };
        for (to, why) in [(at(-9, 0), "lake"), (at(30, 0), "sea"), (at(8, 7), "cliff of the mesa")] {
            assert!(!m.can_cast(0, &Spell::Teleport { to }), "{why}");
            m.apply(&cast(to));
            assert_ne!((m.units[0].x, m.units[0].z), to, "{why}");
        }
        assert!(!m.can_cast(1, &Spell::Teleport { to: at(0, 3) }), "no shaman for player 1");
        let to = at(0, -10);
        assert_eq!(m.apply(&cast(to)), None, "no terrain change");
        assert!(matches!(m.units[0].action, crate::unit::Action::Casting { .. }), "the cast jump first");
        assert_eq!(m.units[0].teleport_target(), Some(to));
        for _ in 0..crate::unit::CAST_TICKS {
            m.tick();
        }
        assert_eq!((m.units[0].x, m.units[0].z), to, "then on the hill top");
    }

    #[test]
    fn sandbox_units_has_every_kind_on_land() {
        let m = GameMap::sandbox_units();
        let count = |owner: u8, kind: UnitKind| m.units.iter().filter(|u| u.owner == owner && u.kind == kind).count();
        assert_eq!(count(0, UnitKind::Shaman), 1);
        assert_eq!(count(1, UnitKind::Shaman), 0);
        for kind in &UnitKind::ALL[1..] {
            assert_eq!((count(0, *kind), count(1, *kind)), (3, 1), "{kind:?}");
        }
        let mut ids: Vec<u32> = m.units.iter().map(|u| u.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), m.units.len(), "unique ids");
        for u in &m.units {
            assert!(Mobility::Walk.passable(&m.terrain, u.cell()), "{:?} on walkable land", u.kind);
        }
        let c = MAP_SIZE as i32 / 2;
        assert!(m.terrain.is_water(c, c - 9), "pond");
    }

    #[test]
    fn orders_and_casts_go_to_the_shaman_not_the_first_unit() {
        let mut m = GameMap::sandbox_units();
        m.units.rotate_left(1);
        m.apply(&Command::Order { player: 0, order: Order::Pray });
        assert_eq!(m.shaman_of(0).unwrap().action, crate::unit::Action::Praying);
        assert!(m.units.iter().filter(|u| u.kind != UnitKind::Shaman).all(|u| u.action == crate::unit::Action::Idle));
    }

    #[test]
    fn generated_and_sandbox_maps_have_groves_that_grow() {
        let mut m = GameMap::generate(42);
        assert!(m.trees.len() > 30, "{} trees", m.trees.len());
        assert_eq!(m.trees, GameMap::generate(42).trees, "deterministic");
        for map in [GameMap::sandbox_walk(), GameMap::sandbox_units()] {
            assert!(map.trees.len() > 10, "{}: {} trees", map.name, map.trees.len());
            for t in &map.trees {
                assert!(Mobility::Walk.passable(&map.terrain, t.cell()));
            }
        }
        let small = m.trees.iter().position(|t| t.size < crate::tree::MAX_SIZE).unwrap();
        let before = m.trees[small].size;
        for _ in 0..crate::tree::GROW_TICKS {
            m.tick();
        }
        assert_eq!(m.trees[small].size, before + 1);
    }

    #[test]
    fn a_group_sent_to_one_spot_stands_packed_on_different_spots() {
        let mut m = GameMap::sandbox_units();
        m.trees.clear();
        let c = MAP_SIZE as i32 / 2;
        let to = slots::cell_centre((c + 2, c + 6));
        let group: Vec<u32> = m.units.iter().filter(|u| u.owner == 0).map(|u| u.id).collect();
        for cmd in m.dispatch(0, &group, to) {
            m.apply(&cmd);
        }
        for _ in 0..600 {
            m.tick();
        }
        let spots: Vec<_> = m.units.iter().filter(|u| u.owner == 0).map(|u| slots::spot_of((u.x, u.z))).collect();
        assert!(m.units.iter().filter(|u| u.owner == 0).all(|u| u.action == Action::Idle), "all arrived");
        let distinct: BTreeSet<_> = spots.iter().collect();
        assert_eq!(distinct.len(), spots.len(), "one spot each: {spots:?}");
        let cells: Vec<_> = m.units.iter().filter(|u| u.owner == 0).map(Unit::cell).collect();
        assert!(cells.iter().all(|&(x, z)| (x - c - 2).abs() <= 2 && (z - c - 6).abs() <= 2), "16 units within 2 cells: {cells:?}");
    }

    #[test]
    fn never_stops_on_a_tree_or_another_unit() {
        let mut m = GameMap::sandbox_walk();
        let c = MAP_SIZE as i32 / 2;
        m.trees = vec![Tree::new((c + 5, c), 0, 3)];
        let shaman = m.units[0].id;
        m.units.push(Unit::new(99, 0, UnitKind::Brave, slots::cell_centre((c + 6, c))));
        for target in [(c + 5, c), (c + 6, c)] {
            let to = slots::cell_centre(target);
            m.apply(&Command::OrderUnit { player: 0, unit: shaman, order: Order::MoveTo { x: to.0, z: to.1 } });
            for _ in 0..300 {
                m.tick();
            }
            let u = &m.units[0];
            assert_eq!(u.action, Action::Idle);
            assert_ne!(u.cell(), (c + 5, c), "never under the tree");
            assert_ne!(slots::spot_of((u.x, u.z)), slots::spot_of((m.units[1].x, m.units[1].z)), "never on the brave's spot");
            assert!((u.cell().0 - target.0).abs() <= 1 && (u.cell().1 - target.1).abs() <= 1, "right next to it");
        }
    }

    #[test]
    fn teleporting_onto_a_tree_steps_off_it() {
        let mut m = GameMap::sandbox_walk();
        let c = MAP_SIZE as i32 / 2;
        m.trees = vec![Tree::new((c + 4, c), 0, 4)];
        m.apply(&Command::Cast { player: 0, spell: Spell::Teleport { to: slots::cell_centre((c + 4, c)) } });
        for _ in 0..100 {
            m.tick();
        }
        assert_ne!(m.units[0].cell(), (c + 4, c));
        assert_eq!(m.units[0].action, Action::Idle);
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
