# Winning and losing

`GameMap::outcome: Option<game_core::outcome::Outcome>` is the match's one and only result: `winner: Option<u8>`
(`None`: a draw), `reason` (`Eliminated`, `Armageddon`, or `Scripted` — reserved for a future level-script
interpreter, [TODO.md](../../TODO.md) "Level scripting"), and the tick it was declared. `GameMap::declare_outcome`
is the single write path, idempotent: the first call wins, every later one is a no-op, so nothing can un-declare or
relabel a result once set. The instant it succeeds, every unit still acting stands down to `Idle` (a death already
in train — `Dying`, `Dead`, `Spirit` — finishes on its own instead of being cut short). From the next tick on,
`GameMap::tick` freezes the same systems an Armageddon match suspends (below) — training, building work, wrecking,
spotting/attacking, mana/spellbooks — plus a `Dead` shaman's reincarnation, so nothing trains, builds, demolishes,
fights or comes back from the dead once the result is in; `GameMap::apply` rejects every `Command` on top of that,
same as while an Armageddon match is active. What keeps running past that point is purely cosmetic, and only for the
winning side — the winner and its allies, `GameMap::on_winning_side` (`Tribes::same_team`); on a draw (`winner:
None`) nobody gets it: every living unit of theirs permanently shielded (`shield_everyone_once_ended`) and,
client-side, a small happy jump and celebrating pose on every one of theirs (`units/mod.rs::victory_bounce`,
`bounce_start`, our own invented stand-in, not traced from the original) — the "victory lap" epilogue. The losing
side's survivors just stand there, frozen like everyone else, not celebrating.

Client-side, the win/lose dialog (`game_end.rs`, `AppState::Ended`) shows once a short hold lets the final blow's
animation finish; unlike `AppState::Paused`, `Ended` does **not** freeze the simulation or rendering (`menu::Gameplay`
runs in both `Playing` and `Ended`) — only manual input (camera, selection, orders, spell-casting) is locked
(`armageddon::input_locked`), so the epilogue above is actually visible behind the dialog.

## Automatic elimination

Checked once at the very end of every `GameMap::tick` (`check_elimination`, after every death, chained order and
mana effect that turn has already applied): a tribe counts as wiped out the instant it has no living unit of any
kind, right now (`GameMap::tribe_wiped_out`, an instantaneous snapshot by current owner — not an
original-tribe/permanent-death mechanic). A tribe reduced to only a `Dead`-and-respawning shaman is wiped out
immediately: there is no last-stand reincarnation comeback — and, per the freeze above, she now never actually comes
back either, so the sole unit a spectator sees stays down.

Elimination is resolved per **side** (alliance group, [`tribes.rs`](#tribes-and-alliances)), not per raw tribe:
allied tribes with no enemies left win together. The check is skipped entirely on single-tribe maps (every
sandbox/editor map) and once a result is already declared.

## Tribes and alliances

`GameMap::tribes: game_core::tribes::Tribes` is the tribe roster and starting alliances, read once from the level
or generator and never re-derived from mutable state — unlike `GameMap::sites.len()`, it stays correct even for a
tribe with no reincarnation site at all (a real mode: it just can't reincarnate if its shaman dies, not a
hypothetical). From an original level, the roster comes from the distinct owners among its shaman things (the same
set `site::sites_from_level` builds sites from, but independent of whether a site also exists for each); allies
default to "every tribe allied with itself only" and are overwritten by the level header's real bitmask
(`LevelHeader::allies`, [level-format.md](level-format.md) "DefaultAllies") when one exists. Generated maps and
sandboxes derive the roster from their `sites` list instead (1:1 there, no header to read allies from).

`Tribes::teams()` groups tribes by alliance transitively (two tribes allied only via a shared third still count as
one side); `alive_sides` gives one representative tribe per side that still has at least one living tribe.

Two places also respect alliances now, so gathered or adjacent allies never attack each other:
`attack::attack_goal` and `spotting::first_seen`. This doesn't yet cover every alliance-aware nuance the original
has (Blast sparing allies except on the last turn, spotting during some orders, preacher conversion targeting —
[TODO.md](../../TODO.md) "Default allies").

## Armageddon

Stops the match and settles it, aimed like Blast or Lightning. The trace has one line for it (pop3-rev-analysis.md
"Per spell" 18: clears the map, turns every building into one person, gathers every tribe on an arena around the
target, makes them fight, anyone more than 9 cells away dies); the rest is the user's own design, in
`crates/game-core/src/armageddon.rs`. Every number below is `[ours]` unless it is the trace's.

1. **On cast** (`Spell::Armageddon { at }`, `GameMap::start_armageddon`), at the height point nearest `at`
   (`arena_centre`):
   - every shaman dying, dead or gone whose tribe still has a reincarnation site is brought back, whole and at
     once (`revive_shamans`), then every site is removed (`remove_reincarnation`): nobody comes back from here on,
     and their stones are no longer drawn;
   - every building becomes one brave of its owner at its door (the trace's "turns every building into one
     person"); then every building, totem and vehicle goes, their people set down where they were (at the door,
     or where the vehicle stood); wildmen, angels and Ghost Army copies go; every fight and formation ends and
     everyone stands idle (`wipe_for_armageddon`); the trees on the arena and its rim go;
   - the arena is carved into the ground (`arena_heights`), square: a flat floor `FLOOR_HALF` (5) cells out each
     way at the centre's height (at least `FLOOR_MIN`, above the sea); on each of its 4 sides a slope
     `SLOPE_CELLS` (4) deep rising `SLOPE_STEP` (80) a cell, walkable; behind them a rim `RIM_CELLS` (2) wide,
     `RIM_HEIGHT` (400) higher, a cliff nobody walks up. Floor and slopes together are the trace's 9 cells;
   - every tribe with a living person on the map takes part, in the level's roster or not (`armageddon_sides`,
     allies next to each other). Each gets a slope (two tribes face each other; allies on neighbouring slopes):
     its people in rows from the slope's foot up (`arena_offset`), its shaman at the top corner at the end of its
     slope (`shaman_offset`).
2. **Arriving** (`Phase::Arriving`): the match teleports them in one per turn (`WorldEvent::Arrive`,
   `Unit::arrive_at`: they land like a teleported shaman, dropping in with a puff of dust). The shamans come first,
   then one person of each tribe in turn (`arrival_order`). Anyone standing in the sea at the cast (set down off a
   boat) is teleported at once instead, before drowning.
3. **Gathering** (`Phase::Gathering`): `GATHERING` (3 s) once the last has landed, everyone standing.
   Until the fight, nothing starts one: no challenge, melee or sermon (`GameMap::detect_coordinators`).
4. **Fighting** (`Phase::Fighting`): every follower first runs down into the middle (`WorldEvent::Rally`, a move to
   a quarter of the way out from the centre it was, keeping its place in the ranks); then combat-seeking runs
   again, and every turn each idle follower, or one going for an enemy already in a fight while a free one is left,
   is sent at the nearest free enemy follower it may attack (else the nearest; `WorldEvent::StrikeBack`). No shaman
   is auto-attacked, and no shaman auto-attacks (`spotting::first_seen`): only the fighters settle it. Anyone on
   the ground past `ARENA_HALF_CELLS` (9) from the centre, only ever someone thrown onto the rim or out, is struck by
   a bolt (the trace's "anyone more than 9 cells away dies").
5. Every turn of the fight, a tribe still in it whose followers are all dead is out: its shaman's shield drops
   (`WorldEvent::Shield { steps: 0 }`) and a bolt strikes where she stands, the Lightning spell's own effect
   (`lightning::effect`, drawn like any cast) cast by nobody, so it spares no shaman; with no site left, she does not
   come back. The tribes still standing keep their shaman shielded (`SHIELD_STEPS`, topped up every turn).
6. Once the standing tribes are all one side (`Tribes::same_team`), it wins (`WorldEvent::Declare`). If every side
   falls the same turn it is a draw and nobody is struck; still undecided `STALEMATE` (180 s) into the fight, a draw.

While a match is active (`GameMap::armageddon()`), the economy and the AI are suspended, `GameMap::apply` rejects
every `Command`, and the client locks every manual input (`armageddon::input_locked`). The camera flies to the
arena, framing it whole (`ORBIT_DISTANCE`, `ORBIT_PITCH`), and turns around it for the whole match (`orbit_yaw`,
`ORBIT_SPEED`: a full turn in about 40 s, on UI time).

Not done: the trace's "weakest shaman struck by blast or lightning at intervals" (the design strikes a shaman when
her fighters are gone instead), and the client's flash, arena tint and survivor bars (TODO.md "Spells").
