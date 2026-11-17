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
target"): the user's own design over the trace's one line, in `crates/game-core/src/armageddon.rs`. The numbers are `[ours]`.

1. Casting it (`Spell::Armageddon { at }`, `GameMap::start_armageddon`) first brings back, whole and at once, every
   shaman dying, dead or gone whose tribe still has a reincarnation site (`revive_shamans`), then removes every site
   (`remove_reincarnation`: nobody comes back from here on, and their stones are no longer drawn). It counts each
   tribe with people on the map (in the level's roster or not, `armageddon_sides`) has its followers counted by
   kind. Then every building, totem and vehicle goes, every unit but the shamans (wildmen and angels too), any fight
   or formation in progress, and the trees around `at`. The arena is carved into the ground there (`arena_heights`):
   a flat floor `ARENA_RADIUS_CELLS` (9) wide at the centre's height (at least `FLOOR_MIN`, above the sea), walled
   by a `RIM_CELLS` (2) wide rim `RIM_HEIGHT` (400) higher that nobody walks up. As many fresh followers of each kind
   are spawned per tribe on the floor, one wedge each, allies' wedges side by side (`arena_offset`, rows of 8 or more
   from 2 cells out, so they stay inside); its shaman stands a row behind them (`shaman_offset`).
2. While a match is active, `GameMap::armageddon()` returns `Some(at)` (the client camera's target), the economy and
   the AI are suspended, and `GameMap::apply` rejects every `Command`.
3. `ArmageddonMatch` (a `Coordinator` no unit links to) holds combat off for `GATHERING` (5 s). Then it fights:
   combat-seeking runs again, and every turn each idle follower is sent at the nearest enemy follower it may attack
   (`WorldEvent::StrikeBack`), since sight alone would leave the wedges apart. No shaman is auto-attacked, and no
   shaman auto-attacks (`spotting::first_seen`): only the fighters settle it.
4. Every turn of the fight, a tribe still in it whose followers are all dead is out: its shaman's shield drops
   (`WorldEvent::Shield { steps: 0 }`) and lightning strikes her (`DamageKind::Lightning`); with no site left, she
   does not come back. The tribes still standing keep their shaman shielded (`SHIELD_STEPS`, topped up
   every turn).
5. Once the standing tribes are all one side (`Tribes::same_team`), it wins (`WorldEvent::Declare`). If every side
   falls the same turn it is a draw and nobody is struck; still undecided `STALEMATE` (180 s) into the fight, a draw.

Not done: the trace's "anyone past 9 cells away dies" (the rim keeps everyone in instead), and the client side beyond the camera (TODO.md "Spells").
