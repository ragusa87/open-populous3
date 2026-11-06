# Porting pop3-rev's `docs/for-open-populous.md` backlog — native AI (Tier D)

Scratch (AGENTS.md "Plans"), not documentation. Source: `pop3-rev/docs/for-open-populous.md`'s own "Open-populous
propagation plan" section (updated 2026-11-04, extended through its own §17 as of 2026-11-06). Split out of the
combined `for-open-populous-port.md` plan into per-tier files on 2026-11-06 so each can be picked up independently;
this file carries former Tier D, the bulk of what's left.

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

Delete this file once all items below are ported and trimmed. Given the size of this tier, this is also a
candidate to archive into `docs/archived-plans/` if it stalls mid-way with real investigation behind a sub-item
(AGENTS.md "Plans"), rather than just deleting partial notes.

## Triage notes (2026-11-06) — don't trust the queue file's status claims without checking

- **§6 base findings (the `EVERY n m` phase formula, `GIVE_UP_AND_SULK`'s real "force success" flag, the
  `ATTACK`-is-the-same-task-queue-as-native-AI finding) already survived into `ai-scripts.md`** — confirmed
  present and matching. The risk the "Known starting condition" warning describes is real but narrower than it
  sounds: it's **§6i through §6r specifically** (and §7, handled in `docs/plans/port-corrections.md`) that did
  not survive, not the whole of §6. Confirmed still-open/stale in this repo: `SET_ATTACK_VARIABLE` is still an
  unresolved "(?)" in `ai-scripts.md` and `TODO.md`; `PUT_PERSON_IN_DT` is still documented as a flat one-shot
  teleport with no mechanism; `TODO.md`'s worker-assignment note still says "idle braves in id order" (§6k says
  this is wrong — real assignment is nearest-eligible-candidate, and flags this as a likely **code** change, not
  just a doc fix).
- **Didn't individually re-verify** §6l/m/n/o/p/q/r's exact current wording beyond the spot checks above — treat
  the propagation plan's own target pointers for those as a starting point, not as independently confirmed
  stale/fresh here; the first step of each item below is re-reading the current target before writing anything.

## Items
`SET_ATTACK_VARIABLE` (§6j), worker selection — **flag for a coding session, not just docs** (§6k), `ATTACK`'s
gather/regroup state machine (§6l), guard/patrol groups never walk (§6m), `PUT_PERSON_IN_DT`'s real tower-resolve
chain (§6n), task 2 shaman autonomous Convert (§6o), task 19 vehicle readiness/loading (§6p), task 27 firewarrior
vehicle coordinator (§6q), the vehicle-attack loop closing §6j's addendum (§6r). All target `ai-scripts.md` and/or
`open-populous`'s native-AI docs plus `TODO.md`; several (§6o/p/q) are "new ground to add," not corrections, so
don't go looking for an existing wrong statement to replace. Do this as one focused pass once picked up.

## Open questions this plan surfaces but doesn't answer
- Does §6k's worker-selection fix require an actual `game_core` code change, or can it land as a doc correction
  pending a later coding session? (The source file itself flags this as likely code, not just docs.)
- Is `ai-scripts.md`'s "Native AI" section structured to take nine more sub-findings (§6l-r) without becoming
  unwieldy, or does it need its own sub-headings first?
