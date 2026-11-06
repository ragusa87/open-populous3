# Porting pop3-rev's `docs/for-open-populous.md` backlog — skip candidates (Tier E)

Scratch (AGENTS.md "Plans"), not documentation. Source: `pop3-rev/docs/for-open-populous.md`'s own "Open-populous
propagation plan" section (updated 2026-11-04, extended through its own §17 as of 2026-11-06) — §§2-5 of that
source file are not in its own propagation plan at all. Split out of the combined `for-open-populous-port.md` plan
into per-tier files on 2026-11-06 so each can be picked up independently; this file carries former Tier E.

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

Delete this file once the items below are closed out (ported, or noted as intentionally skipped) and trimmed.

## Items
Notification clear/dismiss-all, the UI-state debug bitmask, the camera zoom-stepper commands (check for overlap
with the camera work in `docs/plans/port-camera.md` before assuming still relevant), and the rest of the BYRNE
cheat cluster. Lowest priority; `pop3-rev`'s own file already frames these as "probably skip, confirm first" — a
quick pass to note each as "intentionally not implemented" (or genuinely port the one or two that turn out real)
closes them out, but nothing here blocks on it.
