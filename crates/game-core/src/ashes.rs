//! Ashes (docs/specs/combat.md "Ashes"): the mark a building destroyed by losing its wood leaves on the ground. Its
//! cells are dark and nothing can be built on them; after a long while the mark fades, a step every 16 turns, and
//! each cell is free again once faded.

use crate::terrain::DirtyRect;
use crate::time::{Every, Tick};

/// The most patches at once; a new one when full clears the oldest ("Building ashes" [exe]).
pub const ASHES_SLOTS: usize = 100;
/// Ashes age on this rhythm, by its period ("Building ashes" [exe]).
pub const ASHES_EVERY: Every = Every::new(16);
/// How long a patch stays fully dark (constant.dat value).
pub const ASHES_FULL_TICKS: u32 = 6000;
/// A fresh patch's darkness, and the step it fades by each `ASHES_EVERY` once old ("Building ashes" [exe]).
pub const DARK: u8 = 127;
pub const FADE_STEP: u8 = 2;
/// At this darkness or below a cell is ground again ("Building ashes" [exe]).
pub const FADED: u8 = 92;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    /// The cells it covers.
    pub cells: Vec<(i32, i32)>,
    /// Turns since it was left.
    pub age: u32,
    pub darkness: u8,
}

impl Patch {
    /// The ground to draw again when it changes: one rectangle bounding all its cells (not one per cell — a
    /// `WorldEvent` can only carry a single `DirtyRect` out of `GameMap::apply_event`, so a patch spread over many
    /// cells, like a cooled lava flow's, must fit in one rect or most of it would never get rebaked).
    fn dirty(&self) -> DirtyRect {
        DirtyRect::bounding(self.cells.iter().copied())
    }
}

/// Every patch, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ashes {
    pub patches: Vec<Patch>,
}

impl Ashes {
    /// A new patch over `cells`; the ground to draw again (and the oldest patch's, if it was cleared to make room).
    pub fn add(&mut self, cells: Vec<(i32, i32)>) -> Vec<DirtyRect> {
        let mut dirty = Vec::new();
        if self.patches.len() >= ASHES_SLOTS {
            dirty.push(self.patches.remove(0).dirty());
        }
        let patch = Patch { cells, age: 0, darkness: DARK };
        dirty.push(patch.dirty());
        self.patches.push(patch);
        dirty
    }

    /// One turn at `now`: on its rhythm every patch ages; old ones fade a step, faded ones go. The ground to draw
    /// again.
    pub fn tick(&mut self, now: Tick) -> Vec<DirtyRect> {
        if !ASHES_EVERY.fires(now, 0) {
            return Vec::new();
        }
        let mut dirty = Vec::new();
        for p in &mut self.patches {
            p.age += ASHES_EVERY.period();
            if p.age >= ASHES_FULL_TICKS {
                p.darkness = p.darkness.saturating_sub(FADE_STEP);
                dirty.push(p.dirty());
            }
        }
        self.patches.retain(|p| p.darkness > FADED);
        dirty
    }

    /// How dark the ashes on `cell` are, None without ashes.
    pub fn darkness(&self, cell: (i32, i32)) -> Option<u8> {
        self.patches.iter().filter(|p| p.cells.contains(&cell)).map(|p| p.darkness).max()
    }

    /// Whether something may be built on `cell`.
    pub fn buildable(&self, cell: (i32, i32)) -> bool {
        self.darkness(cell).is_none()
    }
}

impl crate::map::GameMap {
    /// The building `id` lost its last piece of wood: removed, its footprint left in ashes. The ground to draw again.
    pub(crate) fn burn_down(&mut self, id: crate::building::BuildingId) -> Vec<DirtyRect> {
        let Some(i) = self.building_index(id) else { return Vec::new() };
        let cells = self.buildings[i].walled_cells(self.terrain.size() as i32);
        self.remove_building(i);
        self.ashes.add(cells)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turns(ashes: &mut Ashes, from: u32, to: u32) {
        (from..to).for_each(|t| _ = ashes.tick(Tick::from_wire(t)));
    }

    #[test]
    fn dark_for_6000_turns_then_fades_in_18_steps() {
        let mut ashes = Ashes::default();
        ashes.add(vec![(3, 4), (4, 4)]);
        assert_eq!((ashes.darkness((3, 4)), ashes.buildable((4, 4)), ashes.buildable((5, 4))), (Some(DARK), false, true));
        turns(&mut ashes, 1, ASHES_FULL_TICKS);
        assert_eq!(ashes.darkness((3, 4)), Some(DARK), "full until then");
        let mut steps = 0;
        let mut t = ASHES_FULL_TICKS;
        while !ashes.buildable((3, 4)) {
            turns(&mut ashes, t, t + 16);
            t += 16;
            steps += 1;
        }
        assert_eq!(steps, 18);
        assert!(ashes.patches.is_empty());
    }

    #[test]
    fn a_hut_burnt_down_leaves_ashes_nobody_builds_on() {
        use crate::building::{Building, BuildingKind};
        let mut map = crate::map::GameMap::sandbox_units();
        let hut = map.add_building(Building { used: 1, ..Building::new(BuildingKind::Hut { size: 1 }, 1, 70 * 512, 70 * 512, 0) });
        let (corner, cells) = map.building(hut).map(|b| ((b.x, b.z), b.walled_cells(128))).unwrap();
        map.set_fire(hut, 1);
        let dirty: Vec<DirtyRect> = (0..60).flat_map(|_| map.tick()).collect();
        assert!(map.building(hut).is_none(), "burnt down");
        assert!(cells.iter().all(|&c| !map.ashes.buildable(c)));
        assert!(cells.iter().all(|&(x, z)| dirty.iter().any(|d| (d.min.0..d.max.0).contains(&x) && (d.min.1..d.max.1).contains(&z))), "drawn again");
        let plan = Building::site(BuildingKind::Hut { size: 1 }, 1, corner.0, corner.1, 0);
        assert!(!crate::placement::can_place(&map, &plan, None), "not on ashes");
    }

    #[test]
    fn a_spread_out_patch_dirties_one_rect_covering_every_cell_not_just_one() {
        let mut ashes = Ashes::default();
        let dirty = ashes.add(vec![(3, 4), (9, 4), (3, 11)]);
        assert_eq!(dirty, vec![DirtyRect { min: (3, 4), max: (10, 12) }], "one rect bounding them all, not one per cell");
    }

    #[test]
    fn the_101st_patch_clears_the_oldest() {
        let mut ashes = Ashes::default();
        for k in 0..ASHES_SLOTS as i32 {
            ashes.add(vec![(k, 0)]);
        }
        let dirty = ashes.add(vec![(0, 9)]);
        assert_eq!(ashes.patches.len(), ASHES_SLOTS);
        assert!(ashes.buildable((0, 0)) && !ashes.buildable((0, 9)));
        assert_eq!(dirty.len(), 2, "the old patch's ground and the new one's");
    }
}
