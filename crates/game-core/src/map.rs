//! A playable map: terrain + metadata, built from an original level or generated.

use crate::building::{buildings_from_level, Building, BuildingId, BuildingKind};
use crate::campfire::{self, Campfire};
use crate::command::Command;
use crate::coordinator::{Coordinators, Link};
use crate::effect::{DamageKind, Effect, EffectKind, TerrainChange};
use crate::event::{run_in_order, WorldEvent};
use crate::path::{Ground, Mobility, Walled, Walls};
use crate::id::{Id, Registry};
use crate::slots;
use crate::spell::Spell;
use crate::build_book::{BuildAvailability, BuildBook};
use crate::mana::ManaPool;
use crate::spell_book::{SpellBook, SpellKind};
use std::collections::{BTreeSet, HashMap};
use crate::tree::{self, scatter, Tree};
use crate::site::{generated_sites, sites_from_level, ReincarnationSite};
use crate::terrain::{DirtyRect, Heightmap, MAX_HEIGHT};
use crate::totem::{totems_from_level, Totem, TotemKind};
use crate::status::Timed;
use crate::outcome::Outcome;
use crate::shading::{CellShading, SunParams};
use crate::tribes::{Owner, TribeId, Tribes};
use crate::unit::{torus_delta, Action, Order, Target, Unit, UnitEvent, UnitId, UnitKind};
use crate::wood::WoodPiece;
use pop3_format::header::TRIBES;
use pop3_format::{Level, LevelHeader, MAP_SIZE, WORLD_UNITS_PER_CELL};
use crate::time::{Countdown, Tick, Ticks};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct GameMap {
    pub name: String,
    /// Original landscape theme index (`pop3_format::theme_char`), None for generated maps.
    pub theme: Option<u8>,
    pub terrain: Heightmap,
    /// One per tribe, sorted by owner.
    pub sites: Vec<ReincarnationSite>,
    /// The tribes actually in this match and their alliances (`tribes.rs`), read once and independent of whether
    /// each one still has (or ever had) a reincarnation site.
    pub tribes: Tribes,
    /// One shaman per site for now, in site order.
    pub units: Vec<Unit>,
    /// Wood on the map: an original level's scenery trees, or groves on generated maps and sandboxes.
    pub trees: Vec<Tree>,
    /// An original level's buildings (none on generated maps and sandboxes yet).
    pub buildings: Vec<Building>,
    /// Rewards granted so far, in order, with the tribe that got each (`worship`).
    pub granted: Vec<(TribeId, crate::building::Reward)>,
    /// Places to pray at: a level's scenery 9, the Worship sandbox's row.
    pub totems: Vec<Totem>,
    /// Pieces of wood lying on the ground (none in original levels).
    pub wood: Vec<WoodPiece>,
    /// Each player's mana, by owner (`mana::ManaPool`, ticked with its spell book).
    pub mana: Vec<ManaPool>,
    /// Each player's spells, by owner: an original level's (`SpellBook::from_level`, the same for every
    /// player), `SpellBook::demo` on generated maps and levels without a header, `SpellBook::sandbox` in the
    /// sandboxes.
    pub spell_books: Vec<SpellBook>,
    /// Each player's buildings, by owner: an original level's (`BuildBook::from_level`), all available
    /// elsewhere.
    pub build_books: Vec<BuildBook>,
    /// Lit camp fires, in lighting order (none in original levels).
    pub campfires: Vec<Campfire>,
    /// The cells the buildings stand on, past their plan stage (`update_walls`).
    pub walls: Walls,
    /// Where the camera starts: an original level's header (None when it does not say), `OUR_MAPS_CAMERA`
    /// on generated maps and sandboxes.
    pub start_camera: Option<StartCamera>,
    /// An original level's object bank (header byte 97): its tree style. 0 elsewhere.
    pub object_bank: u8,
    /// The player is a god (level flag "shaman omni", level 25): spells starting at the caster start at the
    /// view instead (`Spell::LandBridge::from` as aimed), and casts have no range.
    pub shaman_omni: bool,
    /// Spell effects going on (`effect::Effect`), in the order they were added.
    pub effects: Registry<Effect>,
    /// Objects coordinating several units, one registry per kind (coordinator.rs).
    pub coordinators: Coordinators,
    /// Groups of units spacing themselves while walking to one shared move order's target (formation.rs); unlike
    /// `coordinators`, membership is not owned through the units' own action, so this is its own typed list.
    pub formations: Vec<crate::formation::Formation>,
    /// Firebolts in flight (`crate::firebolt`).
    pub bolts: Registry<crate::firebolt::Firebolt>,
    /// Blast's own fireball shots in flight, from the shaman to the cast point (`crate::blast`): the general
    /// "every spell flies from the shaman as a shot" piece (TODO.md "Shots"), done for Blast only so far.
    pub fireballs: Registry<crate::blast::Fireball>,
    /// Boats and balloons (`crate::vehicle`).
    pub vehicles: Registry<crate::vehicle::Vehicle>,
    /// Shared patrol points (`crate::patrol`), referenced by zero or more units' own `patrol` queue.
    pub patrol_points: Registry<crate::patrol::PatrolPoint>,
    /// Invisible, stationary markers a move-type patrol point targets (`crate::hidden_point`).
    pub hidden_points: Registry<crate::hidden_point::HiddenPoint>,
    /// Ground left in ashes by destroyed buildings (`crate::ashes`).
    pub ashes: crate::ashes::Ashes,
    /// Per-cell blend/light/downhill, derived from `terrain` (+ `ashes`/active lava), kept current by
    /// `recompute_shading` (`shading.rs`); the client's terrain texture bake reads it, nothing in the
    /// simulation does.
    pub shading: CellShading,
    /// The match's result, once declared (`outcome.rs`): never unset, never relabelled.
    pub outcome: Option<Outcome>,
    /// Fights won, by tribe (combat.md "Melee", "Fight" [exe] `stats[tribe][0]`).
    pub melee_wins: [u32; TRIBES],
    /// Own deaths suffered in a fight, by tribe (combat.md "Melee", "Fight" [exe] `stats[tribe][1]`): only a
    /// `DamageKind::Melee` blow, which only ever comes from one, counts here - not every death.
    pub melee_deaths: [u32; TRIBES],
    /// Buildings a fire finished off, by the tribe that lit it ("The tick" [exe] `stats[tribe][7]`,
    /// `WorldEvent::BuildingBurnedDown`, `crate::fire::tend_fires`): fire-specific, not every destruction.
    pub fires_put_out: [u32; TRIBES],
    /// Ticks run since the map was made (`tick`).
    pub now: Tick,
    /// The next id `new_id` gives: units, buildings and effects share it, an id is never given twice.
    next_id: u32,
    /// The game's random draws while it runs (effects), the same on every peer: seeded per game (`seeded`).
    pub rng: Lcg,
    /// Each computer tribe's own script (`ai_script.rs`), one per tribe with a `cpscrNNN.dat` assigned
    /// (`LevelHeader::ai_scripts`); empty on generated maps and sandboxes.
    pub(crate) tribe_scripts: Vec<crate::ai_script::TribeScript>,
    /// Each computer tribe's native AI state (`ai.rs`): one per tribe with an assigned script, the same
    /// set `tribe_scripts` tracks ("Blue is the human player and never has one" — ai-scripts.md). Empty
    /// on generated maps and sandboxes, same as `tribe_scripts`.
    pub(crate) ai: Vec<crate::ai::NativeAi>,
    /// A script's flyby playing or just finished (`ai_script.rs`), read by the client to pose its own
    /// camera and HUD; `game-core` never touches a camera. At most one at a time.
    pub active_flyby: Option<crate::ai_script::ActiveFlyby>,
}

/// Generated maps and sandboxes are laid out for a camera on their south side looking north (towards -z):
/// half a turn from angle 0, which looks towards +z.
pub const OUR_MAPS_CAMERA: StartCamera = StartCamera { cell: None, angle: 1024 };

/// An original level's start camera (`.hdr`): a cell to look at (None: the player's site) and an angle about
/// the vertical axis, 2048ths of a turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartCamera {
    pub cell: Option<(i32, i32)>,
    pub angle: u16,
}

impl StartCamera {
    /// None when the header leaves both at 0, as most levels do.
    pub fn from_header(h: &LevelHeader) -> Option<Self> {
        let cell = (h.start != 0).then(|| {
            let (x, z) = h.start_cell();
            (x as i32, z as i32)
        });
        (cell.is_some() || h.start_angle != 0).then_some(StartCamera { cell, angle: h.start_angle % 2048 })
    }
}

/// The Worship sandbox's totems, in `TotemKind::ALL` order: units counted, shaman only, Angel of
/// Death. One unit, the shaman with an Angel of Death, 6, 8, 2, the shaman, 4.
const SANDBOX_TOTEMS: [(u16, bool, bool); 7] = [(1, false, false), (1, true, true), (6, false, false), (8, false, false), (2, false, false), (1, true, false), (4, false, false)];

/// What each of the Worship sandbox's totems gives, in `TotemKind::ALL` order: one cast of a spell,
/// mana, or (the winged death totem) the Angel of Death's effect, not handled yet.
fn sandbox_gift(k: usize) -> crate::building::Reward {
    use crate::building::Reward;
    [Reward::OneShot(SpellKind::Blast), Reward::Unhandled { kind: 7, model: 91 }, Reward::OneShot(SpellKind::Swarm), Reward::OneShot(SpellKind::Lightning), Reward::Mana(50_000), Reward::OneShot(SpellKind::Invisibility), Reward::OneShot(SpellKind::Convert)][k]
}

fn per_player<T: Clone>(t: T) -> Vec<T> {
    vec![t; TRIBES]
}

impl GameMap {
    /// `terrain` and `sites` with nothing else on them yet (generated maps, sandboxes). `tribes` (its roster, not
    /// its alliances, which these maps have no header to set) comes straight from `sites`: 1:1 for every map built
    /// this way.
    pub(crate) fn bare(name: impl Into<String>, terrain: Heightmap, sites: Vec<ReincarnationSite>) -> Self {
        let tribes = Tribes::from_sites(&sites);
        let shading = CellShading::new(terrain.size(), SunParams::default());
        GameMap { name: name.into(), theme: None, terrain, sites, tribes, units: Vec::new(), trees: Vec::new(), buildings: Vec::new(), granted: Vec::new(), totems: Vec::new(), wood: Vec::new(), mana: per_player(ManaPool::default()), spell_books: per_player(SpellBook::demo()), build_books: per_player(BuildBook::all(BuildAvailability::Available)), campfires: Vec::new(), walls: Walls::default(), start_camera: Some(OUR_MAPS_CAMERA), object_bank: 0, shaman_omni: false, effects: Registry::default(), coordinators: Coordinators::default(), formations: Vec::new(), bolts: Registry::default(), fireballs: Registry::default(), vehicles: Registry::default(), patrol_points: Registry::default(), hidden_points: Registry::default(), ashes: Default::default(), shading, outcome: None, melee_wins: [0; TRIBES], melee_deaths: [0; TRIBES], fires_put_out: [0; TRIBES], now: Tick::ZERO, next_id: 1, rng: Lcg(0), tribe_scripts: Vec::new(), ai: Vec::new(), active_flyby: None }
    }

    /// `bare`, with the sandboxes' spells for every player.
    fn sandbox_bare(name: impl Into<String>, terrain: Heightmap, sites: Vec<ReincarnationSite>) -> Self {
        GameMap { spell_books: per_player(SpellBook::sandbox()), ..GameMap::bare(name, terrain, sites) }
    }

    /// An original level: its terrain, sites, trees and buildings. Buildings level their ground
    /// above the sea first, then the sites theirs (`with_shamans`).
    pub fn from_level(level: &Level, name: impl Into<String>, theme: Option<u8>) -> Self {
        let terrain = Heightmap::from_heights(MAP_SIZE, level.heights.clone());
        let sites = sites_from_level(level, &terrain);
        GameMap {
            name: name.into(),
            theme,
            terrain,
            sites,
            tribes: Tribes::from_level(level),
            units: Vec::new(),
            trees: level
                .things
                .iter()
                .filter_map(|t| Some(Tree::new(((t.x as u32 / 512) as i32, (t.z as u32 / 512) as i32), t.tree_type()?, crate::tree::MAX_SIZE).with_angle(t.angle()?)))
                .collect(),
            buildings: Vec::new(),
            granted: Vec::new(),
            totems: Vec::new(),
            wood: Vec::new(),
            mana: per_player(ManaPool::default()),
            spell_books: per_player(SpellBook::demo()),
            build_books: per_player(BuildBook::all(BuildAvailability::Available)),
            campfires: Vec::new(),
            walls: Walls::default(),
            start_camera: None,
            object_bank: 0,
            shaman_omni: false,
            effects: Registry::default(),
            coordinators: Coordinators::default(),
            formations: Vec::new(),
            bolts: Registry::default(),
            fireballs: Registry::default(),
            vehicles: Registry::default(),
            patrol_points: Registry::default(),
            hidden_points: Registry::default(),
            ashes: Default::default(),
            shading: CellShading::new(MAP_SIZE, SunParams::default()),
            outcome: None,
            melee_wins: [0; TRIBES],
            melee_deaths: [0; TRIBES],
            fires_put_out: [0; TRIBES],
            now: Tick::ZERO,
            next_id: 1,
            rng: Lcg(0),
            tribe_scripts: Vec::new(),
            ai: Vec::new(),
            active_flyby: None,
        }
        .with_buildings(buildings_from_level(level))
        .with_totems(totems_from_level(level))
        .with_building_ground()
        .with_shamans()
        .with_people(level)
        .with_shading()
    }

    /// The level's people other than shamans (who spawn at their sites), where they are placed.
    /// Wildmen around a site (`ReincarnationSite::welcomes`) are that tribe's first followers: they
    /// start as its braves (the original converts them when the shaman appears).
    fn with_people(mut self, level: &Level) -> Self {
        let people = level.things.iter().filter(|t| t.kind == pop3_format::level::KIND_PERSON && !t.is_shaman());
        for t in people {
            let Some(kind) = UnitKind::from_person_model(t.model) else { continue };
            let cell = ((t.x as u32 / 512) as i32, (t.z as u32 / 512) as i32);
            let welcomed = (kind == UnitKind::Wildman).then(|| self.sites.iter().find(|s| s.enabled && s.welcomes(cell))).flatten();
            let (owner, kind) = welcomed.map_or((Owner::from_u8(t.owner), kind), |s| (Owner::Tribe(s.owner), UnitKind::Brave));
            self.spawn(owner, kind, (t.x, t.z));
        }
        self
    }

    /// Every constructor's last step, once its terrain is in its final shape: a full `shading` recompute (`bare`
    /// itself does not, so test-only maps built straight from `bare`/`sandbox_bare` stay cheap).
    fn with_shading(mut self) -> Self {
        self.recompute_shading_all();
        self
    }

    /// `shading`'s own recompute, for the cells `touched` reaches: called from the two places `GameMap` already
    /// assembles its `DirtyRect` output (`apply`, `tick`), so no future spell/effect author has to remember it.
    fn recompute_shading(&mut self, touched: &[DirtyRect]) {
        let lava_blend: HashMap<(i32, i32), u8> = self
            .effects
            .iter()
            .filter_map(|(_, e)| if let EffectKind::Lava(lava) = &e.kind { Some(lava.cells_lit()) } else { None })
            .flatten()
            .map(|(pos, blend, _)| (pos, blend))
            .collect();
        self.shading.recompute(&self.terrain, &self.ashes, &lava_blend, touched);
    }

    /// The whole map: constructors' own last step (`with_shading`), once their terrain is in its final shape.
    fn recompute_shading_all(&mut self) {
        let whole = DirtyRect { min: (0, 0), max: (self.terrain.size() as i32, self.terrain.size() as i32) };
        self.recompute_shading(std::slice::from_ref(&whole));
    }

    fn with_buildings(mut self, buildings: Vec<Building>) -> Self {
        for b in buildings {
            self.add_building(b);
        }
        self
    }

    fn with_totems(mut self, totems: Vec<Totem>) -> Self {
        for t in totems {
            self.add_totem(t);
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
        for k in 0..self.sites.len() {
            self.sites[k].flatten_for_spawn(&mut self.terrain);
            let id = self.new_id_of();
            self.units.push(Unit::shaman(id, &self.sites[k]));
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
        let map = Self::from_level(&level, name, header.as_ref().map(|h| h.theme));
        let mut spell_books = header.as_ref().map_or(map.spell_books.clone(), |h| per_player(SpellBook::from_level(h, &level)));
        // The install's own balance numbers (constants.md "Use here"), never shipped: silently keep the
        // defaults (`SpellStats::default_for`) when the file is missing or doesn't parse, same as the `.hdr`
        // above.
        if let Ok(raw) = pop3_format::Constants::load(dat.with_file_name("constant.dat")) {
            for book in spell_books.iter_mut() {
                book.apply_constants(&raw);
                book.fill();
            }
        }
        let build_books = header.as_ref().map_or(map.build_books.clone(), |h| per_player(BuildBook::from_level(h, &level)));
        let start_camera = header.as_ref().and_then(StartCamera::from_header);
        let object_bank = header.as_ref().map_or(0, |h| h.object_bank);
        let shaman_omni = header.as_ref().is_some_and(|h| h.flags.has(pop3_format::header::LevelFlags::SHAMAN_OMNI));
        let tribes = header.as_ref().map_or(map.tribes.clone(), |h| map.tribes.clone().with_allies(h.allies));
        let tribe_scripts = header.as_ref().map_or(Vec::new(), |h| crate::ai_script::load_tribe_scripts(dat, h.ai_scripts));
        let ai = tribe_scripts.iter().map(|ts| crate::ai::NativeAi::new(ts.owner)).collect();
        let mut map = GameMap { spell_books, build_books, start_camera, object_bank, shaman_omni, tribes, tribe_scripts, ai, ..map };
        // "Turn 0" (ai-scripts.md "How a script runs"): a real script's setup block, run once here
        // while `now` is still `Tick::ZERO`, before the game truly starts. `tick()`'s own later calls
        // to `tend_ai_scripts` (turns 1, 2, 3, ...) run the `ELSE { EVERY ... }` branch from then on.
        map.tend_ai_scripts();
        Ok(map)
    }

    /// Deterministic island map from a seed (fallback when no original data exists).
    pub fn generate(seed: u32) -> Self {
        let mut rng = Lcg(seed.max(1));
        let mut terrain = Heightmap::new(MAP_SIZE);
        for _ in 0..40 {
            let c = ((rng.next_u32() % MAP_SIZE as u32) as i32, (rng.next_u32() % MAP_SIZE as u32) as i32);
            let radius = 4 + (rng.next_u32() % 14) as i32;
            let amount = 150 + (rng.next_u32() % 350) as i32;
            terrain.raise(c, radius, amount);
        }
        for h in 0..MAP_SIZE * MAP_SIZE {
            let (x, z) = ((h % MAP_SIZE) as i32, (h / MAP_SIZE) as i32);
            let v = terrain.get(x, z).saturating_sub(120).min(MAX_HEIGHT);
            terrain.set(x, z, v);
        }
        let sites = generated_sites(&terrain);
        GameMap::bare(format!("Generated #{seed}"), terrain, sites).with_shamans().with_trees(seed, 60).with_shading()
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
        let sites = vec![ReincarnationSite::at_cell(TribeId::Blue, (C, C), terrain.get(C, C))];
        GameMap::sandbox_bare("Sandbox: walk", terrain, sites).with_shamans().with_trees(1, 150).with_shading()
    }

    /// Test ground for buildings: a flat island with the player's site at the centre.
    /// - South (in view of the starting camera): one building of every known model (1-19) for the
    ///   player in rows, facing each quarter turn in turn, and a few for tribe 1 (red) to compare
    ///   colours, with some wood pieces between them and the site.
    /// - North: one row per buildable kind (`BUILDABLE` order) showing each `showcase_states`
    ///   column, west to east.
    /// - East: free ground to build on, with braves, a pile of wood and a few trees, a low mound
    ///   (north-east of the braves) to try flattening on, and a camp fire three of the braves go round.
    /// - Further south: a row of built buildings the client blows up in a loop (`SANDBOX_BLASTS`).
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
        let sites = vec![ReincarnationSite::at_cell(TribeId::Blue, (C, C), terrain.get(C, C))];
        let mut map = GameMap::sandbox_bare(SANDBOX_BUILDINGS, terrain, sites).with_shamans();
        let at = |dx: i32, dz: i32| ((C + dx) as u16 * 512 + 256, (C + dz) as u16 * 512 + 256);
        let place = |owner: TribeId, kind: BuildingKind, (x, z): (u16, u16), facing: u8| Building::new(kind, owner, x - 256, z - 256, facing);
        for model in 1..=19u8 {
            let i = model as i32 - 1;
            map.add_building(place(TribeId::Blue, BuildingKind::from_model(model), at(-12 + (i % 7) * 4, 6 + (i / 7) * 4), (i % 4) as u8 * 2));
        }
        for (i, model) in [1u8, 3, 4, 7].iter().enumerate() {
            map.add_building(place(TribeId::Red, BuildingKind::from_model(*model), at(-6 + i as i32 * 4, 19), 0));
        }
        let (cx, cz) = at(0, 3);
        for (dx, dz) in [(0, 0), (90, 40), (-80, 60), (20, -70), (-600, 100), (700, -50)] {
            map.wood.push(WoodPiece::new((cx as i32 + dx) as u16, (cz as i32 + dz) as u16));
        }
        for (row, &kind) in crate::build_book::BUILDABLE.iter().enumerate() {
            for (col, state) in showcase_states(kind).into_iter().enumerate() {
                let (x, z) = at(-22 + 5 * col as i32, -7 - 5 * row as i32);
                map.add_building(Building { x: x - 256, z: z - 256, ..state });
            }
        }
        for (i, &kind) in SANDBOX_BLASTS.iter().enumerate() {
            let (x, z) = sandbox_blast_corner(i);
            map.add_building(Building::new(kind, TribeId::Blue, x, z, 0));
        }
        for n in 0..SANDBOX_FREE_BRAVES {
            map.spawn(TribeId::Blue, UnitKind::Brave, at(20 + n % 10, -2 - n / 10));
        }
        // The showcase's occupied buildings get real people inside.
        let occupied: Vec<Building> = map.buildings.iter().filter(|b| b.inside > 0).cloned().collect();
        for b in occupied {
            let room = if b.kind.capacity() > 0 { b.kind.capacity() } else { b.inside };
            for _ in 0..b.inside.min(room) {
                map.spawn(TribeId::Blue, UnitKind::Brave, b.centre()).inside = Some(crate::unit::Inside { building: b.id, door: b.door() });
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
        if let Some(fire) = map.place_campfire(TribeId::Blue, (C + 21, C + 4)) {
            let braves: Vec<UnitId> = map.units.iter().filter(|u| u.kind == UnitKind::Brave).take(3).map(|u| u.id).collect();
            for c in map.gather(TribeId::Blue, &braves, fire) {
                map.apply(&c);
            }
        }
        map.recompute_shading_all();
        map
    }

    /// Test ground for units: a flat island with the player's site and shaman at the centre, three
    /// of every other kind for the player in columns to the west (one column per kind), one of each
    /// for tribe 1 (red, no shaman) in a row to the east, all in view of the starting camera (south
    /// of the site), and a pond to the north to walk around. Further south in the same columns, one of
    /// each kind per marker drawn over people (`SANDBOX_MARKERS`): low health, shielded, bloodlust,
    /// hypnotised, then all of them at once. Two empty balloons by the firewarrior column, to board.
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
        let sites = vec![ReincarnationSite::at_cell(TribeId::Blue, (C, C), terrain.get(C, C))];
        let mut map = GameMap::sandbox_bare("Sandbox: units", terrain, sites).with_shamans().with_trees(2, 150).with_shading();
        let at = |dx: i32, dz: i32| ((C + dx) as u16 * 512 + 256, (C + dz) as u16 * 512 + 256);
        for (row, &kind) in UnitKind::FOLLOWERS.iter().enumerate() {
            let row = row as i32;
            for n in 0..3 {
                map.spawn(TribeId::Blue, kind, at(-5 - 2 * row, 2 * n - 2));
            }
            map.spawn(TribeId::Red, kind, at(6 + 2 * row, 0));
            for (n, marked) in SANDBOX_MARKERS.iter().enumerate() {
                let u = map.spawn(TribeId::Blue, kind, at(-5 - 2 * row, 5 + 2 * n as i32));
                marked(u);
            }
        }
        {
            use crate::vehicle::{Vehicle, VehicleKind};
            for dz in [-1, 1] {
                map.add_vehicle(Vehicle::new(VehicleKind::Balloon, TribeId::Blue, at(-15, dz), crate::angle::Angle::ZERO));
            }
        }
        map
    }

    /// Test ground for praying: a flat island with the player's site at the centre.
    /// - South: a pyramid of knowledge (neutral, teaching the temple, as in level 3), its door facing
    ///   the site, for the shaman.
    /// - North: one totem of each look (`TotemKind::ALL`) in a row, west to east, each asking for
    ///   something else (`SANDBOX_TOTEMS`), and 8 of the player's braves in front of them.
    pub fn sandbox_worship() -> Self {
        const C: i32 = MAP_SIZE as i32 / 2;
        const ISLAND: i32 = 30;
        let mut terrain = Heightmap::new(MAP_SIZE);
        for z in 0..MAP_SIZE as i32 {
            for x in 0..MAP_SIZE as i32 {
                if (x - C) * (x - C) + (z - C) * (z - C) <= ISLAND * ISLAND {
                    terrain.set(x, z, 64);
                }
            }
        }
        let sites = vec![ReincarnationSite::at_cell(TribeId::Blue, (C, C), terrain.get(C, C))];
        let mut map = GameMap::sandbox_bare("Sandbox: worship", terrain, sites).with_shamans().with_shading();
        let at = |dx: i32, dz: i32| ((C + dx) as u16 * 512 + 256, (C + dz) as u16 * 512 + 256);
        let mut vault = Building::new(BuildingKind::Vault, Owner::Wild, (C * 512) as u16, ((C + 8) * 512) as u16, 0);
        vault.vault = Some(crate::vault::Vault::new(Some(crate::building::Reward::Building(BuildingKind::Temple)), crate::vault::DEFAULT_PRAY_TIME));
        map.add_building(vault);
        for (k, &kind) in TotemKind::ALL.iter().enumerate() {
            let (prayers, shaman_only, summons_angel) = SANDBOX_TOTEMS[k];
            // The stone totem gives once, to show it turning, sinking and gone; the others give again and again.
            let occurrences = u8::from(kind == TotemKind::Totem);
            map.add_totem(Totem { prayers, shaman_only, summons_angel, gifts: vec![sandbox_gift(k)], occurrences, ..Totem::new(kind, at(-12 + 4 * k as i32, -12)) });
        }
        for n in 0..8 {
            map.spawn(TribeId::Blue, UnitKind::Brave, at(-7 + 2 * n, -7));
        }
        map
    }
}

/// The vehicles sandbox's name.
pub const SANDBOX_VEHICLES: &str = "Sandbox: vehicles";

/// The vehicles sandbox's hills, to fly balloons over: offset from the map's middle (cells), radius (cells), height.
pub const SANDBOX_VEHICLE_HILLS: [(i32, i32, i32, i32); 3] = [(-13, -6, 4, 500), (13, 0, 3, 250), (0, 21, 2, 200)];

impl GameMap {
    /// Test ground for vehicles: a flat island with the player's site, its south coast in view of the starting camera,
    /// and a smaller island south across a strait.
    /// - South coast: a built boat hut, its jetty out to sea, and an airship hut, with 8 braves and a pile of wood
    ///   between them to crew them; an empty boat on the water by the shore, a boat with 2 braves aboard, and an
    ///   empty boat out at sea, drifting back to the shore.
    /// - By the site: an empty balloon for the shaman.
    /// - Hills (`SANDBOX_VEHICLE_HILLS`): a tall one west, a lower one east, a mound on the south island.
    /// - The south island: an empty boat of tribe 1 on the water by its shore, to take.
    pub fn sandbox_vehicles() -> Self {
        use crate::vehicle::{Vehicle, VehicleKind};
        const C: i32 = MAP_SIZE as i32 / 2;
        let mut terrain = Heightmap::new(MAP_SIZE);
        let land = |x: i32, z: i32| ((C - 18..=C + 18).contains(&x) && (C - 14..=C + 8).contains(&z)) || ((C - 8..=C + 8).contains(&x) && (C + 18..=C + 24).contains(&z));
        for z in 0..MAP_SIZE as i32 {
            for x in 0..MAP_SIZE as i32 {
                if land(x, z) {
                    terrain.set(x, z, 64);
                }
            }
        }
        for (dx, dz, radius, height) in SANDBOX_VEHICLE_HILLS {
            terrain.raise((C + dx, C + dz), radius, height);
        }
        let sites = vec![ReincarnationSite::at_cell(TribeId::Blue, (C, C - 6), terrain.get(C, C - 6))];
        let mut map = GameMap::sandbox_bare(SANDBOX_VEHICLES, terrain, sites).with_shamans();
        let corner = |dx: i32, dz: i32| (((C + dx) * 512) as u16, ((C + dz) * 512) as u16);
        let at = |dx: i32, dz: i32| crate::slots::cell_centre((C + dx, C + dz));
        let (x, z) = corner(-8, 7);
        map.add_building(Building::new(BuildingKind::BoatHut, TribeId::Blue, x, z, 0));
        let (x, z) = corner(5, 1);
        map.add_building(Building::new(BuildingKind::AirshipHut, TribeId::Blue, x, z, 0));
        for n in 0..8 {
            map.spawn(TribeId::Blue, UnitKind::Brave, at(-4 + n % 4, 1 + n / 4));
        }
        let (px, pz) = at(-1, 3);
        for k in 0..10u16 {
            map.wood.push(WoodPiece::new(px + (k % 5) * 80, pz + (k / 5) * 90));
        }
        for (i, (dx, dz)) in [(12, -10), (14, -8), (13, -12), (15, -11)].into_iter().enumerate() {
            map.trees.push(Tree::new((C + dx, C + dz), i as u8, crate::tree::MAX_SIZE));
        }
        let boat = |owner: TribeId, cell: (i32, i32)| Vehicle::new(VehicleKind::Boat, owner, at(cell.0, cell.1), crate::angle::Angle::new(512));
        map.add_vehicle(boat(TribeId::Blue, (-2, 9)));
        let manned = map.add_vehicle(boat(TribeId::Blue, (4, 9)));
        for _ in 0..2 {
            let u = map.spawn(TribeId::Blue, UnitKind::Brave, at(4, 9));
            u.aboard = Some(manned);
            let id = u.id;
            map.vehicles.get_mut(manned).unwrap().seat(id, TribeId::Blue);
        }
        map.add_vehicle(boat(TribeId::Blue, (10, 13)));
        map.add_vehicle(Vehicle::new(VehicleKind::Balloon, TribeId::Blue, at(3, -4), crate::angle::Angle::ZERO));
        map.add_vehicle(boat(TribeId::Red, (0, 16)));
        map.update_walls();
        map.recompute_shading_all();
        map
    }
}

/// Islands of the spells sandbox (`GameMap::sandbox_spells`): centre offset from the map's middle and radius, cells.
/// The south-west islet sits at `(-20, 16)`, not further out at its originally-placed `(-24, 20)`: that position
/// left a ~12.2-cell gap to the home island's own near shore, past Land Bridge's own ~8.5-cell range at this
/// island height (`sandbox_spells_land_bridge_reaches_the_south_west_islet`) - the one spell the doc comment below
/// names this islet for. `(-20, 16)` narrows the gap to ~6.6 cells, a comfortable margin under range, the same
/// spirit as `BRIDGE_TARGET_CELL_OFFSET`'s own margin in `sandbox_bridge`.
const SPELL_ISLANDS: [((i32, i32), i32); 4] = [((0, 0), 14), ((28, 0), 10), ((0, -30), 10), ((-20, 16), 5)];

impl GameMap {
    /// Test ground for spells, every one cast at will (`SpellBook::unlimited`, Teleport too), on islands
    /// (`SPELL_ISLANDS`) a few cells of sea apart, to bridge or teleport across:
    /// - Middle: the player's site and shaman; south, 3 of each follower kind of the player, one group per kind 5 cells
    ///   apart, west to east (`UnitKind::FOLLOWERS`); north, the same for tribe 1.
    /// - East: a village of tribe 1 (red): huts with people inside, a drum tower, a warrior hut, braves and
    ///   warriors around, to strike.
    /// - North: a hill to flatten or erode, with a few people of tribe 2 on its slopes.
    /// - South-west: an empty islet.
    pub fn sandbox_spells() -> Self {
        const C: i32 = MAP_SIZE as i32 / 2;
        let mut terrain = Heightmap::new(MAP_SIZE);
        for z in 0..MAP_SIZE as i32 {
            for x in 0..MAP_SIZE as i32 {
                if SPELL_ISLANDS.iter().any(|&((ox, oz), r)| (x - C - ox).pow(2) + (z - C - oz).pow(2) <= r * r) {
                    terrain.set(x, z, 64);
                }
            }
        }
        terrain.raise((C, C - 31), 6, 420);
        let sites = vec![ReincarnationSite::at_cell(TribeId::Blue, (C, C), terrain.get(C, C))];
        let mut map = GameMap { spell_books: per_player(SpellBook::unlimited()), ..GameMap::bare("Sandbox: spells", terrain, sites) }.with_shamans();
        map.trees = scatter(&map.terrain, 3, &[(C, C)], 40);
        let at = |dx: i32, dz: i32| ((C + dx) as u16 * 512 + 256, (C + dz) as u16 * 512 + 256);
        let corner = |dx: i32, dz: i32| ((C + dx) as u16 * 512, (C + dz) as u16 * 512);
        for (k, &kind) in UnitKind::FOLLOWERS.iter().enumerate() {
            let dx = -10 + 5 * k as i32;
            for (ox, oz) in [(0, 0), (1, 0), (0, 1)] {
                map.spawn(TribeId::Blue, kind, at(dx + ox, 6 + oz));
                map.spawn(TribeId::Red, kind, at(dx + ox, -8 + oz));
            }
        }
        let village = [(BuildingKind::Hut { size: 1 }, (25, -4), 2), (BuildingKind::Hut { size: 2 }, (31, -4), 3), (BuildingKind::Hut { size: 3 }, (25, 3), 4), (BuildingKind::DrumTower, (31, 3), 1), (BuildingKind::WarriorTraining, (28, -8), 0)];
        for (kind, (dx, dz), people) in village {
            let (x, z) = corner(dx, dz);
            let b = Building::new(kind, TribeId::Red, x, z, 4);
            let (id, door, centre) = (map.add_building(b.clone()), b.door(), b.centre());
            for _ in 0..people {
                map.spawn(TribeId::Red, UnitKind::Brave, centre).inside = Some(crate::unit::Inside { building: id, door });
            }
        }
        for n in 0..4 {
            map.spawn(TribeId::Red, UnitKind::Brave, at(22 + 2 * n, 7));
            map.spawn(TribeId::Red, UnitKind::Warrior, at(23 + 2 * n, 0));
        }
        for (n, kind) in [UnitKind::Brave, UnitKind::Brave, UnitKind::Warrior, UnitKind::Preacher].into_iter().enumerate() {
            map.spawn(TribeId::Yellow, kind, at(-4 + 3 * n as i32, -26));
        }
        let buildings = &map.buildings;
        map.trees.retain(|t| !buildings.iter().any(|b| b.covers((t.x, t.z), 512)));
        map.update_walls();
        map.count_inside();
        map.recompute_shading_all();
        map
    }

    /// Islands of `GameMap::sandbox_bridge`: centre offset from the map's middle and radius, cells. Bigger and
    /// further apart than `SPELL_ISLANDS`' own pair, for a realistic-looking Land Bridge cast (not a teleport-range
    /// demo): a proper 6-cell stretch of open sea between their facing shores.
    const BRIDGE_ISLANDS: [((i32, i32), i32); 2] = [((-18, 0), 16), ((18, 0), 14)];
    /// Flat island height: modest (not `SPELL_ISLANDS`' sea-level 64), enough to lift `SpellKind::LandBridge`'s
    /// range (`cast_range::range`) from its sea-level ~8.5 cells to ~9.7, comfortable margin over the 8-cell
    /// crossing below without the sheer, mesa-like cliff a much taller flat island renders as.
    const BRIDGE_ISLAND_HEIGHT: u16 = 220;
    /// The shaman's spawn cell: 2 cells inland of the home island's own facing (east) shore.
    const BRIDGE_SITE_CELL_OFFSET: (i32, i32) = (-4, 0);
    /// A target cell right at the far island's facing (west) shore, 8 cells from the shaman's spawn - within
    /// `BRIDGE_ISLAND_HEIGHT`'s cast range with margin (`sandbox_bridge_crosses_the_sea_within_cast_range`).
    pub const BRIDGE_TARGET_CELL_OFFSET: (i32, i32) = (4, 0);
    /// Level 1's own theme (`theme_char(12)` = 'c'): a real, already-validated texture set (ground_material.rs's
    /// sRGB fix and terrain-textures.md "Our bake" were both checked against it), used here (unlike the other
    /// sandboxes' `theme: None`) when an original install is found, for original-asset screenshots/videos.
    const BRIDGE_THEME: u8 = 12;

    /// Sandbox for the Land Bridge spell (`POP3_START=sandbox-bridge`, or Sandbox > Bridge in the menu) - two
    /// islands (`BRIDGE_ISLANDS`) a realistic stretch of open sea apart, the
    /// player's shaman alone on the home island's shore. Cast with (cells, `BRIDGE_TARGET_CELL_OFFSET` added to
    /// the map's middle): `POP3_START=sandbox-bridge BRIDGE=68,64 BRIDGE_TICKS=0 GAME_SPEED=1 just video`, 64
    /// ticks (`land_bridge::DURATION`) at `TICKS_PER_SECOND` = ~5.3 s to watch it finish; `BRIDGE_TICKS=51` for a
    /// stopped-at-80% screenshot instead. Textured with `BRIDGE_THEME` when an original install is found
    /// (`--no-original` still falls back to the procedural theme, `world::original_theme`).
    pub fn sandbox_bridge() -> Self {
        const C: i32 = MAP_SIZE as i32 / 2;
        let mut terrain = Heightmap::new(MAP_SIZE);
        for z in 0..MAP_SIZE as i32 {
            for x in 0..MAP_SIZE as i32 {
                if Self::BRIDGE_ISLANDS.iter().any(|&((ox, oz), r)| (x - C - ox).pow(2) + (z - C - oz).pow(2) <= r * r) {
                    terrain.set(x, z, Self::BRIDGE_ISLAND_HEIGHT);
                }
            }
        }
        let site_cell = (C + Self::BRIDGE_SITE_CELL_OFFSET.0, C + Self::BRIDGE_SITE_CELL_OFFSET.1);
        let sites = vec![ReincarnationSite::at_cell(TribeId::Blue, site_cell, terrain.get(site_cell.0, site_cell.1))];
        let mut map = GameMap { theme: Some(Self::BRIDGE_THEME), spell_books: per_player(SpellBook::unlimited()), ..GameMap::bare("Sandbox: bridge", terrain, sites) }.with_shamans();
        map.trees = scatter(&map.terrain, 3, &[site_cell], 20);
        map.recompute_shading_all();
        map
    }

    pub fn site_of(&self, owner: TribeId) -> Option<&ReincarnationSite> {
        self.sites.iter().find(|s| s.owner == owner && s.enabled)
    }

    /// Disables `owner`'s reincarnation site in place: its position never moves, so there's nothing
    /// to remember for `restore_reincarnation` to bring back later. A no-op if already disabled, or if
    /// `owner` never had a site at all.
    pub(crate) fn remove_reincarnation(&mut self, owner: TribeId) {
        if let Some(s) = self.sites.iter_mut().find(|s| s.owner == owner) {
            s.enabled = false;
        }
    }

    /// Re-enables a site disabled by `remove_reincarnation`, re-reading its height from the live
    /// terrain (the ground may have changed while it was off) unless that cell is underwater now, in
    /// which case the site keeps whatever height it last had rather than reincarnating into the sea.
    /// A no-op if it wasn't disabled (including: never existed at all).
    pub(crate) fn restore_reincarnation(&mut self, owner: TribeId) {
        let Some(i) = self.sites.iter().position(|s| s.owner == owner && !s.enabled) else { return };
        let (x, z) = self.sites[i].cell();
        if !self.terrain.is_water(x, z) {
            self.sites[i].height = self.terrain.get(x, z);
        }
        self.sites[i].enabled = true;
    }

    pub fn shaman_of(&self, owner: TribeId) -> Option<&Unit> {
        self.units.iter().find(|u| u.owner == owner && u.kind == UnitKind::Shaman)
    }

    fn shaman_mut(&mut self, owner: TribeId) -> Option<&mut Unit> {
        self.units.iter_mut().find(|u| u.owner == owner && u.kind == UnitKind::Shaman)
    }

    /// Uses one cast of `kind` from the player's book; false when there is none. No kind: nothing to use.
    fn use_charge(&mut self, player: TribeId, kind: Option<SpellKind>) -> bool {
        let Some(kind) = kind else { return true };
        self.spell_books.get_mut(player as usize).is_some_and(|b| b.cast(kind))
    }

    /// Whether `player` can cast `spell` here and now: never while their shaman is `locked` (inside a
    /// vault, tumbling or lifted); Teleport needs a living shaman and ground she can walk at the target; a
    /// spell starting at the caster (land bridge) needs a living shaman on her feet, never aboard a vehicle, unless the
    /// player is a god (`shaman_omni`), and a land bridge a target on land (the water on its way is raised); other spells always
    /// apply. Out of range (`in_cast_range`), she first walks into it (`walk_to_cast`).
    pub fn can_cast(&self, player: TribeId, spell: &Spell) -> bool {
        if self.shaman_locked(player) {
            return false;
        }
        let shaman_alive = self.shaman_of(player).is_some_and(Unit::is_alive);
        let aboard = self.shaman_of(player).is_some_and(|u| u.aboard.is_some());
        match *spell {
            Spell::Teleport { to } => {
                let cell = ((to.0 as u32 / 512) as i32, (to.1 as u32 / 512) as i32);
                shaman_alive && self.ground().passable(Mobility::Walk, cell)
            }
            Spell::LandBridge { to, .. } => (self.shaman_omni || (shaman_alive && !aboard)) && !crate::unit::is_sea(&self.terrain, to),
            Spell::Armageddon { .. } => self.armageddon().is_none(),
            _ => true,
        }
    }

    /// Where `u` stands, in height units (the original reads its z): the level of the building it is in, its
    /// vehicle's (`Vehicle::altitude`: a balloon's is high), the ground under it otherwise.
    pub fn altitude(&self, u: &Unit) -> u16 {
        if let Some(v) = u.aboard.and_then(|v| self.vehicle(v)) {
            return v.altitude(&self.terrain);
        }
        let height = match u.inside.and_then(|i| self.building(i.building)) {
            Some(b) => b.level as i32,
            None => self.terrain.height_at(u.x as u32, u.z as u32, WORLD_UNITS_PER_CELL),
        };
        height.clamp(0, MAX_HEIGHT as i32) as u16
    }

    /// `kind`'s own base range (`SpellSlot::stats`, spells.md "Original balance"), read through `player`'s
    /// own book since that's where the resolved `SpellStats` live (`SpellBook::apply_constants`).
    fn spell_base_range(&self, player: TribeId, kind: SpellKind) -> Option<u32> {
        self.spell_books.get(player as usize)?.slot(kind)?.stats.range
    }

    /// How far `player`'s shaman casts `kind` from where she stands (`cast_range`): None when there is no
    /// limit (the spell has none, or the player is a god), or no shaman free to cast (dead or `locked`: off the
    /// ground she casts nothing until she lands).
    pub fn cast_range(&self, player: TribeId, kind: SpellKind) -> Option<u32> {
        let base = self.spell_base_range(player, kind).filter(|_| !self.shaman_omni)?;
        let shaman = self.shaman_of(player).filter(|u| u.is_alive() && !self.shaman_locked(player))?;
        let in_tower = shaman.inside.and_then(|i| self.building(i.building)).is_some_and(|b| b.kind == BuildingKind::DrumTower && b.stage() == crate::building::Stage::Built);
        Some(crate::cast_range::range(base, self.altitude(shaman), in_tower))
    }

    /// Whether `player` can aim `kind` at `to` (world units): within their shaman's `cast_range`, anywhere
    /// for a spell without a limit or on a level where the player is a god; never without a shaman free to
    /// cast otherwise.
    pub fn in_cast_range(&self, player: TribeId, kind: SpellKind, to: (u16, u16)) -> bool {
        if self.shaman_omni || self.spell_base_range(player, kind).is_none() {
            return true;
        }
        let (Some(shaman), Some(range)) = (self.shaman_of(player), self.cast_range(player, kind)) else { return false };
        crate::cast_range::within((shaman.x, shaman.z), to, range)
    }

    /// The nearest own followers (`UnitKind::FOLLOWERS`, not the shaman or a wildman, nor already a Ghost Army
    /// copy) in the 3 × 3 cells (`nearest::in_block`) around `at`, as they stand when the cast releases, each
    /// touched by `set` (`release`'s `MagicalShield`/`Bloodlust`/`Invisibility` arms). One not already `fresh` for
    /// this spell sorts after every one that is (`nearest::nearest_n_fresh_first`, the shared selector,
    /// pop3-rev-analysis.md 0x515E30): a recast reaches fresh followers first, and only refreshes an
    /// already-touched one's timer once there is nobody fresh left among the nearest `n` (confirmed,
    /// docs/specs/spells.md "Magical Shield").
    fn cast_nearest_status(&mut self, at: (u16, u16), player: TribeId, n: u32, fresh: impl Fn(&Unit) -> bool, set: impl Fn(&mut Unit)) {
        let size = self.terrain.size() as i32;
        let pred = |u: &Unit| u.owner == player && UnitKind::FOLLOWERS.contains(&u.kind) && u.is_alive() && !u.statuses.ghost && crate::nearest::in_block(at, (u.x, u.z), size);
        for id in crate::nearest::nearest_n_fresh_first(&self.units, at, n as usize, pred, fresh) {
            if let Some(u) = self.unit_mut(id) {
                set(u);
            }
        }
    }

    /// `spell` as it takes effect: a land bridge starts at the caster's shaman, as she stands when the command
    /// runs, unless the player is a god (then where it was aimed from, the view).
    fn from_caster(&self, player: TribeId, spell: Spell) -> Spell {
        match spell {
            Spell::LandBridge { to, .. } if !self.shaman_omni => match self.shaman_of(player) {
                Some(u) => Spell::LandBridge { from: ((u.x as u32 / WORLD_UNITS_PER_CELL) as i32, (u.z as u32 / WORLD_UNITS_PER_CELL) as i32), to },
                None => spell,
            },
            _ => spell,
        }
    }

    /// Where `spell` is aimed, world units: the centre of a target cell, a target height point itself; None for the
    /// editor's own Raise brush (not cast from a book).
    pub fn spell_target(&self, spell: &Spell) -> Option<(u16, u16)> {
        let n = self.terrain.size() as i32;
        let world = |(x, z): crate::spell::Cell, centre: u32| {
            let at = |c: i32| (c.rem_euclid(n) as u32 * WORLD_UNITS_PER_CELL + centre) as u16;
            (at(x), at(z))
        };
        match *spell {
            Spell::Teleport { to } => Some(to),
            Spell::Blast { at }
            | Spell::Lightning { at }
            | Spell::Armageddon { at }
            | Spell::Swamp { at }
            | Spell::Convert { at }
            | Spell::Hypnotism { at }
            | Spell::Volcano { at }
            | Spell::Firestorm { at }
            | Spell::Earthquake { at }
            | Spell::Swarm { at }
            | Spell::MagicalShield { at }
            | Spell::Bloodlust { at }
            | Spell::Invisibility { at } => Some(at),
            Spell::LandBridge { to, .. } => Some(world(to, WORLD_UNITS_PER_CELL / 2)),
            Spell::Flatten { at } | Spell::Erode { at } => Some(world(at, 0)),
            Spell::Raise { .. } => None,
        }
    }

    /// Where `player` aims `spell` when it is beyond their living shaman's reach (`in_cast_range`). Without one the
    /// spell takes effect at once.
    fn out_of_range(&self, player: TribeId, spell: &Spell) -> Option<(u16, u16)> {
        self.shaman_of(player).filter(|u| u.is_alive())?;
        let (kind, to) = (spell.kind()?, self.spell_target(spell)?);
        (!self.in_cast_range(player, kind, to)).then_some(to)
    }

    /// The player's shaman walks towards where `spell` is aimed, to cast it once within range
    /// (`cast_when_in_range`), if she could cast it now (`can_cast`, a charge left, none used yet): as close to the
    /// target as she can get, reachable or not (another island, the sea): the point nearest it of the cell nearest it
    /// among those she can get to as she moves (`Unit::mobility` on foot; aboard a vehicle, it takes her: a balloon
    /// anywhere, a boat over the sea; `path::nearest_reachable`, `path::closest_point_in_cell`).
    fn walk_to_cast(&mut self, player: TribeId, spell: Spell) {
        let charged = spell.kind().is_some_and(|k| self.spell_books.get(player as usize).and_then(|b| b.slot(k)).is_some_and(|s| s.can_cast()));
        let (Some(to), true) = (self.spell_target(&spell), charged && self.can_cast(player, &spell)) else { return };
        if let Some(vehicle) = self.shaman_of(player).and_then(|u| u.aboard) {
            if self.shaman_mut(player).is_some_and(|u| u.ride_to_cast(spell)) {
                self.move_vehicle(vehicle, to);
            }
            return;
        }
        let Some((from, mobility)) = self.shaman_of(player).map(|u| (u.cell(), u.mobility())) else { return };
        let goal = (crate::unit::cell_of(to.0), crate::unit::cell_of(to.1));
        let closest = crate::path::nearest_reachable(&self.ground(), mobility, from, goal);
        let to = crate::path::closest_point_in_cell(closest, to);
        if let Some(u) = self.shaman_mut(player) {
            u.walk_to_cast(spell, to);
        }
    }

    /// Shamans walking (or carried by their vehicle) to cast (`walk_to_cast`) stop and cast once within range; one
    /// that stopped short gives up.
    fn cast_when_in_range(&mut self) -> Vec<DirtyRect> {
        let stopped = |u: &Unit| match u.aboard.and_then(|v| self.vehicle(v)) {
            Some(v) => !matches!(v.state, crate::vehicle::State::Moving { .. }),
            None => u.action == Action::Idle,
        };
        let walking: Vec<(TribeId, Spell, bool)> = self
            .units
            .iter()
            .filter(|u| u.kind == UnitKind::Shaman)
            .filter_map(|u| u.walking_to_cast().map(|spell| (u.owner.tribe().expect("a shaman is never a wildman"), spell, stopped(u))))
            .collect();
        let mut dirty = Vec::new();
        for (player, spell, stopped) in walking {
            if self.out_of_range(player, &spell).is_none() {
                self.order(player, Order::Stop);
                dirty.extend(self.apply(&Command::Cast { player, spell }));
            } else if stopped {
                self.order(player, Order::Stop);
            }
        }
        dirty
    }

    /// A shaman's own death (pop3-rev-analysis.md "Shaman death"): her tribe loses `SHAMEN_DEAD_MANA_PCT_LOST`
    /// (25 %, exe-confirmed) of `base` from its bank. `base` is the bank itself here - the file's own "needed +
    /// stored building mana" alternative only applies to a controller-type-2 ("human") tribe, and no controller
    /// type is modelled yet (the same gap `training::train_cost` already has). The killer's own share
    /// (`SHAMEN_DEAD_MANA_PCT_GAIN`) is exe-confirmed 0 %, not the file's 25 % - always nothing to deliver, so the
    /// delayed reservoir it would go through (TODO.md "Mana") is left unimplemented rather than built to move 0.
    fn shaman_died(&mut self, i: usize) {
        let Some(owner) = self.units[i].owner.tribe() else { return };
        let Some(pool) = self.mana.get_mut(owner.as_usize()) else { return };
        let base = pool.bank;
        pool.bank = pool.bank.saturating_sub(crate::mana::Mana(base.0 * crate::mana::SHAMEN_DEAD_MANA_PCT_LOST as u32 / 100));
    }

    /// `spell` cast by `player` takes effect now: its effect starts (ticking from the next turn), or the terrain
    /// brush applies (the region it changed).
    fn release(&mut self, player: TribeId, spell: Spell) -> Option<DirtyRect> {
        match spell {
            Spell::Blast { at } => {
                let from = self.shaman_of(player).map(|u| (u.x, u.z, self.altitude(u) as i32));
                match from {
                    Some((x, z, height)) => {
                        let ground_at = self.terrain.height_at(at.0 as u32, at.1 as u32, WORLD_UNITS_PER_CELL);
                        let id = self.new_id_of();
                        self.fireballs.insert(id, crate::blast::fireball(player, (x, z), height, at, ground_at));
                    }
                    // No living shaman to fly the shot from (she died between cast and release): keep the old
                    // instant-effect behaviour rather than losing the spell.
                    None => {
                        self.add_effect(crate::blast::effect(player, at));
                    }
                }
                None
            }
            Spell::Lightning { at } => {
                self.add_effect(crate::lightning::effect(player, at));
                None
            }
            Spell::Swamp { at } => {
                self.add_effect(crate::swamp::effect(player, at));
                None
            }
            Spell::Swarm { at } => {
                self.add_effect(crate::swarm::effect(player, at));
                None
            }
            Spell::MagicalShield { at } => {
                self.cast_nearest_status(
                    at,
                    player,
                    crate::status::SHIELD_NUM_PEOPLE,
                    |u| !u.statuses.shield.is_on(),
                    |u| u.statuses.shield = crate::status::Timed::steps(crate::status::SHIELD_STEPS),
                );
                None
            }
            Spell::Bloodlust { at } => {
                self.cast_nearest_status(
                    at,
                    player,
                    crate::status::BLOODLUST_NUM_PEOPLE,
                    |u| !u.statuses.bloodlust.is_on(),
                    |u| u.statuses.bloodlust = crate::status::Timed::steps(crate::status::BLOODLUST_STEPS),
                );
                None
            }
            Spell::Invisibility { at } => {
                self.cast_nearest_status(
                    at,
                    player,
                    crate::status::INVIS_NUM_PEOPLE,
                    |u| !u.statuses.invisible.is_on(),
                    |u| u.statuses.invisible = crate::status::Timed::steps(crate::status::INVIS_STEPS),
                );
                None
            }
            Spell::Convert { at } => {
                self.add_effect(crate::convert::effect(player, at));
                None
            }
            Spell::Hypnotism { at } => {
                self.add_effect(crate::hypnotism::effect(player, at));
                None
            }
            Spell::Volcano { at } => {
                self.add_effect(crate::volcano::effect(player, at, &self.terrain));
                None
            }
            Spell::Firestorm { at } => {
                self.add_effect(crate::firestorm::effect(player, at));
                None
            }
            Spell::Earthquake { at } => {
                self.add_effect(crate::earthquake::effect(player, at));
                None
            }
            Spell::LandBridge { from, to } => {
                self.add_effect(crate::land_bridge::effect(player, from, to, &self.terrain));
                None
            }
            Spell::Flatten { at } => {
                self.add_effect(crate::flatten::effect(player, at, &self.terrain));
                None
            }
            Spell::Erode { at } => {
                self.add_effect(crate::erode::effect(player, at, &self.terrain));
                None
            }
            Spell::Armageddon { at } => {
                self.start_armageddon(at);
                None
            }
            _ => spell.cast(&mut self.terrain),
        }
    }

    /// Apply a player's command; returns the terrain region it changed, if any. Casting a spell
    /// makes the caster's shaman do her cast jump (Teleport: she moves when it ends); a spell that
    /// cannot be cast (`can_cast`) does nothing. Rejected outright while an Armageddon match is resolving, or once
    /// the match has ended (`outcome`) — the simulation itself keeps running past that point (the "victory lap"
    /// epilogue, `outcome.rs`), but nothing can act on it any more.
    pub fn apply(&mut self, command: &Command) -> Option<DirtyRect> {
        let dirty = self.apply_inner(command);
        if let Some(rect) = dirty {
            self.recompute_shading(&[rect]);
        }
        dirty
    }

    fn apply_inner(&mut self, command: &Command) -> Option<DirtyRect> {
        if self.armageddon().is_some() || self.outcome.is_some() {
            return None;
        }
        match command.clone() {
            Command::Cast { player, spell } if self.out_of_range(player, &spell).is_some() => {
                self.walk_to_cast(player, spell);
                None
            }
            Command::Cast { player, spell } if !self.can_cast(player, &spell) || !self.use_charge(player, spell.kind()) => None,
            Command::Cast { player, spell: Spell::Teleport { to } } => {
                if let Some(u) = self.shaman_mut(player) {
                    u.cast_teleport(to);
                }
                None
            }
            Command::Cast { player, spell } => {
                let spell = self.from_caster(player, spell);
                let held = spell.kind().is_some() && self.shaman_mut(player).is_some_and(|u| u.cast_spell(spell));
                if held {
                    return None;
                }
                self.order(player, Order::Cast);
                self.release(player, spell)
            }
            Command::CastSelf { player, kind } => {
                // Every spell is aimed now (`ground_spell`); not-yet-implemented kinds still get the cast
                // animation with no effect, as before.
                if !self.shaman_locked(player) && self.use_charge(player, Some(kind)) {
                    self.order(player, Order::Cast);
                }
                None
            }
            Command::PauseSpell { player, kind, paused } => {
                if let Some(book) = self.spell_books.get_mut(player as usize) {
                    book.set_paused(kind, paused);
                }
                None
            }
            Command::Order { player, order } => {
                if !self.shaman_locked(player) {
                    self.order(player, order);
                }
                None
            }
            Command::OrderUnit { player, unit, order } => {
                if let Some(i) = self.unit_index(unit).filter(|&i| self.units[i].owner == player && !self.locked(i)) {
                    if self.units[i].aboard.is_some() {
                        self.order_aboard(i, order);
                        return None;
                    }
                    self.units[i].clear_queue();
                    self.units[i].clear_patrol();
                    self.start_order(i, order);
                }
                None
            }
            Command::QueueOrder { player, unit, order } => {
                if let Some(i) = self.unit_index(unit).filter(|&i| self.units[i].owner == player && !self.locked(i) && self.units[i].aboard.is_none()) {
                    self.units[i].clear_patrol();
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
            Command::PlaceTree { at, .. } => {
                self.place_tree(tree::cell_at(at));
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
            Command::Dismantle { player, building, on } => {
                self.set_dismantling(player, building, on);
                None
            }
            Command::MoveVehicle { player, vehicle, to } => {
                if self.vehicle(vehicle).is_some_and(|v| v.owner == player && !v.passengers.is_empty()) {
                    self.move_vehicle(vehicle, to);
                }
                None
            }
            Command::Unload { player, vehicle } => {
                if self.vehicle(vehicle).is_some_and(|v| v.owner == player) {
                    self.unload(vehicle);
                }
                None
            }
            Command::ClearSwamp { player, swamp } => {
                self.clear_swamp(player, swamp);
                None
            }
            Command::SetPatrol { player, units, at } => {
                let action = crate::patrol::PatrolAction::GoTo(self.add_hidden_point(at.0, at.1));
                self.set_patrol(player, &units, action);
                None
            }
            Command::SetPatrolTarget { player, units, target } => {
                self.set_patrol(player, &units, crate::patrol::PatrolAction::Attack(target));
                None
            }
            Command::AddPatrolPoint { player, units, at } => {
                let action = crate::patrol::PatrolAction::GoTo(self.add_hidden_point(at.0, at.1));
                self.add_patrol_point_to(player, &units, action);
                None
            }
            Command::AddPatrolTargetPoint { player, units, target } => {
                self.add_patrol_point_to(player, &units, crate::patrol::PatrolAction::Attack(target));
                None
            }
            Command::ReleasePatrol { player, units } => {
                self.release_patrol(player, &units);
                None
            }
            Command::CancelGuard { player } => {
                for i in 0..self.units.len() {
                    if self.units[i].owner == player && matches!(self.units[i].action, Action::Guarding { .. }) {
                        self.units[i].action = Action::Idle;
                    }
                }
                None
            }
            Command::Formation { player, units, to } => {
                self.form_up(player, &units, to);
                None
            }
            Command::CancelFlyby { .. } => {
                self.cancel_flyby();
                None
            }
        }
    }

    /// `Command::Formation`: the player's own units among `units` still walking (so not already busy with
    /// something else by the time this applies), up to `formation::MAX_MEMBERS`, become one new formation seeded at
    /// `to` ("Formations" [exe]). A lone walker needs no spacing: a no-op below two. Each walker's own move order
    /// (`move_orders`, applied before this one) already took it out of whatever formation it was in through
    /// `start_order`'s `leave_formation` - a unit reassigned elsewhere leaves its old formation right away rather
    /// than having to finish walking to its old slot first.
    fn form_up(&mut self, player: TribeId, units: &[UnitId], to: (u16, u16)) {
        let members: Vec<UnitId> = units
            .iter()
            .copied()
            .filter(|&id| self.unit(id).is_some_and(|u| u.owner == player && matches!(u.action, Action::Walking { .. })))
            .take(crate::formation::MAX_MEMBERS)
            .collect();
        if members.len() > 1 {
            self.formations.push(crate::formation::Formation::new(player, to, members));
        }
    }

    /// Leaves the formation it was part of, if any (`start_order`: a new order takes it over, so it must not keep
    /// being nudged back towards its old slot). A formation left with nobody in it is removed right away.
    fn leave_formation(&mut self, id: UnitId) {
        for f in &mut self.formations {
            f.members.retain(|&(m, _)| m != id);
        }
        self.formations.retain(|f| !f.members.is_empty());
    }

    /// Ends the player's own swamp `swamp` early (right click + Ctrl on it, like a blueprint put away): as if
    /// its own tally had already reached its cap (`tend_swamps`), gone by the next tick's cleanup
    /// (`tend_effects`). Ignored for a swamp not theirs, or gone already.
    pub(crate) fn clear_swamp(&mut self, player: TribeId, swamp: Id<Effect>) {
        if let Some(e) = self.effects.get_mut(swamp).filter(|e| e.owner == player && matches!(e.kind, EffectKind::Swamp(_))) {
            e.left = Countdown::with_left(e.left.total(), Ticks::ZERO);
        }
    }

    /// Lights a camp fire for `player` in `cell` if it can be (`campfire::can_place`); its id.
    pub fn place_campfire(&mut self, player: TribeId, cell: (i32, i32)) -> Option<u32> {
        if !campfire::can_place(self, cell) {
            return None;
        }
        let id = self.campfires.iter().map(|f| f.id).max().unwrap_or(0) + 1;
        self.campfires.push(Campfire::new(id, player, cell));
        Some(id)
    }

    /// Plants a tree in `cell` for the world editor's object-placement brush (`Command::PlaceTree`); false
    /// (nothing done) when a living tree (not size 0) is already there - no building/shore check yet, this is
    /// an editor-only tool, not real placement rules.
    pub fn place_tree(&mut self, cell: (i32, i32)) -> bool {
        if self.trees.iter().any(|t| t.cell() == cell && t.is_visible()) {
            return false;
        }
        self.trees.push(Tree::new(cell, 0, tree::MAX_SIZE));
        true
    }

    /// Puts out `player`'s camp fire whose cell holds world point `p`, if any: the units going to it
    /// or round it stop (idle).
    pub fn remove_campfire(&mut self, player: TribeId, p: (u16, u16)) {
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
    pub fn gather(&self, player: TribeId, units: &[UnitId], fire: u32) -> Vec<Command> {
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

    /// An order to the player's shaman (aboard a vehicle: `order_aboard`).
    fn order(&mut self, player: TribeId, order: Order) {
        match self.units.iter().position(|u| u.owner == player && u.kind == UnitKind::Shaman) {
            Some(i) if self.units[i].aboard.is_some() && order != Order::Cast => self.order_aboard(i, order),
            Some(i) => self.units[i].order(order),
            None => {}
        }
    }

    /// Spots a unit cannot stop on: every spot of a visible tree's cell, and every living unit's but
    /// `except`'s (where it stands, or where it is going).
    pub fn taken_spots(&self, except: &[UnitId]) -> BTreeSet<slots::Spot> {
        let trees = self.trees.iter().filter(|t| t.is_visible()).flat_map(|t| slots::cell_spots(t.cell()));
        let units = self.units.iter().filter(|u| u.is_alive() && !except.contains(&u.id)).map(|u| match u.action {
            Action::Walking { to } | Action::Stranded { to } => slots::spot_of(to),
            _ => slots::spot_of((u.x, u.z)),
        });
        trees.chain(units).collect()
    }

    /// Orders sending the player's living `units` to `to`, each to a free spot of its own around it
    /// (`slots::dispatch`).
    pub fn dispatch(&self, player: TribeId, units: &[UnitId], to: (u16, u16)) -> Vec<Command> {
        let movers: Vec<(UnitId, (u16, u16))> =
            self.units.iter().filter(|u| u.owner == player && u.is_alive() && units.contains(&u.id)).map(|u| (u.id, (u.x, u.z))).collect();
        let ids: Vec<UnitId> = movers.iter().map(|m| m.0).collect();
        slots::dispatch(&self.ground(), &movers, to, &self.taken_spots(&ids))
            .into_iter()
            .map(|(unit, (x, z))| Command::OrderUnit { player, unit, order: Order::MoveTo { x, z } })
            .collect()
    }

    /// One tick of the game: `commands` (that tick's, `schedule::Schedule::take`) apply, then the tick runs;
    /// returns the ground changed.
    pub fn step(&mut self, commands: &[Command]) -> Vec<DirtyRect> {
        let mut dirty: Vec<DirtyRect> = commands.iter().filter_map(|c| self.apply(c)).collect();
        dirty.extend(self.tick());
        dirty
    }

    /// `ticks` ticks in a row; returns the ground they levelled.
    pub fn run(&mut self, ticks: Ticks) -> Vec<DirtyRect> {
        (0..ticks.get()).flat_map(|_| self.tick()).collect()
    }

    /// One simulation tick for every unit, in order; returns the ground levelled by reincarnations.
    /// A unit that arrives (walking, or landing from a teleport) on a taken spot moves on to the
    /// nearest free one.
    pub fn tick(&mut self) -> Vec<DirtyRect> {
        self.now = self.now.next();
        debug_assert!(self.units.windows(2).all(|w| w[0].id < w[1].id), "units sorted by id");
        let mut dirty = Vec::new();
        // Armageddon (armageddon.rs): while a match is active, the economy and combat-seeking are suspended —
        // nothing to tend to (buildings/totems/vehicles are gone once the full mechanic lands) and nothing should
        // start a fresh fight before the match's own resolution decides things. Once the match has ended
        // (outcome.rs), everyone already stood down to Idle the instant it was declared, so the same freeze applies
        // for the rest of the game: nothing trains, builds, wrecks, fights or reincarnates in the "victory lap".
        let armageddon = self.armageddon().is_some();
        let frozen = armageddon || self.outcome.is_some();
        // Caught at the end of this turn, whatever killed her this turn (melee, a spell, drowning, stranded,
        // lava...): `Unit::die` sets `Action::Dying` synchronously wherever damage is dealt, not through a
        // tick-returned event, so there is no single call site to hook a "just died" check into instead.
        let shamans_alive: Vec<bool> = self.units.iter().map(|u| u.kind == UnitKind::Shaman && u.is_alive()).collect();
        self.update_walls();
        let (buildings, now, terrain) = (&self.buildings, self.now, &self.terrain);
        self.trees.iter_mut().filter(|t| !buildings.iter().any(|b| b.covers((t.x, t.z), 0))).for_each(|t| t.tick(now, crate::unit::is_sea(terrain, t.cell())));
        self.tend_reloads();
        let hits = self.tend_bolts();
        dirty.extend(self.apply_events(hits));
        let landed = self.tend_fireballs();
        dirty.extend(self.apply_events(landed));
        dirty.extend(self.tend_swamps());
        dirty.extend(self.tend_effects());
        dirty.extend(self.tend_fires());
        dirty.extend(self.tend_building_wear());
        dirty.extend(self.tend_building_foundations());
        dirty.extend(self.ashes.tick(now));
        self.tend_campfires();
        if !frozen {
            self.tend_vaults();
            self.tend_totems();
            self.tend_queues();
            self.tend_training();
            self.tend_vehicle_huts();
            self.tend_huts();
        }
        // Coordinators, before the units, so that one ending this turn is gone when its units tick (coordinators.md):
        // a. scans that start coordinators or add units to them; b. each one's turn, kind after kind; c. the finished
        // ones removed; d. their events applied in order; e. units whose coordinator is gone released. Formations
        // (formation.rs) tick right after, for the same reason, though they are not coordinators themselves
        // (membership isn't owned through the units' own action).
        if !frozen {
            self.tend_flyby();
            self.tend_ai_scripts();
            self.tend_native_ai();
            self.spot_enemies();
            self.tend_attacks();
            self.tend_wrecking();
            self.tend_wildmen();
            self.tend_guards();
        }
        self.detect_coordinators();
        dirty.extend(self.tend_coordinators());
        self.release_orphans();
        dirty.extend(self.tend_formations());
        self.prune_patrol_points();
        self.prune_hidden_points();
        self.tend_vehicles();
        self.units.iter_mut().for_each(|u| u.statuses.tick(now, u.id.raw()));
        self.revert_expired_hypnosis();
        if crate::unit::HEAL_EVERY.fires(now, 0) {
            self.units.iter_mut().for_each(Unit::heal);
        }
        let arriving: Vec<bool> = self.units.iter().map(|u| matches!(u.action, Action::Walking { .. } | Action::Landing)).collect();
        let (mut events, mut gone) = (Vec::new(), Vec::new());
        for (i, unit) in self.units.iter_mut().enumerate() {
            // Frozen: a lone shaman's pending reincarnation never gets to matter (outcome.md "Automatic
            // elimination") — once the match has ended, Dead stays Dead instead of coming back.
            if frozen && matches!(unit.action, Action::Dead { .. }) {
                continue;
            }
            let site = self.sites.iter().find(|s| s.owner == unit.owner && s.enabled);
            let ground = Walled { terrain: &self.terrain, walls: &self.walls, inside: unit.inside.map(|i| i.building) };
            match unit.tick(now, &ground, site) {
                Some(UnitEvent::Reincarnated) => dirty.extend(site.map(|s| s.flatten_for_spawn(&mut self.terrain))),
                Some(UnitEvent::Gone) => gone.push(unit.id),
                Some(event) => events.push((i, event)),
                None => {}
            }
        }
        for (i, event) in events {
            match event {
                UnitEvent::Jumped { .. } | UnitEvent::Built => dirty.extend(self.work_event(i, event)),
                UnitEvent::AtDoor { building } => self.come_in(i, building),
                UnitEvent::AtVehicle { vehicle } => self.board(i, vehicle),
                UnitEvent::Released { spell } => {
                    if let Some(owner) = self.units[i].owner.tribe() {
                        dirty.extend(self.release(owner, spell));
                    }
                }
                _ => self.wood_event(i, event),
            }
        }
        dirty.extend(self.cast_when_in_range());
        let in_formation = |id: UnitId| self.formations.iter().any(|f| f.members.iter().any(|&(m, _)| m == id));
        for (i, &arrived) in arriving.iter().enumerate() {
            // A formation member keeps the spot it was nudged onto even if another unit already stands there
            // (formation.rs slots are not spot.rs aware): relocating it here would only have the formation nudge it
            // straight back next turn, forever. It leaves the formation once idle like any other member; nothing
            // moves it again until a new order does.
            if arrived && self.units[i].action == Action::Idle && self.units[i].in_the_open() && !in_formation(self.units[i].id) {
                let u = &self.units[i];
                let (taken, spot) = (self.taken_spots(&[u.id]), slots::spot_of((u.x, u.z)));
                if taken.contains(&spot) && let Some(&free) = slots::free_spots_near(&self.ground(), spot, &taken, 1).first() {
                    let (x, z) = slots::spot_centre(free);
                    self.units[i].start(Order::MoveTo { x, z });
                }
            }
        }
        if !frozen {
            self.start_chained();
            self.advance_patrols();
            dirty.extend(self.work());
            self.drop_wood();
        }
        self.count_inside();
        for (i, &was_alive) in shamans_alive.iter().enumerate() {
            if was_alive && !self.units[i].is_alive() {
                self.shaman_died(i);
            }
        }
        if !frozen {
            let incomes: Vec<crate::mana::Mana> = (0..self.mana.len()).map(|player| self.mana_income(TribeId::ALL[player])).collect();
            let buildings = &mut self.buildings;
            for (player, ((pool, book), &income)) in self.mana.iter_mut().zip(&mut self.spell_books).zip(&incomes).enumerate() {
                let mut stores: Vec<&mut crate::production::Production> = buildings.iter_mut().filter(|b| b.owner == TribeId::ALL[player]).filter_map(|b| b.production.as_mut()).collect();
                pool.tick(now, income, book, &mut stores);
            }
        }
        self.units.retain(|u| !gone.contains(&u.id));
        self.check_elimination();
        self.shield_everyone_once_ended();
        self.recompute_shading(&dirty);
        dirty
    }
}

/// The buildings sandbox's name.
pub const SANDBOX_BUILDINGS: &str = "Sandbox: buildings";

/// The buildings sandbox's row blown up in a loop by the client (its explosion is drawn only), west to east.
pub const SANDBOX_BLASTS: [BuildingKind; 9] = [
    BuildingKind::Hut { size: 1 },
    BuildingKind::Hut { size: 2 },
    BuildingKind::Hut { size: 3 },
    BuildingKind::SpyTraining,
    BuildingKind::WarriorTraining,
    BuildingKind::FirewarriorTraining,
    BuildingKind::BoatHut,
    BuildingKind::AirshipHut,
    BuildingKind::Prison,
];

/// Where the `i`-th of `SANDBOX_BLASTS` stands (its corner, as `Building::x`, `z`).
pub fn sandbox_blast_corner(i: usize) -> (u16, u16) {
    let c = MAP_SIZE as i32 / 2;
    (((c - 20 + 5 * i as i32) * 512) as u16, ((c + 27) * 512) as u16)
}

/// Braves standing free in the buildings sandbox, enough to fill a training unit's queue round its walls.
pub const SANDBOX_FREE_BRAVES: i32 = 100;

/// The rows of marked people in the units sandbox, north to south: what each row's people carry. Low health is
/// 1 point left: they heal out of it in about 6 s.
pub const SANDBOX_MARKERS: [fn(&mut Unit); 5] = [
    |u| {
        u.health.damage(u.health.max() - 1);
    },
    |u| u.statuses.shield = Timed::steps(crate::status::SHIELD_STEPS),
    |u| u.statuses.bloodlust = Timed::steps(crate::status::BLOODLUST_STEPS),
    |u| u.statuses.hypnotized = Timed::steps(crate::status::HYPNO_STEPS),
    |u| SANDBOX_MARKERS[..4].iter().for_each(|marked| marked(u)),
];

impl GameMap {
    /// The map for one game whose random draws start from `seed`. No level stores one: the client draws it for
    /// each new game, and in multiplayer the host sends its seed to every peer (multiplayer.md).
    pub fn seeded(self, seed: u32) -> Self {
        GameMap { rng: Lcg(seed), ..self }
    }

    /// A fresh id, never given before on this map.
    pub fn new_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// A fresh id for a `T`, from the same counter as `new_id`.
    pub fn new_id_of<T>(&mut self) -> Id<T> {
        Id::from_raw(self.new_id())
    }

    /// A new unit with a fresh id, last in `units` (which stays sorted by id).
    pub fn spawn(&mut self, owner: impl Into<Owner>, kind: UnitKind, at: (u16, u16)) -> &mut Unit {
        let id = self.new_id_of();
        self.units.push(Unit::new(id, owner, kind, at));
        self.units.last_mut().unwrap()
    }

    /// Where the unit `id` is in `units`; None once it has left the map.
    pub fn unit_index(&self, id: UnitId) -> Option<usize> {
        self.units.binary_search_by_key(&id, |u| u.id).ok()
    }

    pub fn unit(&self, id: UnitId) -> Option<&Unit> {
        self.unit_index(id).map(|i| &self.units[i])
    }

    pub fn unit_mut(&mut self, id: UnitId) -> Option<&mut Unit> {
        self.unit_index(id).map(|i| &mut self.units[i])
    }

    /// Adds `b` with a fresh id, last in `buildings` (which stays sorted by id); its id.
    pub fn add_building(&mut self, mut b: Building) -> BuildingId {
        b.id = self.new_id_of();
        let id = b.id;
        self.buildings.push(b);
        id
    }

    /// Where the building `id` is in `buildings`; None once it is gone.
    pub fn building_index(&self, id: BuildingId) -> Option<usize> {
        self.buildings.binary_search_by_key(&id, |b| b.id).ok()
    }

    pub fn building(&self, id: BuildingId) -> Option<&Building> {
        self.building_index(id).map(|i| &self.buildings[i])
    }

    /// Adds `t` with a fresh id, last in `totems` (which stays sorted by id); its id.
    pub fn add_totem(&mut self, mut t: Totem) -> crate::totem::TotemId {
        t.id = self.new_id_of();
        let id = t.id;
        self.totems.push(t);
        id
    }

    /// Where the totem `id` is in `totems` (stays sorted by id: `retain` keeps relative order); None
    /// once it is gone.
    pub fn totem_index(&self, id: crate::totem::TotemId) -> Option<usize> {
        self.totems.binary_search_by_key(&id, |t| t.id).ok()
    }

    /// Adds `e` with a fresh id, last in `effects`; its id.
    pub fn add_effect(&mut self, e: Effect) -> Id<Effect> {
        let id = self.new_id_of();
        self.effects.insert(id, e);
        id
    }

    /// Every swamp's kill check (`swamp::KILL_EVERY`, phase 0: every swamp at once), before `tend_effects` so a
    /// swamp whose tally ends it this turn is already gone when the generic removal runs: for each cell at least
    /// one swamp covers, the most recently cast one among them (`Registry`'s ids only grow, so the highest is the
    /// newest) is credited with whoever dies there now — any tribe, even its own, shielded or not
    /// (`DamageKind::Swamp` is never gated by `effect::hurts`), inside a building there too, unlike lightning.
    fn tend_swamps(&mut self) -> Vec<DirtyRect> {
        if !crate::swamp::KILL_EVERY.fires(self.now, 0) {
            return Vec::new();
        }
        let size = self.terrain.size() as i32;
        let mut claims: HashMap<(i32, i32), Id<Effect>> = HashMap::new();
        for (id, e) in self.effects.iter() {
            if !matches!(e.kind, EffectKind::Swamp(_)) {
                continue;
            }
            for cell in crate::swamp::cells((e.x, e.z), size) {
                claims.entry(cell).and_modify(|claimer| *claimer = (*claimer).max(id)).or_insert(id);
            }
        }
        if claims.is_empty() {
            return Vec::new();
        }
        let mut tally: HashMap<Id<Effect>, u32> = HashMap::new();
        let mut events = Vec::new();
        for u in self.units.iter().filter(|u| u.is_alive()) {
            let Some(&claimer) = claims.get(&u.cell()) else { continue };
            let owner = self.effects.get(claimer).map_or(u.owner, |e| e.owner);
            events.push(WorldEvent::Damage { unit: u.id, amount: u16::MAX, kind: DamageKind::Swamp, from: Some(owner) });
            *tally.entry(claimer).or_insert(0) += 1;
        }
        for (id, killed) in tally {
            let Some(e) = self.effects.get_mut(id) else { continue };
            let EffectKind::Swamp(ref mut swamp) = e.kind else { continue };
            swamp.processed += killed;
            if swamp.processed >= crate::swamp::SWAMP_NUM_PEOPLE {
                e.left = Countdown::with_left(e.left.total(), Ticks::ZERO);
            }
        }
        self.apply_events(events)
    }

    /// Each effect's turn, in order, reading the map as it was before any of them; finished ones go. Then their
    /// events apply in order (`apply_events`); returns the ground they changed.
    fn tend_effects(&mut self) -> Vec<DirtyRect> {
        let (mut effects, mut rng) = (std::mem::take(&mut self.effects), self.rng);
        let events: Vec<WorldEvent> = effects.iter_mut().flat_map(|(_, e)| e.tick(self.now, self, &mut rng)).collect();
        effects.retain(|_, e| !e.is_over());
        // A lava effect's own blend decays every turn even on a turn it fires no `WorldEvent` (still growing with
        // nothing mature to spread, or quietly fading/cooling): without its own dirty rect here, `shading`'s
        // blend-driven cliff glow would only ever update on the turns something else also changed.
        let lava_cells: Vec<(i32, i32)> = effects.iter().filter_map(|(_, e)| if let EffectKind::Lava(lava) = &e.kind { Some(lava.cells()) } else { None }).flatten().collect();
        (self.effects, self.rng) = (effects, rng);
        let mut dirty = self.apply_events(events);
        if !lava_cells.is_empty() {
            dirty.push(DirtyRect::bounding(lava_cells));
        }
        dirty
    }

    /// Scans that start coordinators or add units to them, one per kind in `Coordinators::tick`'s order.
    fn detect_coordinators(&mut self) {
        self.start_sermons();
        self.start_challenges();
        self.escalate_challenges();
        self.join_melees();
    }

    /// Every coordinator's turn, reading the map as it was before any of them, plus every sermon's (not a
    /// coordinator: `tend_sermons`, sermon.rs); the finished ones removed; then their events apply in order. The
    /// ground they changed.
    fn tend_coordinators(&mut self) -> Vec<DirtyRect> {
        let (mut coordinators, mut rng) = (std::mem::take(&mut self.coordinators), self.rng);
        let mut events = coordinators.tick(self.now, self, &mut rng);
        (self.coordinators, self.rng) = (coordinators, rng);
        events.extend(self.tend_sermons());
        self.apply_events(events)
    }

    /// Every formation's turn (formation.rs), reading the map as it was before any of them: each one drops its own
    /// members no longer walking (arrived, reassigned, dead, or stranded) and nudges the rest towards their own
    /// slot; the finished ones (no members left) removed, their events applied in order.
    fn tend_formations(&mut self) -> Vec<DirtyRect> {
        let (mut formations, mut rng) = (std::mem::take(&mut self.formations), self.rng);
        let mut events = Vec::new();
        formations.retain_mut(|f| {
            let (finished, step_events) = f.tick(self.now, self, &mut rng);
            events.extend(step_events);
            !finished
        });
        (self.formations, self.rng) = (formations, rng);
        self.apply_events(events)
    }

    /// The coordinator `u` takes part in, if it still exists.
    pub fn linked(&self, u: &Unit) -> Option<Link> {
        u.action.link().filter(|&link| self.coordinators.exists(link))
    }

    /// Units whose coordinator is gone go back to Idle.
    fn release_orphans(&mut self) {
        let coordinators = &self.coordinators;
        for u in self.units.iter_mut().filter(|u| u.action.link().is_some_and(|link| !coordinators.exists(link))) {
            u.release();
        }
    }

    /// `batch` applied in order, then what applying it caused (`event::run_in_order`); the ground changed.
    pub(crate) fn apply_events(&mut self, batch: Vec<WorldEvent>) -> Vec<DirtyRect> {
        let mut dirty = Vec::new();
        run_in_order(batch, |event| {
            let (changed, follow_ups) = self.apply_event(event);
            dirty.extend(changed);
            follow_ups
        });
        dirty
    }

    /// The one rule for each kind of event: the ground it changed, and the events it causes. Things gone are skipped.
    pub(crate) fn apply_event(&mut self, event: WorldEvent) -> (Option<DirtyRect>, Vec<WorldEvent>) {
        let mut follow_ups = Vec::new();
        let changed = match event {
            WorldEvent::Damage { unit, amount, kind, from } => {
                let owner = self.unit(unit).map(|u| u.owner);
                if self.damage(unit, amount, kind, from)
                    && kind == DamageKind::Melee
                    && let Some(tribe) = owner.and_then(Owner::tribe)
                {
                    self.melee_deaths[tribe.as_usize()] += 1;
                }
                None
            }
            WorldEvent::DamageVehicle { vehicle, amount, from } => {
                self.damage_vehicle(vehicle, amount, from);
                None
            }
            WorldEvent::Push { unit, vel, kind } => {
                if let Some(i) = self.unit_index(unit).filter(|&i| crate::effect::pushes(&self.units[i], kind)) {
                    let u = &self.units[i];
                    let ground = self.terrain.height_at(u.x as u32, u.z as u32, WORLD_UNITS_PER_CELL);
                    match kind {
                        crate::event::PushKind::Spell => self.units[i].fling(vel, ground),
                        crate::event::PushKind::Melee => self.units[i].knock(vel, ground),
                    }
                }
                None
            }
            WorldEvent::SetFire { building, by } => {
                self.set_fire(building, by);
                None
            }
            WorldEvent::Wear { building, amount } => {
                if let Some(i) = self.building_index(building) {
                    self.buildings[i].wear_down(amount);
                }
                None
            }
            WorldEvent::Destroy { building } => self.burn_down(building).pop(),
            WorldEvent::BurnTrees { cell } => {
                self.trees.iter_mut().filter(|t| t.cell() == cell).for_each(|t| {
                    t.ignite();
                });
                None
            }
            WorldEvent::Scorch { cells } => self.ashes.add(cells).pop(),
            WorldEvent::Panic { unit } => {
                let away = crate::unit::panic_heading(self.rng.next_u32(), self.rng.next_u32());
                if let Some(u) = self.unit_mut(unit) {
                    u.panic(away);
                }
                None
            }
            WorldEvent::Ignite { unit } => {
                let away = crate::unit::panic_heading(self.rng.next_u32(), self.rng.next_u32());
                if let Some(u) = self.unit_mut(unit) {
                    u.catch_fire(away);
                }
                None
            }
            WorldEvent::Raid { building } => {
                follow_ups = self.throw_out(building);
                None
            }
            WorldEvent::Shake { building } => {
                let Some(i) = self.building_index(building) else { return (None, follow_ups) };
                let destroyed = self.buildings[i].lose_a_piece();
                let b = &self.buildings[i];
                follow_ups.push(WorldEvent::SpawnEffect(crate::smoke::damage(b.owner, b.centre())));
                if destroyed {
                    follow_ups.extend(self.throw_out(building));
                    return (self.burn_down(building).pop(), follow_ups);
                } else if crate::wreck::large(b.kind) {
                    follow_ups.extend(self.throw_out(building));
                }
                None
            }
            WorldEvent::PutOut { vehicle } => {
                if self.can_unload(vehicle) {
                    self.unload(vehicle);
                } else {
                    self.put_out(vehicle);
                }
                None
            }
            WorldEvent::SpawnEffect(e) => {
                self.add_effect(e);
                None
            }
            WorldEvent::Terrain(TerrainChange::Raise { at, radius, by }) => Some(self.terrain.raise(at, radius, by)),
            WorldEvent::Terrain(TerrainChange::Flatten { at, height, turn }) => Some(crate::flatten::apply(&mut self.terrain, at, height, turn)),
            WorldEvent::Terrain(TerrainChange::Bridge { from, to, heights, turn }) => Some(crate::land_bridge::apply(&mut self.terrain, from, to, heights, turn)),
            WorldEvent::Terrain(TerrainChange::Volcano { at, ground, turn }) => Some(crate::volcano::apply(&mut self.terrain, at, ground, turn)),
            WorldEvent::Terrain(TerrainChange::Erode { at }) => Some(crate::erode::apply(&mut self.terrain, at)),
            WorldEvent::Placed { unit, at } => {
                if let Some(u) = self.unit_mut(unit) {
                    u.placed(at);
                }
                None
            }
            WorldEvent::Face { unit, at } => {
                if let Some(u) = self.unit_mut(unit) {
                    u.face(at);
                }
                None
            }
            WorldEvent::Taunted { unit, at, variant } => {
                if let Some(u) = self.unit_mut(unit) {
                    u.taunt(at, variant);
                }
                None
            }
            WorldEvent::Struck { attacker, target, blow, knock } => {
                self.struck(attacker, target, blow, knock);
                None
            }
            WorldEvent::ChangeOwner { unit, tribe } => {
                self.change_owner(unit, tribe);
                None
            }
            WorldEvent::Hypnotize { unit, tribe } => {
                self.hypnotize(unit, tribe);
                None
            }
            WorldEvent::StartMelee { attacker, target } => {
                self.start_melee(attacker, target);
                None
            }
            WorldEvent::FightWon { tribe } => {
                if let Some(tribe) = tribe.tribe() {
                    self.melee_wins[tribe.as_usize()] += 1;
                }
                None
            }
            WorldEvent::BuildingBurnedDown { tribe } => {
                if let Some(tribe) = tribe.tribe() {
                    self.fires_put_out[tribe.as_usize()] += 1;
                }
                None
            }
            WorldEvent::Shove { attacker } => {
                self.shove(attacker);
                None
            }
            WorldEvent::Release { unit, from } => {
                if let Some(u) = self.unit_mut(unit).filter(|u| u.action.link() == Some(from)) {
                    u.release();
                }
                None
            }
            WorldEvent::StrikeBack { unit, at } => {
                let target = Target::Unit(at);
                match self.unit_index(unit) {
                    Some(i) if self.units[i].locked() => self.units[i].chain_after_landing(Order::Attack { target }),
                    Some(i) => self.go_attack(i, target),
                    None => {}
                }
                None
            }
            WorldEvent::Declare { winner, reason } => {
                self.declare_outcome(winner, reason);
                None
            }
            WorldEvent::RemoveSite { tribe } => {
                self.remove_reincarnation(tribe);
                None
            }
            WorldEvent::RestoreSite { tribe } => {
                self.restore_reincarnation(tribe);
                None
            }
            WorldEvent::StartFlyby(flyby) => {
                self.active_flyby = Some(crate::ai_script::ActiveFlyby { flyby, start: self.now, interrupted_at: None });
                None
            }
        };
        (changed, follow_ups)
    }

    /// The one way a unit is hurt: `amount` of `kind` from tribe `from`, unless it is immune (`effect::hurts`); a
    /// bloodlusted target takes a blow or a bolt `>> BLOODLUST_HEALTH_SHIFT` (independently of the attacker's own
    /// bloodlust, already folded into `amount` by `combat::strength`/`firebolt`). True when this kills it.
    /// `Unit::take_hit` remembers `from` and, if it kills, `kind` (`hurt.rs`).
    pub(crate) fn damage(&mut self, unit: UnitId, amount: u16, kind: DamageKind, from: Option<Owner>) -> bool {
        match self.unit_index(unit).filter(|&i| crate::effect::hurts(&self.units[i], kind, from)) {
            Some(i) => {
                let lessened = matches!(kind, DamageKind::Melee | DamageKind::Fire) && self.units[i].statuses.bloodlust.is_on();
                self.units[i].take_hit(if lessened { amount >> crate::status::BLOODLUST_HEALTH_SHIFT } else { amount }, kind, from)
            }
            None => false,
        }
    }

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
    /// door when it moves; one walking to a newly walled spot stops.
    pub fn update_walls(&mut self) {
        let size = self.terrain.size() as i32;
        let mut cells = std::collections::BTreeMap::new();
        for b in self.buildings.iter().filter(|b| b.stage() != crate::building::Stage::Blueprint) {
            cells.extend(b.walled_cells(size).into_iter().map(|c| (c, b.id)));
        }
        if self.walls.set(cells) {
            for u in self.units.iter_mut().filter(|u| u.in_the_open() && u.is_alive()) {
                let Some(id) = self.walls.at(u.cell()) else { continue };
                if let Some(i) = self.buildings.binary_search_by_key(&id, |b| b.id).ok() {
                    u.inside = Some(crate::unit::Inside { building: id, door: self.buildings[i].door() });
                }
            }
            // Walking to a spot now behind walls (even in their own building: only `Unit::enter` walks
            // in): they stop, and go on with their next order or task from there.
            for u in self.units.iter_mut().filter(|u| u.is_alive()) {
                let (Action::Walking { to } | Action::Stranded { to }) = u.action else { continue };
                if self.walls.at(((to.0 as u32 / 512) as i32, (to.1 as u32 / 512) as i32)).is_some() {
                    u.start(Order::Stop);
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
    pub(crate) fn start_order(&mut self, i: usize, order: Order) {
        self.unassign(i);
        self.leave_formation(self.units[i].id);
        match order {
            Order::Build { site } => self.assign(i, site),
            Order::Enter { site } => self.go_in(i, site),
            Order::Worship { at } => self.go_worship(i, at),
            Order::CutTree { tree } => self.go_cut(i, tree),
            Order::FetchWood => self.fetch_wood(i),
            Order::PickUp { at } => self.pick_up(i, at),
            Order::Attack { target } => self.go_attack(i, target),
            Order::Board { vehicle } => self.go_board(i, vehicle),
            Order::Guard => self.go_guard(i),
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
        let spare = |t: &Tree| t.wood() as usize > self.tree_claims((t.x, t.z), id) && !self.behind_walls((t.x, t.z));
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

    /// `i` (any non-shaman follower) starts escorting its tribe's shaman (`G`, pop3-rev-analysis.md "Guard the
    /// shaman"); a no-op without one (she is dead/reincarnating, or `i` is itself the shaman).
    fn go_guard(&mut self, i: usize) {
        if self.units[i].kind == UnitKind::Shaman {
            return;
        }
        let Some(owner) = self.units[i].owner.tribe() else { return };
        let Some(shaman) = self.shaman_of(owner).map(|s| s.id) else { return };
        let at = self.unit(shaman).map_or((self.units[i].x, self.units[i].z), |s| (s.x, s.z));
        self.units[i].start(Order::Stop);
        if self.units[i].action.can_take_orders() {
            self.units[i].action = Action::Guarding { shaman, to: at };
        }
    }

    /// Every unit escorting its tribe's shaman: stays within `crate::guard::GUARD_RANGE` of her current position
    /// on both axes, re-aimed every tick (same shape as `tend_attacks`); stops guarding (back to Idle) once she
    /// is gone, reincarnating or not alive.
    pub(crate) fn tend_guards(&mut self) {
        for i in 0..self.units.len() {
            let Action::Guarding { shaman, .. } = self.units[i].action else { continue };
            let Some((sx, sz)) = self.unit(shaman).filter(|s| s.is_alive()).map(|s| (s.x, s.z)) else {
                self.units[i].action = Action::Idle;
                continue;
            };
            let u = &self.units[i];
            if torus_delta(u.x, sx).abs() > crate::guard::GUARD_RANGE || torus_delta(u.z, sz).abs() > crate::guard::GUARD_RANGE {
                self.units[i].retarget((sx, sz));
            }
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
            .filter(|t| t.wood() as usize > self.tree_claims((t.x, t.z), id) && !self.behind_walls((t.x, t.z)))
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
    pub fn pick_orders(&self, player: TribeId, units: &[UnitId], at: (u16, u16)) -> Vec<Command> {
        let mine = |u: &&Unit| u.owner == player && u.is_alive() && units.contains(&u.id);
        let takers: Vec<UnitId> = self.units.iter().filter(mine).filter(|u| Self::can_take_wood(u)).map(|u| u.id).collect();
        let others: Vec<UnitId> = units.iter().copied().filter(|id| !takers.contains(id)).collect();
        let pick = takers.iter().map(|&unit| Command::OrderUnit { player, unit, order: Order::PickUp { at } });
        pick.chain(if others.is_empty() { Vec::new() } else { self.dispatch(player, &others, at) }).collect()
    }

    /// Living units other than `except` cutting the tree at `tree` or walking to cut it.
    pub fn tree_claims(&self, tree: (u16, u16), except: UnitId) -> usize {
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
                u.action = Action::Holding { left: crate::time::Countdown::new(crate::unit::HOLD_TICKS) };
            }
        }
    }

    /// Orders sending the player's `units` to the tree `tree` (index in `trees`): braves cut it (or
    /// a neighbour), the others walk next to it.
    pub fn cut_orders(&self, player: TribeId, units: &[UnitId], tree: usize) -> Vec<Command> {
        let Some(t) = self.trees.get(tree) else { return Vec::new() };
        let at = (t.x, t.z);
        let mine = |u: &&Unit| u.owner == player && u.is_alive() && units.contains(&u.id);
        let braves: Vec<UnitId> = self.units.iter().filter(mine).filter(|u| u.kind == UnitKind::Brave).map(|u| u.id).collect();
        let others: Vec<UnitId> = units.iter().copied().filter(|id| !braves.contains(id)).collect();
        let cut = braves.iter().map(|&unit| Command::OrderUnit { player, unit, order: Order::CutTree { tree: at } });
        cut.chain(if others.is_empty() { Vec::new() } else { self.dispatch(player, &others, at) }).collect()
    }

    /// Whether a chained order still makes sense when its turn comes: building needs a building
    /// still to build.
    fn still_valid(&self, order: &Order) -> bool {
        match *order {
            Order::Build { site } => self.still_building(site),
            Order::Enter { site } => self.building_at_corner(site).is_some(),
            Order::Attack { target: Target::Unit(id) } => self.unit(id).is_some_and(Unit::is_alive),
            Order::Attack { target: Target::Building(id) } => self.building(id).is_some(),
            Order::Board { vehicle } => self.vehicle(vehicle).is_some(),
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
pub(crate) fn torus_cells(a: (u16, u16), b: (u16, u16)) -> i32 {
    torus_delta(a.0, b.0).abs().max(torus_delta(a.1, b.1).abs()) / 512
}

/// Tiny deterministic PRNG, same sequence on every platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lcg(pub u32);

impl Lcg {
    pub fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0 >> 8
    }
}

/// A buildable kind in each state worth seeing, for the buildings sandbox (owner 0, facing 0, at
/// the origin): blueprint; under construction with none, a third, two thirds and all but one
/// piece in; built; dismantling at half; built and attacked (shaking); built with people inside.
pub fn showcase_states(kind: BuildingKind) -> Vec<Building> {
    let built = Building::new(kind, TribeId::Blue, 0, 0, 0);
    let of = kind.wood_cost();
    let building = |used: u8| Building { used, ..built.clone() };
    vec![
        Building::site(kind, TribeId::Blue, 0, 0, 0),
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

    /// Mana actually banked into a slot so far: its full charges plus the partial one in progress — unlike
    /// `charges` alone, strictly increases as soon as any mana reaches it, even short of a whole new charge.
    fn spell_progress(map: &GameMap, player: TribeId, kind: SpellKind) -> u32 {
        let s = map.spell_books[player as usize].slot(kind).unwrap();
        s.charges as u32 * s.stats.cost + s.recharge
    }

    fn charges(map: &GameMap, player: TribeId, kind: SpellKind) -> u8 {
        map.spell_books[player as usize].slot(kind).unwrap().charges
    }

    /// A scratch directory under the OS temp dir, removed on drop (mirrors `pop3_format::install`'s own
    /// `TempTree` test helper).
    struct TempTree(std::path::PathBuf);

    impl TempTree {
        fn new(tag: &str) -> Self {
            let root = std::env::temp_dir().join(format!("game-core-map-test-{tag}-{}", std::process::id()));
            std::fs::create_dir_all(&root).unwrap();
            TempTree(root)
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A minimal, all-zero but correctly-sized level file (same baseline `level.rs`'s own tests parse).
    fn blank_level_bytes() -> Vec<u8> {
        vec![0u8; pop3_format::level::DAT_SIZE]
    }

    #[test]
    fn load_original_reads_spell_costs_from_a_sibling_constant_dat() {
        let dir = TempTree::new("constants-hit");
        let dat = dir.0.join("levl0001.dat");
        std::fs::write(&dat, blank_level_bytes()).unwrap();
        std::fs::write(dir.0.join("constant.dat"), "P3CONST_SPELL_BLAST = 12345\n").unwrap();
        let map = GameMap::load_original(&dat).unwrap();
        assert_eq!(map.spell_books[0].slot(SpellKind::Blast).unwrap().stats.cost, 12_345, "overridden by the file");
    }

    #[test]
    fn load_original_keeps_the_default_cost_without_a_constant_dat() {
        let dir = TempTree::new("constants-miss");
        let dat = dir.0.join("levl0001.dat");
        std::fs::write(&dat, blank_level_bytes()).unwrap();
        let map = GameMap::load_original(&dat).unwrap();
        let default_cost = crate::spell_book::SpellStats::default_for(SpellKind::Blast).cost;
        assert_eq!(map.spell_books[0].slot(SpellKind::Blast).unwrap().stats.cost, default_cost, "no file: our own default");
    }

    #[test]
    fn a_guard_order_walks_towards_the_shaman_until_within_range() {
        let mut map = GameMap::sandbox_units();
        let shaman = map.shaman_of(TribeId::Blue).unwrap();
        let (sx, sz) = (shaman.x, shaman.z);
        let brave = map.units.iter().find(|u| u.owner == Owner::Tribe(TribeId::Blue) && u.kind == UnitKind::Brave).unwrap().id;
        map.apply(&Command::OrderUnit { player: TribeId::Blue, unit: brave, order: Order::Guard });
        assert!(matches!(map.unit(brave).unwrap().action, Action::Guarding { .. }), "guarding at once");
        let arrived = (0..400).find(|_| {
            map.tick();
            let u = map.unit(brave).unwrap();
            torus_delta(u.x, sx).abs() <= crate::guard::GUARD_RANGE && torus_delta(u.z, sz).abs() <= crate::guard::GUARD_RANGE
        });
        assert!(arrived.is_some(), "walked within range");
        assert!(matches!(map.unit(brave).unwrap().action, Action::Guarding { .. }), "still guarding: never done on its own");
    }

    #[test]
    fn the_shaman_herself_cannot_be_ordered_to_guard() {
        let mut map = GameMap::sandbox_units();
        let shaman = map.shaman_of(TribeId::Blue).unwrap().id;
        map.apply(&Command::OrderUnit { player: TribeId::Blue, unit: shaman, order: Order::Guard });
        assert_eq!(map.unit(shaman).unwrap().action, Action::Idle);
    }

    #[test]
    fn guarding_ends_once_the_shaman_is_no_longer_alive() {
        let mut map = GameMap::sandbox_units();
        let shaman = map.shaman_of(TribeId::Blue).unwrap().id;
        let brave = map.units.iter().find(|u| u.owner == Owner::Tribe(TribeId::Blue) && u.kind == UnitKind::Brave).unwrap().id;
        map.apply(&Command::OrderUnit { player: TribeId::Blue, unit: brave, order: Order::Guard });
        map.damage(shaman, u16::MAX, DamageKind::Lightning, None);
        map.tick();
        assert_eq!(map.unit(brave).unwrap().action, Action::Idle, "nothing left to escort");
    }

    #[test]
    fn cancel_guard_stops_every_one_of_that_player_guarding_but_not_another_players() {
        let mut map = GameMap::sandbox_units();
        // sandbox_units only sets up a reincarnation site (and so a shaman) for tribe 0; spawn tribe 1's own
        // directly so its brave has someone to guard too.
        map.spawn(TribeId::Red, UnitKind::Shaman, (60 * 512, 60 * 512));
        let braves: Vec<_> = map.units.iter().filter(|u| u.owner == Owner::Tribe(TribeId::Blue) && u.kind == UnitKind::Brave).map(|u| u.id).take(2).collect();
        assert_eq!(braves.len(), 2, "test setup: two braves of the player");
        let other = map.units.iter().find(|u| u.owner == Owner::Tribe(TribeId::Red) && u.kind == UnitKind::Brave).unwrap().id;
        for &b in &braves {
            map.apply(&Command::OrderUnit { player: TribeId::Blue, unit: b, order: Order::Guard });
        }
        map.apply(&Command::OrderUnit { player: TribeId::Red, unit: other, order: Order::Guard });
        map.apply(&Command::CancelGuard { player: TribeId::Blue });
        for &b in &braves {
            assert_eq!(map.unit(b).unwrap().action, Action::Idle, "{b:?} stood down");
        }
        assert!(matches!(map.unit(other).unwrap().action, Action::Guarding { .. }), "another player's own guard is untouched");
    }

    #[test]
    fn a_cast_uses_a_charge_of_its_players_book() {
        let mut map = GameMap::sandbox_walk();
        let s = map.units.iter().find(|u| u.kind == UnitKind::Shaman).unwrap();
        let at = (s.x.wrapping_add(1024), s.z);
        let before = charges(&map, TribeId::Blue, SpellKind::Blast);
        map.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::Blast { at } });
        assert_eq!((charges(&map, TribeId::Blue, SpellKind::Blast), charges(&map, TribeId::Red, SpellKind::Blast)), (before - 1, before));
        while charges(&map, TribeId::Blue, SpellKind::Blast) > 0 {
            map.spell_books[0].cast(SpellKind::Blast);
        }
        let effects = map.effects.len();
        map.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::Blast { at } });
        assert_eq!(map.effects.len(), effects, "no charge left");
        map.apply(&Command::CastSelf { player: TribeId::Blue, kind: SpellKind::Convert });
        assert_eq!(charges(&map, TribeId::Blue, SpellKind::Convert), 1, "2 from the demo loadout, one used");
    }

    #[test]
    fn raid_throws_a_buildings_occupants_out_and_panics_them() {
        let mut map = GameMap::sandbox_buildings();
        let hut = map.buildings.iter().find(|b| b.stage() == crate::building::Stage::Built && b.kind.capacity() > 0).unwrap().id;
        let door = map.building(hut).unwrap().door();
        let inside = map.spawn(TribeId::Red, UnitKind::Brave, door).id;
        map.unit_mut(inside).unwrap().inside = Some(crate::unit::Inside { building: hut, door });
        let i = map.building_index(hut).unwrap();
        map.buildings[i].inside = 1;
        assert_eq!(map.apply_event(WorldEvent::Raid { building: hut }), (None, vec![WorldEvent::Panic { unit: inside }]));
        assert!(matches!(map.unit(inside).unwrap().action, Action::Walking { .. }), "sent walking out by the door");
    }

    #[test]
    fn a_shamans_death_costs_her_tribe_a_quarter_of_its_bank() {
        let mut map = GameMap::sandbox_walk();
        let before = map.mana[0].bank;
        assert!(before > crate::mana::Mana::ZERO, "test setup: something to lose");
        let i = map.unit_index(map.shaman_of(TribeId::Blue).unwrap().id).unwrap();
        map.shaman_died(i);
        assert_eq!(map.mana[0].bank, before.saturating_sub(crate::mana::Mana(before.0 / 4)), "lost a quarter");
    }

    #[test]
    fn a_shaman_killed_mid_tick_is_caught_the_same_tick_whatever_the_cause() {
        // A real kill (unlike calling `damage` directly before `tick`, which would already show her dead by the
        // time the tick's own before/after snapshot runs - `Unit::die` sets `Action::Dying` synchronously, not
        // through a tick-returned event): an effect resolved from inside `tick` itself, same as any spell.
        let mut map = GameMap::sandbox_walk();
        let shaman = map.shaman_of(TribeId::Blue).unwrap();
        let (owner, at) = (shaman.owner.tribe().unwrap().as_usize(), (shaman.x, shaman.z));
        let before = map.mana[owner].bank;
        map.add_effect(crate::lightning::effect(TribeId::Red, at));
        map.tick();
        assert!(map.shaman_of(TribeId::Blue).is_none_or(|s| !s.is_alive()), "test setup: the lightning killed her");
        assert_eq!(map.mana[owner].bank, before.saturating_sub(crate::mana::Mana(before.0 / 4)), "caught within the same tick the lightning killed her");
    }

    #[test]
    fn a_living_shaman_or_another_kinds_death_costs_the_tribe_nothing() {
        let mut map = GameMap::sandbox_units();
        let before = map.mana[0].bank;
        map.tick();
        assert_eq!(map.mana[0].bank, before, "nobody died");
        let brave = map.units.iter().find(|u| u.owner == Owner::Tribe(TribeId::Blue) && u.kind != UnitKind::Shaman).unwrap().id;
        map.damage(brave, u16::MAX, DamageKind::Lightning, None);
        map.tick();
        assert_eq!(map.mana[0].bank, before, "not the shaman");
    }

    #[test]
    fn a_tick_drops_a_trees_cell_that_has_become_open_sea_to_size_0() {
        let mut terrain = crate::terrain::Heightmap::new(64);
        for z in 0..64 {
            for x in 0..64 {
                terrain.set(x, z, 64);
            }
        }
        let mut map = GameMap::bare("submerge-test", terrain, Vec::new());
        map.trees = vec![crate::tree::Tree::new((10, 10), 0, crate::tree::MAX_SIZE)];
        map.tick();
        assert_eq!(map.trees[0].size, crate::tree::MAX_SIZE, "still dry: untouched");
        for &(x, z) in &[(10, 10), (11, 10), (10, 11), (11, 11)] {
            map.terrain.set(x, z, 0);
        }
        map.tick();
        assert_eq!(map.trees[0].size, 0, "its cell is now open sea");
    }

    #[test]
    fn put_out_unloads_near_shore_else_puts_passengers_out_where_the_vehicle_is() {
        use crate::vehicle::{Vehicle, VehicleKind};
        let mut map = GameMap::sandbox_vehicles();
        let shore = map.buildings.iter().find(|b| b.kind == crate::building::BuildingKind::BoatHut).unwrap().door();
        let near_shore = map.add_vehicle(Vehicle::new(VehicleKind::Boat, TribeId::Red, shore, crate::angle::Angle::ZERO));
        let aboard = map.spawn(TribeId::Red, UnitKind::Brave, shore).id;
        map.unit_mut(aboard).unwrap().aboard = Some(near_shore);
        map.vehicles.get_mut(near_shore).unwrap().passengers.push(aboard);
        assert!(map.can_unload(near_shore), "set up by the boat hut's own shore");
        map.apply_event(WorldEvent::PutOut { vehicle: near_shore });
        assert!(map.unit(aboard).unwrap().aboard.is_none() && map.unit(aboard).unwrap().is_alive(), "got off onto the ground");

        let at_sea = map.add_vehicle(Vehicle::new(VehicleKind::Boat, TribeId::Red, (0, 0), crate::angle::Angle::ZERO));
        let drowning = map.spawn(TribeId::Red, UnitKind::Brave, (0, 0)).id;
        map.unit_mut(drowning).unwrap().aboard = Some(at_sea);
        map.vehicles.get_mut(at_sea).unwrap().passengers.push(drowning);
        assert!(!map.can_unload(at_sea), "the open sea: nowhere to get off");
        map.apply_event(WorldEvent::PutOut { vehicle: at_sea });
        assert!(map.unit(drowning).unwrap().aboard.is_none(), "put out where it is instead");
    }

    #[test]
    fn the_tick_charges_spells_from_mana_except_paused_ones() {
        let mut map = GameMap::sandbox_walk();
        // sandbox_walk only sites player 0 (one shaman, a deliberately small income): give player 1 a
        // population too, big enough that Swarm (cost 120) clears a charge within the 2s below.
        for _ in 0..25 {
            map.spawn(TribeId::Red, UnitKind::Brave, (0, 0));
        }
        map.apply(&Command::PauseSpell { player: TribeId::Blue, kind: SpellKind::Swarm, paused: true });
        let swarm = map.spell_books[0].slot(SpellKind::Swarm).unwrap().recharge;
        let convert = spell_progress(&map, TribeId::Blue, SpellKind::Convert);
        map.run(Ticks::secs(2));
        assert_eq!(map.spell_books[0].slot(SpellKind::Swarm).unwrap().recharge, swarm, "paused");
        assert!(spell_progress(&map, TribeId::Blue, SpellKind::Convert) > convert, "not paused: still gains mana");
        assert!(charges(&map, TribeId::Red, SpellKind::Swarm) > charges(&map, TribeId::Blue, SpellKind::Swarm), "another player's is not paused");
        assert!(map.mana[0].received > crate::mana::Mana::ZERO);
    }

    #[test]
    fn no_order_or_spell_reaches_a_tumbling_unit() {
        let mut map = GameMap::sandbox_walk();
        let s = map.units.iter().position(|u| u.kind == UnitKind::Shaman).unwrap();
        let (player, id) = (map.units[s].owner.tribe().unwrap(), map.units[s].id);
        map.units[s].action = Action::Tumbling;
        map.units[s].motion.velocity = crate::motion::Velocity::new(1, 0, 0);
        let to = (map.units[s].x.wrapping_add(2048), map.units[s].z);
        map.apply(&Command::OrderUnit { player, unit: id, order: Order::MoveTo { x: to.0, z: to.1 } });
        map.apply(&Command::Order { player, order: Order::MoveTo { x: to.0, z: to.1 } });
        assert_eq!(map.units[s].action, Action::Tumbling);
        assert!(!map.can_cast(player, &Spell::Teleport { to }));
        map.apply(&Command::Cast { player, spell: Spell::Teleport { to } });
        assert_eq!(map.units[s].action, Action::Tumbling);
    }

    #[test]
    fn a_blast_throws_people_away_and_they_land() {
        let mut map = GameMap::sandbox_walk();
        let i = map.units.iter().position(|u| u.is_alive() && u.kind == UnitKind::Shaman).unwrap();
        let (owner, start) = (map.units[i].owner.tribe().unwrap(), (map.units[i].x, map.units[i].z));
        let at = (start.0.wrapping_add(300), start.1);
        map.apply(&Command::Cast { player: owner, spell: Spell::Blast { at } });
        map.run(Ticks::new(crate::unit::CAST_RELEASE.get() - 1));
        assert!(map.effects.is_empty(), "her cast jump first");
        map.tick();
        assert!(map.effects.is_empty() && map.fireballs.len() == 1, "released into the jump: the fireball flies first");
        map.tick();
        assert_eq!(map.effects.len(), 1, "close by, the fireball lands and the blast starts the same turn");
        assert!(map.units[i].action != Action::Tumbling && !map.units[i].motion.airborne(), "the caster's own shaman is spared");
        let other_tribe = if owner == TribeId::Blue { TribeId::Red } else { TribeId::Blue };
        map.apply(&Command::Cast { player: other_tribe, spell: Spell::Blast { at } });
        map.tick();
        assert!(map.units[i].action == Action::Tumbling && map.units[i].motion.airborne(), "thrown up");
        assert_eq!(map.effects.len(), 2, "the first one's last turn to come");
        map.run(crate::blast::VISUAL_TICKS);
        assert!(map.effects.is_empty(), "both blasts' own 3 turns, plus the idle visual ticks past them");
        map.run(Ticks::secs(2));
        let u = &map.units[i];
        assert!(matches!(u.action, Action::Settling { .. }) && !u.motion.airborne() && u.z == start.1, "landed on its feet, settling rather than idle at once");
        assert!(torus_delta(start.0, u.x) < -512, "away from the blast: {}", torus_delta(start.0, u.x));
        map.run(Ticks::new(124)); // past the worst case of `unit::settle_ticks`
        assert_eq!(map.units[i].action, Action::Idle, "settled");
    }

    #[test]
    fn a_game_draws_from_its_own_seed() {
        let draws = |seed| {
            let mut rng = GameMap::sandbox_walk().seeded(seed).rng;
            (0..4).map(|_| rng.next_u32()).collect::<Vec<_>>()
        };
        assert_eq!(draws(7), draws(7), "same seed on every peer, same draws");
        assert_ne!(draws(7), draws(8));
    }

    #[test]
    fn the_dead_are_gone_once_their_spirit_has_risen() {
        let mut map = GameMap::sandbox_walk();
        let s = map.units.iter().position(|u| u.kind == UnitKind::Shaman).unwrap();
        let (x, z) = (map.units[s].x, map.units[s].z);
        let dead = map.spawn(TribeId::Blue, UnitKind::Brave, (x.wrapping_add(512), z)).id;
        let other = map.spawn(TribeId::Blue, UnitKind::Brave, (x.wrapping_add(1024), z)).id;
        map.unit_mut(dead).unwrap().action = Action::Dying { left: crate::time::Countdown::new(crate::unit::DYING_TICKS) };
        map.run(crate::unit::DYING_TICKS);
        assert!(matches!(map.unit(dead).unwrap().action, Action::Spirit { climbing: false, .. }), "on dry land: the calm variant");
        map.run(crate::unit::SPIRIT_CALM_TICKS);
        assert!(map.unit(dead).is_none(), "removed");
        assert!(map.unit(other).is_some(), "the others kept");
        assert!(map.spawn(TribeId::Blue, UnitKind::Brave, (x, z)).id > other, "an id is never given twice");
        map.tick();
    }

    #[test]
    fn the_clock_counts_ticks_run() {
        let mut map = GameMap::sandbox_walk();
        map.tick();
        map.run(Ticks::secs(1));
        assert_eq!(map.now - Tick::ZERO, Ticks::new(13));
    }

    #[test]
    fn a_header_gives_the_start_camera_when_it_sets_one() {
        let header = |start: u16, angle: u16| {
            let mut hdr = vec![0u8; 616];
            hdr[612..614].copy_from_slice(&start.to_le_bytes());
            hdr[614..616].copy_from_slice(&angle.to_le_bytes());
            LevelHeader::parse(&hdr)
        };
        assert_eq!(StartCamera::from_header(&header(0, 0)), None);
        assert_eq!(StartCamera::from_header(&header(166 << 8 | 42, 1144)), Some(StartCamera { cell: Some((21, 83)), angle: 1144 }));
        assert_eq!(StartCamera::from_header(&header(0, 1016)), Some(StartCamera { cell: None, angle: 1016 }));
    }

    #[test]
    fn generation_is_deterministic_and_has_land_and_sea() {
        let a = GameMap::generate(42);
        assert_eq!(a.terrain, GameMap::generate(42).terrain);
        let hs = a.terrain.heights();
        assert!(hs.contains(&0));
        assert!(hs.iter().any(|&h| h > 100));
        assert_eq!(a.sites, GameMap::generate(42).sites);
        assert!(a.site_of(TribeId::Blue).is_some());
        for s in &a.sites {
            assert!(!a.terrain.is_water(s.cell().0, s.cell().1), "spawn ground is land");
        }
    }

    #[test]
    fn remove_and_restore_reincarnation_only_touch_the_given_tribe() {
        let mut m = GameMap::generate(42);
        assert_eq!(m.sites.len(), 2, "test assumes two tribes");
        let other = m.sites.iter().find(|s| s.owner != TribeId::Blue).unwrap().owner;
        m.remove_reincarnation(TribeId::Blue);
        assert!(m.site_of(TribeId::Blue).is_none());
        assert!(m.site_of(other).is_some(), "the other tribe's site is untouched");
        m.remove_reincarnation(TribeId::Blue); // a no-op, already disabled
        assert!(m.site_of(TribeId::Blue).is_none());
        m.restore_reincarnation(TribeId::Blue);
        assert!(m.site_of(TribeId::Blue).is_some());
        m.restore_reincarnation(TribeId::Yellow); // a no-op, no site for that owner at all
        assert!(m.site_of(TribeId::Yellow).is_none());
    }

    #[test]
    fn sandbox_walk_has_a_ramp_a_hill_a_lake_and_a_mesa() {
        let m = GameMap::sandbox_walk();
        let c = MAP_SIZE as i32 / 2;
        let h = |dx: i32, dz: i32| m.terrain.get(c + dx, c + dz) as i32;
        assert_eq!(m.units.len(), 1);
        assert!(m.units[0].owner == Owner::Tribe(TribeId::Blue) && !m.terrain.is_water(c, c));
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
            m.apply(&Command::OrderUnit { player: TribeId::Blue, unit, order: Order::MoveTo { x: to.0, z: to.1 } });
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
                    u.tick(m.now, &m.terrain, None);
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
        let cast = |to| Command::Cast { player: TribeId::Blue, spell: Spell::Teleport { to } };
        for (to, why) in [(at(-9, 0), "lake"), (at(30, 0), "sea"), (at(8, 7), "cliff of the mesa")] {
            assert!(!m.can_cast(TribeId::Blue, &Spell::Teleport { to }), "{why}");
            m.apply(&cast(to));
            assert_ne!((m.units[0].x, m.units[0].z), to, "{why}");
        }
        assert!(!m.can_cast(TribeId::Red, &Spell::Teleport { to: at(0, 3) }), "no shaman for player 1");
        let to = at(0, -10);
        assert_eq!(m.apply(&cast(to)), None, "no terrain change");
        assert!(matches!(m.units[0].action, crate::unit::Action::Casting { .. }), "the cast jump first");
        assert_eq!(m.units[0].teleport_target(), Some(to));
        m.run(crate::unit::CAST_TICKS);
        assert_eq!((m.units[0].x, m.units[0].z), to, "then on the hill top");
    }

    /// The walk sandbox with a small island east of its own, across the water, at cells `x..x + 5`.
    fn with_island_east(x: i32) -> GameMap {
        let mut m = GameMap::sandbox_walk();
        m.spell_books[0].fill();
        for px in x..=x + 5 {
            for pz in 61..=67 {
                m.terrain.set(px, pz, 64);
            }
        }
        m
    }

    #[test]
    fn a_target_she_cannot_reach_she_walks_to_the_nearest_shore_then_casts_from_there() {
        let mut m = with_island_east(90);
        let (start, target) = (m.units[0].cell(), (92, 64));
        assert!(!crate::unit::is_sea(&m.terrain, target) && !m.in_cast_range(TribeId::Blue, SpellKind::LandBridge, m.spell_target(&Spell::LandBridge { from: start, to: target }).unwrap()));
        let charges = |m: &GameMap| m.spell_books[0].slot(SpellKind::LandBridge).unwrap().charges;
        let before = charges(&m);
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::LandBridge { from: start, to: target } });
        assert!(matches!(m.units[0].action, Action::Walking { .. }), "on her way though the island is out of reach");
        let cast = (1..1200).find(|_| {
            m.tick();
            charges(&m) < before
        });
        assert!(cast.is_some(), "cast once in range");
        assert!(m.units[0].cell().0 > start.0 + 10, "walked east towards it: {:?}", m.units[0].cell());
    }

    #[test]
    fn too_far_even_from_the_nearest_shore_she_goes_there_and_casts_nothing() {
        let mut m = with_island_east(108);
        let (start, target) = (m.units[0].cell(), (110, 64));
        let charges = |m: &GameMap| m.spell_books[0].slot(SpellKind::LandBridge).unwrap().charges;
        let before = charges(&m);
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::LandBridge { from: start, to: target } });
        m.run(Ticks::secs(30));
        let u = &m.units[0];
        assert_eq!((u.action, u.walking_to_cast(), charges(&m)), (Action::Idle, None, before), "gave up, no charge used");
        assert!(u.cell().0 >= start.0 + 18, "but on the shore nearest it: {:?}", u.cell());
        assert!(m.effects.is_empty());
    }

    #[test]
    fn out_of_range_she_walks_until_in_range_then_casts_unless_ordered_otherwise() {
        let mut m = GameMap::sandbox_walk();
        let (x, z) = (m.units[0].x, m.units[0].z);
        let range = m.cast_range(TribeId::Blue, SpellKind::Blast).unwrap();
        let at = (x, z.wrapping_add(range as u16 + 6 * 512));
        let blasts = |m: &GameMap| m.spell_books[0].slot(SpellKind::Blast).unwrap().charges;
        let before = blasts(&m);
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::Blast { at } });
        assert!(matches!(m.units[0].action, Action::Walking { .. }) && m.effects.is_empty() && blasts(&m) == before, "walks first, no charge used");
        let cast = (1..600).find(|_| {
            let charges = blasts(&m);
            m.tick();
            matches!(m.units[0].action, Action::Casting { .. }) && blasts(&m) == charges - 1
        });
        assert!(cast.is_some(), "casts once in range, using a charge then");
        let u = &m.units[0];
        assert!(crate::cast_range::within((u.x, u.z), at, m.cast_range(TribeId::Blue, SpellKind::Blast).unwrap()), "stopped within range");
        assert!(torus_delta(z, u.z) > 6 * 512 - 512, "not farther than needed: {}", torus_delta(z, u.z));
        m.run(crate::unit::CAST_RELEASE);
        assert!((1..200).any(|_| { m.tick(); m.effects.len() == 1 }), "the blast, once its fireball has flown in");

        let mut m = GameMap::sandbox_walk();
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::Blast { at } });
        m.run(Ticks::new(6));
        m.apply(&Command::Order { player: TribeId::Blue, order: Order::Stop });
        m.run(Ticks::secs(10));
        assert!(m.effects.is_empty() && m.units[0].action == Action::Idle, "another order drops the cast");
    }

    #[test]
    fn magical_shield_shields_the_nearest_own_followers_around_the_cast_point_once_it_releases() {
        let mut m = GameMap::sandbox_walk();
        m.spell_books[0].set(SpellKind::MagicalShield, crate::spell_book::Availability::Unlimited);
        let shaman = m.units[0].id;
        let at = (m.units[0].x, m.units[0].z);
        let near: Vec<UnitId> = (0..crate::status::SHIELD_NUM_PEOPLE).map(|k| m.spawn(TribeId::Blue, UnitKind::Brave, (at.0.wrapping_add(k as u16 * 20), at.1)).id).collect();
        let far = m.spawn(TribeId::Blue, UnitKind::Brave, (at.0.wrapping_add(4000), at.1)).id;
        let enemy = m.spawn(TribeId::Red, UnitKind::Brave, (at.0.wrapping_add(10), at.1)).id;
        let wildman = m.spawn(TribeId::Blue, UnitKind::Wildman, (at.0.wrapping_add(10), at.1)).id;
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::MagicalShield { at } });
        assert!(near.iter().all(|&id| !m.unit(id).unwrap().statuses.shield.is_on()), "not yet, still casting");
        m.run(crate::unit::CAST_RELEASE);
        assert!(near.iter().all(|&id| m.unit(id).unwrap().statuses.shield.is_on()), "the 6 nearest, shielded");
        for id in [far, enemy, wildman, shaman] {
            assert!(!m.unit(id).unwrap().statuses.shield.is_on(), "spared: too far, another tribe, a wildman, or the shaman herself");
        }
    }

    #[test]
    fn magical_shield_shields_around_where_it_was_aimed_not_around_the_shaman() {
        let mut m = GameMap::sandbox_walk();
        m.spell_books[0].set(SpellKind::MagicalShield, crate::spell_book::Availability::Unlimited);
        let shaman_pos = (m.units[0].x, m.units[0].z);
        // Within cast range, but well away from the shaman herself.
        let at = (shaman_pos.0, shaman_pos.1.wrapping_add(4 * 512));
        let near_target = m.spawn(TribeId::Blue, UnitKind::Brave, (at.0, at.1.wrapping_add(10))).id;
        let near_shaman = m.spawn(TribeId::Blue, UnitKind::Brave, (shaman_pos.0, shaman_pos.1.wrapping_add(10))).id;
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::MagicalShield { at } });
        m.run(Ticks::secs(10));
        assert!(m.unit(near_target).unwrap().statuses.shield.is_on(), "shielded: near the cast point");
        assert!(!m.unit(near_shaman).unwrap().statuses.shield.is_on(), "spared: near the shaman, not the cast point");
    }

    #[test]
    fn recasting_magical_shield_reaches_fresh_followers_before_refreshing_an_already_shielded_one() {
        let mut m = GameMap::sandbox_walk();
        m.spell_books[0].set(SpellKind::MagicalShield, crate::spell_book::Availability::Unlimited);
        let at = (m.units[0].x, m.units[0].z);
        // The closest one already has a shield about to run out; 6 fresh ones stand a little farther, within the
        // same cast's reach.
        let stale = m.spawn(TribeId::Blue, UnitKind::Brave, (at.0.wrapping_add(5), at.1)).id;
        // Comfortably above 0 so it is still clearly on (not accidentally expired by its own `STEP` tick) through
        // the cast's `CAST_RELEASE`-turn animation, yet well under `SHIELD_STEPS` so a refresh is visible.
        m.unit_mut(stale).unwrap().statuses.shield = crate::status::Timed::steps(10);
        let fresh: Vec<UnitId> = (0..crate::status::SHIELD_NUM_PEOPLE).map(|k| m.spawn(TribeId::Blue, UnitKind::Brave, (at.0.wrapping_add(50 + k as u16 * 20), at.1)).id).collect();
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::MagicalShield { at } });
        m.run(crate::unit::CAST_RELEASE);
        assert!(fresh.iter().all(|&id| m.unit(id).unwrap().statuses.shield.steps_left() == crate::status::SHIELD_STEPS), "the 6 fresh ones, not the closer stale one");
        let left = m.unit(stale).unwrap().statuses.shield.steps_left();
        assert!(left > 0 && left < crate::status::SHIELD_STEPS, "left untouched (just ticked down some): there was room for everyone fresh");

        // Everyone nearby is shielded now (nobody left fresh): a recast falls back to plain distance, reaching the
        // closest one again and resetting its timer to a new full duration, rather than refusing to touch it again.
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::MagicalShield { at } });
        m.run(crate::unit::CAST_RELEASE);
        assert_eq!(m.unit(stale).unwrap().statuses.shield.steps_left(), crate::status::SHIELD_STEPS, "the closest, refreshed");
    }

    #[test]
    fn bloodlust_bloodlusts_the_nearest_own_followers_around_the_cast_point_once_it_releases() {
        let mut m = GameMap::sandbox_walk();
        m.spell_books[0].set(SpellKind::Bloodlust, crate::spell_book::Availability::Unlimited);
        let at = (m.units[0].x, m.units[0].z);
        let near: Vec<UnitId> = (0..crate::status::BLOODLUST_NUM_PEOPLE).map(|k| m.spawn(TribeId::Blue, UnitKind::Brave, (at.0.wrapping_add(k as u16 * 20), at.1)).id).collect();
        let far = m.spawn(TribeId::Blue, UnitKind::Brave, (at.0.wrapping_add(4000), at.1)).id;
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::Bloodlust { at } });
        m.run(crate::unit::CAST_RELEASE);
        assert!(near.iter().all(|&id| m.unit(id).unwrap().statuses.bloodlust.is_on()), "the 6 nearest, bloodlusted");
        assert!(!m.unit(far).unwrap().statuses.bloodlust.is_on(), "too far: not among the 6 nearest");
    }

    #[test]
    fn a_bloodlusted_defender_takes_an_eighth_of_a_blow_or_a_bolt_but_not_of_a_blast() {
        let mut map = GameMap::sandbox_walk();
        let enemy = map.spawn(TribeId::Red, UnitKind::Brave, (map.units[0].x.wrapping_add(1024), map.units[0].z)).id;
        let max = map.unit(enemy).unwrap().health.max();
        map.unit_mut(enemy).unwrap().statuses.bloodlust = crate::status::Timed::steps(crate::status::BLOODLUST_STEPS);
        map.damage(enemy, 80, DamageKind::Melee, Some(Owner::Tribe(TribeId::Blue)));
        assert_eq!(map.unit(enemy).unwrap().health.current(), max - (80 >> crate::status::BLOODLUST_HEALTH_SHIFT), "a blow, lessened");
        let after_melee = map.unit(enemy).unwrap().health.current();
        map.damage(enemy, 80, DamageKind::Fire, Some(Owner::Tribe(TribeId::Blue)));
        assert_eq!(map.unit(enemy).unwrap().health.current(), after_melee - (80 >> crate::status::BLOODLUST_HEALTH_SHIFT), "a bolt, lessened too");
        let after_fire = map.unit(enemy).unwrap().health.current();
        map.damage(enemy, 80, DamageKind::Blast, Some(Owner::Tribe(TribeId::Blue)));
        assert_eq!(map.unit(enemy).unwrap().health.current(), after_fire - 80, "a blast: full amount, bloodlust only softens a fight");
    }

    #[test]
    fn invisibility_turns_the_nearest_own_followers_invisible_once_the_cast_releases() {
        let mut m = GameMap::sandbox_walk();
        m.spell_books[0].set(SpellKind::Invisibility, crate::spell_book::Availability::Unlimited);
        let shaman = m.units[0].id;
        let at = (m.units[0].x, m.units[0].z);
        let near: Vec<UnitId> = (0..crate::status::INVIS_NUM_PEOPLE).map(|k| m.spawn(TribeId::Blue, UnitKind::Brave, (at.0.wrapping_add(k as u16 * 20), at.1)).id).collect();
        let far = m.spawn(TribeId::Blue, UnitKind::Brave, (at.0.wrapping_add(4000), at.1)).id;
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::Invisibility { at } });
        m.run(crate::unit::CAST_RELEASE);
        assert!(near.iter().all(|&id| m.unit(id).unwrap().statuses.invisible.is_on()), "the 6 nearest, invisible");
        assert!(!m.unit(far).unwrap().statuses.invisible.is_on(), "too far: not among the 6 nearest");
        assert!(!m.unit(shaman).unwrap().statuses.invisible.is_on(), "never the shaman herself");
    }

    fn invisibility_duel() -> (GameMap, UnitId, UnitId) {
        let mut map = GameMap::sandbox_units();
        let s = map.shaman_of(TribeId::Blue).unwrap();
        let at = (s.x.wrapping_add(1024), s.z);
        let a = map.spawn(TribeId::Blue, UnitKind::Brave, at).id;
        let t = map.spawn(TribeId::Red, UnitKind::Brave, (at.0.wrapping_add(6 * 512), at.1)).id;
        (map, a, t)
    }

    #[test]
    fn an_invisible_enemy_is_never_auto_spotted_but_a_player_can_still_order_the_attack() {
        let (mut map, a, t) = invisibility_duel();
        map.unit_mut(t).unwrap().statuses.invisible = crate::status::Timed::steps(crate::status::INVIS_STEPS);
        let found = (0..300).find(|_| {
            map.tick();
            matches!(map.unit(a).unwrap().action, Action::Attacking { .. })
        });
        assert!(found.is_none(), "never spotted: invisible to auto-targeting");
        map.apply(&Command::OrderUnit { player: TribeId::Blue, unit: a, order: Order::Attack { target: Target::Unit(t) } });
        assert!(matches!(map.unit(a).unwrap().action, Action::Attacking { .. }), "a player who already knows its id can still order the attack");
    }

    #[test]
    fn attacking_ends_the_attackers_own_invisibility_but_walking_does_not() {
        let (mut map, a, t) = invisibility_duel();
        map.unit_mut(a).unwrap().statuses.invisible = crate::status::Timed::steps(crate::status::INVIS_STEPS);
        map.apply(&Command::OrderUnit { player: TribeId::Blue, unit: a, order: Order::MoveTo { x: map.unit(a).unwrap().x.wrapping_add(50), z: map.unit(a).unwrap().z } });
        map.run(Ticks::new(5));
        assert!(map.unit(a).unwrap().statuses.invisible.is_on(), "walking does not end it");
        map.apply(&Command::OrderUnit { player: TribeId::Blue, unit: a, order: Order::Attack { target: Target::Unit(t) } });
        assert!(matches!(map.unit(a).unwrap().action, Action::Attacking { .. }));
        assert!(!map.unit(a).unwrap().statuses.invisible.is_on(), "attacking ends its own invisibility");
    }

    #[test]
    fn lightning_kills_seven_in_its_cell_after_the_jump_but_not_its_caster() {
        let mut m = GameMap::sandbox_walk();
        let s = &m.units[0];
        let (shaman, at) = (s.id, (s.x.wrapping_add(4 * 512), s.z));
        let crowd: Vec<UnitId> = (0..9).map(|k| m.spawn(TribeId::Red, UnitKind::Brave, (at.0.wrapping_add(k * 30), at.1)).id).collect();
        let next_cell = m.spawn(TribeId::Red, UnitKind::Brave, (at.0.wrapping_add(512), at.1)).id;
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::Lightning { at } });
        m.run(crate::unit::CAST_RELEASE);
        let dead = |m: &GameMap, id| !m.unit(id).is_some_and(Unit::is_alive);
        assert!(crowd.iter().all(|&id| !dead(&m, id)), "released, not struck yet");
        m.tick();
        assert_eq!(crowd.iter().filter(|&&id| dead(&m, id)).count(), crate::lightning::KILLS);
        assert!(!dead(&m, next_cell), "the next cell: the blast's, not killed");
        assert!(!dead(&m, shaman), "far from it");
        let mut m = GameMap::sandbox_walk();
        let s = &m.units[0];
        let (shaman, at) = (s.id, (s.x, s.z));
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::Lightning { at } });
        m.run(Ticks::new(crate::unit::CAST_RELEASE.get() + 2));
        assert!(m.unit(shaman).is_some_and(Unit::is_alive), "on her own cell, she is spared");
    }

    #[test]
    fn a_flatten_levels_its_disc_after_the_cast_jump() {
        let mut m = GameMap::sandbox_walk();
        m.spell_books[0].set(SpellKind::Flatten, crate::spell_book::Availability::Known);
        m.spell_books[0].fill();
        let shaman = m.units[0].cell();
        let at = (shaman.0, shaman.1 - 6);
        let level = m.terrain.get(at.0, at.1);
        assert!(crate::flatten::disc(at).any(|(x, z)| m.terrain.get(x, z) != level), "the hill's slope");
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::Flatten { at } });
        assert!(m.effects.is_empty(), "her cast jump first");
        m.run(crate::unit::CAST_RELEASE);
        assert_eq!(m.effects.len(), 1, "released into the jump");
        m.run(Ticks::new(crate::flatten::DURATION));
        assert!(m.effects.is_empty());
        assert!(crate::flatten::disc(at).all(|(x, z)| m.terrain.get(x, z) == level), "level at the target's height");
    }

    #[test]
    fn a_land_bridge_starts_at_the_shaman_unless_the_player_is_a_god() {
        let mut m = GameMap::sandbox_walk();
        m.spell_books[0].fill();
        let shaman = ((m.units[0].x as u32 / 512) as i32, (m.units[0].z as u32 / 512) as i32);
        let (aimed_from, to) = ((shaman.0 - 20, shaman.1), (shaman.0, shaman.1 - 6));
        let mut god = m.clone();
        god.shaman_omni = true;
        let cast = Command::Cast { player: TribeId::Blue, spell: Spell::LandBridge { from: aimed_from, to } };
        let bridge = |map: &GameMap| map.effects.iter().find_map(|(_, e)| match e.kind {
            crate::effect::EffectKind::LandBridge(b) => Some(b),
            _ => None,
        });
        let before = m.terrain.clone();
        assert_eq!(m.apply(&cast), None, "nothing at once");
        assert!(bridge(&m).is_none(), "her cast jump first");
        m.run(crate::unit::CAST_RELEASE);
        assert_eq!(m.terrain.heights(), before.heights());
        let b = bridge(&m).unwrap();
        assert_eq!((b.from, b.to), (shaman, to), "from the shaman, not where it was aimed from");
        god.apply(&cast);
        god.run(crate::unit::CAST_RELEASE);
        assert_eq!(bridge(&god).map(|b| (b.from, b.to)), Some((aimed_from, to)), "a god's bridge starts where it was aimed from");
        let mut built = before.clone();
        for turn in 0..crate::land_bridge::DURATION {
            crate::land_bridge::apply(&mut built, b.from, b.to, b.heights, turn);
        }
        m.run(crate::time::Ticks::new(crate::land_bridge::DURATION / 2));
        assert!(m.terrain.heights() != before.heights() && m.terrain.heights() != built.heights(), "part-way");
        m.run(crate::time::Ticks::new(crate::land_bridge::DURATION / 2));
        assert_eq!(m.terrain.heights(), built.heights(), "built in its turns");
        assert!(bridge(&m).is_none(), "done");
    }

    #[test]
    fn a_land_bridge_targets_land_only_and_raises_the_water_on_its_way() {
        let mut m = GameMap::sandbox_walk();
        m.spell_books[0].fill();
        let c = MAP_SIZE as i32 / 2;
        let (lake, past_lake) = ((c - 9, c), (c - 14, c));
        assert!(crate::unit::is_sea(&m.terrain, lake) && !crate::unit::is_sea(&m.terrain, past_lake), "the walk sandbox's lake, land west of it");
        let charges = |m: &GameMap| m.spell_books[0].slot(SpellKind::LandBridge).unwrap().charges;
        let before = charges(&m);
        assert!(!m.can_cast(TribeId::Blue, &Spell::LandBridge { from: (c, c), to: lake }));
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::LandBridge { from: (c, c), to: lake } });
        assert_eq!((m.effects.iter().count(), charges(&m)), (0, before), "refused: no bridge, no charge");
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::LandBridge { from: (c, c), to: past_lake } });
        assert_eq!(charges(&m), before, "out of range: she walks first");
        let walked = (1..600).find(|_| {
            m.tick();
            charges(&m) < before
        });
        assert!(walked.is_some_and(|t| t > 12), "then casts, once in range");
        assert!(m.in_cast_range(TribeId::Blue, SpellKind::LandBridge, m.spell_target(&Spell::LandBridge { from: (c, c), to: past_lake }).unwrap()));
        m.run(crate::time::Ticks::new(crate::land_bridge::DURATION));
        assert!(!m.terrain.is_water(lake.0, lake.1), "the lake raised under the bridge");
    }

    #[test]
    fn raising_ground_updates_shading_incrementally_matching_a_full_recompute() {
        let mut m = GameMap::sandbox_walk();
        let c = MAP_SIZE as i32 / 2;
        let before = m.shading.blend(c, c);
        let rect = m.terrain.raise((c, c), 2, 300);
        m.recompute_shading(&[rect]);
        assert_ne!(m.shading.blend(c, c), before, "the edit changed the local slope");
        let mut from_scratch = m.shading.clone();
        let whole = DirtyRect { min: (0, 0), max: (MAP_SIZE as i32, MAP_SIZE as i32) };
        from_scratch.recompute(&m.terrain, &m.ashes, &HashMap::new(), &[whole]);
        assert_eq!(m.shading.blend(c, c), from_scratch.blend(c, c));
        assert_eq!(m.shading.light(c, c), from_scratch.light(c, c));
    }

    #[test]
    fn a_lit_lava_cells_shading_blend_matches_its_own_current_blend() {
        let mut m = GameMap::sandbox_walk();
        let c = MAP_SIZE as i32 / 2;
        let at = ((c as u16) * 512 + 256, (c as u16) * 512 + 256);
        m.add_effect(crate::lava::effect(TribeId::Blue, at, vec![(c, c)]));
        m.tick();
        let lit = m.effects.iter().find_map(|(_, e)| match &e.kind {
            EffectKind::Lava(lava) => Some(lava.cells_lit()),
            _ => None,
        });
        let (_, blend, _) = lit.unwrap().into_iter().find(|&(pos, ..)| pos == (c, c)).unwrap();
        assert_eq!(m.shading.blend(c, c), blend, "shading tracks the lava effect's own current blend, not the slope");
    }

    #[test]
    fn an_ash_covered_cell_with_no_lava_takes_its_shading_blend_from_the_ashes() {
        let mut m = GameMap::sandbox_walk();
        let hut = m.add_building(Building { used: 1, ..Building::new(BuildingKind::Hut { size: 1 }, TribeId::Red, MAP_SIZE as u16 / 2 * 512, MAP_SIZE as u16 / 2 * 512, 0) });
        m.set_fire(hut, Owner::Tribe(TribeId::Red));
        for _ in 0..60 {
            m.tick();
        }
        assert!(m.building(hut).is_none(), "burnt down");
        let &cell = m.ashes.patches.last().unwrap().cells.first().unwrap();
        assert_eq!(m.shading.blend(cell.0, cell.1), crate::ashes::DARK);
    }

    #[test]
    fn spells_are_aimed_within_the_shamans_range() {
        let mut m = GameMap::sandbox_walk();
        let (x, z) = (m.units[0].x, m.units[0].z);
        let range = m.cast_range(TribeId::Blue, SpellKind::Blast).unwrap();
        assert!((2457..=4608).contains(&range), "the blast's 3072 scaled by her altitude: {range}");
        assert!(m.in_cast_range(TribeId::Blue, SpellKind::Blast, (x + range as u16, z)));
        assert!(!m.in_cast_range(TribeId::Blue, SpellKind::Blast, (x + range as u16 + 1, z)));
        assert!(m.in_cast_range(TribeId::Blue, SpellKind::Teleport, (x + 20_000, z)), "teleport has no limit");
        assert!(!m.in_cast_range(TribeId::Red, SpellKind::Blast, (x, z)), "no shaman for player 1");
        m.units[0].fling(crate::motion::Velocity::new(0, 98, 0), 200);
        assert!(!m.in_cast_range(TribeId::Blue, SpellKind::Blast, (x, z)) && !m.can_cast(TribeId::Blue, &Spell::Blast { at: (x, z) }), "nothing in the air");
        m.shaman_omni = true;
        assert_eq!(m.cast_range(TribeId::Blue, SpellKind::Blast), None);
        assert!(m.in_cast_range(TribeId::Red, SpellKind::Blast, (x + 20_000, z)), "a god casts anywhere");
    }

    #[test]
    fn a_shaman_in_a_drum_tower_casts_from_its_level_a_third_farther() {
        let mut m = GameMap::sandbox_walk();
        let (x, z) = (m.units[0].x, m.units[0].z);
        let tower = m.add_building(Building { level: 256, ..Building::new(BuildingKind::DrumTower, TribeId::Blue, x, z, 0) });
        m.units[0].inside = Some(crate::unit::Inside { building: tower, door: (x, z) });
        assert_eq!(m.altitude(&m.units[0]), 256);
        assert_eq!(m.cast_range(TribeId::Blue, SpellKind::Blast), Some(4096), "3072 at band 2, + 1/3");
    }

    #[test]
    fn shields_and_bloodlust_run_out_with_the_turns() {
        let mut m = GameMap::sandbox_units();
        let marked = |m: &GameMap| m.units.iter().filter(|u| u.statuses.shield.is_on() || u.statuses.bloodlust.is_on()).count();
        assert!(marked(&m) > 0);
        m.run(Ticks::secs(119));
        assert!(m.units.iter().any(|u| (1..=crate::status::BLINK_STEPS).contains(&u.statuses.shield.steps_left())), "in their last steps");
        m.run(Ticks::secs(1));
        assert_eq!(marked(&m), 0, "gone after 180 steps of 8 turns");
    }

    #[test]
    fn sandbox_units_has_every_kind_on_land() {
        let m = GameMap::sandbox_units();
        let count = |owner: TribeId, kind: UnitKind| m.units.iter().filter(|u| u.owner == owner && u.kind == kind).count();
        assert_eq!(count(TribeId::Blue, UnitKind::Shaman), 1);
        assert_eq!(count(TribeId::Red, UnitKind::Shaman), 0);
        for kind in &UnitKind::FOLLOWERS {
            assert_eq!((count(TribeId::Blue, *kind), count(TribeId::Red, *kind)), (3 + SANDBOX_MARKERS.len(), 1), "{kind:?}");
            let marked: Vec<&Unit> = m.units.iter().filter(|u| u.owner == Owner::Tribe(TribeId::Blue) && u.kind == *kind).skip(3).collect();
            let markers = |u: &Unit| [u.health.is_low(), u.statuses.shield.is_on(), u.statuses.bloodlust.is_on(), u.statuses.hypnotized.is_on()];
            assert_eq!(marked.iter().map(|u| markers(u)).collect::<Vec<_>>(), [
                [true, false, false, false],
                [false, true, false, false],
                [false, false, true, false],
                [false, false, false, true],
                [true, true, true, true],
            ], "{kind:?}: one row per marker, then all");
            assert!(marked.iter().all(|u| u.is_alive()));
        }
        let mut ids: Vec<UnitId> = m.units.iter().map(|u| u.id).collect();
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
        m.apply(&Command::Order { player: TribeId::Blue, order: Order::Cast });
        assert!(matches!(m.shaman_of(TribeId::Blue).unwrap().action, crate::unit::Action::Casting { .. }));
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
        m.run(crate::tree::GROW_TICKS);
        assert_eq!(m.trees[small].size, before + 1);
    }

    #[test]
    fn a_group_sent_to_one_spot_stands_packed_on_different_spots() {
        let mut m = GameMap::sandbox_units();
        m.trees.clear();
        let c = MAP_SIZE as i32 / 2;
        let to = slots::cell_centre((c + 2, c + 6));
        let group: Vec<UnitId> = m.units.iter().filter(|u| u.owner == Owner::Tribe(TribeId::Blue)).map(|u| u.id).collect();
        for cmd in m.dispatch(TribeId::Blue, &group, to) {
            m.apply(&cmd);
        }
        for _ in 0..600 {
            m.tick();
        }
        let spots: Vec<_> = m.units.iter().filter(|u| u.owner == Owner::Tribe(TribeId::Blue)).map(|u| slots::spot_of((u.x, u.z))).collect();
        assert!(m.units.iter().filter(|u| u.owner == Owner::Tribe(TribeId::Blue)).all(|u| u.action == Action::Idle), "all arrived");
        let distinct: BTreeSet<_> = spots.iter().collect();
        assert_eq!(distinct.len(), spots.len(), "one spot each: {spots:?}");
        let cells: Vec<_> = m.units.iter().filter(|u| u.owner == Owner::Tribe(TribeId::Blue)).map(Unit::cell).collect();
        assert!(cells.iter().all(|&(x, z)| (x - c - 2).abs() <= 2 && (z - c - 6).abs() <= 2), "16 units within 2 cells: {cells:?}");
    }

    #[test]
    fn a_plain_group_move_order_forms_up_and_clears_up_once_everyone_arrives() {
        let mut m = GameMap::sandbox_units();
        m.trees.clear();
        let c = MAP_SIZE as i32 / 2;
        let to = slots::cell_centre((c + 2, c + 6));
        let group: Vec<UnitId> = m.units.iter().filter(|u| u.owner == Owner::Tribe(TribeId::Blue)).map(|u| u.id).collect();
        for cmd in m.move_orders(TribeId::Blue, &group, to) {
            m.apply(&cmd);
        }
        assert_eq!(m.formations.len(), 1, "one shared formation for the whole group");
        assert_eq!(m.formations[0].members.len(), group.len().min(crate::formation::MAX_MEMBERS), "up to MAX_MEMBERS joined it");
        for _ in 0..600 {
            m.tick();
        }
        assert!(m.units.iter().filter(|u| u.owner == Owner::Tribe(TribeId::Blue)).all(|u| u.action == Action::Idle), "all arrived");
        assert!(m.formations.is_empty(), "cleared up once nobody is still walking");
    }

    #[test]
    fn a_member_reassigned_elsewhere_leaves_its_formation_right_away() {
        let mut m = GameMap::sandbox_units();
        m.trees.clear();
        let c = MAP_SIZE as i32 / 2;
        let group: Vec<UnitId> = m.units.iter().filter(|u| u.owner == Owner::Tribe(TribeId::Blue)).take(3).map(|u| u.id).collect();
        for cmd in m.move_orders(TribeId::Blue, &group, slots::cell_centre((c + 2, c + 6))) {
            m.apply(&cmd);
        }
        assert_eq!(m.formations[0].members.len(), 3);
        let elsewhere = slots::cell_centre((c - 6, c - 2));
        m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: group[0], order: Order::MoveTo { x: elsewhere.0, z: elsewhere.1 } });
        assert!(
            m.formations[0].members.iter().all(|&(id, _)| id != group[0]),
            "reassigned right away, not left to finish walking to its old slot first"
        );
        assert_eq!(m.units.iter().find(|u| u.id == group[0]).unwrap().action, Action::Walking { to: elsewhere }, "its new order stands");
    }

    #[test]
    fn a_lone_walker_never_forms_up() {
        let mut m = GameMap::sandbox_units();
        let lone = m.units.iter().find(|u| u.owner == Owner::Tribe(TribeId::Blue) && u.kind != UnitKind::Shaman).unwrap().id;
        let to = slots::cell_centre((MAP_SIZE as i32 / 2, MAP_SIZE as i32 / 2));
        for cmd in m.move_orders(TribeId::Blue, &[lone], to) {
            m.apply(&cmd);
        }
        assert!(m.formations.is_empty());
    }

    #[test]
    fn never_stops_on_a_tree_or_another_unit() {
        let mut m = GameMap::sandbox_walk();
        let c = MAP_SIZE as i32 / 2;
        m.trees = vec![Tree::new((c + 5, c), 0, 3)];
        let shaman = m.units[0].id;
        m.spawn(TribeId::Blue, UnitKind::Brave, slots::cell_centre((c + 6, c)));
        for target in [(c + 5, c), (c + 6, c)] {
            let to = slots::cell_centre(target);
            m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: shaman, order: Order::MoveTo { x: to.0, z: to.1 } });
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
        m.apply(&Command::Cast { player: TribeId::Blue, spell: Spell::Teleport { to: slots::cell_centre((c + 4, c)) } });
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
    fn sandbox_worship_has_a_pyramid_for_the_shaman_and_every_totem_with_braves() {
        let m = GameMap::sandbox_worship();
        let site = m.site_of(TribeId::Blue).unwrap();
        let vault = m.buildings.iter().find(|b| b.kind == BuildingKind::Vault).unwrap();
        assert!(vault.vault.unwrap().reward.is_some());
        let (_, cz) = vault.centre();
        assert!(vault.door().1 < cz && site.z < vault.door().1, "its door faces the site");
        let kinds: Vec<TotemKind> = m.totems.iter().map(|t| t.kind).collect();
        assert_eq!(kinds, TotemKind::ALL);
        assert!(m.totems.iter().any(|t| t.shaman_only) && m.totems.iter().any(|t| t.prayers == 8), "every case to try");
        for t in &m.totems {
            assert!(!m.terrain.is_water((t.x / 512) as i32, (t.z / 512) as i32));
        }
        assert!(m.shaman_of(TribeId::Blue).is_some());
        assert_eq!(m.units.iter().filter(|u| u.kind == UnitKind::Brave && u.owner == Owner::Tribe(TribeId::Blue)).count(), 8);
        assert!(m.totems.windows(2).all(|w| w[0].id < w[1].id), "distinct, increasing ids, same as buildings/units");
    }

    #[test]
    fn sandbox_spells_has_its_village_on_its_island_and_every_spell() {
        let m = GameMap::sandbox_spells();
        assert!(m.spell_books[0].slot(SpellKind::Teleport).is_some_and(|s| s.can_cast()));
        assert_eq!(m.buildings.len(), 5);
        for b in &m.buildings {
            assert!(b.walled_cells(MAP_SIZE as i32).iter().all(|&(x, z)| !m.terrain.is_water(x, z)), "{:?} on land", b.kind);
            assert!(!m.terrain.is_water((b.door().0 / 512) as i32, (b.door().1 / 512) as i32), "{:?}'s door on land", b.kind);
        }
        assert_eq!(m.buildings.iter().map(|b| b.inside as usize).sum::<usize>(), 10);
        assert!(m.units.iter().all(|u| !m.terrain.is_water(u.cell().0, u.cell().1)));
        assert!(m.terrain.is_water(64 + 16, 64), "sea between the home island and the village");
        for owner in [TribeId::Blue, TribeId::Red] {
            for kind in UnitKind::FOLLOWERS {
                let group: Vec<(i32, i32)> = m.units.iter().filter(|u| u.owner == owner && u.kind == kind && (u.cell().0 - 64).abs() <= 14 && (u.cell().1 - 64).abs() <= 14).map(Unit::cell).collect();
                assert_eq!(group.len(), 3, "tribe {owner}'s {kind:?} on the home island");
                assert!(m.units.iter().filter(|u| u.kind != kind && u.inside.is_none()).all(|u| group.iter().all(|&c| (c.0 - u.cell().0).abs().max((c.1 - u.cell().1).abs()) >= 3)), "apart from the others");
            }
        }
    }

    /// The south-west islet exists "to bridge or teleport across" (`sandbox_spells`'s own doc comment): proves Land
    /// Bridge's cast range (not just Teleport's unlimited one) actually reaches it from the closest the home
    /// island's own shore can ever get, the same geometric-minimum-distance check `SPELL_ISLANDS`' own comment cites.
    #[test]
    fn sandbox_spells_land_bridge_reaches_the_south_west_islet() {
        let mut m = GameMap::sandbox_spells();
        const C: i32 = MAP_SIZE as i32 / 2;
        let ((iox, ioz), islet_radius) = ((-20i32, 16i32), 5i32);
        let dist = ((iox * iox + ioz * ioz) as f64).sqrt();
        let (dir_x, dir_z) = (iox as f64 / dist, ioz as f64 / dist);
        let home_radius = 14f64;
        // The nearest the two shores can ever be: a point just inside the home circle's own edge facing the islet,
        // and a point just inside the islet's own edge facing home, both along the line joining their centres - half
        // a cell short of each radius so rounding to a grid cell never lands just outside the circle, into the sea.
        let home_shore = (C + ((home_radius - 0.5) * dir_x).round() as i32, C + ((home_radius - 0.5) * dir_z).round() as i32);
        let islet_shore = (C + iox - ((islet_radius as f64 - 0.5) * dir_x).round() as i32, C + ioz - ((islet_radius as f64 - 0.5) * dir_z).round() as i32);
        assert!(!m.terrain.is_water(home_shore.0, home_shore.1), "the home island's own near shore is dry land");
        assert!(!m.terrain.is_water(islet_shore.0, islet_shore.1), "the islet's own near shore is dry land");
        let world = |c: i32| (c.rem_euclid(MAP_SIZE as i32) as u32 * WORLD_UNITS_PER_CELL + WORLD_UNITS_PER_CELL / 2) as u16;
        let shaman = m.unit_mut(m.shaman_of(TribeId::Blue).unwrap().id).unwrap();
        (shaman.x, shaman.z) = (world(home_shore.0), world(home_shore.1));
        let target = (world(islet_shore.0), world(islet_shore.1));
        assert!(m.in_cast_range(TribeId::Blue, SpellKind::LandBridge, target), "Land Bridge reaches the south-west islet from the home island's own shore");
    }

    #[test]
    fn sandbox_bridge_crosses_the_sea_within_cast_range() {
        let m = GameMap::sandbox_bridge();
        assert!(m.spell_books[0].slot(SpellKind::LandBridge).is_some_and(|s| s.can_cast()));
        assert_eq!(m.units.len(), 1, "just the one shaman");
        let shaman = m.shaman_of(TribeId::Blue).unwrap();
        assert!(!m.terrain.is_water(shaman.cell().0, shaman.cell().1), "spawns on dry land");
        assert!(m.terrain.is_water(65, 20), "open sea between the two islands (x alone already clears both circles)");
        let (ox, oz) = GameMap::BRIDGE_TARGET_CELL_OFFSET;
        let target_cell = (64 + ox, 64 + oz);
        assert!(!m.terrain.is_water(target_cell.0, target_cell.1), "the far shore is dry land");
        let world = |c: i32| (c.rem_euclid(MAP_SIZE as i32) as u32 * WORLD_UNITS_PER_CELL + WORLD_UNITS_PER_CELL / 2) as u16;
        let target = (world(target_cell.0), world(target_cell.1));
        let range = m.cast_range(TribeId::Blue, SpellKind::LandBridge).unwrap();
        assert!(m.in_cast_range(TribeId::Blue, SpellKind::LandBridge, target), "the far shore is within range");
        let crossing2 = torus_dist2((shaman.x, shaman.z), target);
        assert!((crossing2 as f64).sqrt() < range as f64 * 0.9, "cast with margin, not right at the limit: {} of {range}", (crossing2 as f64).sqrt());
    }

    #[test]
    fn sandbox_vehicles_has_its_huts_on_the_coast_and_a_crew_makes_a_boat() {
        use crate::vehicle::{resting, VehicleKind};
        let mut m = GameMap::sandbox_vehicles();
        let hut = m.buildings.iter().position(|b| b.kind == BuildingKind::BoatHut).unwrap();
        assert!(crate::placement::shore_ok(&m, &m.buildings[hut]), "jetty out to sea");
        let (c, (dx, dz, _, height)) = (MAP_SIZE as i32 / 2, SANDBOX_VEHICLE_HILLS[0]);
        assert!(m.terrain.get(c + dx, c + dz) as i32 >= 64 + height / 2, "a tall hill west");
        let afloat: Vec<_> = m.vehicles.iter().map(|(_, v)| (v.kind, v.owner, resting(v.kind, &m.terrain, v.cell()))).collect();
        assert_eq!(
            afloat,
            vec![
                (VehicleKind::Boat, Owner::Tribe(TribeId::Blue), true),
                (VehicleKind::Boat, Owner::Tribe(TribeId::Blue), true),
                (VehicleKind::Boat, Owner::Tribe(TribeId::Blue), false),
                (VehicleKind::Balloon, Owner::Tribe(TribeId::Blue), true),
                (VehicleKind::Boat, Owner::Tribe(TribeId::Red), true)
            ]
        );
        let braves: Vec<UnitId> = m.units.iter().filter(|u| u.kind == UnitKind::Brave && u.aboard.is_none()).take(3).map(|u| u.id).collect();
        for c in m.enter_orders(TribeId::Blue, &braves, hut) {
            m.apply(&c);
        }
        let before = m.vehicles.len();
        assert!((0..6000).any(|_| {
            m.tick();
            m.vehicles.len() > before
        }), "a boat out of the hut");
    }

    #[test]
    fn sandbox_buildings_has_every_model_on_land() {
        let m = GameMap::sandbox_buildings();
        let models = m.buildings.iter().filter(|b| b.owner == Owner::Tribe(TribeId::Blue) && b.stage() == crate::building::Stage::Built && b.inside == 0 && b.shaking == 0);
        assert_eq!(models.count(), 19 + crate::build_book::BUILDABLE.len() + SANDBOX_BLASTS.len());
        assert!(m.buildings.iter().any(|b| b.owner == Owner::Tribe(TribeId::Red)));
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
        let braves = m.units.iter().filter(|u| u.kind == UnitKind::Brave && u.owner == Owner::Tribe(TribeId::Blue)).count();
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
        m.run(crate::campfire::ABANDON_TICKS + Ticks::new(10));
        assert_eq!(m.campfires.len(), 1, "tended: still burning");
        assert_eq!(m.units.iter().filter(|u| matches!(u.action, Action::AroundFire { .. })).count(), 3);
    }

    #[test]
    fn camp_fires_are_lit_by_command_on_free_flat_ground_only() {
        let mut m = GameMap::sandbox_buildings();
        let at = |x: i32, z: i32| ((x * 512 + 100) as u16, (z * 512 + 400) as u16);
        m.apply(&Command::PlaceCampfire { player: TribeId::Blue, at: at(84, 70) });
        assert_eq!(m.campfires.last().map(|f| (f.cell(), f.owner)), Some(((84, 70), Owner::Tribe(TribeId::Blue))));
        let n = m.campfires.len();
        m.apply(&Command::PlaceCampfire { player: TribeId::Blue, at: at(84, 70) });
        m.apply(&Command::PlaceCampfire { player: TribeId::Blue, at: at(2, 2) });
        assert_eq!(m.campfires.len(), n, "taken, then the sea");
        assert_eq!(m.campfire_at(at(84, 70)).map(|f| f.id), m.campfires.last().map(|f| f.id));
        let ids: Vec<u32> = m.campfires.iter().map(|f| f.id).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "unique ids");
    }

    #[test]
    fn the_editor_plants_a_tree_by_command_but_not_twice_on_a_living_one() {
        let mut m = GameMap::sandbox_buildings();
        let before = m.trees.len();
        let at = |x: i32, z: i32| ((x * 512 + 100) as u16, (z * 512 + 100) as u16);
        m.apply(&Command::PlaceTree { player: TribeId::Blue, at: at(10, 10) });
        assert_eq!(m.trees.len(), before + 1);
        assert_eq!(m.trees.last().map(|t| (t.cell(), t.size)), Some(((10, 10), crate::tree::MAX_SIZE)));
        m.apply(&Command::PlaceTree { player: TribeId::Blue, at: at(10, 10) });
        assert_eq!(m.trees.len(), before + 1, "a living tree is already there");
    }

    #[test]
    fn units_go_round_their_fire_spread_out_and_an_abandoned_fire_goes_out() {
        use crate::campfire::{ring_point, ABANDON_TICKS, RING_POINTS};
        let mut m = GameMap::sandbox_buildings();
        m.campfires.clear();
        let fire = m.place_campfire(TribeId::Blue, (84, 70)).unwrap();
        let braves: Vec<UnitId> = m.units.iter().filter(|u| u.kind == UnitKind::Brave && u.campfire().is_none()).take(4).map(|u| u.id).collect();
        let orders = m.gather(TribeId::Blue, &braves, fire);
        let points: Vec<u8> = orders.iter().map(|c| match c {
            Command::OrderUnit { order: Order::Campfire { point, .. }, .. } => *point,
            _ => panic!("{c:?}"),
        }).collect();
        assert_eq!(points.len(), 4);
        for w in points.windows(2) {
            assert_eq!((w[1] + RING_POINTS - w[0]) % RING_POINTS, RING_POINTS / 4, "a quarter of the ring apart");
        }
        assert!(m.gather(TribeId::Red, &braves, fire).is_empty(), "not red's fire");
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
                    let d = crate::unit::torus_delta(near.0, u.x).abs() + crate::unit::torus_delta(near.1, u.z).abs();
                    assert!(d <= 2 * crate::campfire::RING, "on the ring");
                }
            }
        }
        assert!(went_round.iter().all(|&n| n > RING_POINTS as u32), "each went round at least once: {went_round:?}");
        for c in braves.iter().map(|&unit| Command::OrderUnit { player: TribeId::Blue, unit, order: Order::Stop }) {
            m.apply(&c);
        }
        for _ in 0..ABANDON_TICKS.get() - 1 {
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
        for order in [Order::MoveTo { x: a.0, z: a.1 }, Order::MoveTo { x: b.0, z: b.1 }, Order::Stop] {
            m.apply(&Command::QueueOrder { player: TribeId::Blue, unit: id, order });
        }
        assert_eq!(m.units[0].action, Action::Walking { to: a }, "idle: the first one starts at once");
        assert_eq!(m.units[0].queued().len(), 2);
        let mut seen_a = false;
        for _ in 0..200 {
            m.tick();
            seen_a |= m.units[0].cell() == (c + 3, c);
        }
        assert!(seen_a, "went by A");
        assert_eq!((m.units[0].cell(), m.units[0].action, m.units[0].queued().len()), ((c + 3, c + 3), Action::Idle, 0), "then B, then stops");
        m.apply(&Command::QueueOrder { player: TribeId::Blue, unit: id, order: Order::MoveTo { x: a.0, z: a.1 } });
        m.apply(&Command::QueueOrder { player: TribeId::Blue, unit: id, order: Order::MoveTo { x: b.0, z: b.1 } });
        assert_eq!((m.units[0].action, m.units[0].queued().len()), (Action::Walking { to: a }, 1));
        m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: id, order: Order::Stop });
        run(&mut m, 50);
        assert_eq!((m.units[0].action, m.units[0].queued().len()), (Action::Idle, 0), "a direct order replaces the chain");
        m.apply(&Command::QueueOrder { player: TribeId::Red, unit: id, order: Order::MoveTo { x: a.0, z: a.1 } });
        assert_eq!(m.units[0].action, Action::Idle, "not red's unit");
    }

    #[test]
    fn putting_a_camp_fire_out_starts_the_chained_orders() {
        let mut m = GameMap::sandbox_buildings();
        run(&mut m, 100);
        let fire = m.campfires[0].centre();
        let id = m.units.iter().find(|u| u.campfire() == Some(fire)).unwrap().id;
        let to = (fire.0.wrapping_sub(3 * 512), fire.1);
        m.apply(&Command::QueueOrder { player: TribeId::Blue, unit: id, order: Order::MoveTo { x: to.0, z: to.1 } });
        run(&mut m, 100);
        let u = m.units.iter().find(|u| u.id == id).unwrap();
        assert!(matches!(u.action, Action::AroundFire { .. }), "going round a tended fire never ends");
        m.apply(&Command::RemoveCampfire { player: TribeId::Blue, at: fire });
        run(&mut m, 100);
        let u = m.units.iter().find(|u| u.id == id).unwrap();
        assert_eq!(u.action, Action::Idle);
        assert!(slots::spot_of((u.x, u.z)).0.abs_diff(slots::spot_of(to).0) <= 3, "went on to the chained spot");
    }

    #[test]
    fn chained_orders_replay_the_same() {
        let play = || {
            let mut m = GameMap::sandbox_units();
            let ids: Vec<UnitId> = m.units.iter().filter(|u| u.owner == Owner::Tribe(TribeId::Blue)).map(|u| u.id).collect();
            let c = MAP_SIZE as i32 / 2;
            for (i, &unit) in ids.iter().enumerate() {
                let to = slots::cell_centre((c + i as i32 % 4, c + 4));
                m.apply(&Command::QueueOrder { player: TribeId::Blue, unit, order: Order::MoveTo { x: to.0, z: to.1 } });
                m.apply(&Command::QueueOrder { player: TribeId::Blue, unit, order: Order::Stop });
            }
            run(&mut m, 300);
            m.units
        };
        assert_eq!(play(), play());
    }

    #[test]
    fn set_patrol_shares_one_point_a_direct_order_drops_only_its_own() {
        let mut m = GameMap::sandbox_walk();
        m.trees.clear();
        let c = MAP_SIZE as i32 / 2;
        let ids: Vec<UnitId> = (0..3).map(|n| m.spawn(TribeId::Blue, UnitKind::Brave, slots::cell_centre((c - 3, c + n))).id).collect();
        let at = slots::cell_centre((c + 3, c));
        m.apply(&Command::SetPatrol { player: TribeId::Blue, units: ids.clone(), at });
        assert_eq!(m.patrol_points.len(), 1, "one shared point, not one per unit");
        assert_eq!(m.hidden_points.len(), 1);
        let point = m.patrol_points.ids().next().unwrap();
        for &id in &ids {
            assert_eq!(m.unit(id).unwrap().queued_patrol().iter().collect::<Vec<_>>(), [point]);
        }
        m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: ids[0], order: Order::Stop });
        assert!(m.unit(ids[0]).unwrap().queued_patrol().is_empty(), "its own patrol dropped at once");
        for &id in &ids[1..] {
            assert_eq!(m.unit(id).unwrap().queued_patrol().iter().collect::<Vec<_>>(), [point], "the others keep patrolling");
        }
        assert!(m.patrol_points.contains(point), "still referenced by the other two");
    }

    #[test]
    fn a_patrol_of_two_points_cycles_between_them() {
        let mut m = GameMap::sandbox_walk();
        m.trees.clear();
        let c = MAP_SIZE as i32 / 2;
        let id = m.spawn(TribeId::Blue, UnitKind::Brave, slots::cell_centre((c, c))).id;
        let (a, b) = (slots::cell_centre((c + 3, c)), slots::cell_centre((c + 3, c + 3)));
        m.apply(&Command::SetPatrol { player: TribeId::Blue, units: vec![id], at: a });
        m.apply(&Command::AddPatrolPoint { player: TribeId::Blue, units: vec![id], at: b });
        let (mut seen_a, mut seen_b) = (0, 0);
        for _ in 0..400 {
            m.tick();
            let cell = m.unit(id).unwrap().cell();
            seen_a += u32::from(cell == (c + 3, c));
            seen_b += u32::from(cell == (c + 3, c + 3));
        }
        assert!(seen_a >= 2 && seen_b >= 2, "cycles back and forth more than once: {seen_a} {seen_b}");
    }

    #[test]
    fn patrolling_warriors_go_back_and_forth_and_kill_an_enemy_crossing_their_path() {
        let mut m = GameMap::sandbox_walk();
        m.trees.clear();
        let c = MAP_SIZE as i32 / 2;
        let ids: Vec<UnitId> = (0..2).map(|n| m.spawn(TribeId::Blue, UnitKind::Warrior, slots::cell_centre((c - 3, c + n))).id).collect();
        let (a, b) = (slots::cell_centre((c + 3, c)), slots::cell_centre((c + 9, c)));
        m.apply(&Command::SetPatrol { player: TribeId::Blue, units: ids.clone(), at: a });
        m.apply(&Command::AddPatrolPoint { player: TribeId::Blue, units: ids.clone(), at: b });
        let (mut seen_a, mut seen_b) = (0, 0);
        for _ in 0..300 {
            m.tick();
            for &id in &ids {
                let cell = m.unit(id).unwrap().cell();
                seen_a += u32::from(cell == (c + 3, c));
                seen_b += u32::from(cell == (c + 9, c));
            }
        }
        assert!(seen_a >= 2 && seen_b >= 2, "goes back and forth more than once: {seen_a} {seen_b}");
        // An enemy brave walks into the middle of the patrol leg and stops there: the next warrior
        // passing through its exact cell, even just walking through (not idle), spots and kills it.
        let enemy = m.spawn(TribeId::Red, UnitKind::Brave, slots::cell_centre((c + 6, c + 10))).id;
        let mid = slots::cell_centre((c + 6, c));
        m.apply(&Command::OrderUnit { player: TribeId::Red, unit: enemy, order: Order::MoveTo { x: mid.0, z: mid.1 } });
        for _ in 0..600 {
            m.tick();
            if !m.unit(enemy).unwrap().is_alive() {
                break;
            }
        }
        assert!(!m.unit(enemy).unwrap().is_alive(), "a patrolling warrior killed the enemy crossing its path");
    }

    #[test]
    fn release_patrol_prunes_both_registries_next_tick() {
        let mut m = GameMap::sandbox_walk();
        m.trees.clear();
        let c = MAP_SIZE as i32 / 2;
        let id = m.spawn(TribeId::Blue, UnitKind::Brave, slots::cell_centre((c, c))).id;
        let at = slots::cell_centre((c + 3, c));
        m.apply(&Command::SetPatrol { player: TribeId::Blue, units: vec![id], at });
        assert_eq!((m.patrol_points.len(), m.hidden_points.len()), (1, 1));
        m.apply(&Command::ReleasePatrol { player: TribeId::Blue, units: vec![id] });
        assert!(m.unit(id).unwrap().queued_patrol().is_empty());
        m.tick();
        assert_eq!((m.patrol_points.len(), m.hidden_points.len()), (0, 0), "nothing references them any more");
    }

    #[test]
    fn killing_a_unit_mid_patrol_frees_the_point_only_once_it_was_the_last_reference() {
        let mut m = GameMap::sandbox_walk();
        m.trees.clear();
        let c = MAP_SIZE as i32 / 2;
        let ids: Vec<UnitId> = (0..2).map(|n| m.spawn(TribeId::Blue, UnitKind::Brave, slots::cell_centre((c - 3, c + n))).id).collect();
        let at = slots::cell_centre((c + 3, c));
        m.apply(&Command::SetPatrol { player: TribeId::Blue, units: ids.clone(), at });
        let i0 = m.unit_index(ids[0]).unwrap();
        m.units[i0].take_damage(u16::MAX);
        m.tick();
        assert_eq!(m.patrol_points.len(), 1, "the other co-patroller still references it");
        let i1 = m.unit_index(ids[1]).unwrap();
        m.units[i1].take_damage(u16::MAX);
        m.tick();
        assert_eq!(m.patrol_points.len(), 0, "pruned once it was the last reference");
    }

    /// Sandbox walk with no trees but `trees`, and braves of tribe 0 at `braves` (cells); their ids.
    fn woodland(trees: &[((i32, i32), u8)], braves: &[(i32, i32)]) -> (GameMap, Vec<UnitId>) {
        let mut m = GameMap::sandbox_walk();
        let c = MAP_SIZE as i32 / 2;
        m.trees = trees.iter().map(|&((x, z), size)| Tree::new((c + x, c + z), 0, size)).collect();
        let ids = braves.iter().map(|&(x, z)| m.spawn(TribeId::Blue, UnitKind::Brave, slots::cell_centre((c + x, c + z))).id).collect();
        (m, ids)
    }

    fn unit(m: &GameMap, id: UnitId) -> &Unit {
        m.unit(id).unwrap()
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
        m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: ids[0], order: Order::CutTree { tree } });
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
        assert_eq!(Ticks::new(chopped), crate::unit::CHOP_TICKS, "chopped for CHOP_TICKS");
        assert_eq!(Ticks::new(held), crate::unit::HOLD_TICKS, "then held it for HOLD_TICKS");
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
        let warrior = m.spawn(TribeId::Blue, UnitKind::Warrior, slots::cell_centre((c, c))).id;
        let tree = tree_at(&m, (4, 0));
        m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: warrior, order: Order::CutTree { tree } });
        run(&mut m, 200);
        assert_eq!((m.trees[0].size, m.wood.len(), unit(&m, warrior).action), (4, 0, Action::Idle));
    }

    #[test]
    fn a_tree_takes_as_many_braves_as_it_has_wood_the_others_find_another() {
        let (mut m, ids) = woodland(&[((4, 0), 2), ((4, 5), 4)], &[(0, 0), (0, 1), (0, -1)]);
        let (small, big) = (tree_at(&m, (4, 0)), tree_at(&m, (4, 5)));
        for cmd in m.cut_orders(TribeId::Blue, &ids, 0) {
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
        let cmds = m.cut_orders(TribeId::Blue, &[ids[0], shaman], 0);
        assert!(matches!(cmds[..], [Command::OrderUnit { order: Order::CutTree { .. }, .. }, Command::OrderUnit { order: Order::MoveTo { .. }, .. }]), "{cmds:?}");
    }

    #[test]
    fn a_chained_order_carries_the_piece_and_drops_it_once_idle() {
        let (mut m, ids) = woodland(&[((4, 0), 4)], &[(0, 0)]);
        let c = MAP_SIZE as i32 / 2;
        let tree = tree_at(&m, (4, 0));
        let (p, q) = (slots::cell_centre((c - 3, c)), slots::cell_centre((c, c + 4)));
        m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: ids[0], order: Order::CutTree { tree } });
        m.apply(&Command::QueueOrder { player: TribeId::Blue, unit: ids[0], order: Order::MoveTo { x: p.0, z: p.1 } });
        let mut carried = false;
        for _ in 0..300 {
            m.tick();
            carried |= unit(&m, ids[0]).carrying == 1 && matches!(unit(&m, ids[0]).action, Action::Walking { .. });
        }
        assert!(carried, "walked with it");
        assert_eq!(m.wood.len(), 1);
        assert!(near((m.wood[0].x, m.wood[0].z), p, 1), "dropped at P");
        m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: ids[0], order: Order::CutTree { tree } });
        m.apply(&Command::QueueOrder { player: TribeId::Blue, unit: ids[0], order: Order::MoveTo { x: p.0, z: p.1 } });
        while unit(&m, ids[0]).carrying == 0 {
            m.tick();
        }
        m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: ids[0], order: Order::MoveTo { x: q.0, z: q.1 } });
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
            m.apply(&Command::QueueOrder { player: TribeId::Blue, unit: ids[0], order });
        }
        m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: ids[0], order: Order::CutTree { tree } });
        assert_eq!((unit(&m, ids[0]).cutting(), unit(&m, ids[0]).action), (None, Action::Walking { to }), "ignored, still on its way");
        run(&mut m, 200);
        assert_eq!((m.trees[0].size, m.wood.len(), unit(&m, ids[0]).carrying), (4, 1, 0), "no cut; put down once idle");
        m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: ids[0], order: Order::CutTree { tree } });
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
            m.apply(&Command::OrderUnit { player: TribeId::Blue, unit, order: Order::FetchWood });
            m.apply(&Command::QueueOrder { player: TribeId::Blue, unit, order: Order::MoveTo { x: p.0, z: p.1 } });
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
        let cmds = m.pick_orders(TribeId::Blue, &ids, (pile.0 + 50, pile.1));
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
    fn picking_up_a_piece_takes_three_turns_a_tree_twenty() {
        let (mut m, ids) = woodland(&[((2, 0), 4)], &[(0, 0)]);
        let c = MAP_SIZE as i32 / 2;
        let piece = slots::cell_centre((c, c + 2));
        m.wood.push(WoodPiece::new(piece.0, piece.1));
        m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: ids[0], order: Order::PickUp { at: piece } });
        run_until_unit(&mut m, ids[0], |u| matches!(u.action, Action::PickingUpWood { .. }));
        m.run(Ticks::new(crate::unit::PICK_TICKS.get() - 1));
        assert_eq!(unit(&m, ids[0]).carrying, 0);
        m.tick();
        assert_eq!((unit(&m, ids[0]).carrying, m.wood.len()), (1, 0));
        let mut m = woodland(&[((2, 0), 4)], &[(0, 0)]);
        let tree = tree_at(&m.0, (2, 0));
        m.0.apply(&Command::OrderUnit { player: TribeId::Blue, unit: m.1[0], order: Order::CutTree { tree } });
        run_until_unit(&mut m.0, m.1[0], |u| matches!(u.action, Action::Chopping { .. }));
        m.0.run(Ticks::new(crate::unit::CHOP_TICKS.get() - 1));
        assert_eq!(unit(&m.0, m.1[0]).carrying, 0);
        m.0.tick();
        assert_eq!(unit(&m.0, m.1[0]).carrying, 1);
    }

    fn run_until_unit(m: &mut GameMap, id: UnitId, done: impl Fn(&Unit) -> bool) {
        for _ in 0..300 {
            m.tick();
            if done(unit(m, id)) {
                return;
            }
        }
        panic!("never got there");
    }

    #[test]
    fn wood_carried_into_the_sea_is_lost() {
        let (mut m, ids) = woodland(&[], &[(0, 0)]);
        let c = MAP_SIZE as i32 / 2;
        let i = m.units.iter().position(|u| u.id == ids[0]).unwrap();
        m.units[i].carrying = 1;
        let to = slots::cell_centre((c + 3, c));
        m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: ids[0], order: Order::MoveTo { x: to.0, z: to.1 } });
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
            for cmd in m.cut_orders(TribeId::Blue, &ids[..3], 0) {
                m.apply(&cmd);
            }
            m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: ids[3], order: Order::FetchWood });
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
        let around: Vec<UnitId> = m.units.iter().filter(|u| u.campfire() == Some(fire)).map(|u| u.id).collect();
        assert_eq!(around.len(), 3);
        m.apply(&Command::RemoveCampfire { player: TribeId::Red, at: fire });
        assert_eq!(m.campfires.len(), 1, "not red's to put out");
        m.apply(&Command::RemoveCampfire { player: TribeId::Blue, at: (fire.0 + 200, fire.1 - 200) });
        assert!(m.campfires.is_empty(), "anywhere in its cell");
        assert!(m.units.iter().filter(|u| around.contains(&u.id)).all(|u| u.action == Action::Idle));
        m.apply(&Command::RemoveCampfire { player: TribeId::Blue, at: fire });
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
        let kinds: Vec<_> = m.units.iter().map(|u| (u.id.raw(), u.owner, u.kind, u.x, u.z)).collect();
        assert_eq!(kinds[0].2, UnitKind::Shaman);
        assert_eq!(
            kinds[1..],
            [(2, Owner::Tribe(TribeId::Blue), UnitKind::Warrior, 0x0b00, 0x1400), (3, Owner::Wild, UnitKind::Wildman, 0x3000, 0x3000), (4, Owner::Tribe(TribeId::Blue), UnitKind::Brave, 0x0c00, 0x1600)],
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
            m.apply(&Command::Order { player: TribeId::Blue, order: Order::MoveTo { x, z } });
            for _ in 0..3 {
                m.tick();
            }
            m.apply(&Command::Order { player: TribeId::Blue, order: Order::Cast });
            m.tick();
            m
        };
        let (a, b) = (run(), run());
        assert_eq!(a.units, b.units);
        assert!(matches!(a.units[0].action, Action::Casting { .. }));
        assert!(a.units[0].x != a.sites[0].x, "she moved");
    }

    #[test]
    fn unit_orders_only_reach_the_owners_units() {
        use crate::unit::Action;
        let mut m = GameMap::generate(7);
        let (mine, theirs) = (m.units[0].id, m.units[1].id);
        let to = |u: &crate::unit::Unit| Order::MoveTo { x: u.x.wrapping_add(1024), z: u.z };
        m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: theirs, order: to(&m.units[1]) });
        assert_eq!(m.units[1].action, Action::Idle);
        m.apply(&Command::OrderUnit { player: TribeId::Blue, unit: mine, order: to(&m.units[0]) });
        assert!(matches!(m.units[0].action, Action::Walking { .. }));
    }
}
