//! Ghost Army (docs/plans/ghost.md, pop3-rev-analysis.md "Ghost Army mechanics", "Per spell" 9): aimed at a point
//! like a blast. A one-shot, 5-tick controller (`Cast`, shaped like `angel_of_death::Cast`): the first 4 ticks are
//! no-ops here (the original's own tick-1 cosmetic mist burst is deferred — no sound system and no reusable
//! "flash"/puff primitive exist in this codebase yet to draw either that or a ghost's own death cue with), the
//! last does the real work:
//! - Scans the caster's own living units (an existing ghost counts too — "nothing excludes it", traced) in the
//!   cast point's 3 × 3 cells (`nearest::in_block`) and picks whichever present kind has the lowest
//!   `KindStats::ghost_cast` priority (shaman > firewarrior/preacher/spy/warrior > brave; ties and "nothing
//!   present" both fall back to brave) — its `count` (1, 3 or 6) is how many ghosts of that kind to make.
//! - Jitters each new ghost independently up to `SCATTER` world units per axis from the cast point
//!   (`WorldEvent::SpawnGhosts`, `GameMap::apply_event`): a loose scatter, no slot-finding or terrain check
//!   (`[ours]`: not traced to that precision, matches the trace's own "not a tight formation or cluster").
//! - Cap `status::GHOST_ARMY_MAX` (60), all kinds of a tribe's ghosts counted together: a cast that would push the
//!   total past it still spawns its full batch, but first force-kills (`WorldEvent::DestroyGhost`) one existing
//!   ghost of the *same count-tier* (brave/shaman/specialist) — the lowest-id one in that tier (`[ours]`: a tie
//!   break the trace doesn't pin down, kept for lockstep replay safety, same spirit as Blast's own "which 5").
//!
//! No duration: `status::Statuses.ghost` is a plain, untimed `bool` (confirmed — no expiry exists in the spawn
//! code at all). A ghost's own destruction is centralized elsewhere (`GameMap::destroy_ghost`, `map.rs`'s
//! `damage`, `sermon.rs`'s `change_owner`, `hypnotism.rs`'s `hypnotize`) — this module only ever creates ghosts.

use crate::effect::{Effect, EffectKind};
use crate::event::WorldEvent;
use crate::map::GameMap;
use crate::nearest::in_block;
use crate::status::GHOST_ARMY_MAX;
use crate::time::{Countdown, Ticks};
use crate::tribes::Owner;
use crate::unit::{UnitId, UnitKind};

/// The cast's own length (0.4 s, exe-confirmed): a cosmetic mist burst on the first tick (deferred, see above),
/// three no-op ticks, the scan/spawn/cap-cull on the last.
pub const CAST_TICKS: Ticks = Ticks::new(5);
/// Each new ghost lands independently jittered up to this many world units per axis from the cast point.
pub const SCATTER: i32 = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cast {
    turn: u32,
}

/// Cast by tribe `owner` at `at` (world units).
pub fn cast(owner: impl Into<Owner>, at: (u16, u16)) -> Effect {
    Effect::new(owner, at, Countdown::new(CAST_TICKS), EffectKind::GhostArmy(Cast::default()))
}

impl Cast {
    /// One turn of the cast; on the last one, the priority scan, spawn and cap-cull all happen at once.
    pub fn turn(&mut self, at: (u16, u16), owner: Owner, world: &GameMap) -> Vec<WorldEvent> {
        let events = if self.turn + 1 == CAST_TICKS.get() { resolve(at, owner, world) } else { Vec::new() };
        self.turn += 1;
        events
    }
}

/// The kind and count a cast at `at` by `owner` would copy: the present kind with the lowest `ghost_cast`
/// priority, defaulting to a brave's own (priority/count) when nothing in range has one at all.
fn best_copy(at: (u16, u16), owner: Owner, world: &GameMap) -> (UnitKind, u8) {
    let size = world.terrain.size() as i32;
    let mut present: Vec<UnitKind> = Vec::new();
    for u in world.units.iter().filter(|u| u.owner == owner && u.is_alive() && in_block(at, (u.x, u.z), size)) {
        if !present.contains(&u.kind) {
            present.push(u.kind);
        }
    }
    present
        .into_iter()
        .filter_map(|k| k.stats().ghost_cast.map(|gc| (gc, k)))
        .min_by_key(|(gc, _)| gc.priority)
        .map_or((UnitKind::Brave, UnitKind::Brave.stats().ghost_cast.unwrap().count), |(gc, k)| (k, gc.count))
}

/// The tribe's living ghosts right now, all kinds together.
fn ghost_count(owner: Owner, world: &GameMap) -> u32 {
    world.units.iter().filter(|u| u.owner == owner && u.is_alive() && u.statuses.ghost).count() as u32
}

/// The lowest-id living ghost of `owner`'s own tribe in the same count-tier as `count` (brave/shaman/specialist),
/// if any — who the cap cull destroys to make room.
fn cull_target(owner: Owner, count: u8, world: &GameMap) -> Option<UnitId> {
    world
        .units
        .iter()
        .filter(|u| u.owner == owner && u.is_alive() && u.statuses.ghost && u.kind.stats().ghost_cast.is_some_and(|gc| gc.count == count))
        .min_by_key(|u| u.id)
        .map(|u| u.id)
}

fn resolve(at: (u16, u16), owner: Owner, world: &GameMap) -> Vec<WorldEvent> {
    let (kind, count) = best_copy(at, owner, world);
    let mut events = Vec::new();
    if ghost_count(owner, world) + count as u32 > GHOST_ARMY_MAX
        && let Some(unit) = cull_target(owner, count, world)
    {
        events.push(WorldEvent::DestroyGhost { unit });
    }
    events.push(WorldEvent::SpawnGhosts { owner, kind, count, at });
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tribes::TribeId;
    use crate::unit::torus_delta;

    const AT: (u16, u16) = (10 * 512 + 256, 20 * 512 + 256);

    fn empty() -> GameMap {
        let mut map = GameMap::sandbox_units();
        map.units.clear();
        map.buildings.clear();
        map.totems.clear();
        map
    }

    #[test]
    fn copies_the_best_present_kind_shaman_beats_everyone() {
        let mut map = empty();
        map.spawn(TribeId::Blue, UnitKind::Brave, AT);
        map.spawn(TribeId::Blue, UnitKind::Firewarrior, AT);
        map.spawn(TribeId::Blue, UnitKind::Shaman, AT);
        assert_eq!(best_copy(AT, Owner::Tribe(TribeId::Blue), &map), (UnitKind::Shaman, 1));
    }

    #[test]
    fn specialists_beat_braves_but_not_the_shaman() {
        let mut map = empty();
        map.spawn(TribeId::Blue, UnitKind::Brave, AT);
        map.spawn(TribeId::Blue, UnitKind::Warrior, AT);
        let (kind, count) = best_copy(AT, Owner::Tribe(TribeId::Blue), &map);
        assert_eq!((kind, count), (UnitKind::Warrior, 3));
    }

    #[test]
    fn nothing_present_falls_back_to_brave() {
        let map = empty();
        assert_eq!(best_copy(AT, Owner::Tribe(TribeId::Blue), &map), (UnitKind::Brave, 6));
    }

    #[test]
    fn an_existing_ghost_is_as_eligible_a_candidate_as_a_live_unit() {
        let mut map = empty();
        let ghost = map.spawn(TribeId::Blue, UnitKind::Shaman, AT).id;
        map.unit_mut(ghost).unwrap().statuses.ghost = true;
        assert_eq!(best_copy(AT, Owner::Tribe(TribeId::Blue), &map), (UnitKind::Shaman, 1), "a ghost shaman is still the caster's best candidate, not excluded");
    }

    #[test]
    fn only_the_casters_own_tribe_and_the_3x3_block_count() {
        let mut map = empty();
        map.spawn(TribeId::Red, UnitKind::Shaman, AT); // another tribe: never copied
        map.spawn(TribeId::Blue, UnitKind::Shaman, (AT.0.wrapping_add(2000), AT.1)); // outside the block
        assert_eq!(best_copy(AT, Owner::Tribe(TribeId::Blue), &map), (UnitKind::Brave, 6));
    }

    #[test]
    fn spawns_land_jittered_within_scatter_of_the_cast_point() {
        let mut map = empty();
        map.spawn(TribeId::Blue, UnitKind::Brave, AT);
        map.add_effect(cast(TribeId::Blue, AT));
        map.run(CAST_TICKS);
        let ghosts: Vec<_> = map.units.iter().filter(|u| u.statuses.ghost).collect();
        assert_eq!(ghosts.len(), 6, "a brave's own count");
        for g in &ghosts {
            assert!(torus_delta(AT.0, g.x).abs() <= SCATTER && torus_delta(AT.1, g.z).abs() <= SCATTER, "{g:?} too far from the cast point");
            assert_eq!(g.kind, UnitKind::Brave);
        }
    }

    #[test]
    fn a_cast_past_the_cap_still_spawns_the_full_batch_but_culls_one_of_the_same_tier_first() {
        let mut map = empty();
        map.spawn(TribeId::Blue, UnitKind::Brave, AT);
        let existing: Vec<_> = (0..GHOST_ARMY_MAX)
            .map(|k| {
                let id = map.spawn(TribeId::Blue, UnitKind::Brave, (AT.0.wrapping_add(k as u16 * 8), AT.1)).id;
                map.unit_mut(id).unwrap().statuses.ghost = true;
                id
            })
            .collect();
        // Well outside the 3 × 3 scan block, so it never wins the priority scan over the nearby braves (it would
        // otherwise, being a lower-priority shaman) — only relevant here to the cap/cull bucket-matching itself.
        let shaman_ghost = map.spawn(TribeId::Blue, UnitKind::Shaman, (AT.0.wrapping_add(4000), AT.1)).id;
        map.unit_mut(shaman_ghost).unwrap().statuses.ghost = true;
        map.add_effect(cast(TribeId::Blue, AT));
        map.run(CAST_TICKS);
        assert!(map.unit(existing[0]).is_none(), "the lowest-id brave-tier ghost was culled to make room");
        for &id in &existing[1..] {
            assert!(map.unit(id).is_some(), "every other existing brave ghost survives");
        }
        assert!(map.unit(shaman_ghost).is_some(), "a different count-tier: never touched by a brave-tier cull");
        let total_ghosts = map.units.iter().filter(|u| u.statuses.ghost).count();
        assert_eq!(total_ghosts, GHOST_ARMY_MAX as usize - 1 + 6 + 1, "one brave-tier culled, the full new batch of 6 still spawned, the shaman ghost untouched");
    }
}
