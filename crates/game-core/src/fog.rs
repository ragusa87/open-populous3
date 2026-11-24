//! Fog of war (pop3-rev-analysis.md "Fog of war"): a permanent shroud of explored land, not line of sight. One
//! `Explored` grid per human tribe (the original keeps one for the local player only; every peer here keeps every
//! human's, for lockstep), all clear at level start and never cleared again. It hides nothing from the simulation:
//! only building over it is refused and a human tribe's braves ignore the wood on it.

use crate::angle::{rotate, Angle, TURN};
use crate::map::GameMap;
use crate::totem::TRIBES;
use crate::tribes::TribeId;
use pop3_format::level::{Level, ThingData, EFFECT_REVEAL_FOG, KIND_EFFECT};
use pop3_format::WORLD_UNITS_PER_CELL;

/// A person's, vehicle's or angel's reveal: disc 5, 109 cells ("Fog of war" `reveal` mode 2).
pub const SIGHT: u8 = 5;
/// The player's reincarnation site at level start: disc 14, 697 cells (mode 5; the disc 7 after it lies inside).
pub const SITE_SIGHT: u8 = 14;
/// Armageddon's arena: every cell within 11 cells of its centre (0x1600 world units).
pub const ARMAGEDDON_SIGHT: u8 = 10;
/// Cells looked at straight ahead of a person or vehicle, one cell apart, on top of its own.
pub const AHEAD: i32 = 4;

/// The cell offsets of disc `r`: every cell whose distance from the centre is under `r + 1` (rings 0..=r).
pub fn disc(r: u8) -> impl Iterator<Item = (i32, i32)> {
    let r = r as i32;
    let reach = (r + 1) * (r + 1);
    (-r..=r).flat_map(move |dz| (-r..=r).map(move |dx| (dx, dz))).filter(move |&(dx, dz)| dx * dx + dz * dz < reach)
}

/// The cells of `centre`'s 3x3 block, each the centre of a disc 5: effect 81 "reveal fog area" and the portal.
pub fn block_centres(centre: (i32, i32)) -> impl Iterator<Item = (i32, i32)> {
    (-1..=1).flat_map(move |dz| (-1..=1).map(move |dx| (centre.0 + dx, centre.1 + dz)))
}

/// One tribe's explored cells, one bit each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Explored {
    size: usize,
    bits: Vec<u64>,
}

impl Explored {
    /// Nothing explored on a `size` x `size` map.
    pub fn new(size: usize) -> Self {
        Explored { size, bits: vec![0; (size * size).div_ceil(64)] }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    fn index(&self, (x, z): (i32, i32)) -> usize {
        let s = self.size as i32;
        (z.rem_euclid(s) * s + x.rem_euclid(s)) as usize
    }

    /// Whether `cell` (wrapped) is explored.
    pub fn is_explored(&self, cell: (i32, i32)) -> bool {
        let i = self.index(cell);
        self.bits[i / 64] & (1 << (i % 64)) != 0
    }

    /// Marks `cell` (wrapped) explored.
    pub fn explore(&mut self, cell: (i32, i32)) {
        let i = self.index(cell);
        self.bits[i / 64] |= 1 << (i % 64);
    }

    /// Marks disc `r` around `centre` explored.
    pub fn reveal(&mut self, centre: (i32, i32), r: u8) {
        disc(r).for_each(|(dx, dz)| self.explore((centre.0 + dx, centre.1 + dz)));
    }

    /// How many cells are explored.
    pub fn count(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }
}

/// The cell of a world point.
fn cell_at((x, z): (i32, i32)) -> (i32, i32) {
    (x.div_euclid(WORLD_UNITS_PER_CELL as i32), z.div_euclid(WORLD_UNITS_PER_CELL as i32))
}

/// A person's heading in eighths (`Unit::facing`: 0 = +z, 2 = +x) as an `Angle` (from +x towards +z).
pub fn facing_angle(facing: u8) -> Angle {
    Angle::new((TURN / 4 + TURN - (facing as u32 % 8) * TURN / 8) % TURN)
}

/// Whether the cell under `at` (world units) or one of the `AHEAD` cells straight ahead along `heading` is unexplored:
/// what makes a moving person or vehicle reveal disc `SIGHT` around its own cell.
pub fn unexplored_ahead(explored: &Explored, at: (u16, u16), heading: Angle) -> bool {
    let (dx, dz) = rotate((WORLD_UNITS_PER_CELL as i32, 0), heading.steps());
    (0..=AHEAD).any(|k| !explored.is_explored(cell_at((at.0 as i32 + k * dx, at.1 as i32 + k * dz))))
}

/// Every human tribe's explored cells (`GameMap::fog`, None on a level without fog).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fog {
    tribes: [Option<Explored>; TRIBES],
}

impl Fog {
    /// Nothing explored yet, for each of `humans`.
    pub fn new(size: usize, humans: impl Iterator<Item = TribeId>) -> Self {
        let mut tribes: [Option<Explored>; TRIBES] = Default::default();
        humans.for_each(|t| tribes[t.as_usize()] = Some(Explored::new(size)));
        Fog { tribes }
    }

    /// `tribe`'s explored cells, None for a tribe that has no fog (a computer one).
    pub fn of(&self, tribe: TribeId) -> Option<&Explored> {
        self.tribes[tribe.as_usize()].as_ref()
    }

    /// Whether `cell` is still hidden from `tribe`: false for a tribe without fog.
    pub fn hides(&self, tribe: TribeId, cell: (i32, i32)) -> bool {
        self.of(tribe).is_some_and(|e| !e.is_explored(cell))
    }

    /// The tribes with fog.
    pub fn humans(&self) -> impl Iterator<Item = TribeId> + '_ {
        TribeId::ALL.into_iter().filter(|t| self.of(*t).is_some())
    }

    /// Reveals disc `r` around `centre` to `tribe`, if it has fog.
    pub fn reveal(&mut self, tribe: TribeId, centre: (i32, i32), r: u8) {
        if let Some(e) = self.tribes[tribe.as_usize()].as_mut() {
            e.reveal(centre, r);
        }
    }

    /// Reveals disc `r` around `centre` to every tribe with fog.
    pub fn reveal_all(&mut self, centre: (i32, i32), r: u8) {
        self.tribes.iter_mut().flatten().for_each(|e| e.reveal(centre, r));
    }

    /// Effect 81's area around `centre`, to `tribe`: a disc `SIGHT` around each cell of its 3x3 block.
    pub fn reveal_area(&mut self, tribe: TribeId, centre: (i32, i32)) {
        block_centres(centre).for_each(|c| self.reveal(tribe, c, SIGHT));
    }

    /// A person or vehicle of `tribe` at `at` heading `heading`: reveals disc `SIGHT` around it when its own cell or
    /// one ahead is unexplored (`unexplored_ahead`).
    pub fn look(&mut self, tribe: TribeId, at: (u16, u16), heading: Angle) {
        if let Some(e) = self.tribes[tribe.as_usize()].as_mut()
            && unexplored_ahead(e, at, heading)
        {
            e.reveal(cell_at((at.0 as i32, at.1 as i32)), SIGHT);
        }
    }

    /// An angel of `tribe` at `at`: reveals disc `SIGHT` around it when its own cell is unexplored.
    pub fn look_down(&mut self, tribe: TribeId, at: (u16, u16)) {
        let cell = cell_at((at.0 as i32, at.1 as i32));
        if let Some(e) = self.tribes[tribe.as_usize()].as_mut()
            && !e.is_explored(cell)
        {
            e.reveal(cell, SIGHT);
        }
    }
}

/// The cells of a level's effect-81 things ("reveal fog area"): those a trigger points at (`targeted`, given when it
/// fires, `totem::gift`) or the others (revealed at level load).
pub fn fog_areas(level: &Level, targeted: bool) -> Vec<(i32, i32)> {
    let aimed: std::collections::BTreeSet<u16> = level
        .things
        .iter()
        .filter_map(|t| match t.data() {
            ThingData::Trigger(trigger) => Some(trigger),
            _ => None,
        })
        .flat_map(|trigger| trigger.target_slots().collect::<Vec<_>>())
        .collect();
    level
        .things
        .iter()
        .filter(|t| t.kind == KIND_EFFECT && t.model == EFFECT_REVEAL_FOG && aimed.contains(&(t.slot + 1)) == targeted)
        .map(|t| cell_at((t.x as i32, t.z as i32)))
        .collect()
}

impl GameMap {
    /// Fog of war from now on: nothing explored for each human tribe (one without a computer script) but disc
    /// `SITE_SIGHT` around its reincarnation site.
    pub fn enable_fog(&mut self) {
        let humans: Vec<TribeId> = self.tribes.all().filter(|t| !self.tribe_scripts.iter().any(|s| s.owner == *t)).collect();
        let mut fog = Fog::new(self.terrain.size(), humans.into_iter());
        for site in &self.sites {
            fog.reveal(site.owner, site.cell(), SITE_SIGHT);
        }
        self.fog = Some(fog);
    }

    /// `level`'s effect-81 things no trigger points at: their areas start explored, for every human tribe.
    pub(crate) fn reveal_fog_areas(&mut self, level: &Level) {
        if let Some(fog) = self.fog.as_mut() {
            for at in fog_areas(level, false) {
                fog.humans().collect::<Vec<_>>().into_iter().for_each(|t| fog.reveal_area(t, at));
            }
        }
    }

    /// Whether `cell` is still hidden from `tribe` (always false without fog, or for a computer tribe).
    pub fn fogged(&self, tribe: TribeId, cell: (i32, i32)) -> bool {
        self.fog.as_ref().is_some_and(|f| f.hides(tribe, cell))
    }

    /// Each turn, after the units and vehicles moved: what the human tribes' people, angels and vehicles uncover.
    pub(crate) fn tend_fog(&mut self) {
        let Some(mut fog) = self.fog.take() else { return };
        for u in self.units.iter().filter(|u| u.is_alive() && u.inside.is_none()) {
            let Some(tribe) = u.owner.tribe() else { continue };
            if u.angel.is_some() {
                fog.look_down(tribe, (u.x, u.z));
            } else {
                fog.look(tribe, (u.x, u.z), facing_angle(u.facing));
            }
        }
        for (_, v) in self.vehicles.iter() {
            if let Some(tribe) = v.owner.tribe() {
                fog.look(tribe, (v.x, v.z), v.heading);
            }
        }
        self.fog = Some(fog);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discs_hold_the_originals_cell_counts() {
        assert_eq!(disc(0).count(), 1);
        assert_eq!(disc(SIGHT).count(), 109);
        assert_eq!(disc(SITE_SIGHT).count(), 697);
        let ring = |r: u8| disc(r).count() - if r == 0 { 0 } else { disc(r - 1).count() };
        assert_eq!((0..=5).map(ring).collect::<Vec<_>>(), vec![1, 8, 16, 20, 24, 40]);
    }

    #[test]
    fn a_disc_wraps_round_the_map_and_counts_each_cell_once() {
        let mut e = Explored::new(16);
        e.reveal((0, 0), 2);
        assert_eq!(e.count(), disc(2).count());
        assert!(e.is_explored((15, 15)) && e.is_explored((-1, -1)) && e.is_explored((0, 14)));
        assert!(!e.is_explored((3, 0)) && !e.is_explored((8, 8)));
    }

    #[test]
    fn facings_point_along_their_eighths() {
        let step = |f| rotate((512, 0), facing_angle(f).steps());
        assert_eq!(step(0), (0, 512), "+z");
        assert_eq!(step(2), (512, 0), "+x");
        assert_eq!(step(4), (0, -512), "-z");
        assert_eq!(step(6), (-512, 0), "-x");
    }

    #[test]
    fn a_walker_reveals_only_with_unexplored_land_under_or_ahead_of_it() {
        let at = |x: i32, z: i32| ((x * 512 + 256) as u16, (z * 512 + 256) as u16);
        let mut fog = Fog::new(128, [TribeId::Blue].into_iter());
        fog.look(TribeId::Blue, at(10, 10), facing_angle(2));
        assert!(fog.of(TribeId::Blue).unwrap().count() == 109, "all dark: disc 5");
        fog.look(TribeId::Blue, at(11, 10), facing_angle(2));
        assert_eq!(fog.of(TribeId::Blue).unwrap().count(), 109, "the 4 cells ahead are inside the disc already");
        fog.look(TribeId::Blue, at(13, 10), facing_angle(2));
        assert!(fog.of(TribeId::Blue).unwrap().count() > 109, "the 4th cell ahead is dark");
        let before = fog.of(TribeId::Blue).unwrap().count();
        fog.look(TribeId::Blue, at(10, 15), facing_angle(2));
        assert_eq!(fog.of(TribeId::Blue).unwrap().count(), before, "walking along the edge of the dark, not into it");
    }

    #[test]
    fn an_angel_only_looks_at_its_own_cell() {
        let mut fog = Fog::new(128, [TribeId::Red].into_iter());
        fog.reveal(TribeId::Red, (10, 10), SIGHT);
        fog.look_down(TribeId::Red, (15 * 512, 10 * 512));
        assert_eq!(fog.of(TribeId::Red).unwrap().count(), 109, "own cell explored, the dark ahead ignored");
        fog.look_down(TribeId::Red, (30 * 512, 10 * 512));
        assert_eq!(fog.of(TribeId::Red).unwrap().count(), 218);
    }

    #[test]
    fn only_tribes_with_fog_have_anything_hidden() {
        let mut fog = Fog::new(128, [TribeId::Blue].into_iter());
        assert!(fog.hides(TribeId::Blue, (5, 5)) && !fog.hides(TribeId::Red, (5, 5)));
        fog.reveal(TribeId::Red, (5, 5), SIGHT);
        fog.reveal_area(TribeId::Blue, (5, 5));
        assert!(!fog.hides(TribeId::Blue, (5, 11)) && fog.hides(TribeId::Blue, (5, 13)), "a 3x3 block of disc 5: about 7 cells");
        assert_eq!(fog.humans().collect::<Vec<_>>(), vec![TribeId::Blue]);
    }

    mod on_the_map {
        use super::*;
        use crate::command::Command;
        use crate::unit::Order;
        use crate::unit::UnitKind;
        use pop3_format::level::{KIND_GENERAL, THING_SIZE};

        fn thing(model: u8, kind: u8, (cx, cz): (u16, u16)) -> [u8; THING_SIZE] {
            let mut t = [0u8; THING_SIZE];
            let (x, z) = ((cx * 512 + 256).to_le_bytes(), (cz * 512 + 256).to_le_bytes());
            t[..7].copy_from_slice(&[model, kind, 0, x[0], x[1], z[0], z[1]]);
            t
        }

        fn level_with(things: &[[u8; THING_SIZE]]) -> Level {
            let mut d = vec![0u8; pop3_format::level::DAT_SIZE];
            for (i, t) in things.iter().enumerate() {
                d[81_987 + i * THING_SIZE..][..THING_SIZE].copy_from_slice(t);
            }
            Level::parse(&d).unwrap()
        }

        #[test]
        fn effect_81s_no_trigger_aims_at_are_revealed_at_load_the_others_when_given() {
            let mut trigger = thing(6, KIND_GENERAL, (5, 5));
            trigger[13..15].copy_from_slice(&3u16.to_le_bytes());
            let level = level_with(&[thing(EFFECT_REVEAL_FOG, KIND_EFFECT, (40, 40)), trigger, thing(EFFECT_REVEAL_FOG, KIND_EFFECT, (90, 20))]);
            assert_eq!(fog_areas(&level, false), vec![(40, 40)]);
            assert_eq!(fog_areas(&level, true), vec![(90, 20)]);
            assert_eq!(crate::totem::gift(&level.things[2]), crate::building::Reward::RevealFog { cell: (90, 20) });
        }

        #[test]
        fn the_site_starts_explored_and_a_walker_uncovers_its_way() {
            let mut map = GameMap::sandbox_walk();
            map.enable_fog();
            let site = map.sites[0].cell();
            assert!(!map.fogged(TribeId::Blue, site) && !map.fogged(TribeId::Blue, (site.0 + 14, site.1)));
            assert!(map.fogged(TribeId::Blue, (site.0 + 18, site.1)));
            let (sx, sz) = ((site.0 * 512 + 256) as u16, (site.1 * 512 + 256) as u16);
            let brave = map.spawn(TribeId::Blue, UnitKind::Brave, (sx + 512 * 2, sz)).id;
            let to = (sx + 512 * 18, sz);
            map.step(&[Command::OrderUnit { player: TribeId::Blue, unit: brave, order: Order::MoveTo { x: to.0, z: to.1 } }]);
            for _ in 0..600 {
                map.step(&[]);
            }
            assert!(!map.fogged(TribeId::Blue, (site.0 + 18, site.1)), "walked there");
            assert!(map.fogged(TribeId::Blue, (site.0, site.1 + 25)), "nobody went there");
            let mut no_fog = GameMap::sandbox_walk();
            no_fog.step(&[]);
            assert!(!no_fog.fogged(TribeId::Blue, (site.0, site.1 + 25)), "no fog on the level");
        }
    }
}
