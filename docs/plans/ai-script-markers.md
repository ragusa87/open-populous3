# AI script markers: wiring the level `Markers[256]` table into the native AI

## Why
Investigating a report (2026-11-02, chat only, no issue) that the AI attacked the human on level
1 when it shouldn't have: decoding the user's own `cpscr010.dat` (red's script on `levl2001.dat`)
showed its raid is gated on a `GET_HEIGHT_AT_POS`-based land-connection check (`ai-scripts.md`
"Worked example: level 1's opening flyby", extended this session) - red should stay passive until
the human bridges across. That gate, `COUNT_PEOPLE_IN_MARKER`, and the `ATTACK` command's `Marker`
target type are all unimplemented today (`ai_script.rs`'s `do_call` catch-all), not because the
behaviour is unknown - `ai-scripts.md` already specifies all three - but because nothing threads
the level header's `Markers[256]` table into `game-core` for them to read. This plan is that
wiring.

**Clean-room note**: purely an `open-populous` coding-session task. No code in `pop3-rev` changes;
that repo stays "document only" (its own `AGENTS.md`). Everything here is implemented from
`docs/specs/ai-scripts.md` and the already-parsed file format (`pop3-format`) - nothing here reads,
disassembles or runs `D3DPOPTB.EXE` directly. Where a sub-detail turns out to need the exe
confirmed (see "Explicitly out of scope" below), that's a question for a separate `pop3-rev` spec
session, not something traced from here.

## Scope, in order

1. **Thread the marker table into `GameMap`**
   - `pop3_format::header::LevelHeader` already parses `markers: [u16; MARKERS]` and exposes
     `marker_cell(index) -> (u8, u8)` (`crates/pop3-format/src/header.rs`) - nothing to change there.
   - Add a field to `GameMap` (`map.rs`, next to `start_camera`) holding the resolved cells, built
     the same way `StartCamera::from_header` already is in `load_original`:
     `header.as_ref().map(|h| std::array::from_fn(|i| h.marker_cell(i)))` - `None` on generated
     maps/sandboxes (no header at all).
   - Unit tests the same shape as `StartCamera`'s existing ones, built from a synthetic header in
     test code - never a real level file (original data, never committed, `AGENTS.md`).

2. **`GET_HEIGHT_AT_POS marker $v`** (`ai-scripts.md` line ~575)
   - New `Env::do_call` arm in `ai_script.rs`: resolve the marker field to a cell via the new
     `GameMap` field, read terrain height there (`Heightmap::height_at`, `terrain.rs` - check its
     exact signature/units when implementing), return `Some(height)`.
   - Decide and document inline what an out-of-range marker index or a headerless map returns
     (a safe default, not a panic) - this is our own guess, tag it as such.

3. **`COUNT_PEOPLE_IN_MARKER team|COUNT_WILD marker radius $v`** (line ~573), and
   **`IS_SHAMAN_IN_AREA team marker radius $v`** while in the same area of the file
   - Same cell resolution, then reuse the `torus_dist2`-radius-filter pattern `IS_BUILDING_NEAR`
     already uses (`ai_script.rs`), but over `self.map.units` instead of `self.map.buildings`.
   - `TeamWild`'s wildman case needs the same team-matching already used elsewhere (check
     `attack.rs`'s existing wildman handling for the right comparison).
   - `IS_SHAMAN_IN_AREA`: same cell/radius, filtered to `UnitKind::Shaman`, boolean result.

4. **`ATTACK`'s `TargetType::Marker` case** (`ai_script.rs`'s `"ATTACK"` arm - currently
   `TargetType::Marker => None`, "needs the level header's Markers[256] table, not read anywhere
   yet")
   - Add `AttackGoal::Marker(usize)` to `ai.rs`, resolved in `ai_attack_target` by the same
     `min_by_key(torus_dist2(...))` shape `AttackGoal::Building` already uses, against the
     marker's own cell.
   - This is what directly unblocks level 1's own red raid (both `DO ATTACK ... Marker 3 ...`
     calls in the extended worked example) and closes the "`ATTACK_MARKER` targets (same
     marker-table gap)" line already on record in `TODO.md`'s "Computer players (AI)" section.

5. **Tests**
   - Synthetic scripts built the same way existing tests already do (see `ai_script.rs`'s
     `attack_sends_fighters_at_the_named_teams_shaman_once_auto_attack_and_max_attacks_allow_it`
     and friends: hand-built `Call`s / `turn_zero_do` helper) - never the real `cpscr010.dat`/
     `levl2001.dat` bytes.
   - Worth one regression test reconstructing level 1's own gate *shape* (height-sum-of-two-marker-
     clusters, then a 3-unit `Marker`-targeted raid) from synthetic field values and a hand-built
     script fragment - proves the wiring, not a copy of the original file.

6. **Docs/TODO once done**
   - `TODO.md` "Computer players (AI)": replace the 2026-11-02 "Reported" entry (this session's
     finding) with what actually changed; update the "ATTACK_MARKER targets (same marker-table
     gap)" line in the "first native engine" bullet above it.
   - `docs/specs/ai-scripts.md`: note `GET_HEIGHT_AT_POS`/`COUNT_PEOPLE_IN_MARKER`/`IS_SHAMAN_IN_AREA`/
     `ATTACK`'s `Marker` case as implemented, not just specified.

## Explicitly out of scope (needs a `pop3-rev` spec session first, not guessable from here)
- `SET_MARKER_ENTRY`/`MARKER_ENTRIES`/`ONLY_STAND_AT_MARKERS` (patrol/guard groups): a bigger,
  separate feature. `ai-scripts.md` has their design-level shape (patrol between two markers or
  circle one, stand-still variant), but the circling pattern/speed and what happens when a guard
  dies are unconfirmed - soft enough to deserve a short pop3-rev look before attempting it, logged
  to its own `docs/todo/<slug>.md` entry rather than guessed silently here.
- `GUARD_AT_MARKER`/`GUARD_BETWEEN_MARKERS`/`VEHICLE_PATROL`/`SET_BASE_MARKER`/`RESET_BASE_MARKER`/
  `SET_DEFENCE_RADIUS`, and the six `[DONT_]TARGET_*` commands: `ai-scripts.md` marks these
  outright "(?)"/"unknown"/"showed no effect in testing" - genuinely need a pop3-rev investigation,
  not an implementation attempt.
- The separate, already-implemented `ai_defend_base`/`tend_ai_defense` reactive counter-attack's
  own guessed base radius (likely the actual cause of what was observed in play) is untouched by
  this plan - it doesn't depend on the marker table at all, and tightening it is a different task.

## Status
Not started - plan only, written 2026-11-02.
