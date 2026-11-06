# Spells: design decisions for the 7 not yet implemented

Scratch, not documentation (AGENTS.md "Plans"): design decisions gathered from the user for the spells
`docs/specs/spells.md` doesn't cover yet (Land Bridge, Flatten, Erode, Raise, Teleport, Blast, Lightning, Swamp,
Magical Shield, Bloodlust, Invisibility, Convert, Hypnotism and Volcano are done). `pop3-rev-analysis.md` leaves
real gaps the trace can't resolve that only the user's own knowledge of the original game can answer. Everything
below is "(reported, not traced)" unless marked otherwise — it's the user's word, not a disassembly trace, and
should be corrected if a spec session traces it differently later.

No implementation has started on any of these 7 spells, except Armageddon: `crates/game-core/src/armageddon.rs`
has a v1 placeholder (pauses, cancels fights, settles by headcount) that only partly reaches the design below —
see its own section and `TODO.md` for what's left. Volcano's own two shared sub-effects, `panicking_blast` and
`lava`, are now implemented too — Firestorm and Earthquake should reuse them, not reimplement. One other exception
already landed on `main` (it was a correction to *already-implemented* behaviour, not new spell work):
`effect::hurts` and the firebolt's knockback now both respect a Magical Shield against `DamageKind::Fire`
(firewarrior bolts), with tests, and `docs/specs/spells.md`/`combat.md` were corrected in a few other places
against the current code. See recent commits on `main`.

Move a spell's section into `docs/specs/spells.md` (its own table row/paragraph, matching the style of the
"Implemented" entries) once it's actually implemented; delete it from here at that point.

## Moderate tier

### Firestorm
`panicking_blast::PanickingBlast` is already implemented (built for Volcano, which uses it for its own fireballs
too) — reuse it as-is, no new effect module needed here.
- No terrain constraint on the fireball's landing point within its 6×6-cell square — sea is fine. TODO: check in
  the original what actually happens when one lands on water (not resolved).
- The whole effect can be a pure scheduler: spawn a `PanickingBlast` every 4 turns for ~55 spawns over 220 turns,
  holding no other state itself — confirmed, implementation details left to the coding session.
- Shielded units are already spared the panic (not the damage, same as a plain blast) in the implemented
  `PanickingBlast` — inferred, not explicitly confirmed, consistent with Shield blocking "most offensive spells"
  generally; revisit if a spec session finds otherwise.

### Whirlwind / Tornado
Motion is a genuine random walk ("implement something random") — no need to chase an exact step/bias model from
the original, an approximate wander is fine (same spirit as the approximate hydraulic Erosion below). It exerts a
lifting force on people (makes them fly), damages buildings, and can travel over water (not land-restricted).
- Water visual (client-side `WorldView`, not simulation logic): passing over water, the water's base turns blue
  and is drawn being pulled upward the same way the force lifts people (a waterspout look); settles back down the
  same way in reverse when the funnel leaves.
- Suggested shape (the user's own steer, not traced): like a fire effect, let the whirlwind's visual/force taper
  down in size as it nears the end of its life, rather than an abrupt stop (matches the trace's "wanders 100
  turns, then drifts away").
- Lift/throw: the fall at release is what deals the damage, not continuous damage while lifted (reported, not
  certain). Reported belief: lethal to a Brave (1000 hp) but survivable for a Warrior (1800 hp) — in tension with
  the traced flat `FALL_OUT_OF_WW_DAMAGE` (700) constant, which alone would not kill a 1000 hp Brave; needs
  reconciling (maybe cumulative/height-scaled, not flat) before implementation — **not resolved**.
- Vehicles: the already-traced "simple blast" applies (occupants put out, then spin-and-explode — already written
  up in `TODO.md`); additionally the lift mechanic may *also* catch vehicles directly, not just a blast tick —
  reported as a hypothesis ("probably"), not confirmed, needs checking.

### Earthquake
120 turns of shake; at turns 24/56/88 each building in 16×16 cells has a 20/128 chance to be shaken (occupants
out, partial explosion — this part is itself an open question already flagged in `pop3-rev-analysis.md` "Open
questions", not something to guess at here). At turn 60, a half-disc of radius 8 cells sinks, with a lava fissure.
- **Not asked this round**: the half-disc's orientation relative to the cast direction. Still open.
- `lava::effect`/`EffectKind::Lava` is already implemented (built for Volcano, which spawns it over its crater) —
  Earthquake's fissure should call it with its own cells and a shorter lifetime, not reimplement it. Its look
  (`crates/game-client/src/lava.rs`, the procedural texture set and tiling material) is still not wired to a
  `views::WorldView` for either spell yet — that client-side piece remains open.
- Lava/drowning-by-terrain-change bypasses Magical Shield entirely (confirmed via the Swamp discussion, which
  generalizes to the whole landscape-kill family) — already how `lava`'s own `DamageKind::Lava` works.

### Insect plague / Swarm
Aimed at a target point like Blast (traced: cost 40 000, range 6144/12 cells). 60 insects spawn there and wander
(confirmed: a genuine random walk, same "approximate is fine" spirit as Whirlwind below, not a chase/patrol
pattern) within at most 6 cells of the cast point — not pinned to the exact spot.
- Every 8 turns, enemies in its current 3 × 3 cells (around wherever the swarm has wandered to, not the original
  cast point) take `SWARM_PERSON_DAMAGE` (100) and panic; shamans and angels are immune; ghosts there are destroyed
  outright, not damaged (traced).
- Damage spares the caster's own tribe (confirmed: reuses the same `effect::hurts` rule Blast already has via
  `DamageKind::Blast`, no new `DamageKind` needed) — matches the trace's own "insects do not damage allies".
- Magical Shield blocks it (already noted in Shield's section above, and explicit in the trace: "deflects burn,
  blast, lightning, whirlwind and insect shots"). The trace also describes a **deflection**, not mere immunity: a
  shot arriving on a shielded enemy is redirected to a random point 8 cells away, owned by the shield's own tribe,
  and can repeat if it lands on another shielded target. Whether this is already how `effect::hurts` behaves for
  Blast/Lightning or still needs building is open — not Swarm-specific, flag for whoever implements it first.
- "Up to 3 times it raids an enemy building: occupants thrown out, damaged and panicked" (traced) — this is the
  first real use of `WorldEvent::Raid` (`event.rs`), which already exists as a defined, tested-safe, but
  unimplemented no-op (`map.rs`'s handler just returns `None`). Swarm's job here is to give it a body, not invent a
  new event.
- Movement speed: the trace's spell-speed table gives insect plague 80 (vs. panic/burn 110, whirlwind 120) — a
  guess for how far the swarm itself drifts per tick if a concrete number is needed; not confirmed against our own
  tick/turn units.
- Duration conflict, unresolved: the per-spell table says "200 turns (16.7 s)" for how long the swarm lasts, while
  the balance table's recharge-duration column says "45 s" for the spell slot itself — these may be two different
  things (effect lifetime vs. cast cooldown) rather than a contradiction; flag as a guess until checked against the
  existing `Spell::cooldown`-style handling of other aimed spells.

### Erosion (the real hydraulic version)
Approximate algorithm confirmed fine. Reported shape (uncertain, "I can be wrong"): the centre goes deeper, the
rest is a smooth falloff curve outward — closer to the existing `raise()`-style falloff brush in `terrain.rs` than
a true multi-cell hydraulic flow sim, so this may not need new machinery at all, just a slower/deeper/
longer-duration version of the falloff pattern already there.
- Jitter size/frequency of the 8×8 window around the cast target: unknown — tag as a guess when implemented.
- Floor: can reach sea level (0) but not below — same clamp as elsewhere.
- No conservation requirement for a first pass (shape over physical accuracy) — eroded height doesn't need to
  reappear lower down.
- Sparkles/visual: confirmed fine — reuse the generic terrain-effect sparkle treatment (`Effect::reshaping`), same
  family as Land Bridge/Flatten.
- Buildings collapsing from Erosion's terrain change: in scope, same general "uneven ground" rule as other
  terrain spells (already a known `TODO.md` gap, not Erosion-specific).
- **General rule surfaced here, not Erosion-specific** (already logged in `TODO.md`): a tree whose cell becomes
  water after any terrain change must drop to size 0 (dormant/invisible, like a burnt one) and not grow back while
  submerged. `Tree::tick` (`tree.rs`) takes no terrain/water input today — needs the heightmap threaded in,
  likely alongside the building-collapse handling in `GameMap::apply_event`'s `Terrain` case.

### Ghost Army
Aimed at a target point like Blast (traced: cost 18 000, range 4096/8 cells) — looks at the caster's own units near
that point and copies the single best kind present, by priority shaman > firewarrior > preacher > spy > warrior >
brave (traced). The count produced depends on which tier that best kind falls in: 1 ghost if it's the shaman, 3 if
it's any other specialist (firewarrior/preacher/spy/warrior), 6 if the best kind present is only braves (traced).
- Cap `GHOST_ARMY_MAX` (60) per tribe, counting all of a tribe's ghosts together, not per cast. Once at the cap, a
  new cast simply produces no new ghosts (confirmed) — it does not evict or replace older ones, and nothing says
  the charge itself is refunded or withheld either way.
- No expiry (traced: "No expiry found") — `status::Statuses.ghost` is already a plain, untimed `bool` for exactly
  this reason, not a `Timed`. Ghosts persist until destroyed: Blast removes any ghost in its centre cell outright
  (already traced, matches `effect::hurts`'s existing "not pushed: own shaman, angel of death... ghosts in the
  centre cell are removed"), and Swarm's own cells destroy ghosts outright too (see above) — neither is normal
  damage, both are an instant removal.
- Combat targeting is the real new mechanic: a ghost unit's auto-target selection must be restricted to enemy
  ghosts and enemy shamans only (traced) — not the normal "nearest enemy" rule every other unit kind uses. Melee
  behaviour is untraced ("melee untested"); treat it as working normally once a ghost is in melee range of an
  allowed target, same restricted target list, until shown otherwise.
- Ranged attacks are cosmetic only (confirmed): a ghost firewarrior/spy still goes through its throwing
  animation, but no `WorldEvent::Damage` follows — matches the trace's "their shots do nothing" / "a ghost
  firewarrior's shots are ghosts too: they do nothing".
- Visual polish, deferred: the player's own ghosts render translucent, an enemy's ghosts look like ordinary units
  to that enemy (traced, a per-viewer rendering nuance) — client-side only, not blocking the core mechanic above.
- Training/mana interaction, deferred as an edge case: if a ghost ends up crewing a building, any unit that
  building then trains also becomes a ghost, and that training draws mana only from current income (never the
  bank) and never empties the store (traced). Only matters once a ghost can occupy a building at all — not needed
  for a first pass that just spawns ghosts and lets them fight in the open.

## Hardest tier

### Angel of Death
Not yet reached in the Q&A — session ended before this (Ghost Army, the other half of this original combined
entry, is now planned above in the Moderate tier). What's already in `pop3-rev-analysis.md` "Per spell" (13) plus
"Reference"'s combat tables (sight 7×7 cells, never pushed, never targeted by melee/preacher scans, no healing,
state 31 burn reaction at 0x4DE570) is the starting point for the next round of questions — nothing from the user
recorded yet beyond the initial complexity classification (this, plus Volcano and Armageddon, were flagged as the
hardest to implement).

### Armageddon
Reached in the session that also built the generic win/lose Outcome model (`game_core::outcome`) — not traced from
`pop3-rev-analysis.md` beyond its one "Per spell" (18) line (clears the map, turns every building into a person,
gathers every tribe into an arena, the weakest shaman struck by blast/lightning at intervals, anyone past 9 cells
away dies); the design below replaces that exact mechanic with the user's own, simpler one. `crates/game-core/src/
armageddon.rs` currently implements only a v1 placeholder (pauses the economy and combat-seeking, cancels any fight
in progress, settles the match immediately by comparing each side's combined headcount) — see `TODO.md` for what's
left to reach the design below.

Sequence, on cast (aimed, like Blast/Lightning — "gathers every tribe onto an arena around the target"):
1. Each tribe's headcount, by kind, is tallied before anything changes.
2. Every building, totem, and vehicle is removed globally (the "wipe"); trees are cleared only within the arena's
   radius around the cast point.
3. That many fresh fighters of each kind are spawned per tribe directly in the arena, grouped by team (allied
   tribes' wedges adjacent) — not a teleport of each existing unit (a pre-wipe headcount tally already accounts for
   every location a unit could be in: housed, aboard a vehicle, mid-fight). Each tribe's existing shaman is
   relocated into the arena (not respawned — she's a persistent, unique entity), a little behind her own fighters.
   In-progress training is cancelled, not fast-forwarded into a counted fighter.
4. The camera takes over automatically and orbits the arena continuously for the whole sequence; the player cannot
   select units, give orders, or cast anything until the match resolves (already enforced at the simulation layer
   regardless of this step's status, since v1 already locks all commands while a match exists).
5. A short "Gathering" phase holds combat off (fighters stand `Idle`) while the camera's reveal plays out. Once it
   ends, every tribe whose own fighters are still alive gets its shaman a permanent shield (`status::Timed`,
   continuously refreshed every turn — "permanent" by being topped up, not a new status kind) and fighting begins.
6. The instant a tribe's own fighters (not its allies') are all dead, its shield is explicitly dropped that same
   turn (a fresh `WorldEvent::Shield { steps: 0 }` — not refreshing it for one turn would still leave residual
   steps), its reincarnation site is removed (`WorldEvent::RemoveSite`), and its shaman takes lethal
   `DamageKind::Lightning` damage (the existing `WorldEvent::Damage`, not a new kill path — a direct, single-unit
   event bypasses `lightning::effect`'s own "caster's own shaman spared" rule, which lives in its target-selection
   function, not in `effect::hurts`). She dies and does **not** reincarnate, relying on a related fix:
   `Unit::tick`'s `Action::Dead` branch currently gets a unit **stuck forever** (never reincarnates, never dies)
   when there's no `ReincarnationSite` for its owner, instead of falling through to `Action::Spirit` like any other
   death — a real, independent bug, not Armageddon-specific, that this mechanic depends on being fixed.
7. This repeats until only one side (alliance group) still has a tribe with any survivors — allies never fight each
   other and win together. A stalemate timeout forces a draw if it runs too long.

`armageddon::ArmageddonMatch`'s `Coordinator` shape doesn't need to change to grow into this: it already resolves
via one `WorldEvent::Declare`, the same event this full version will eventually emit too, just after many turns of
`Phase::Gathering`/`Phase::Fighting` instead of one placeholder tick.
