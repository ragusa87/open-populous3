//! A playable map: terrain + metadata, built from an original level or generated.

use crate::building::{buildings_from_level, Building, BuildingKind};
use crate::campfire::{self, Campfire};
use crate::command::Command;
use crate::path::{Ground, Mobility, Walled, Walls};
use crate::slots;
use crate::spell::Spell;
use crate::build_book::BuildBook;
use crate::spell_book::SpellBook;
use std::collections::BTreeSet;
use crate::tree::{scatter, Tree};
use crate::site::{generated_sites, sites_from_level, ReincarnationSite};
use crate::terrain::{DirtyRect, Heightmap, MAX_HEIGHT};
use crate::unit::{torus_delta, Action, Order, Unit, UnitEvent, UnitKind};
use crate::wood::WoodPiece;
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
    /// Wood on the map: an original level's scenery trees, or groves on generated maps and sandboxes.
    pub trees: Vec<Tree>,
    /// An original level's buildings (none on generated maps and sandboxes yet).
    pub buildings: Vec<Building>,
    /// Pieces of wood lying on the ground (none in original levels).
    pub wood: Vec<WoodPiece>,
    /// The spells an original level gives (`SpellBook::from_level`, the same for every tribe for
    /// now); None for generated maps, sandboxes and levels without a header.
    pub spell_book: Option<SpellBook>,
    /// The buildings an original level lets the tribes build (`BuildBook::from_level`); None like
    /// `spell_book`.
    pub build_book: Option<BuildBook>,
    /// Lit camp fires, in lighting order (none in original levels).
    pub campfires: Vec<Campfire>,
    /// The cells the buildings stand on, past their plan stage (`update_walls`).
    pub walls: Walls,
}

impl GameMap {
    /// An original level: its terrain, sites, trees and buildings. Buildings level their ground
    /// above the sea first, then the sites theirs (`with_shamans`).
    pub fn from_level(level: &Level, name: impl Into<String>, theme: Option<u8>) -> Self {
        GameMap {
            name: name.into(),
            theme,
            terrain: Heightmap::from_heights(MAP_SIZE, level.heights.clone()),
            sites: sites_from_level(level),
            units: Vec::new(),
            trees: level
                .things
                .iter()
                .filter_map(|t| Some(Tree::new(((t.x as u32 / 512) as i32, (t.z as u32 / 512) as i32), t.tree_type()?, crate::tree::MAX_SIZE).with_angle(t.angle()?)))
                .collect(),
            buildings: buildings_from_level(level),
            wood: Vec::new(),
            spell_book: None,
            build_book: None,
            campfires: Vec::new(),
            walls: Walls::default(),
        }
        .with_building_ground()
        .with_shamans()
        .with_people(level)
    }

    /// The level's people other than shamans (who spawn at their sites), where they are placed.
    /// Wildmen around a site (`ReincarnationSite::welcomes`) are that tribe's first followers: they
    /// start as its braves (the original converts them when the shaman appears).
    fn with_people(mut self, level: &Level) -> Self {
        let people = level.things.iter().filter(|t| t.kind == pop3_format::level::KIND_PERSON && !t.is_shaman());
        for t in people {
            let Some(kind) = UnitKind::from_person_model(t.model) else { continue };
            let cell = ((t.x as u32 / 512) as i32, (t.z as u32 / 512) as i32);
            let welcomed = (kind == UnitKind::Wildman).then(|| self.sites.iter().find(|s| s.welcomes(cell))).flatten();
            let (owner, kind) = welcomed.map_or((t.owner, kind), |s| (s.owner, UnitKind::Brave));
            let id = self.units.len() as u32 + 1;
            self.units.push(Unit::new(id, owner, kind, (t.x, t.z)));
        }
        self
    }

    fn with_building_ground(mut self) -> Self {
        for b in &self.buildings {
            b.flatten(&mut self.terrain);
        }
        self.update_walls();
        self
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
        let spell_book = header.as_ref().map(|h| SpellBook::from_level(h, &level));
        let build_book = header.as_ref().map(|h| BuildBook::from_level(h, &level));
        Ok(GameMap { spell_book, build_book, ..Self::from_level(&level, name, header.map(|h| h.theme)) })
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
        GameMap { name: format!("Generated #{seed}"), theme: None, terrain, sites, units: Vec::new(), trees: Vec::new(), buildings: Vec::new(), wood: Vec::new(), spell_book: None, build_book: None, campfires: Vec::new(), walls: Walls::default() }.with_shamans().with_trees(seed, 60)
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
        GameMap { name: "Sandbox: walk".into(), theme: None, terrain, sites, units: Vec::new(), trees: Vec::new(), buildings: Vec::new(), wood: Vec::new(), spell_book: None, build_book: None, campfires: Vec::new(), walls: Walls::default() }.with_shamans().with_trees(1, 150)
    }

    /// Test ground for buildings: a flat island with the player's site at the centre.
    /// - South (in view of the starting camera): one building of every known model (1-19) for the
    ///   player in rows, facing each quarter turn in turn, and a few for tribe 1 (red) to compare
    ///   colours, with some wood pieces between them and the site.
    /// - North: one row per buildable kind (`BUILDABLE` order) showing each `showcase_states`
    ///   column, west to east.
    /// - East: free ground to build on, with braves, a pile of wood and a few trees, a low mound
    ///   (north-east of the braves) to try flattening on, and a camp fire three of the braves go round.
    pub fn sandbox_buildings() -> Self {
        const C: i32 = MAP_SIZE as i32 / 2;
        const ISLAND: i32 = 48;
        let mut terrain = Heightmap::new(MAP_SIZE);
        for z in 0..MAP_SIZE as i32 {
            for x in 0..MAP_SIZE as i32 {
                if (x - C) * (x - C) + (z - C) * (z - C) <= ISLAND * ISLAND {
                    terrain.set(x, z, 64);
                }
            }
        }
        terrain.raise((C + 26, C - 6), 3, 90);
        let sites = vec![ReincarnationSite::at_cell(0, (C, C))];
        let mut map = GameMap { name: "Sandbox: buildings".into(), theme: None, terrain, sites, units: Vec::new(), trees: Vec::new(), buildings: Vec::new(), wood: Vec::new(), spell_book: None, build_book: None, campfires: Vec::new(), walls: Walls::default() }.with_shamans();
        let at = |dx: i32, dz: i32| ((C + dx) as u16 * 512 + 256, (C + dz) as u16 * 512 + 256);
        let place = |owner: u8, kind: BuildingKind, (x, z): (u16, u16), facing: u8| Building::new(kind, owner, x - 256, z - 256, facing);
        for model in 1..=19u8 {
            let i = model as i32 - 1;
            map.buildings.push(place(0, BuildingKind::from_model(model), at(-12 + (i % 7) * 4, 6 + (i / 7) * 4), (i % 4) as u8 * 2));
        }
        for (i, model) in [1u8, 3, 4, 7].iter().enumerate() {
            map.buildings.push(place(1, BuildingKind::from_model(*model), at(-6 + i as i32 * 4, 19), 0));
        }
        let (cx, cz) = at(0, 3);
        for (dx, dz) in [(0, 0), (90, 40), (-80, 60), (20, -70), (-600, 100), (700, -50)] {
            map.wood.push(WoodPiece::new((cx as i32 + dx) as u16, (cz as i32 + dz) as u16));
        }
        for (row, &kind) in crate::build_book::BUILDABLE.iter().enumerate() {
            for (col, state) in showcase_states(kind).into_iter().enumerate() {
                let (x, z) = at(-22 + 5 * col as i32, -7 - 5 * row as i32);
                map.buildings.push(Building { x: x - 256, z: z - 256, ..state });
            }
        }
        for n in 0..8 {
            let id = map.units.len() as u32 + 1;
            map.units.push(Unit::new(id, 0, UnitKind::Brave, at(20 + n % 4, -2 + 2 * (n / 4))));
        }
        // The showcase's occupied buildings get real people inside.
        let occupied: Vec<Building> = map.buildings.iter().filter(|b| b.inside > 0).cloned().collect();
        for b in occupied {
            for _ in 0..b.inside {
                let id = map.units.len() as u32 + 1;
                let mut u = Unit::new(id, 0, UnitKind::Brave, b.centre());
                u.inside = Some(crate::unit::Inside { site: (b.x, b.z), door: b.door() });
                map.units.push(u);
            }
        }
        map.update_walls();
        map.count_inside();
        let (px, pz) = at(27, 6);
        for (dx, dz) in [(0, 0), (60, 30), (-50, 40), (30, -60), (-40, -30), (90, -20), (-90, 0), (0, 90), (120, 60), (-120, -70), (70, 110), (-30, -110)] {
            map.wood.push(WoodPiece::new((px as i32 + dx) as u16, (pz as i32 + dz) as u16));
        }
        for (i, (dx, dz)) in [(34, -8), (36, -5), (35, -1), (37, 3), (34, 9), (36, 12)].into_iter().enumerate() {
            map.trees.push(Tree::new((C + dx, C + dz), i as u8, crate::tree::MAX_SIZE));
        }
        if let Some(fire) = map.place_campfire(0, (C + 21, C + 4)) {
            let braves: Vec<u32> = map.units.iter().filter(|u| u.kind == UnitKind::Brave).take(3).map(|u| u.id).collect();
            for c in map.gather(0, &braves, fire) {
                map.apply(&c);
            }
        }
        map
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
        let mut map = GameMap { name: "Sandbox: units".into(), theme: None, terrain, sites, units: Vec::new(), trees: Vec::new(), buildings: Vec::new(), wood: Vec::new(), spell_book: None, build_book: None, campfires: Vec::new(), walls: Walls::default() }.with_shamans().with_trees(2, 150);
        let at = |dx: i32, dz: i32| ((C + dx) as u16 * 512 + 256, (C + dz) as u16 * 512 + 256);
        for (row, &kind) in UnitKind::FOLLOWERS.iter().enumerate() {
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
                self.shaman_of(player).is_some_and(Unit::is_alive) && self.ground().passable(Mobility::Walk, cell)
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
                if let Some(i) = self.units.iter().position(|u| u.id == unit && u.owner == player) {
                    self.units[i].clear_queue();
                    self.start_order(i, order);
                }
                None
            }
            Command::QueueOrder { player, unit, order } => {
                if let Some(i) = self.units.iter().position(|u| u.id == unit && u.owner == player) {
                    if self.units[i].is_free() && self.units[i].queued().is_empty() {
                        self.start_order(i, order);
                    } else {
                        self.units[i].enqueue(order);
                    }
                }
                None
            }
            Command::PlaceCampfire { player, at } => {
                self.place_campfire(player, campfire::cell_at(at));
                None
            }
            Command::RemoveCampfire { player, at } => {
                self.remove_campfire(player, at);
                None
            }
            Command::PlaceBuilding { player, kind, at, facing } => {
                self.place_building(player, kind, at, facing);
                None
            }
            Command::CancelBuilding { player, at } => {
                self.cancel_building(player, at);
                None
            }
        }
    }

    /// Lights a camp fire for `player` in `cell` if it can be (`campfire::can_place`); its id.
    pub fn place_campfire(&mut self, player: u8, cell: (i32, i32)) -> Option<u32> {
        if !campfire::can_place(self, cell) {
            return None;
        }
        let id = self.campfires.iter().map(|f| f.id).max().unwrap_or(0) + 1;
        self.campfires.push(Campfire::new(id, player, cell));
        Some(id)
    }

    /// Puts out `player`'s camp fire whose cell holds world point `p`, if any: the units going to it
    /// or round it stop (idle).
    pub fn remove_campfire(&mut self, player: u8, p: (u16, u16)) {
        let Some(i) = self.campfires.iter().position(|f| f.owner == player && f.cell() == campfire::cell_at(p)) else { return };
        let centre = self.campfires.remove(i).centre();
        for u in self.units.iter_mut().filter(|u| u.owner == player && u.campfire() == Some(centre)) {
            u.start(Order::Stop);
        }
    }

    /// The camp fire whose cell holds world point `p`, if any.
    pub fn campfire_at(&self, p: (u16, u16)) -> Option<&Campfire> {
        let cell = campfire::cell_at(p);
        self.campfires.iter().find(|f| f.cell() == cell)
    }

    /// Orders sending the player's living `units` round their own camp fire `fire`, spread evenly
    /// over its ring from the point nearest the first of them; none if the fire is not theirs.
    pub fn gather(&self, player: u8, units: &[u32], fire: u32) -> Vec<Command> {
        let Some(f) = self.campfires.iter().find(|f| f.id == fire && f.owner == player) else { return Vec::new() };
        let going: Vec<&Unit> = self.units.iter().filter(|u| u.owner == player && u.is_alive() && units.contains(&u.id)).collect();
        let Some(first) = going.first() else { return Vec::new() };
        let start = campfire::nearest_point(f.centre(), (first.x, first.z)) as usize;
        let n = going.len();
        going
            .iter()
            .enumerate()
            .map(|(i, u)| {
                let point = ((start + i * campfire::RING_POINTS as usize / n) % campfire::RING_POINTS as usize) as u8;
                Command::OrderUnit { player, unit: u.id, order: Order::Campfire { fire: f.centre(), point } }
            })
            .collect()
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
        slots::dispatch(&self.ground(), &movers, to, &self.taken_spots(&ids))
            .into_iter()
            .map(|(unit, (x, z))| Command::OrderUnit { player, unit, order: Order::MoveTo { x, z } })
            .collect()
    }

    /// One simulation tick for every unit, in order; returns the ground levelled by reincarnations.
    /// A unit that arrives (walking, or landing from a teleport) on a taken spot moves on to the
    /// nearest free one.
    pub fn tick(&mut self) -> Vec<DirtyRect> {
        let mut dirty = Vec::new();
        self.update_walls();
        let buildings = &self.buildings;
        self.trees.iter_mut().filter(|t| !buildings.iter().any(|b| b.covers((t.x, t.z), 0))).for_each(Tree::tick);
        self.tend_campfires();
        let arriving: Vec<bool> = self.units.iter().map(|u| matches!(u.action, Action::Walking { .. } | Action::Landing { .. })).collect();
        let mut events = Vec::new();
        for (i, unit) in self.units.iter_mut().enumerate() {
            let site = self.sites.iter().find(|s| s.owner == unit.owner);
            let ground = Walled { terrain: &self.terrain, walls: &self.walls, inside: unit.inside.map(|i| i.site) };
            match unit.tick(&ground, site) {
                Some(UnitEvent::Reincarnated) => dirty.extend(site.map(|s| s.flatten_for_spawn(&mut self.terrain))),
                Some(event) => events.push((i, event)),
                None => {}
            }
        }
        for (i, event) in events {
            match event {
                UnitEvent::Jumped { .. } | UnitEvent::Built => dirty.extend(self.work_event(i, event)),
                UnitEvent::AtDoor { site } => self.come_in(i, site),
                _ => self.wood_event(i, event),
            }
        }
        for i in 0..self.units.len() {
            if arriving[i] && self.units[i].action == Action::Idle && self.units[i].inside.is_none() {
                let u = &self.units[i];
                let (taken, spot) = (self.taken_spots(&[u.id]), slots::spot_of((u.x, u.z)));
                if taken.contains(&spot) {
                    if let Some(&free) = slots::free_spots_near(&self.ground(), spot, &taken, 1).first() {
                        let (x, z) = slots::spot_centre(free);
                        self.units[i].start(Order::MoveTo { x, z });
                    }
                }
            }
        }
        self.start_chained();
        dirty.extend(self.work());
        self.drop_wood();
        self.count_inside();
        dirty
    }
}

impl GameMap {
    /// The terrain with the buildings' walls, as walkers outside every building see it.
    pub fn ground(&self) -> Walled<'_> {
        Walled { terrain: &self.terrain, walls: &self.walls, inside: None }
    }

    /// Whether world point `p` is in a building's walls: nobody outside can get there.
    pub fn behind_walls(&self, p: (u16, u16)) -> bool {
        self.walls.at(((p.0 as u32 / 512) as i32, (p.1 as u32 / 512) as i32)).is_some()
    }

    /// Walls the cells under every building past its plan stage (its footprint covers their
    /// centre); a unit standing in a newly walled cell is inside that building, and walks out by its
    /// door when it moves.
    pub fn update_walls(&mut self) {
        let size = self.terrain.size() as i32;
        let mut cells = std::collections::BTreeMap::new();
        for b in self.buildings.iter().filter(|b| b.stage() != crate::building::Stage::Blueprint) {
            cells.extend(b.walled_cells(size).into_iter().map(|c| (c, (b.x, b.z))));
        }
        if self.walls.set(cells) {
            for u in self.units.iter_mut().filter(|u| u.inside.is_none() && u.is_alive()) {
                let Some(site) = self.walls.at(u.cell()) else { continue };
                if let Some(b) = self.buildings.iter().find(|b| (b.x, b.z) == site) {
                    u.inside = Some(crate::unit::Inside { site, door: b.door() });
                }
            }
        }
    }

    /// Every living idle unit starts its next chained order that is still valid.
    fn start_chained(&mut self) {
        for i in 0..self.units.len() {
            while self.units[i].is_free() {
                let Some(order) = self.units[i].pop_queued() else { break };
                if self.still_valid(&order) {
                    self.start_order(i, order);
                }
            }
        }
    }

    /// Starts `order` for unit `i`, keeping its chained orders; it leaves the building it worked on.
    /// Wood and build orders are for braves only (others ignore them) and pick their tree, piece or
    /// building here, where the map is known.
    fn start_order(&mut self, i: usize, order: Order) {
        self.unassign(i);
        match order {
            Order::Build { site } => self.assign(i, site),
            Order::Enter { site } => self.go_in(i, site),
            Order::CutTree { tree } => self.go_cut(i, tree),
            Order::FetchWood => self.fetch_wood(i),
            Order::PickUp { at } => self.pick_up(i, at),
            order => self.units[i].start(order),
        }
    }

    /// Braves that may take wood: living, not carrying any yet.
    fn can_take_wood(u: &Unit) -> bool {
        u.kind == UnitKind::Brave && u.is_alive() && u.carrying == 0
    }

    /// Unit `i` cuts the tree at `tree` if it has wood to spare, else the nearest tree that has within
    /// `REFIND_CELLS` of it; stands on the free spot next to the tree nearest to her.
    fn go_cut(&mut self, i: usize, tree: (u16, u16)) {
        let u = &self.units[i];
        if !Self::can_take_wood(u) {
            return;
        }
        let id = u.id;
        let spare = |t: &Tree| t.size as usize > self.tree_claims((t.x, t.z), id) && !self.behind_walls((t.x, t.z));
        let chosen = match self.trees.iter().find(|t| (t.x, t.z) == tree).filter(|t| spare(t)) {
            Some(t) => t,
            None => match self.trees.iter().filter(|t| spare(t) && torus_cells(tree, (t.x, t.z)) <= REFIND_CELLS).min_by_key(|t| torus_dist2(tree, (t.x, t.z))) {
                Some(t) => t,
                None => return,
            },
        };
        let tree = (chosen.x, chosen.z);
        let taken = self.taken_spots(&[id]);
        let me = (u.x, u.z);
        let spots = slots::free_spots_near(&self.ground(), slots::spot_of(tree), &taken, 8);
        if let Some(stand) = spots.iter().map(|&s| slots::spot_centre(s)).min_by_key(|&p| torus_dist2(me, p)) {
            self.units[i].go_cut(tree, stand);
        }
    }

    /// Unit `i` goes for the nearest wood: a piece on the floor nobody is fetching, or a tree with wood
    /// to spare (`go_cut`); the floor wins ties.
    pub(crate) fn fetch_wood(&mut self, i: usize) {
        let u = &self.units[i];
        if !Self::can_take_wood(u) {
            return;
        }
        let (me, id) = ((u.x, u.z), u.id);
        let piece = self
            .wood
            .iter()
            .map(|w| (w.x, w.z))
            .filter(|&p| !self.behind_walls(p) && self.units.iter().all(|o| o.id == id || o.fetching() != Some(p)))
            .min_by_key(|&p| torus_dist2(me, p));
        let tree = self
            .trees
            .iter()
            .filter(|t| t.size as usize > self.tree_claims((t.x, t.z), id) && !self.behind_walls((t.x, t.z)))
            .map(|t| (t.x, t.z))
            .min_by_key(|&p| torus_dist2(me, p));
        match (piece, tree) {
            (Some(p), Some(t)) if torus_dist2(me, t) < torus_dist2(me, p) => self.go_cut(i, t),
            (Some(p), _) => self.units[i].go_pick(p),
            (None, Some(t)) => self.go_cut(i, t),
            (None, None) => {}
        }
    }

    /// Unit `i` picks up the piece of wood nearest to `at` within `PICK_RADIUS` that nobody else is
    /// fetching; none: it does nothing.
    fn pick_up(&mut self, i: usize, at: (u16, u16)) {
        let u = &self.units[i];
        if !Self::can_take_wood(u) {
            return;
        }
        let id = u.id;
        let piece = self
            .wood
            .iter()
            .map(|w| (w.x, w.z))
            .filter(|&p| torus_dist2(at, p) <= PICK_RADIUS * PICK_RADIUS && !self.behind_walls(p))
            .filter(|&p| self.units.iter().all(|o| o.id == id || o.fetching() != Some(p)))
            .min_by_key(|&p| torus_dist2(at, p));
        if let Some(p) = piece {
            self.units[i].go_pick(p);
        }
    }

    /// The piece of wood nearest to `at` within `PICK_RADIUS`, if any (a click on a pile).
    pub fn wood_at(&self, at: (u16, u16)) -> Option<(u16, u16)> {
        self.wood.iter().map(|w| (w.x, w.z)).filter(|&p| torus_dist2(at, p) <= PICK_RADIUS * PICK_RADIUS).min_by_key(|&p| torus_dist2(at, p))
    }

    /// Orders sending the player's `units` to the wood pile at `at`: braves with empty hands pick up
    /// a piece each, the others walk next to it.
    pub fn pick_orders(&self, player: u8, units: &[u32], at: (u16, u16)) -> Vec<Command> {
        let mine = |u: &&Unit| u.owner == player && u.is_alive() && units.contains(&u.id);
        let takers: Vec<u32> = self.units.iter().filter(mine).filter(|u| Self::can_take_wood(u)).map(|u| u.id).collect();
        let others: Vec<u32> = units.iter().copied().filter(|id| !takers.contains(id)).collect();
        let pick = takers.iter().map(|&unit| Command::OrderUnit { player, unit, order: Order::PickUp { at } });
        pick.chain(if others.is_empty() { Vec::new() } else { self.dispatch(player, &others, at) }).collect()
    }

    /// Living units other than `except` cutting the tree at `tree` or walking to cut it.
    pub fn tree_claims(&self, tree: (u16, u16), except: u32) -> usize {
        self.units.iter().filter(|u| u.id != except && u.is_alive() && u.cutting() == Some(tree)).count()
    }

    /// A chopped tree gives its piece (cut meanwhile to nothing: on to another tree); a piece reached
    /// on the floor is picked up (taken meanwhile: fetch again).
    fn wood_event(&mut self, i: usize, event: UnitEvent) {
        match event {
            UnitEvent::Chopped { tree } => {
                if self.trees.iter_mut().find(|t| (t.x, t.z) == tree).is_some_and(Tree::cut) {
                    self.units[i].carrying = 1;
                } else {
                    self.go_cut(i, tree);
                }
            }
            UnitEvent::PutDown => {
                let u = &mut self.units[i];
                if u.carrying > 0 {
                    u.carrying = 0;
                    self.wood.push(WoodPiece::new(u.x, u.z));
                }
            }
            UnitEvent::PickedUp { at } => match self.wood.iter().position(|w| (w.x, w.z) == at) {
                Some(w) => {
                    self.wood.remove(w);
                    self.units[i].carrying = 1;
                }
                None => self.fetch_wood(i),
            },
            _ => {}
        }
    }

    /// An idle unit with wood holds it for `HOLD_TICKS`, then puts it down where it stands
    /// (`UnitEvent::PutDown`); a dead one drops it where it fell, and it is lost in the sea.
    fn drop_wood(&mut self) {
        for u in self.units.iter_mut().filter(|u| u.carrying > 0) {
            let lost = crate::unit::is_sea(&self.terrain, u.cell());
            if lost || !u.is_alive() {
                if !lost {
                    self.wood.push(WoodPiece::new(u.x, u.z));
                }
                u.carrying = 0;
            } else if u.action == Action::Idle {
                u.action = Action::Holding { left: crate::unit::HOLD_TICKS };
            }
        }
    }

    /// Orders sending the player's `units` to the tree `tree` (index in `trees`): braves cut it (or
    /// a neighbour), the others walk next to it.
    pub fn cut_orders(&self, player: u8, units: &[u32], tree: usize) -> Vec<Command> {
        let Some(t) = self.trees.get(tree) else { return Vec::new() };
        let at = (t.x, t.z);
        let mine = |u: &&Unit| u.owner == player && u.is_alive() && units.contains(&u.id);
        let braves: Vec<u32> = self.units.iter().filter(mine).filter(|u| u.kind == UnitKind::Brave).map(|u| u.id).collect();
        let others: Vec<u32> = units.iter().copied().filter(|id| !braves.contains(id)).collect();
        let cut = braves.iter().map(|&unit| Command::OrderUnit { player, unit, order: Order::CutTree { tree: at } });
        cut.chain(if others.is_empty() { Vec::new() } else { self.dispatch(player, &others, at) }).collect()
    }

    /// Whether a chained order still makes sense when its turn comes: building needs a building
    /// still to build.
    fn still_valid(&self, order: &Order) -> bool {
        match *order {
            Order::Build { site } => self.still_building(site),
            Order::Enter { site } => self.building_at_corner(site).is_some(),
            _ => true,
        }
    }

    /// Camp fires with someone of their tribe going to them or round them keep burning, the others
    /// count towards going out; those out are removed.
    fn tend_campfires(&mut self) {
        for f in &mut self.campfires {
            let tended = self.units.iter().any(|u| u.owner == f.owner && u.is_alive() && u.campfire() == Some(f.centre()));
            f.unattended = if tended { 0 } else { f.unattended.saturating_add(1) };
        }
        self.campfires.retain(|f| !f.is_out());
    }
}

/// How far (cells) from a tree with no wood to spare a brave looks for another one.
pub const REFIND_CELLS: i32 = 8;
/// How far (world units, 3/4 of a cell) from a click a piece of wood is picked up.
pub const PICK_RADIUS: i64 = 384;

/// Squared distance between two world points the short way around the torus.
pub(crate) fn torus_dist2(a: (u16, u16), b: (u16, u16)) -> i64 {
    let (dx, dz) = (torus_delta(a.0, b.0) as i64, torus_delta(a.1, b.1) as i64);
    dx * dx + dz * dz
}

/// Cells between two world points along the farther axis, the short way around the torus.
fn torus_cells(a: (u16, u16), b: (u16, u16)) -> i32 {
    torus_delta(a.0, b.0).abs().max(torus_delta(a.1, b.1).abs()) / 512
}

/// Tiny deterministic PRNG, same sequence on every platform.
pub struct Lcg(pub u32);

impl Lcg {
    pub fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0 >> 8
    }
}

/// A buildable kind in each state worth seeing, for the buildings sandbox (owner 0, facing 0, at
/// the origin): blueprint; under construction with none, a third, two thirds and all but one
/// piece in; built; dismantling at half; built and attacked (shaking); built with people inside.
pub fn showcase_states(kind: BuildingKind) -> Vec<Building> {
    let built = Building::new(kind, 0, 0, 0, 0);
    let of = kind.wood_cost();
    let building = |used: u8| Building { used, ..built.clone() };
    vec![
        Building::site(kind, 0, 0, 0, 0),
        building(0),
        building(of / 3),
        building(of * 2 / 3),
        building(of - 1),
        built.clone(),
        Building { used: of / 2, dismantling: true, ..built.clone() },
        Building { shaking: u16::MAX, ..built.clone() },
        Building { inside: 3, ..built },
    ]
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
        for kind in &UnitKind::FOLLOWERS {
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
    fn original_levels_get_their_scenery_trees() {
        use pop3_format::level::{DAT_SIZE, KIND_SCENERY};
        let mut d = vec![0u8; DAT_SIZE];
        let base = d.len() - 95 - 2000 * 55;
        d[base..base + 7].copy_from_slice(&[2, KIND_SCENERY, 0, 0x00, 0x0a, 0x00, 0x14]);
        d[base + 55..base + 62].copy_from_slice(&[8, KIND_SCENERY, 0, 0x00, 0x0b, 0x00, 0x14]);
        let m = GameMap::from_level(&Level::parse(&d).unwrap(), "test", None);
        assert_eq!(m.trees.len(), 1, "a tree, not the plant");
        assert_eq!((m.trees[0].cell(), m.trees[0].variant, m.trees[0].size), ((5, 10), 1, crate::tree::MAX_SIZE));
    }

    #[test]
    fn sandbox_buildings_has_every_model_on_land() {
        let m = GameMap::sandbox_buildings();
        let models = m.buildings.iter().filter(|b| b.owner == 0 && b.stage() == crate::building::Stage::Built && b.inside == 0 && b.shaking == 0);
        assert_eq!(models.count(), 19 + crate::build_book::BUILDABLE.len());
        assert!(m.buildings.iter().any(|b| b.owner == 1));
        assert!(!m.wood.is_empty(), "some wood to look at");
        for w in &m.wood {
            assert!(!m.terrain.is_water((w.x / 512) as i32, (w.z / 512) as i32));
        }
        for b in &m.buildings {
            assert!(!m.terrain.is_water((b.x / 512) as i32, (b.z / 512) as i32));
            for o in &m.buildings {
                assert!(std::ptr::eq(b, o) || !b.covers(o.centre(), 0), "{:?} and {:?} overlap", b.kind, o.kind);
            }
        }
        let braves = m.units.iter().filter(|u| u.kind == UnitKind::Brave && u.owner == 0).count();
        assert!(braves >= 4 && m.wood.len() >= 10 && m.trees.len() >= 4, "something to build with");
        for t in &m.trees {
            assert!(!m.terrain.is_water(t.cell().0, t.cell().1));
        }
    }

    #[test]
    fn the_buildings_sandbox_has_a_tended_camp_fire() {
        let mut m = GameMap::sandbox_buildings();
        assert_eq!(m.campfires.len(), 1);
        let fire = m.campfires[0].centre();
        assert_eq!(m.units.iter().filter(|u| u.campfire() == Some(fire)).count(), 3);
        for _ in 0..crate::campfire::ABANDON_TICKS + 10 {
            m.tick();
        }
        assert_eq!(m.campfires.len(), 1, "tended: still burning");
        assert_eq!(m.units.iter().filter(|u| matches!(u.action, Action::AroundFire { .. })).count(), 3);
    }

    #[test]
    fn camp_fires_are_lit_by_command_on_free_flat_ground_only() {
        let mut m = GameMap::sandbox_buildings();
        let at = |x: i32, z: i32| ((x * 512 + 100) as u16, (z * 512 + 400) as u16);
        m.apply(&Command::PlaceCampfire { player: 0, at: at(84, 70) });
        assert_eq!(m.campfires.last().map(|f| (f.cell(), f.owner)), Some(((84, 70), 0)));
        let n = m.campfires.len();
        m.apply(&Command::PlaceCampfire { player: 0, at: at(84, 70) });
        m.apply(&Command::PlaceCampfire { player: 0, at: at(2, 2) });
        assert_eq!(m.campfires.len(), n, "taken, then the sea");
        assert_eq!(m.campfire_at(at(84, 70)).map(|f| f.id), m.campfires.last().map(|f| f.id));
        let ids: Vec<u32> = m.campfires.iter().map(|f| f.id).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "unique ids");
    }

    #[test]
    fn units_go_round_their_fire_spread_out_and_an_abandoned_fire_goes_out() {
        use crate::campfire::{ring_point, ABANDON_TICKS, RING_POINTS};
        let mut m = GameMap::sandbox_buildings();
        m.campfires.clear();
        let fire = m.place_campfire(0, (84, 70)).unwrap();
        let braves: Vec<u32> = m.units.iter().filter(|u| u.kind == UnitKind::Brave && u.campfire().is_none()).take(4).map(|u| u.id).collect();
        let orders = m.gather(0, &braves, fire);
        let points: Vec<u8> = orders.iter().map(|c| match c {
            Command::OrderUnit { order: Order::Campfire { point, .. }, .. } => *point,
            _ => panic!("{c:?}"),
        }).collect();
        assert_eq!(points.len(), 4);
        for w in points.windows(2) {
            assert_eq!((w[1] + RING_POINTS - w[0]) % RING_POINTS, RING_POINTS / 4, "a quarter of the ring apart");
        }
        assert!(m.gather(1, &braves, fire).is_empty(), "not red's fire");
        for c in &orders {
            m.apply(c);
        }
        let centre = m.campfires[0].centre();
        let mut went_round = vec![0u32; braves.len()];
        let mut last = vec![None; braves.len()];
        for _ in 0..600 {
            m.tick();
            for (i, id) in braves.iter().enumerate() {
                let u = m.units.iter().find(|u| u.id == *id).unwrap();
                if let Action::AroundFire { point, .. } = u.action {
                    if last[i].is_some_and(|p| p != point) {
                        went_round[i] += 1;
                    }
                    last[i] = Some(point);
                    let near = ring_point(centre, point);
                    let d = (crate::unit::torus_delta(near.0, u.x).abs() + crate::unit::torus_delta(near.1, u.z).abs()) as i32;
                    assert!(d <= 2 * crate::campfire::RING, "on the ring");
                }
            }
        }
        assert!(went_round.iter().all(|&n| n > RING_POINTS as u32), "each went round at least once: {went_round:?}");
        for c in braves.iter().map(|&unit| Command::OrderUnit { player: 0, unit, order: Order::Stop }) {
            m.apply(&c);
        }
        for _ in 0..ABANDON_TICKS - 1 {
            m.tick();
        }
        assert_eq!(m.campfires.iter().filter(|f| f.id == fire).count(), 1, "not yet");
        m.tick();
        assert!(m.campfires.iter().all(|f| f.id != fire), "gone out");
    }

    fn run(m: &mut GameMap, ticks: u32) {
        for _ in 0..ticks {
            m.tick();
        }
    }

    #[test]
    fn chained_orders_run_one_after_the_other_and_a_direct_order_drops_them() {
        let mut m = GameMap::sandbox_walk();
        let c = MAP_SIZE as i32 / 2;
        m.trees.clear();
        let id = m.units[0].id;
        let (a, b) = (slots::cell_centre((c + 3, c)), slots::cell_centre((c + 3, c + 3)));
        for order in [Order::MoveTo { x: a.0, z: a.1 }, Order::MoveTo { x: b.0, z: b.1 }, Order::Pray] {
            m.apply(&Command::QueueOrder { player: 0, unit: id, order });
        }
        assert_eq!(m.units[0].action, Action::Walking { to: a }, "idle: the first one starts at once");
        assert_eq!(m.units[0].queued().len(), 2);
        let mut seen_a = false;
        for _ in 0..200 {
            m.tick();
            seen_a |= m.units[0].cell() == (c + 3, c);
        }
        assert!(seen_a, "went by A");
        assert_eq!((m.units[0].cell(), m.units[0].action), ((c + 3, c + 3), Action::Praying), "then B, then prays");
        m.apply(&Command::QueueOrder { player: 0, unit: id, order: Order::MoveTo { x: a.0, z: a.1 } });
        run(&mut m, 50);
        assert_eq!(m.units[0].action, Action::Praying, "praying (no target yet) does not end on its own");
        m.apply(&Command::OrderUnit { player: 0, unit: id, order: Order::Stop });
        run(&mut m, 50);
        assert_eq!((m.units[0].action, m.units[0].queued().len()), (Action::Idle, 0), "a direct order replaces the chain");
        m.apply(&Command::QueueOrder { player: 1, unit: id, order: Order::Pray });
        assert_eq!(m.units[0].action, Action::Idle, "not red's unit");
    }

    #[test]
    fn putting_a_camp_fire_out_starts_the_chained_orders() {
        let mut m = GameMap::sandbox_buildings();
        run(&mut m, 100);
        let fire = m.campfires[0].centre();
        let id = m.units.iter().find(|u| u.campfire() == Some(fire)).unwrap().id;
        let to = (fire.0.wrapping_sub(3 * 512), fire.1);
        m.apply(&Command::QueueOrder { player: 0, unit: id, order: Order::MoveTo { x: to.0, z: to.1 } });
        run(&mut m, 100);
        let u = m.units.iter().find(|u| u.id == id).unwrap();
        assert!(matches!(u.action, Action::AroundFire { .. }), "going round a tended fire never ends");
        m.apply(&Command::RemoveCampfire { player: 0, at: fire });
        run(&mut m, 100);
        let u = m.units.iter().find(|u| u.id == id).unwrap();
        assert_eq!(u.action, Action::Idle);
        assert!(slots::spot_of((u.x, u.z)).0.abs_diff(slots::spot_of(to).0) <= 3, "went on to the chained spot");
    }

    #[test]
    fn chained_orders_replay_the_same() {
        let play = || {
            let mut m = GameMap::sandbox_units();
            let ids: Vec<u32> = m.units.iter().filter(|u| u.owner == 0).map(|u| u.id).collect();
            let c = MAP_SIZE as i32 / 2;
            for (i, &unit) in ids.iter().enumerate() {
                let to = slots::cell_centre((c + i as i32 % 4, c + 4));
                m.apply(&Command::QueueOrder { player: 0, unit, order: Order::MoveTo { x: to.0, z: to.1 } });
                m.apply(&Command::QueueOrder { player: 0, unit, order: Order::Pray });
            }
            run(&mut m, 300);
            m.units
        };
        assert_eq!(play(), play());
    }

    /// Sandbox walk with no trees but `trees`, and braves of tribe 0 at `braves` (cells); their ids.
    fn woodland(trees: &[((i32, i32), u8)], braves: &[(i32, i32)]) -> (GameMap, Vec<u32>) {
        let mut m = GameMap::sandbox_walk();
        let c = MAP_SIZE as i32 / 2;
        m.trees = trees.iter().map(|&((x, z), size)| Tree::new((c + x, c + z), 0, size)).collect();
        let ids: Vec<u32> = (0..braves.len() as u32).map(|i| 100 + i).collect();
        for (&id, &(x, z)) in ids.iter().zip(braves) {
            m.units.push(Unit::new(id, 0, UnitKind::Brave, slots::cell_centre((c + x, c + z))));
        }
        (m, ids)
    }

    fn unit(m: &GameMap, id: u32) -> &Unit {
        m.units.iter().find(|u| u.id == id).unwrap()
    }

    fn near(a: (u16, u16), b: (u16, u16), cells: i32) -> bool {
        torus_cells(a, b) <= cells
    }

    fn tree_at(m: &GameMap, (x, z): (i32, i32)) -> (u16, u16) {
        let c = MAP_SIZE as i32 / 2;
        let t = m.trees.iter().find(|t| t.cell() == (c + x, c + z)).unwrap();
        (t.x, t.z)
    }

    #[test]
    fn a_brave_cuts_one_piece_and_drops_it_by_the_tree() {
        let (mut m, ids) = woodland(&[((4, 0), 4)], &[(0, 0)]);
        let tree = tree_at(&m, (4, 0));
        m.apply(&Command::OrderUnit { player: 0, unit: ids[0], order: Order::CutTree { tree } });
        let (mut chopped, mut held) = (0, 0);
        for _ in 0..200 {
            m.tick();
            match unit(&m, ids[0]).action {
                Action::Chopping { .. } => chopped += 1,
                Action::Holding { .. } => {
                    held += 1;
                    assert_eq!((unit(&m, ids[0]).carrying, m.wood.len()), (1, 0), "holds it before putting it down");
                }
                _ => {}
            }
        }
        assert_eq!(chopped, crate::unit::CHOP_TICKS, "chopped for CHOP_TICKS");
        assert_eq!(held, crate::unit::HOLD_TICKS, "then held it for HOLD_TICKS");
        assert_eq!(m.trees[0].size, 3);
        let b = unit(&m, ids[0]);
        assert_eq!((b.action, b.carrying), (Action::Idle, 0));
        assert_eq!(m.wood.len(), 1);
        assert!(near((m.wood[0].x, m.wood[0].z), tree, 1) && near((b.x, b.z), tree, 1), "dropped right by the tree");
    }

    #[test]
    fn only_braves_cut() {
        let (mut m, _) = woodland(&[((4, 0), 4)], &[]);
        let c = MAP_SIZE as i32 / 2;
        m.units.push(Unit::new(200, 0, UnitKind::Warrior, slots::cell_centre((c, c))));
        let tree = tree_at(&m, (4, 0));
        m.apply(&Command::OrderUnit { player: 0, unit: 200, order: Order::CutTree { tree } });
        run(&mut m, 200);
        assert_eq!((m.trees[0].size, m.wood.len(), unit(&m, 200).action), (4, 0, Action::Idle));
    }

    #[test]
    fn a_tree_takes_as_many_braves_as_it_has_wood_the_others_find_another() {
        let (mut m, ids) = woodland(&[((4, 0), 2), ((4, 5), 4)], &[(0, 0), (0, 1), (0, -1)]);
        let (small, big) = (tree_at(&m, (4, 0)), tree_at(&m, (4, 5)));
        for cmd in m.cut_orders(0, &ids, 0) {
            m.apply(&cmd);
        }
        let targets: Vec<_> = ids.iter().map(|&id| unit(&m, id).cutting()).collect();
        assert_eq!(targets.iter().filter(|&&t| t == Some(small)).count(), 2);
        assert_eq!(targets.iter().filter(|&&t| t == Some(big)).count(), 1);
        run(&mut m, 300);
        assert_eq!((m.trees[0].size, m.trees[1].size, m.wood.len()), (0, 3, 3));
    }

    #[test]
    fn other_kinds_sent_to_a_tree_just_walk_next_to_it() {
        let (m, ids) = woodland(&[((4, 0), 4)], &[(0, 0)]);
        let shaman = m.units[0].id;
        let cmds = m.cut_orders(0, &[ids[0], shaman], 0);
        assert!(matches!(cmds[..], [Command::OrderUnit { order: Order::CutTree { .. }, .. }, Command::OrderUnit { order: Order::MoveTo { .. }, .. }]), "{cmds:?}");
    }

    #[test]
    fn a_chained_order_carries_the_piece_and_drops_it_once_idle() {
        let (mut m, ids) = woodland(&[((4, 0), 4)], &[(0, 0)]);
        let c = MAP_SIZE as i32 / 2;
        let tree = tree_at(&m, (4, 0));
        let (p, q) = (slots::cell_centre((c - 3, c)), slots::cell_centre((c, c + 4)));
        m.apply(&Command::OrderUnit { player: 0, unit: ids[0], order: Order::CutTree { tree } });
        m.apply(&Command::QueueOrder { player: 0, unit: ids[0], order: Order::MoveTo { x: p.0, z: p.1 } });
        let mut carried = false;
        for _ in 0..300 {
            m.tick();
            carried |= unit(&m, ids[0]).carrying == 1 && matches!(unit(&m, ids[0]).action, Action::Walking { .. });
        }
        assert!(carried, "walked with it");
        assert_eq!(m.wood.len(), 1);
        assert!(near((m.wood[0].x, m.wood[0].z), p, 1), "dropped at P");
        m.apply(&Command::OrderUnit { player: 0, unit: ids[0], order: Order::CutTree { tree } });
        m.apply(&Command::QueueOrder { player: 0, unit: ids[0], order: Order::MoveTo { x: p.0, z: p.1 } });
        while unit(&m, ids[0]).carrying == 0 {
            m.tick();
        }
        m.apply(&Command::OrderUnit { player: 0, unit: ids[0], order: Order::MoveTo { x: q.0, z: q.1 } });
        assert_eq!(unit(&m, ids[0]).carrying, 1, "a direct order keeps the piece");
        run(&mut m, 300);
        assert_eq!(m.wood.len(), 2);
        assert!(near((m.wood[1].x, m.wood[1].z), q, 1), "dropped where the new order ended");
    }

    #[test]
    fn a_brave_carrying_wood_cannot_cut_or_fetch_more() {
        let (mut m, ids) = woodland(&[((4, 0), 4)], &[(0, 0)]);
        let c = MAP_SIZE as i32 / 2;
        let tree = tree_at(&m, (4, 0));
        let i = m.units.iter().position(|u| u.id == ids[0]).unwrap();
        let to = slots::cell_centre((c - 4, c));
        m.units[i].carrying = 1;
        m.units[i].order(Order::MoveTo { x: to.0, z: to.1 });
        for order in [Order::CutTree { tree }, Order::FetchWood] {
            m.apply(&Command::QueueOrder { player: 0, unit: ids[0], order });
        }
        m.apply(&Command::OrderUnit { player: 0, unit: ids[0], order: Order::CutTree { tree } });
        assert_eq!((unit(&m, ids[0]).cutting(), unit(&m, ids[0]).action), (None, Action::Walking { to }), "ignored, still on its way");
        run(&mut m, 200);
        assert_eq!((m.trees[0].size, m.wood.len(), unit(&m, ids[0]).carrying), (4, 1, 0), "no cut; put down once idle");
        m.apply(&Command::OrderUnit { player: 0, unit: ids[0], order: Order::CutTree { tree } });
        run(&mut m, 200);
        assert_eq!(m.trees[0].size, 3, "empty-handed again: cuts");
    }

    #[test]
    fn fetching_takes_the_nearest_wood_and_never_the_same_piece_twice() {
        let (mut m, ids) = woodland(&[((6, 0), 4)], &[(0, 0), (0, 1)]);
        let c = MAP_SIZE as i32 / 2;
        let piece = slots::cell_centre((c + 2, c));
        m.wood.push(WoodPiece::new(piece.0, piece.1));
        let p = slots::cell_centre((c - 4, c));
        for &unit in &ids {
            m.apply(&Command::OrderUnit { player: 0, unit, order: Order::FetchWood });
            m.apply(&Command::QueueOrder { player: 0, unit, order: Order::MoveTo { x: p.0, z: p.1 } });
        }
        assert_eq!(unit(&m, ids[0]).fetching(), Some(piece), "the floor piece is nearer");
        assert_eq!(unit(&m, ids[1]).cutting(), Some(tree_at(&m, (6, 0))), "taken: on to the tree");
        run(&mut m, 400);
        assert_eq!(m.trees[0].size, 3);
        assert_eq!(m.wood.len(), 2);
        assert!(m.wood.iter().all(|w| near((w.x, w.z), p, 1)), "both brought to P");
        assert!(ids.iter().all(|&id| unit(&m, id).carrying == 0));
    }

    #[test]
    fn braves_with_empty_hands_pick_up_a_piece_of_the_clicked_pile_each() {
        let (mut m, ids) = woodland(&[], &[(0, 0), (0, 1), (1, 0)]);
        let c = MAP_SIZE as i32 / 2;
        let pile = slots::cell_centre((c + 4, c));
        for dx in [0u16, 100] {
            m.wood.push(WoodPiece::new(pile.0 + dx, pile.1));
        }
        let i = m.units.iter().position(|u| u.id == ids[2]).unwrap();
        m.units[i].carrying = 1;
        assert_eq!(m.wood_at((pile.0 + 50, pile.1 + 50)), Some(pile));
        assert_eq!(m.wood_at(slots::cell_centre((c + 8, c))), None, "too far");
        let cmds = m.pick_orders(0, &ids, (pile.0 + 50, pile.1));
        assert!(matches!(cmds[..], [Command::OrderUnit { order: Order::PickUp { .. }, .. }, Command::OrderUnit { order: Order::PickUp { .. }, .. }, Command::OrderUnit { order: Order::MoveTo { .. }, .. }]), "{cmds:?}");
        for c in &cmds {
            m.apply(c);
        }
        assert_ne!(unit(&m, ids[0]).fetching(), unit(&m, ids[1]).fetching(), "a piece each");
        let mut held = 0;
        for _ in 0..60 {
            m.tick();
            held = held.max(m.units.iter().filter(|u| ids[..2].contains(&u.id) && u.carrying == 1).count());
        }
        assert_eq!(held, 2, "both picked one up");
        assert!(m.units.iter().all(|u| !matches!(u.action, Action::Holding { .. }) || u.carrying == 1));
    }

    #[test]
    fn wood_carried_into_the_sea_is_lost() {
        let (mut m, ids) = woodland(&[], &[(0, 0)]);
        let c = MAP_SIZE as i32 / 2;
        let i = m.units.iter().position(|u| u.id == ids[0]).unwrap();
        m.units[i].carrying = 1;
        let to = slots::cell_centre((c + 3, c));
        m.apply(&Command::OrderUnit { player: 0, unit: ids[0], order: Order::MoveTo { x: to.0, z: to.1 } });
        for (x, z) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            m.terrain.set(c + x, c + z, 0);
        }
        run(&mut m, 2);
        assert_eq!((unit(&m, ids[0]).carrying, m.wood.len()), (0, 0));
    }

    #[test]
    fn cutting_replays_the_same() {
        let play = || {
            let (mut m, ids) = woodland(&[((4, 0), 2), ((4, 3), 3), ((-4, 2), 1)], &[(0, 0), (0, 1), (1, 0), (1, 1)]);
            m.wood.push(WoodPiece::new(slots::cell_centre((70, 64)).0, slots::cell_centre((70, 64)).1));
            for cmd in m.cut_orders(0, &ids[..3], 0) {
                m.apply(&cmd);
            }
            m.apply(&Command::OrderUnit { player: 0, unit: ids[3], order: Order::FetchWood });
            run(&mut m, 400);
            (m.units, m.trees, m.wood)
        };
        assert_eq!(play(), play());
    }

    #[test]
    fn putting_a_camp_fire_out_leaves_its_people_idle() {
        let mut m = GameMap::sandbox_buildings();
        for _ in 0..100 {
            m.tick();
        }
        let fire = m.campfires[0].centre();
        let around: Vec<u32> = m.units.iter().filter(|u| u.campfire() == Some(fire)).map(|u| u.id).collect();
        assert_eq!(around.len(), 3);
        m.apply(&Command::RemoveCampfire { player: 1, at: fire });
        assert_eq!(m.campfires.len(), 1, "not red's to put out");
        m.apply(&Command::RemoveCampfire { player: 0, at: (fire.0 + 200, fire.1 - 200) });
        assert!(m.campfires.is_empty(), "anywhere in its cell");
        assert!(m.units.iter().filter(|u| around.contains(&u.id)).all(|u| u.action == Action::Idle));
        m.apply(&Command::RemoveCampfire { player: 0, at: fire });
    }

    #[test]
    fn showcase_covers_every_stage() {
        use crate::building::Stage;
        let states = showcase_states(BuildingKind::Temple);
        let stages: Vec<Stage> = states.iter().map(Building::stage).collect();
        assert_eq!(stages[..7], [
            Stage::Blueprint,
            Stage::UnderConstruction { used: 0, of: 8 },
            Stage::UnderConstruction { used: 2, of: 8 },
            Stage::UnderConstruction { used: 5, of: 8 },
            Stage::UnderConstruction { used: 7, of: 8 },
            Stage::Built,
            Stage::Dismantling { used: 4, of: 8 },
        ]);
        assert!(states[7].shaking > 0 && states[8].inside > 0);
    }

    #[test]
    fn a_building_on_the_water_gets_land_under_it() {
        use pop3_format::level::{DAT_SIZE, KIND_BUILDING};
        let mut d = vec![0u8; DAT_SIZE];
        let base = 81_987;
        d[base..base + 7].copy_from_slice(&[3, KIND_BUILDING, 1, 0x00, 0x0a, 0x00, 0x14]);
        let m = GameMap::from_level(&Level::parse(&d).unwrap(), "test", None);
        for (x, z) in [(5, 10), (6, 10), (5, 11), (6, 11)] {
            assert!(!m.terrain.is_water(x, z), "({x}, {z}) under the hut centred on cell (5, 10)");
        }
        assert!(Mobility::Walk.passable(&m.terrain, (5, 10)));
    }

    #[test]
    fn a_levels_people_spawn_where_placed_shamans_at_their_site() {
        use pop3_format::level::{DAT_SIZE, KIND_PERSON, PERSON_SHAMAN};
        let mut d = vec![0u8; DAT_SIZE];
        let base = 81_987;
        let things: [[u8; 7]; 4] = [
            [PERSON_SHAMAN, KIND_PERSON, 0, 0x00, 0x0a, 0x00, 0x14],
            [3, KIND_PERSON, 0, 0x00, 0x0b, 0x00, 0x14],
            [1, KIND_PERSON, 255, 0x00, 0x30, 0x00, 0x30],
            [1, KIND_PERSON, 255, 0x00, 0x0c, 0x00, 0x16],
        ];
        for (i, t) in things.iter().enumerate() {
            d[base + i * 55..base + i * 55 + 7].copy_from_slice(t);
        }
        let m = GameMap::from_level(&Level::parse(&d).unwrap(), "test", None);
        let kinds: Vec<_> = m.units.iter().map(|u| (u.id, u.owner, u.kind, u.x, u.z)).collect();
        assert_eq!(kinds[0].2, UnitKind::Shaman);
        assert_eq!(
            kinds[1..],
            [(2, 0, UnitKind::Warrior, 0x0b00, 0x1400), (3, 255, UnitKind::Wildman, 0x3000, 0x3000), (4, 0, UnitKind::Brave, 0x0c00, 0x1600)],
            "the wildman next to the site (cell 6, 11 by 5, 10) joins blue as a brave"
        );
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
