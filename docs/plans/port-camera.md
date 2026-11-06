# Porting pop3-rev's `docs/for-open-populous.md` backlog — camera (Tier B)

Scratch (AGENTS.md "Plans"), not documentation. Source: `pop3-rev/docs/for-open-populous.md`'s own "Open-populous
propagation plan" section (updated 2026-11-04, extended through its own §17 as of 2026-11-06). Split out of the
combined `for-open-populous-port.md` plan into per-tier files on 2026-11-06 so each can be picked up independently;
this file carries former Tier B.

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

- **§16/§17 (camera) both have an exact landing spot**: `TODO.md`'s camera block already carries the exact two
  open notes these resolve — the zoom-80-100 `KNOTS` extrapolation hedge (§16) and the "`origin_x` has an
  unexplained constant ~50px offset, not yet resolved" line (§17). Both are small, surgical edits once confirmed.

## Suggested order (statically resolved, low gameplay risk, quick once picked up)
1. **§17** screen-origin X/Y asymmetry — removes the "~50px offset not yet resolved" hedge in `TODO.md`'s camera
   block.
2. **§16** camera zoom-preset index — removes/narrows the zoom-80-100 `KNOTS` extrapolation hedge in the same
   block; the concrete implication (3 reachable sub-presets per resolution row, readable off static data) may be
   worth a `TODO.md` note even if no code changes yet.

## Note for Tier E overlap
Before closing out the skip-candidates tier (`docs/plans/port-skip-candidates.md`, former Tier E — camera
zoom-stepper commands), check for overlap with the camera work done here first.
