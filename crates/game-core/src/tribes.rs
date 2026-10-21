//! Which tribes are actually in this match, and their starting alliances (`docs/specs/level-format.md`
//! "DefaultAllies"), read once from the level or generator and never re-derived from mutable state — unlike
//! `GameMap::sites`, this stays correct even for a tribe with no reincarnation site: a real mode (it just can't
//! reincarnate if its shaman dies), not a hypothetical.

use crate::site::ReincarnationSite;
use pop3_format::header::TRIBES;
use pop3_format::Level;

/// The tribes actually in this match (by owner) and their starting alliances.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tribes {
    present: Vec<u8>,
    allies: [u8; TRIBES],
}

impl Tribes {
    /// From an original level: the distinct owners among its shaman things (the same set `site::sites_from_level`
    /// builds sites from, but computed independently so a tribe counts as "in the match" from its starting shaman
    /// alone, whether or not a site also exists for it), with the header's own default allies (own bit only) — a
    /// real header's allies overwrite this later (`with_allies`).
    pub fn from_level(level: &Level) -> Self {
        Tribes { present: tribes_from_level(level), allies: default_allies() }
    }

    /// For generated maps and sandboxes: the distinct owners among `sites` (one per tribe there, 1:1).
    pub fn from_sites(sites: &[ReincarnationSite]) -> Self {
        Self::from_owners(sites.iter().map(|s| s.owner))
    }

    /// An explicit roster, own-bit-only allies — for tests, and any other caller that already knows exactly which
    /// tribes are in play without a level or a sites list to derive it from.
    pub fn from_owners(owners: impl IntoIterator<Item = u8>) -> Self {
        let mut present: Vec<u8> = owners.into_iter().collect();
        present.sort_unstable();
        present.dedup();
        Tribes { present, allies: default_allies() }
    }

    /// Overrides the allies bitmask with an original level's real header, keeping the same roster.
    pub fn with_allies(mut self, allies: [u8; TRIBES]) -> Self {
        self.allies = allies;
        self
    }

    pub fn count(&self) -> u8 {
        self.present.len() as u8
    }

    pub fn all(&self) -> impl Iterator<Item = u8> + '_ {
        self.present.iter().copied()
    }

    /// Whether `a` considers `b` an ally (the raw bitmask need not be symmetric — see `same_team`).
    pub fn allied(&self, a: u8, b: u8) -> bool {
        (a as usize) < TRIBES && b < 8 && self.allies[a as usize] & (1 << b) != 0
    }

    /// `a` and `b` are on the same side: the same tribe, or allied either way.
    pub fn same_team(&self, a: u8, b: u8) -> bool {
        a == b || self.allied(a, b) || self.allied(b, a)
    }

    /// Every tribe grouped by alliance, transitively: two tribes allied only via a shared third still group
    /// together. A cheap fixed-point merge over `same_team`, not a general graph library — fine at the 4-tribe cap.
    pub fn teams(&self) -> Vec<Vec<u8>> {
        let mut groups: Vec<Vec<u8>> = self.present.iter().map(|&t| vec![t]).collect();
        loop {
            let mut merged = false;
            'outer: for i in 0..groups.len() {
                for j in (i + 1)..groups.len() {
                    if groups[i].iter().any(|&a| groups[j].iter().any(|&b| self.same_team(a, b))) {
                        let extra = groups.remove(j);
                        groups[i].extend(extra);
                        merged = true;
                        break 'outer;
                    }
                }
            }
            if !merged {
                break;
            }
        }
        groups
    }

    /// One representative tribe per side that still has at least one living tribe.
    pub fn alive_sides(&self, wiped_out: impl Fn(u8) -> bool) -> Vec<u8> {
        self.teams().into_iter().filter_map(|team| team.into_iter().find(|&t| !wiped_out(t))).collect()
    }
}

/// Own bit only: every tribe allied with itself alone (the header's own documented default).
fn default_allies() -> [u8; TRIBES] {
    let mut allies = [0u8; TRIBES];
    for (t, a) in allies.iter_mut().enumerate() {
        *a = 1 << t;
    }
    allies
}

/// The distinct owners among the level's shaman things, sorted.
fn tribes_from_level(level: &Level) -> Vec<u8> {
    let mut owners: Vec<u8> = level.things.iter().filter(|t| t.is_shaman()).map(|t| t.owner).collect();
    owners.sort_unstable();
    owners.dedup();
    owners
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Level 14's shape (`TODO.md` "Default allies"): `[1, e, e, e]` = red(1), yellow(2) and green(3) allied
    /// together against blue(0).
    fn level_14() -> Tribes {
        Tribes { present: vec![0, 1, 2, 3], allies: [0b0001, 0b1110, 0b1110, 0b1110] }
    }

    #[test]
    fn allies_default_to_their_own_tribe_only() {
        let t = Tribes::from_sites(&[ReincarnationSite::at_cell(0, (0, 0), 0), ReincarnationSite::at_cell(1, (1, 1), 0)]);
        assert!(t.allied(0, 0) && t.allied(1, 1));
        assert!(!t.allied(0, 1) && !t.allied(1, 0));
        assert_eq!(t.count(), 2);
        assert_eq!(t.all().collect::<Vec<_>>(), vec![0, 1]);
    }

    #[test]
    fn same_team_is_symmetric_even_when_the_raw_bitmask_is_not() {
        // Only tribe 0 declares tribe 1 an ally (bit 1 set); tribe 1's own row says nothing back.
        let t = Tribes { present: vec![0, 1], allies: [0b0010, 0, 0, 0] };
        assert!(t.allied(0, 1) && !t.allied(1, 0));
        assert!(t.same_team(0, 1) && t.same_team(1, 0));
    }

    #[test]
    fn three_tribes_allied_against_one_group_to_two_sides_not_four() {
        let t = level_14();
        let teams = t.teams();
        assert_eq!(teams.len(), 2, "{teams:?}");
        let sizes: Vec<usize> = teams.iter().map(Vec::len).collect();
        assert!(sizes.contains(&1) && sizes.contains(&3), "{sizes:?}");
    }

    #[test]
    fn alive_sides_counts_by_team_not_by_raw_tribe() {
        let t = level_14();
        // Only tribe 0 (blue) survives among the allied trio: still one side for them collectively? No —
        // each tribe wiped except one ally still leaves that team alive via its survivor.
        let wiped = |tribe: u8| tribe == 1 || tribe == 2; // 3 (green) still alive, allied with 1 and 2
        let sides = t.alive_sides(wiped);
        assert_eq!(sides, vec![0, 3], "blue alone, and green representing its still-alive team");
    }

    #[test]
    fn a_site_less_tribe_is_still_part_of_the_roster() {
        // tribes_from_level derives the roster from shaman things, independent of `GameMap::sites` — exercised at
        // the `GameMap` level in map.rs's own tests; here just confirm a hand-built roster with no site info at
        // all is a perfectly normal `Tribes`.
        let t = Tribes { present: vec![0, 1], allies: default_allies() };
        assert_eq!(t.count(), 2);
    }
}
