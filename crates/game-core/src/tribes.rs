//! Which tribes are actually in this match, their starting alliances (`docs/specs/level-format.md`
//! "DefaultAllies") and which are computer-controlled (ai-scripts.md: "Blue is the human player and
//! never has one [a script]" — true of any tribe the level assigns a `cpscrNNN.dat` to), read once
//! from the level or generator and never re-derived from mutable state — unlike `GameMap::sites`, this
//! stays correct even for a tribe with no reincarnation site: a real mode (it just can't reincarnate if
//! its shaman dies), not a hypothetical.
//!
//! `TribeId` (one of the four colours) and `Owner` (a tribe, or no tribe at all) replace a bare `u8`
//! wherever code used to read an owner byte directly (TODO.md "Tribes"). Almost everything that owns
//! something — a building, a vehicle, a campfire, a spell effect, a `Command` — is always one of the
//! four tribes, never neutral, so those fields are plain `TribeId`. A `Unit` is the one exception: the
//! original's own file format also uses a person's owner byte for a neutral wild person (owner 255 in
//! the levels, `pop3_format::level::ThingData::owner`), so `Unit::owner` is `Owner`, not `TribeId` —
//! `Owner::Wild` is that same sentinel, type-safe. `Owner` and `TribeId` compare directly against each
//! other (`PartialEq` both ways) so a check like `unit.owner == building.owner` still reads the same as
//! it always did.

use crate::site::ReincarnationSite;
use pop3_format::header::TRIBES;
use pop3_format::Level;
use std::fmt;

/// One of the four tribes a match can have (`pop3_format::header::TRIBES`): blue is always the human
/// player in the original campaign (ai-scripts.md "Each computer tribe... Blue is the human player").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TribeId {
    Blue,
    Red,
    Yellow,
    Green,
}

impl TribeId {
    pub const ALL: [TribeId; TRIBES] = [TribeId::Blue, TribeId::Red, TribeId::Yellow, TribeId::Green];

    pub const fn as_u8(self) -> u8 {
        match self {
            TribeId::Blue => 0,
            TribeId::Red => 1,
            TribeId::Yellow => 2,
            TribeId::Green => 3,
        }
    }

    pub const fn as_usize(self) -> usize {
        self.as_u8() as usize
    }

    /// The level/original-file convention: 0 blue, 1 red, 2 yellow, 3 green. `None` for anything else
    /// (in particular 255, the neutral wildman sentinel — see `Owner::from_u8`).
    pub const fn from_u8(v: u8) -> Option<TribeId> {
        match v {
            0 => Some(TribeId::Blue),
            1 => Some(TribeId::Red),
            2 => Some(TribeId::Yellow),
            3 => Some(TribeId::Green),
            _ => None,
        }
    }

    /// The next tribe round-robin (`Blue -> Red -> Yellow -> Green -> Blue`): no game rule, just a way to
    /// name "some other tribe" (dev/demo commands' own enemy stand-in).
    pub const fn next(self) -> TribeId {
        Self::ALL[(self.as_usize() + 1) % TRIBES]
    }
}

impl fmt::Display for TribeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TribeId::Blue => "blue",
            TribeId::Red => "red",
            TribeId::Yellow => "yellow",
            TribeId::Green => "green",
        })
    }
}

/// Whoever owns a unit: one of the four tribes, or nobody (a neutral wild person — the only thing in
/// the simulation that isn't tribe-owned). See the module doc for why `Unit::owner` needs this and
/// nothing else does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    Tribe(TribeId),
    Wild,
}

impl Owner {
    pub const fn tribe(self) -> Option<TribeId> {
        match self {
            Owner::Tribe(t) => Some(t),
            Owner::Wild => None,
        }
    }

    /// The level/original-file convention (`Unit::owner`'s own doc): 0..3 a tribe, 255 (and, leniently,
    /// anything else never seen in practice) a neutral wild person.
    pub const fn from_u8(v: u8) -> Owner {
        match TribeId::from_u8(v) {
            Some(t) => Owner::Tribe(t),
            None => Owner::Wild,
        }
    }

    pub const fn as_u8(self) -> u8 {
        match self {
            Owner::Tribe(t) => t.as_u8(),
            Owner::Wild => 255,
        }
    }
}

impl From<TribeId> for Owner {
    fn from(t: TribeId) -> Owner {
        Owner::Tribe(t)
    }
}

impl PartialEq<TribeId> for Owner {
    fn eq(&self, other: &TribeId) -> bool {
        matches!(self, Owner::Tribe(t) if t == other)
    }
}

impl PartialEq<Owner> for TribeId {
    fn eq(&self, other: &Owner) -> bool {
        other == self
    }
}

/// One tribe's standing membership in the match: always present once `Tribes` is built (TODO.md
/// "Tribes" — `defeated` isn't modelled here yet, `GameMap::tribe_wiped_out` derives it from live
/// state instead).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tribe {
    pub id: TribeId,
    /// Computer-controlled, loaded once from the level (`Tribes::with_ai_scripts`) and never
    /// re-derived: `false` for every tribe on a generated map or sandbox (no header to say otherwise).
    pub computer: bool,
}

/// The tribes actually in this match (by owner) and their starting alliances.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tribes {
    present: Vec<Tribe>,
    allies: [u8; TRIBES],
}

impl Tribes {
    /// From an original level: the distinct owners among its shaman things (the same set `site::sites_from_level`
    /// builds sites from, but computed independently so a tribe counts as "in the match" from its starting shaman
    /// alone, whether or not a site also exists for it), with the header's own default allies (own bit only) — a
    /// real header's allies overwrite this later (`with_allies`), and none computer-controlled yet (`with_ai_scripts`).
    pub fn from_level(level: &Level) -> Self {
        Tribes { present: tribes_from_level(level), allies: default_allies() }
    }

    /// For generated maps and sandboxes: the distinct owners among `sites` (one per tribe there, 1:1).
    pub fn from_sites(sites: &[ReincarnationSite]) -> Self {
        Self::from_owners(sites.iter().map(|s| s.owner))
    }

    /// An explicit roster, own-bit-only allies, none computer-controlled — for tests, and any other caller that
    /// already knows exactly which tribes are in play without a level or a sites list to derive it from.
    pub fn from_owners(owners: impl IntoIterator<Item = TribeId>) -> Self {
        let mut ids: Vec<TribeId> = owners.into_iter().collect();
        ids.sort_unstable();
        ids.dedup();
        Tribes { present: ids.into_iter().map(|id| Tribe { id, computer: false }).collect(), allies: default_allies() }
    }

    /// Overrides the allies bitmask with an original level's real header, keeping the same roster.
    pub fn with_allies(mut self, allies: [u8; TRIBES]) -> Self {
        self.allies = allies;
        self
    }

    /// Sets each present tribe's `computer` flag from the level header's own `ai_scripts` slots
    /// (`LevelHeader::ai_scripts`, ai-scripts.md): computer-controlled iff it was assigned a script, the
    /// same test `ai_script.rs` already used before this existed. Blue's slot is always 0 ("Blue is the
    /// human player and never has one"), so this alone is enough to tell a human tribe from a computer
    /// one in every original level; a multiplayer level's stub scripts (ai-scripts.md "Stubs...") are a
    /// known exception not handled here yet (TODO.md "Tribes").
    pub fn with_ai_scripts(mut self, ai_scripts: [u8; TRIBES]) -> Self {
        for t in &mut self.present {
            t.computer = ai_scripts[t.id.as_usize()] != 0;
        }
        self
    }

    pub fn count(&self) -> u8 {
        self.present.len() as u8
    }

    pub fn all(&self) -> impl Iterator<Item = TribeId> + '_ {
        self.present.iter().map(|t| t.id)
    }

    /// Every present tribe's full membership record (id and computer flag).
    pub fn tribes(&self) -> impl Iterator<Item = Tribe> + '_ {
        self.present.iter().copied()
    }

    /// Whether `id` is in this match and computer-controlled; `false` for one not in the match at all.
    pub fn is_computer(&self, id: TribeId) -> bool {
        self.present.iter().any(|t| t.id == id && t.computer)
    }

    /// Whether `a` considers `b` an ally (the raw bitmask need not be symmetric — see `same_team`).
    /// `false` whenever either side isn't a tribe at all (a neutral wild person is never anyone's ally).
    pub fn allied(&self, a: impl Into<Owner>, b: impl Into<Owner>) -> bool {
        let (Some(a), Some(b)) = (a.into().tribe(), b.into().tribe()) else { return false };
        self.allies[a.as_usize()] & (1 << b.as_u8()) != 0
    }

    /// `a` and `b` are on the same side: the same owner, or allied either way.
    pub fn same_team(&self, a: impl Into<Owner>, b: impl Into<Owner>) -> bool {
        let (a, b) = (a.into(), b.into());
        a == b || self.allied(a, b) || self.allied(b, a)
    }

    /// Every tribe grouped by alliance, transitively: two tribes allied only via a shared third still group
    /// together. A cheap fixed-point merge over `same_team`, not a general graph library — fine at the 4-tribe cap.
    pub fn teams(&self) -> Vec<Vec<TribeId>> {
        let mut groups: Vec<Vec<TribeId>> = self.present.iter().map(|t| vec![t.id]).collect();
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
    pub fn alive_sides(&self, wiped_out: impl Fn(TribeId) -> bool) -> Vec<TribeId> {
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

/// The distinct owners among the level's shaman things, sorted, as tribes (a shaman thing's owner is
/// never the neutral wildman sentinel, so every one resolves).
fn tribes_from_level(level: &Level) -> Vec<Tribe> {
    let mut ids: Vec<TribeId> = level.things.iter().filter(|t| t.is_shaman()).filter_map(|t| TribeId::from_u8(t.owner)).collect();
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter().map(|id| Tribe { id, computer: false }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE: TribeId = TribeId::Blue;
    const RED: TribeId = TribeId::Red;
    const YELLOW: TribeId = TribeId::Yellow;
    const GREEN: TribeId = TribeId::Green;

    fn roster(ids: &[TribeId]) -> Vec<Tribe> {
        ids.iter().map(|&id| Tribe { id, computer: false }).collect()
    }

    /// Level 14's shape (`TODO.md` "Default allies"): `[1, e, e, e]` = red(1), yellow(2) and green(3) allied
    /// together against blue(0).
    fn level_14() -> Tribes {
        Tribes { present: roster(&[BLUE, RED, YELLOW, GREEN]), allies: [0b0001, 0b1110, 0b1110, 0b1110] }
    }

    #[test]
    fn allies_default_to_their_own_tribe_only() {
        let t = Tribes::from_sites(&[ReincarnationSite::at_cell(BLUE, (0, 0), 0), ReincarnationSite::at_cell(RED, (1, 1), 0)]);
        assert!(t.allied(BLUE, BLUE) && t.allied(RED, RED));
        assert!(!t.allied(BLUE, RED) && !t.allied(RED, BLUE));
        assert_eq!(t.count(), 2);
        assert_eq!(t.all().collect::<Vec<_>>(), vec![BLUE, RED]);
    }

    #[test]
    fn a_wild_owner_is_never_allied_or_on_anyones_team() {
        let t = Tribes::from_owners([BLUE]);
        assert!(!t.allied(Owner::Wild, Owner::Tribe(BLUE)) && !t.allied(Owner::Tribe(BLUE), Owner::Wild));
        assert!(!t.same_team(Owner::Wild, Owner::Tribe(BLUE)));
        assert!(t.same_team(Owner::Wild, Owner::Wild), "a wild person is its own team of one, same owner");
    }

    #[test]
    fn owner_and_tribe_id_compare_equal_both_ways() {
        assert_eq!(Owner::Tribe(BLUE), BLUE);
        assert_eq!(BLUE, Owner::Tribe(BLUE));
        assert_ne!(Owner::Wild, BLUE);
        assert_ne!(BLUE, Owner::Wild);
    }

    #[test]
    fn with_ai_scripts_marks_every_assigned_slot_computer_blues_never_is() {
        let t = Tribes::from_owners([BLUE, RED, YELLOW]).with_ai_scripts([0, 10, 12, 0]);
        assert!(!t.is_computer(BLUE));
        assert!(t.is_computer(RED) && t.is_computer(YELLOW));
        assert!(!t.is_computer(GREEN), "not even in the match");
    }

    #[test]
    fn same_team_is_symmetric_even_when_the_raw_bitmask_is_not() {
        // Only tribe 0 declares tribe 1 an ally (bit 1 set); tribe 1's own row says nothing back.
        let t = Tribes { present: roster(&[BLUE, RED]), allies: [0b0010, 0, 0, 0] };
        assert!(t.allied(BLUE, RED) && !t.allied(RED, BLUE));
        assert!(t.same_team(BLUE, RED) && t.same_team(RED, BLUE));
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
        let wiped = |tribe: TribeId| tribe == RED || tribe == YELLOW; // green still alive, allied with red and yellow
        let sides = t.alive_sides(wiped);
        assert_eq!(sides, vec![BLUE, GREEN], "blue alone, and green representing its still-alive team");
    }

    #[test]
    fn a_site_less_tribe_is_still_part_of_the_roster() {
        // tribes_from_level derives the roster from shaman things, independent of `GameMap::sites` — exercised at
        // the `GameMap` level in map.rs's own tests; here just confirm a hand-built roster with no site info at
        // all is a perfectly normal `Tribes`.
        let t = Tribes { present: roster(&[BLUE, RED]), allies: default_allies() };
        assert_eq!(t.count(), 2);
    }
}
