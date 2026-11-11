# Ghost Army: implementation plan

Scratch, not documentation (AGENTS.md "Plans"): step breakdown for implementing the Ghost Army spell, plus the
open questions a spec/trace session or the user's own game knowledge still needs to answer. Delete once Ghost Army
is implemented and its content has moved into `docs/specs/spells.md`/`docs/specs/combat.md` (see step 10).

## Architecture: a ghost is a real `UnitKind` plus a status flag, not a new kind

Already decided by the existing code, not a fresh choice: a ghost firewarrior is a plain `Unit { kind:
UnitKind::Firewarrior, statuses: Statuses { ghost: true, .. }, .. }`, never a dedicated `UnitKind::Ghost` with a
`disguised_as` property. The evidence this is already the established shape: `status::Statuses.ghost: bool` is a
flag sitting alongside a unit's real, unchanged `kind` (`status.rs`); the client's translucency pass
(`units/mod.rs`) reads the unit's own `kind`-derived material and just darkens its alpha, it never substitutes a
different mesh/kind; `training.rs:245` copies `statuses.ghost` across a crew replacement while the unit's `kind`
is set from the building's own trained output, not from the person it replaces. Keeping the real kind means a
ghost firewarrior automatically gets the real firewarrior's stats, animations and throw behaviour for free
(`kind_stats.rs`, `firebolt.rs`) — exactly what "ranged attacks are cosmetic only" needs (it still *is* a
firewarrior mechanically, just flagged, so it throws fire normally and only the damage/fire-setting follow-up is
suppressed, see step 9 below). A `disguised_as` property would instead require every stats/animation/sprite
lookup in the codebase to indirect through an "effective kind" instead of plain `.kind` — no call site does that
today, and nothing about Ghost Army needs it to.

This is a different axis from the Spy's disguise (your other question): a spy keeps its real `kind: Spy` and its
real `owner`, but *displays* as if it belonged to another tribe — an owner-identity flag, not a kind-identity one.
**That mechanic doesn't exist in `game-core` at all yet** — confirmed absent, not just unfinished: `ai.rs:287`'s
own comment lists "spy-disguise reveal" alongside Ghost Army and soul-convert teardown as deferred
`GIVE_UP_AND_SULK` sweep items it can't yet do anything for, and `ai-scripts.md`/`roadmap.md` both say plainly
there's no disguise flag, "displayed vs. real owner" split, or reveal trigger anywhere in the simulation yet — a
subsystem still to design, not something to wire into alongside Ghost Army. Worth noting only because it would
likely follow the same spirit (a flag over the real unit, not a parallel kind) when it's eventually built — but it's
unrelated to this spell and out of scope here.

## Already true, no new work needed here

A fair amount of groundwork already exists, built ahead of time for this spell:
- `status::Statuses.ghost: bool` already exists (`status.rs`) and is already read in several places:
  - Client-side translucency is fully implemented: `units/mod.rs`'s `ghost_material`/`GhostMaterials` render the
    local player's own ghost (and invisible) units at `GHOST_ALPHA` (0.34). Nothing to do client-side.
  - `GameMap::cast_nearest_status`'s own predicate (`map.rs:890`) already excludes ghosts, so Magical
    Shield/Bloodlust/Invisibility already skip them correctly, with no Ghost Army-specific change needed there.
  - `tend_training` (`training.rs:216`, line 245) already carries `statuses.ghost` over when a ghost crew member
    is replaced by a freshly trained unit, and mana/cost math is the ordinary building economy, untouched by a
    ghost crewing it — the "training/mana interaction" item `docs/plans/spells.md` flags as deferred is actually
    already done.
- Spell-book plumbing is fully wired: cost/charges/range/cooldown (`spell_book.rs`), the `GhostArmy`/`Bloodlust`
  `STAND_INS` swap, HUD name/icon (`hud/spells.rs`), the cursor sprite 43 (`virtual_cursor.rs`). Nothing to touch
  there either.
- `pop3-ai-script`'s `field.rs` already parses the two relevant AI-script fields (1057
  `INT_M_SPELL_GHOST_ARMY_COST`, 1192 `INT_GHOST_ARMY`) — not wired to any behaviour yet; out of scope here unless
  a script actually needs `DO CAST` ghost army (check `ai_script.rs`/`ai.rs` only if that need shows up).

## What's confirmed/traced already (docs/plans/spells.md "Ghost Army", pop3-rev-analysis.md "Per spell" 9)

Aimed at a target point like Blast (cost 18 000, range 4096/8 cells, 60 s cooldown, 4 charges). Looks at the
caster's own units near the target and copies the single best kind present, by priority shaman > firewarrior >
preacher > spy > warrior > brave. Count depends on the tier: 1 ghost if it's the shaman, 3 if any other specialist,
6 if only braves are present. Cap `GHOST_ARMY_MAX` (60) per tribe, all of a tribe's ghosts counted together; once at
the cap, a cast simply produces nothing more (not a partial top-up, nothing evicted). No expiry — ghosts persist
until destroyed. Auto-targeting for a ghost unit is restricted to enemy ghosts and enemy shamans only (the real new
combat mechanic); melee itself is untraced ("melee untested"). A ghost's ranged attack is cosmetic only: the throw
animation plays, no damage follows. Own ghosts render translucent, an enemy's ghosts look like ordinary units to
that enemy (done, see above).

## Open questions — needs the user's own game knowledge or a trace session before locking in

Carried over from the current conversation, still unconfirmed:
1. **[good trace target]** Whether enemy units *prioritize* attacking a ghost over a real unit — not the same
   question as "what a ghost itself may attack" (already traced): this is about whether the auto-attack
   target-*scoring* function gives a ghost target extra weight. Find that function and check for a ghost-specific
   term, the same way the restricted-target rule itself was found.
2. **[good trace target]** Whether ghosts count toward a tribe's population headcount/quota — plausible they
   don't, given the separate `GHOST_ARMY_MAX` cap, but not stated anywhere; same "does this tally's predicate test
   the ghost bit" shape as questions 6-8 below.
3. **[marginal — only worth tracing if curious]** What "revealed once attacked" means exactly — the enemy only
   learning it was a ghost once it's destroyed (since it looks identical until then), or some distinct reveal
   effect. Fine to guess "no special cue, just normal destruction" for a first pass and revisit only if a trace
   session happens to turn up a dedicated reveal routine along the way.

Resolved by the user (confirmed, not traced, but a clear enough design rule to build to):
- Ghosts are *destroyed, not killed*: outright removal, not a normal health-based death. See the rewritten
  destruction section below.
- A destroyed ghost never tallies as a kill: `GameMap::kills`/`melee_deaths` (the `INT_*_KILLED_BY_*` AI-script
  family, `ai_script.rs`) must not count it.
- A ghost can't be converted (preacher conversion/soul-convert) or hypnotized — either one just destroys it
  instead of changing its owner.
- Melee durability, resolved as a consequence of centralizing destruction in `GameMap::damage` below: *any*
  damage of *any* kind/amount that reaches a ghost destroys it outright, melee included — no separate "one-hit"
  special case needed, since melee damage already goes through the same one `GameMap::damage` path as everything
  else. (The trace's "melee untested" was about a ghost's own attack behaviour in melee, not about what happens
  when a ghost *is* hit — still worth confirming the "any hit destroys it" assumption itself if a trace session
  gets the chance.)
- **Confirmed: a ghost consumes one of Hypnotism's `HYPNO_NUM_PEOPLE` (6) nearest-candidate slots, then is
  destroyed instead of captured** — it does not get skipped so the selector reaches a 7th real target. This means
  Hypnotism's own candidate `pred` (`hypnotism.rs:41`) deliberately gets **no** ghost exclusion added (unlike
  `cast_nearest_status`'s, which already excludes ghosts for Shield/Bloodlust/Invisibility) — a ghost stays a
  normal candidate for `nearest_n_fresh_first`, and only the *outcome* (destroy vs. capture) is redirected, by
  `GameMap::hypnotize`'s own ghost check (destruction section below).

Newly surfaced while mapping this to the code, still open:
4. Release timing **[good trace target]**: every other aimed, non-terrain spell (Blast, Lightning, Swamp,
   Hypnotism, Convert, Magical Shield, Bloodlust, Invisibility...) releases `CAST_RELEASE` (6 turns) into the
   shaman's cast jump, each traced the same way. Nothing says Ghost Army is different — assume the same unless a
   trace session checks it the same way.
5. Exact spawn placement **[good trace target]** for the 1/3/6 ghosts: on the cast point itself, scattered around
   it like a queue-spot search (`slots::free_spots_near`), or clustered on whichever of the caster's own units
   supplied the copied kind? Likely visible in the same function already traced for the priority/count rule
   (pop3-rev-analysis.md "Per spell" 9) — worth a second look at that function's full body rather than a fresh
   hunt.
6. Whether a ghost can itself seed another Ghost Army cast **[good trace target]** (i.e. does the priority scan's
   own candidate predicate test `statuses.ghost`/the exe's ghost bit `+0x10 bit 0x800`, excluding existing ghosts,
   or does it look at anyone present regardless?) — guess here: real units only, since copying a copy doesn't
   match "copies the caster's own units near the target" as traced, but not explicit. Same shape as question 2
   above and the mana-income item below: "does this predicate test the ghost bit."

One more of the same shape, currently only flagged inline in the integration-risk section below — worth tracing
together with question 2 and 6 above:
7. **[good trace target]** `headcount.rs`'s mana income (`income.shaman`/`mana_rate`): does a ghost shaman
   generate mana, or does the original's own income-tally function already exclude the ghost bit the way it
   excludes shamans/angels from other counts?

**Confirmed by the user, no longer open**: the "spare the shaman" exemptions on `effect::hurts` do **not** apply
to a ghost shaman — she still "has no real body", so the usual reason to spare a real shaman doesn't carry over:
- Lightning/Lava's "spare the caster's own shaman" (`effect.rs:208`): **confirmed — lava kills a ghost shaman**
  (the user's own words: "No lava will kill the ghost"). Same presumably goes for Lightning, which shares this
  exact exclusion clause.
- Swarm's own, broader "spare every shaman of any tribe" (`effect.rs:209`, the insect plague/"bees"): **reported,
  not fully confirmed** ("Bees too most probably") — implement the same way, but this one specifically is worth
  a trace/replay check if the chance comes up, since it's hedged rather than stated outright.

Both are the same one-line fix in `effect::hurts` (step 7 below folds this in): the shaman clause in each arm
gains `&& !u.statuses.ghost`, so it only exempts a *real* shaman — a ghost one falls through to the normal
"hits" branch and gets destroyed by `GameMap::damage`'s own ghost check like anything else.

## Key integration risk: `UnitKind::Shaman` is assumed unique per tribe almost everywhere

A ghost shaman (the 1-ghost case) is just a second `Unit` with `kind: UnitKind::Shaman`, `owner` the same tribe,
and `statuses.ghost: true`. A large number of call sites filter on `u.kind == UnitKind::Shaman` assuming there is
exactly one per tribe — these will all need an explicit `&& !u.statuses.ghost` exclusion, or they'll risk a ghost
shaman standing in for (or shadowing) the real one:
- `GameMap::shaman_of`/`shaman_of_mut` (`map.rs:791`, `:809`) — the shaman lookup almost everything else goes
  through (casting permission, camera, cast range, teleport, worship eligibility...). Since `units` stays sorted
  by id and the real shaman is always spawned first (lower id), `.find()` happens to still return her today even
  with a later-spawned ghost shaman alive alongside — but that's incidental, not a guarantee (e.g. after the real
  shaman dies and reincarnates, is her new id still lower than the ghost's? needs checking, not assumed).
- `headcount.rs`'s mana income (`income.shaman`, `mana_rate(... UnitKind::Shaman, true)`) — a ghost shaman must not
  generate mana income; check whether headcount's own tallying already filters by `statuses.ghost` anywhere or
  needs it added.
- `map.rs:1430` (`shamans_alive`, a per-tribe table) and `map.rs:2117`'s death-handling branch (reincarnation
  timing, `RESPAWN_TICKS`) — a ghost shaman's death must **not** drive the tribe's own reincarnation logic.
- `armageddon.rs:57`'s own shaman pick, `ai.rs:701`'s `AttackGoal::Shaman`, `ai_script.rs:384`'s shaman-near-marker
  check — an AI targeting or scripted "the enemy shaman" lookup landing on a ghost instead of the real one would
  be a real bug, not a cosmetic one.
- `worship.rs` (`shaman_only` gating, `shaman_locked`) — should a ghost shaman be able to worship at a vault? Almost
  certainly not; needs excluding.
- `send_in.rs`/`enter.rs`/`vehicle.rs`'s existing "not a shaman" exclusions (keeping the real shaman out of
  buildings/boarding as cargo) already exclude *every* `UnitKind::Shaman`, ghost or not — check whether that's
  fine as-is (a ghost shaman shouldn't enter/board either) or needs to special-case the ghost back in.
- `effect.rs`'s shaman exemptions need the *opposite* treatment, now confirmed (see "Open questions" above): a
  ghost shaman is **not** spared by lightning/lava (`effect.rs:208`, user-confirmed) or, most likely, Swarm
  (`effect.rs:209`, reported) — she "has no real body", so the usual reason to spare the real shaman (she's a
  unique, precious, reincarnating unit) doesn't apply to a copy. Both get `&& !u.statuses.ghost` added to their
  shaman clause (step 7 below).

## Implementation steps

1. `spell.rs`: add `Spell::GhostArmy { at: (u16, u16) }`, wire it into `Spell::at_point` (currently returns `None`
   for `SpellKind::GhostArmy`), `Spell::kind()`, and `Spell::cast()`'s terrain match (a no-op arm, like Hypnotism).
2. `status.rs`: add `pub const GHOST_ARMY_MAX: u32 = 60;` alongside the other spell-count constants there
   (`HYPNO_NUM_PEOPLE`, `SHIELD_STEPS`, ...).
3. **No hardcoded priority list in the effect module.** `kind_stats.rs`'s `KindStats` is already the per-`UnitKind`
   property table (`life`, `rank`, `sight`, ... via `UnitKind::stats()`) — add the priority/count there instead of
   a second, parallel ordering that `ghost_army.rs` would have to keep in sync by hand:
   ```rust
   /// Ghost Army (docs/plans/ghost.md): `None` for a kind that can never be copied (Wildman, AngelOfDeath — never
   /// one of a tribe's own followers/shaman anyway). For the rest: `priority` picks among several kinds present at
   /// once (lower wins — shaman > firewarrior > preacher > spy > warrior > brave, traced,
   /// pop3-rev-analysis.md "Per spell" 9); `count` is how many ghosts are made when this kind wins.
   pub ghost_cast: Option<GhostCast>,
   ```
   ```rust
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub struct GhostCast { pub priority: u8, pub count: u8 }
   ```
   Each existing `const SHAMAN`/`BRAVE`/`WARRIOR`/... gets its own `ghost_cast` literal (`Some(GhostCast { priority:
   0, count: 1 })` for `SHAMAN`, `priority: 1, count: 3` for `FIREWARRIOR`/`PREACHER`/`SPY`/`WARRIOR` — tie value is
   fine, only the minimum matters — `priority: 2, count: 6` for `BRAVE`; `None` for `WILDMAN`/`ANGEL_OF_DEATH`).
   Adding/rebalancing a kind later only ever touches its own table row, never `ghost_army.rs`.
4. New `crates/game-core/src/ghost_army.rs`, shaped like `hypnotism.rs`: a one-turn `EffectKind::GhostArmy`,
   `effect(owner, at) -> Effect`, and a `turn(&mut self, at, owner, world) -> Vec<WorldEvent>`:
   - Scan the caster's own living, non-ghost units in `nearest::in_block(at, ..., size)`'s 3 × 3 cells (question 6
     above: whether to exclude existing ghosts from the scan).
   - Of the kinds present, pick the one whose `stats().ghost_cast` is `Some` with the lowest `priority`
     (`present.iter().filter_map(|k| k.stats().ghost_cast.map(|gc| (gc, k))).min_by_key(|(gc, _)| gc.priority)`) —
     its `count` is how many ghosts to make. No kind present with a `ghost_cast` at all (shouldn't happen: the
     caster always has at least her own shaman, but guard it) means nothing to copy.
   - Cap check: count the tribe's total ghosts across the whole map (`owner == tribe && statuses.ghost`); if
     adding the new batch would reach/exceed `GHOST_ARMY_MAX`, emit nothing (all-or-nothing, not a partial
     top-up, per the trace).
   - Emit a new `WorldEvent` (step 5) rather than spawning directly from the effect — keeps effects as the usual
     "read state, emit events" shape every other effect module follows.
5. `event.rs`/`map.rs`: add `WorldEvent::SpawnGhosts { owner: Owner, kind: UnitKind, count: u8, at: (u16, u16) }`,
   applied in `GameMap::apply_event` mirroring the `SpawnAngelOfDeath` arm (`map.rs:1896`): spawn `count` units of
   `kind` for `owner` (placement per question 6), set `statuses.ghost = true` on each, leave them `Action::Idle`
   (or `Guarding` near the cast point — also question 6, not traced).
6. `map.rs` (and anywhere else the shaman-uniqueness audit above turns up): add `!u.statuses.ghost` to every "the
   tribe's shaman" lookup that needs it, per the integration-risk section.
7. Destruction paths — "ghosts are destroyed, not killed", centralized at the few real chokepoints instead of
   special-cased per spell (the user's own steer: any code path that would normally damage/convert/hypnotize a
   unit instead just deletes it outright when it's a ghost):
   - New `GameMap::destroy_ghost(&mut self, unit: UnitId)`: finds the unit by id (`unit_index`) and removes it
     from `self.units` directly (`self.units.remove(i)`), the same direct-removal shape `tend_training`
     (`training.rs:240`) already uses for its own trainee replacement — *not* the normal
     `Dying`/`Spirit`/`UnitEvent::Gone`/`retain` pipeline (`unit.rs`, `map.rs`'s end-of-`tick` sweep), which takes
     several ticks and assumes a corpse worth animating. No corpse, no spirit rise, no reincarnation hook: it's
     just gone this same tick (matches "a ghost has no real body to hand off",
     `docs/specs/pop3-rev-analysis.md`'s 0x4D4B50/0x4D83B0 entries). `self.units` stays sorted by id either way
     (a `remove`, unlike a swap-remove, preserves order), so `unit_index`'s binary search keeps working for every
     other event in the same batch — events reference units by `UnitId` and re-resolve their index fresh each
     time (`apply_event`'s own pattern throughout), so removing one mid-batch doesn't invalidate another event's
     target. Caution found while tracing this: `GameMap::tick` already snapshots `shamans_alive: Vec<bool>` by
     index near its very top (`map.rs:1430`) and compares it *by the same index* later in the same tick
     (`map.rs` ~1544) to detect "a shaman just died" — `tend_training`'s existing mid-tick removal sits between
     those two points already, so this plan isn't introducing a new category of risk, but it's worth being
     deliberate about *where* `destroy_ghost` gets called from relative to that snapshot/compare pair rather than
     assuming it's automatically safe; flag as a pre-existing fragility unrelated to Ghost Army if it turns out to
     matter (a `TODO.md` note, not something to fix as part of this spell).
   - `GameMap::damage` (`map.rs:2005`, "the one hurt path" — melee, bolts, blast, lightning, swamp, lava, swarm all
     go through it): if the targeted unit `statuses.ghost`, skip `take_hit` entirely and call `destroy_ghost`
     instead, reporting "no kill" to its caller (not `true`/died) so `apply_event`'s own tally block
     (`map.rs` ~1812-1818) never increments `kills`/`melee_deaths` for it (confirmed: a destroyed ghost must not
     count as a kill, in stats or the `INT_*_KILLED_BY_*` AI-script family). This one change is also what makes
     Blast's "ghost in the centre cell removed" and Swarm's "ghost in its cells destroyed" (both already traced)
     fall out for free — they already emit `WorldEvent::Damage` for everyone in range, so `blast.rs`/`swarm.rs`
     need **no ghost-specific code of their own** once `damage` itself handles it; same for melee and firewarrior
     bolts, which resolves the "melee untraced"/"one-hit" question as a consequence rather than a special case
     (see the open-questions update above). **Risk to verify carefully, not assumed safe**: melee's own fight
     coordinator may read `Action::Dying`/knockback state or the `died` flag off a `WorldEvent::Struck`/`Damage`
     pair to decide when a fight ends (`melee.rs`) — a normal death still leaves the loser present (just
     `!is_alive()`) for several ticks before real removal, while a ghost vanishes from `self.units` the same tick
     it's hit; if any coordinator logic assumes the loser object still exists a tick later, this needs adjusting
     alongside `destroy_ghost`, not after the fact.
   - `effect::hurts` (`effect.rs:196`), a **prerequisite** for the `GameMap::damage` change above to ever matter
     for a ghost shaman: `GameMap::damage` only reaches its own ghost check *after* `hurts` has already said the
     hit lands (`damage`'s own `.filter(|&i| hurts(...))`), so as long as `hurts` keeps exempting "the caster's own
     shaman" unconditionally, a ghost shaman would never even reach the new ghost-destroy branch for Lightning/
     Lava, staying bugged-immortal instead. Fix both shaman clauses to add `&& !u.statuses.ghost` (confirmed by
     the user: lava kills a ghost shaman; Swarm's own, broader "spare every shaman" clause almost certainly the
     same, reported but not fully confirmed) — `DamageKind::Lightning | DamageKind::Lava` becomes `!(from ==
     Some(u.owner) && u.kind == UnitKind::Shaman && !u.statuses.ghost)`, and `DamageKind::Swarm`'s exemption drops
     to just `u.kind == UnitKind::AngelOfDeath || (u.kind == UnitKind::Shaman && !u.statuses.ghost)`.
   - `GameMap::change_owner` (`sermon.rs:175`, preacher conversion/soul-convert's shared path) and
     `GameMap::hypnotize` (`hypnotism.rs`): same pattern — if the target `statuses.ghost`, call `destroy_ghost`
     instead of converting/capturing it (confirmed: a ghost can't be converted or hypnotized, it just
     disappears). Deliberately **not** adding a ghost exclusion to Hypnotism's own candidate `pred`
     (`hypnotism.rs:41`) — confirmed above that a ghost stays selectable there (and so costs the caster one of the
     6 slots) with only the *outcome* redirected here.
8. `spotting.rs`'s `first_seen`: when the looker `u.statuses.ghost`, restrict its candidate `people` to
   `t.statuses.ghost || t.kind == UnitKind::Shaman` instead of the normal "any enemy" filter (question: does this
   include an enemy's own ghost shaman? most likely yes, same rule as any other ghost).
9. `firebolt.rs`: when the thrower `statuses.ghost`, keep the throwing animation (`Action::Throwing`) but drop the
   `WorldEvent::Damage`/fire-setting/building-wear follow-ups it would otherwise push (find the exact emission
   points in the throw function and gate them there).
10. Tests alongside each step (CLAUDE.md "Testing & modularity"), shaped like `hypnotism.rs`'s own test module:
    priority/count selection (including a `kind_stats.rs`-style table test over `UnitKind::ALL`'s own
    `ghost_cast`, matching the existing `fighting_numbers`/`wear_dealt_to_buildings` tests there), cap
    enforcement, auto-targeting restriction, cosmetic ranged attacks, each destruction path, and the
    shaman-uniqueness exclusions from step 6.
11. Once implemented end to end: move `docs/plans/spells.md`'s "Ghost Army" section into `docs/specs/spells.md`
    (its own table row, matching the "Implemented" style) and relevant parts of the integration-risk notes above
    into `docs/specs/combat.md`/`units.md` where they land permanently; delete that section from
    `docs/plans/spells.md` and delete this file (AGENTS.md "Plans").
