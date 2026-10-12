//! Trees: the tribes' wood. Each has a size 0-4, the pieces of wood it can still give; a tree at
//! size 0 stays in place, invisible, and grows back one size at a time. Integer state, ticked with
//! the map; placed deterministically in groves (`scatter`).

use crate::map::Lcg;
use crate::path::Mobility;
use crate::terrain::Heightmap;
use crate::time::{Every, Tick, Ticks};
use pop3_format::WORLD_UNITS_PER_CELL;

pub const MAX_SIZE: u8 = 4;
/// A tree regains `TREE<N>_WOOD_GROW` (file 2) every 16 turns (pop3-rev-analysis.md "Regrowth").
const GROW_EVERY: Every = Every::new(16);
const WOOD_GROW: u16 = 2;
/// Wood in one size: `TREE<N>_WOOD_VALUE` 400 over `MAX_SIZE` pieces, a load of `WOOD_BRAVE` 100.
pub const WOOD_PER_SIZE: u16 = 100;
/// To grow one size: 800 turns.
pub const GROW_TICKS: Ticks = Ticks::new(WOOD_PER_SIZE as u32 / WOOD_GROW as u32 * GROW_EVERY.period());
/// Tree types: 0-5 are the levels' scenery trees (models 1-6), 6-17 the other original tree objects
/// (`pop3_format::catalog::tree_object`); the client maps a variant to a model.
pub const VARIANTS: u8 = pop3_format::catalog::TREE_TYPES;
/// No tree this close (cells) to a reincarnation site: its stones and spawn ground stay clear.
pub const SITE_CLEARANCE: i32 = 7;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tree {
    /// World units, at a cell centre.
    pub x: u16,
    pub z: u16,
    pub variant: u8,
    pub size: u8,
    /// Turn about the vertical axis, 2048ths (the level's scenery angle, else scattered by position).
    pub angle: u16,
    growth: u16,
}

impl Tree {
    pub fn new((cx, cz): (i32, i32), variant: u8, size: u8) -> Self {
        let centre = |c: i32| (c.rem_euclid(128) as u32 * WORLD_UNITS_PER_CELL + WORLD_UNITS_PER_CELL / 2) as u16;
        let (x, z) = (centre(cx), centre(cz));
        Tree { x, z, variant: variant % VARIANTS, size: size.min(MAX_SIZE), angle: scattered_angle(x, z), growth: 0 }
    }

    pub fn with_angle(self, angle: u16) -> Self {
        Tree { angle: angle % pop3_format::level::FULL_TURN, ..self }
    }

    pub fn cell(&self) -> (i32, i32) {
        ((self.x as u32 / WORLD_UNITS_PER_CELL) as i32, (self.z as u32 / WORLD_UNITS_PER_CELL) as i32)
    }

    pub fn is_visible(&self) -> bool {
        self.size > 0
    }

    /// One turn: on the original's growth turns, `grow`.
    pub fn tick(&mut self, now: Tick) {
        if GROW_EVERY.fires(now, 0) {
            self.grow();
        }
    }

    /// Below full size: regains `WOOD_GROW`, one size per `WOOD_PER_SIZE`.
    fn grow(&mut self) {
        if self.size >= MAX_SIZE {
            self.growth = 0;
            return;
        }
        self.growth += WOOD_GROW;
        if self.growth >= WOOD_PER_SIZE {
            self.growth -= WOOD_PER_SIZE;
            self.size += 1;
        }
    }

    /// Takes one piece of wood: false if there is none left (size 0).
    pub fn cut(&mut self) -> bool {
        if self.size == 0 {
            return false;
        }
        self.size -= 1;
        true
    }
}

/// A fixed turn per position (2048ths) for trees without one, the same every run.
pub fn scattered_angle(x: u16, z: u16) -> u16 {
    let h = (x as u32).wrapping_mul(2_654_435_761) ^ (z as u32).wrapping_mul(40_503);
    (h % pop3_format::level::FULL_TURN as u32) as u16
}

/// Groves of trees on ground units can walk, clear of `sites` (cells), at most one tree per cell:
/// `groves` centres picked at random, each with 3-8 trees within 3 cells. Same seed, same trees.
pub fn scatter(terrain: &Heightmap, seed: u32, sites: &[(i32, i32)], groves: usize) -> Vec<Tree> {
    let size = terrain.size() as i32;
    let mut rng = Lcg(seed.max(1));
    let torus = |a: i32, b: i32| {
        let d = (a - b).rem_euclid(size);
        d.min(size - d)
    };
    let clear = |c: (i32, i32)| sites.iter().all(|s| torus(c.0, s.0).max(torus(c.1, s.1)) > SITE_CLEARANCE);
    let mut trees: Vec<Tree> = Vec::new();
    for _ in 0..groves {
        let centre = ((rng.next_u32() % size as u32) as i32, (rng.next_u32() % size as u32) as i32);
        let count = 3 + rng.next_u32() % 6;
        for _ in 0..count {
            let cell = (centre.0 + (rng.next_u32() % 7) as i32 - 3, centre.1 + (rng.next_u32() % 7) as i32 - 3);
            let cell = (cell.0.rem_euclid(size), cell.1.rem_euclid(size));
            let (variant, tree_size) = ((rng.next_u32() % VARIANTS as u32) as u8, 1 + (rng.next_u32() % MAX_SIZE as u32) as u8);
            if Mobility::Walk.passable(terrain, cell) && clear(cell) && trees.iter().all(|t| t.cell() != cell) {
                trees.push(Tree::new(cell, variant, tree_size));
            }
        }
    }
    trees
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::is_sea;

    fn island() -> Heightmap {
        let mut t = Heightmap::new(128);
        for z in 20..100 {
            for x in 20..100 {
                t.set(x, z, 100);
            }
        }
        t
    }

    #[test]
    fn angle_from_the_level_or_scattered() {
        let t = Tree::new((5, 5), 0, 4);
        assert_eq!(t.angle, Tree::new((5, 5), 1, 2).angle, "same cell, same turn");
        assert_ne!(t.angle, Tree::new((6, 5), 0, 4).angle);
        assert!(t.angle < 2048);
        assert_eq!(t.with_angle(2048 + 512).angle, 512);
    }

    fn run(t: &mut Tree, turns: std::ops::RangeInclusive<u32>) {
        for n in turns {
            t.tick(Tick::from_wire(n));
        }
    }

    #[test]
    fn grows_one_size_on_the_800th_turn_up_to_four() {
        assert_eq!(GROW_TICKS, Ticks::new(800));
        let mut t = Tree::new((5, 5), 3, 0);
        assert!(!t.is_visible());
        run(&mut t, 1..=799);
        assert_eq!(t.size, 0);
        run(&mut t, 800..=800);
        assert_eq!(t.size, 1);
        run(&mut t, 801..=3200);
        assert_eq!(t.size, MAX_SIZE);
        run(&mut t, 3201..=6400);
        assert_eq!((t.size, t.growth), (MAX_SIZE, 0));
    }

    #[test]
    fn grows_only_on_the_original_turns() {
        let mut t = Tree::new((5, 5), 0, 0);
        run(&mut t, 1..=15);
        assert_eq!(t.growth, 0);
        run(&mut t, 16..=16);
        assert_eq!(t.growth, WOOD_GROW);
        run(&mut t, 17..=31);
        assert_eq!(t.growth, WOOD_GROW);
        run(&mut t, 32..=32);
        assert_eq!(t.growth, 2 * WOOD_GROW);
    }

    #[test]
    fn cutting_takes_one_piece_and_keeps_the_growth() {
        let mut t = Tree::new((5, 5), 0, 2);
        run(&mut t, 1..=400);
        assert!(t.cut());
        assert_eq!((t.size, t.growth), (1, 50));
        run(&mut t, 401..=800);
        assert_eq!(t.size, 2, "half a size was already there");
        assert!(t.cut() && t.cut());
        assert!(!t.cut(), "nothing left");
        assert_eq!((t.size, t.cell()), (0, (5, 5)), "stays in place, invisible");
    }

    #[test]
    fn scatter_is_deterministic_on_land_and_clear_of_sites() {
        let t = island();
        let site = (60, 60);
        let trees = scatter(&t, 7, &[site], 40);
        assert_eq!(trees, scatter(&t, 7, &[site], 40));
        assert_ne!(trees, scatter(&t, 8, &[site], 40));
        assert!(trees.len() > 20, "{} trees", trees.len());
        for tree in &trees {
            let c = tree.cell();
            assert!(!is_sea(&t, c));
            assert!((c.0 - site.0).abs().max((c.1 - site.1).abs()) > SITE_CLEARANCE);
            assert!((1..=MAX_SIZE).contains(&tree.size) && tree.variant < VARIANTS);
        }
        let mut cells: Vec<_> = trees.iter().map(Tree::cell).collect();
        cells.sort();
        cells.dedup();
        assert_eq!(cells.len(), trees.len(), "one tree per cell");
    }
}
