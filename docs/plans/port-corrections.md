# Porting pop3-rev's `docs/for-open-populous.md` backlog — concrete corrections (Tier A)

Scratch (AGENTS.md "Plans"), not documentation. Source: `pop3-rev/docs/for-open-populous.md`'s own "Open-populous
propagation plan" section (updated 2026-11-04, extended through its own §17 as of 2026-11-06). Split out of the
combined `for-open-populous-port.md` plan into per-tier files on 2026-11-06 so each can be picked up independently;
this file carries former Tier A.

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

## Triage notes (2026-11-06) — don't trust the queue file's status claims without checking

- **§13 (vehicle-hut wood rate) has an exact, already-flagged landing spot**: `docs/specs/vehicles.md` "Vehicle
  huts", the `LAUNCH_WORK` bullet, already says the real mechanism "may just be wood delivery alone, same
  primitive as ordinary construction, not an added per-crew-member work timer" — §13 confirms exactly that and
  gives the real throttle rule. `TODO.md`'s own "Vehicle huts" bullet is the matching TODO entry.
- **§15 (construction worker throttling) has an exact landing spot too**: `TODO.md`'s "Construction pace" bullet
  already names the same controller function as "not fully closed" for exactly the worker-cap question §15
  answers (`max(1, floor(assigned/2))`, no separate per-kind cap enforced there) — this is a precise fill-in, not
  new ground.
- **§8/§9 (preacher conversion) land on `TODO.md`'s existing "Caution for whoever next touches `sermon.rs`'s own
  target eligibility" bullet** — §8 settles exactly the contradiction that bullet flags (plus adds spy as an
  eligible target, a small addition beyond what the bullet asks). §9's range/capacity/visual facts are new
  ground alongside it, not a correction.
- **§7 Angel of Death is NOT as stale as the propagation plan's top-line bullet claims.** The "Known starting
  condition" warning (an earlier session's edits were lost to an uncommitted-working-tree reset) is for real, but
  checking `pop3-rev-analysis.md`'s current "Angel of Death mechanics" section shows most of the *later*
  follow-up findings already survived somehow (the 3D-mesh companion object, its pose-blend ramp, the
  angel-vs-angel pure stat exchange, the per-type stat table, the firewarrior-damage/Whirlwind-immunity
  re-checks) — only the *opening cast sequence* is the old, pre-correction text: it says **36** spark particles
  where §7 corrects this to **42, in 6 groups of 7**, and is missing §7's framing that nothing is drawn at all
  for a beat after casting (two sounds only) before the invisible object starts falling. So this is a **targeted
  edit to one paragraph**, not the "wholesale rewrite" the propagation plan assumes — re-read §7 and the current
  section side by side before touching it, the diff is smaller than advertised.
- **Didn't individually re-verify §14** (ground-fire duration) against the current state — treat the propagation
  plan's own target pointer as a starting point, not as independently confirmed stale/fresh here; the first step
  is re-reading the current target before writing anything.

## Suggested order (do first, fastest value — exact landing spots confirmed above)
1. [x] **§15** construction worker throttling + wood-delivery — ported to `pop3-rev-analysis.md` "Building
   construction and wear" and `TODO.md` "Construction pace" (2026-11-06); trimmed from `pop3-rev`'s
   `docs/for-open-populous.md` (both the §15 section and its propagation-plan bullet). Left as a follow-up, not
   done here: `game-core`'s own `work.rs` wood-fetch throttle still differs from the now-confirmed exe rule
   (ours caps on missing pieces, the exe caps on half of assigned workers) — a behaviour change, out of scope for
   a docs-only port pass. The final "dropped pile -> site stock" hop also stays unresolved (needs a live memory
   check, not a docs fix).
2. **§13** vehicle-hut wood-delivery rate (`vehicles.md` "Vehicle huts", `TODO.md` "Vehicle huts") — same,
   replaces the `LAUNCH_WORK` guess outright.
3. **§8 + §9** preacher-conversion eligibility and sermon residue (`sermon.rs`'s spec coverage, `TODO.md`'s
   sermon-eligibility caution) — settles a flagged contradiction, adds range/capacity/visual facts.
4. **§7** Angel of Death — targeted edit only (particle count 36→42/"6 groups of 7", the "nothing drawn for a
   beat" opening beat): re-diff against the current section first per the triage note above, don't
   blanket-replace.
5. **§14** ground-fire duration (firestorm/lightning split, new companion fire-patch mechanic) — re-read
   `TODO.md`'s current "Earthquake and Firestorm" bullet first (not independently re-checked above) before
   editing; `tree::BURN_TICKS` stays an open guess either way.
