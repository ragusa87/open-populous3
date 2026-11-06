# Porting pop3-rev's `docs/for-open-populous.md` backlog — judgment calls (Tier C)

Scratch (AGENTS.md "Plans"), not documentation. Source: `pop3-rev/docs/for-open-populous.md`'s own "Open-populous
propagation plan" section (updated 2026-11-04, extended through its own §17 as of 2026-11-06). Split out of the
combined `for-open-populous-port.md` plan into per-tier files on 2026-11-06 so each can be picked up independently;
this file carries former Tier C.

**Working method per item**: read the referenced `pop3-rev/docs/for-open-populous.md` section (§N) in full first —
this plan only carries pointers and this-repo-side triage notes, not the findings themselves, to avoid a second
copy to keep in sync. Use the `original-game-spec` skill's "Updating TODO.md and our specs" conventions: rewrite
the affected TODO.md bullet(s) with the concrete rule plus a pointer to the matching `pop3-rev-analysis.md`
section (never a raw address outside that file), fix any spec prose the finding corrects, keep addresses/offsets
confined to `pop3-rev-analysis.md`. Only once an item is actually ported, go back to `pop3-rev`'s
`docs/for-open-populous.md` and trim that bullet (and, once a whole numbered section is absorbed, that section
too) — its own AGENTS.md reserves that trim for "a future session that DOES have open-populous attached," which
is what each of these sessions now is. Tick an item below `[x]` once trimmed on the pop3-rev side, don't just tick
it once the open-populous side is written.

Delete this file once all items below are ported and trimmed.

## Triage notes (2026-11-06)

- **§11 (patrol attack-type point vs. buildings) lands on `units.md`'s own open "Open gap" line** about whether
  an attack-type patrol point can ever target a building — already phrased as an open question there, §11
  answers the engine-capability half only (confirmed yes) and explicitly leaves the UI-exposure half open; don't
  over-port.
- **Didn't individually re-verify §1** (pause/game-speed/cheat console) against `game_speed.rs` and whatever
  pause exists — treat the propagation plan's own target pointer as a starting point, not as independently
  confirmed stale/fresh here; the first step is re-reading the current target before writing anything.

## Items (not pure fill-ins — need a decision, not just a transcription)
1. **§1** pause/game-speed/hidden cheat console — compare against `game_speed.rs` and whatever pause exists;
   decide whether the game-speed multiplier is worth a debug/turbo mode, and whether any of "Computer Player
   off"/"Fog of war"/"Autocast spell" are worth their own debug toggles (the console itself: skip).
2. **§11** patrol attack-type points vs. buildings — confirm engine-capability note lands correctly on
   `units.md`'s existing open-gap line without overstating it as "the original UI lets players do this."
