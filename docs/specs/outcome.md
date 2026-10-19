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

Stops the match and settles it, aimed like Blast or Lightning ("gathers every tribe onto an arena around the
target"). Today (`crates/game-core/src/armageddon.rs`) is a v1 placeholder, not the full mechanic designed in
[docs/plans/spells.md "Armageddon"](../plans/spells.md) (see that file and [TODO.md](../../TODO.md) "Spells" for what's
left):

1. Casting it (`Spell::Armageddon { at }`) cancels any fight in progress (every `Attacking`/`Melee`/`Challenge` unit
   returns to `Idle`) and starts `ArmageddonMatch` (a `Coordinator`, `GameMap::coordinators.armageddons`).
2. While a match is active, `GameMap::armageddon()` returns `Some(at)` (the "Armageddon flag", doubling as the
   client camera's target) — the economy (vaults, totems, queues, training, vehicle huts, mana) and combat-seeking
   (`spot_enemies`/`tend_attacks`) are suspended, and `GameMap::apply` rejects every `Command`: the player cannot
   select units, give orders, or cast anything until it resolves.
3. `ArmageddonMatch::tick` resolves on its very first tick: `armageddon::best_side` compares each side's combined
   headcount (`Tribes::teams`, `GameMap::headcount`) and declares the higher one the winner (an overall tie is a
   draw) via `WorldEvent::Declare` — the same hookable entry point a future level script will also use.
