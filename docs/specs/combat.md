# Combat

Our rules for health, fights, attacks on buildings and firewarriors' fire. The facts come from
pop3-rev-analysis.md (sections named below); the names here are ours. Tags: **[exe]** traced in the executable,
**[measured]** read live from the running game, **[ours]** our choice, **guess**.

## Health
- Per kind (`UnitKind::stats`, `kind_stats.rs`): full life (constant.dat values) and the life regained per heal
  ("Health and healing" [exe, measured]):

| Kind | Life | Heal |
|---|---|---|
| shaman | 2000 | 32 |
| brave | 1000 | 4 |
| warrior | 1800 | 6 |
| preacher | 1100 | 4 |
| spy | 600 | 4 |
| firewarrior | 700 | 6 |
| wildman | 1000 (guess) | never |

- Everyone heals on every eighth turn of the game (`unit::HEAL_EVERY`, phase 0: the same turn for all) [exe],
  whatever it does, fighting included; not in the air, drowning, stranded or dead.
- Never past full [ours]: the original adds the heal while below full, so life can end up to one heal over.
- In the air, a deadly hit does not kill at once: the unit flies on and dies on landing [exe].
- Not yet: no healing while struck by lightning (we have no such state yet), the angel of death never heals.

## Fight numbers
- Per kind (`KindStats`): a blow's strength at full life (constant.dat values: 60, the warrior 360), a rank (brave,
  preacher, spy 1; warrior, shaman 2; firewarrior 3: the lower gives way in a full melee), a pace (turns per
  animation frame: the firewarrior 2, so its moves last twice as long) [exe, measured]. Wildmen never fight;
  firewarriors never join a melee already going on, but fight when challenged [exe].
- A blow hits `max(32, strength × life / max_life)`, × 3 when the attacker is bloodlusted (`combat::strength`) [exe].
  Not yet: the original's damage taken by a bloodlusted person (>> 3) is not placed in the hit; per-person
  maximum life ("Measured": not always the kind's).
- Move lengths in turns (`combat::move_turns`), × pace [exe, measured]:

| Move | Followers | Shaman |
|---|---|---|
| shove (the challenge's opening) | 6 | 4 |
| knocking blow | 6 | 4 |
| trading blow | 7 | 4 |
| clean blow | 7 | 5 |
| reeling | 7 (4 when knocked back) | 7 |

- `DamageKind::Melee`: hurts anyone alive; the shield blocks spells, not blows [ours, from the game].

## Attacking
- `Order::Attack { target }` (`Target::Unit` or `Target::Building`), from a left click on an enemy person or
  building with own people selected (`units::selection::attack_click`). Only those that may attack it go
  (`attack::may_attack`): both fight (never a wildman), other tribes, the target alive. A building only if it
  belongs to another tribe and wears down (`may_attack_building`): never a neutral one (a vault of knowledge, a
  totem), which is prayed at; a click on it gives the prayer orders as before. Firewarriors take no attack
  order yet: theirs throws fire from afar (TODO).
- `Action::Attacking { target, to }`: walks a path to `to`, the person or the building's door, and stands there.
  Each turn before the coordinators `GameMap::tend_attacks` aims `to` at where the target is, planning again once it
  is a cell off; a target gone, dead or no longer an enemy stops the attack (Idle). Once within reach of a person, a
  challenge starts (below).

## Challenge
A coordinator (`challenge::Challenge`, coordinators.md) for one attacker and one target, before a melee
("Pre-fight" [exe, measured]):
- Started in `detect_coordinators` (`GameMap::start_challenges`): an attacker within `FIGHT_REACH` (224) of a
  target that is in no coordinator, outside and on the ground. Both join it (`Unit::join`): the attacker
  `Role::Charging` at its walking speed + a draw below a quarter of it, the target `Role::Waiting`. A target already
  in a challenge or a melee is left alone for now (joining a melee comes with it).
- Each turn: it ends, both released, when one is gone, dead, no longer linked (an order took it away) or of the same
  tribe, or when a charge lasts `CHARGE_TICKS` (40). The charger runs straight at the target (`Placed` gives the
  target's place when it moved). Within `SHOVE_REACH` (315) on both axes of a waiting target: `Shove`, 6 turns
  (shaman 4, × pace); with 2 left the target is thrown (`Push`, `PushKind::Melee`: the shield does not stop it, the
  target stays `Shoved` in the challenge) at 100 + a draw below 100 per turn, turned a third to two thirds of a turn
  from the way to the attacker (away from it), lift `SHOVE_LIFT` 48 [guess: ours does not slide on the ground]; no
  damage. Still again, it waits. The attacker charges again; reaching it again ends the challenge with `StartMelee`: a melee
  at the target's place.
- Already in reach when it starts, the attacker shoves at once.
- Not yet: the target facing the attacker and its taunts; the original's option with no shove; a target at its
  building (the third approach, unverified).


## Melee
A coordinator (`melee::Melee`) holding up to `MAX_FIGHTERS` (6) people of two tribes: one alone on its side in the
middle, the others in a ring around it ("Fight" [exe, measured]). Fighters are `Action::Melee { melee, place, stance }`.
- Started by a challenge (`GameMap::start_melee`): the middle where the target stands, the ring's turn drawn
  [guess: the original's first angle is not traced].
- Each turn, in this order:
  1. fighters gone, dead, unlinked or farther than `MELEE_LEASH` (2048) from the middle leave (`Release`);
  2. fewer than 2 left or one tribe left: it ends, everyone released (the winner's count: TODO);
  3. the middle is the fighter alone on its side (the last one first) and follows it;
  4. on `RING_TURN` (every 32 turns, at its own phase) the ring turns half a turn, then by 1 chance in 2 another
     20° to 80° either way (`RING_EXTRA`);
  5. places (`ring_place`): the middle, the others at `RING_RADIUS` (180) around it at the ring's turn, + 90° with
     3 people, + 120° and 240° with 4, the 5th and 6th in the middle; `Placed` only when a place changed;
  6. each fighter on guard draws 0..16 and picks its opponent (the middle one any of the ring, the ring the
     middle one). Against an opponent on guard (`blow_on_guard`): the shaman 7/16 knocking, 9/16 clean; others
     5/16 knocking, 9/16 clean, 2/16 trading. Against a busy one near its place (`BUSY_REACH` 360), 4/16 of the
     time (`blow_on_busy`): clean, or with more than 2 people clean or trading by the draw's parity.
     A blow (`Struck`): the target loses the attacker's strength, and except a clean blow the attacker the target's
     (both at once, `DamageKind::Melee`); lives already lost this turn count. Both are then busy for the turn.
- A fighter's own cycle (`Stance`): `ToPlace` (walks to its place) → `OnGuard` within `PLACE_REACH` (12) →
  `Striking` (the blow's length) or `Reeling` (7 turns, 4 when knocked) → to its place again. A knocking blow
  against an on-guard follower (never the shaman) throws it back after it reeled: 35 + a draw below 70 per turn,
  away from the attacker, along the ground (`Staggering` until still).
- Fighters heal as anyone (see "Health").
- Joining (`GameMap::join_melees`, in `detect_coordinators` after the challenges): an attacker within
  `FIGHT_REACH` of a target in a melee joins it if it is of one of its two tribes and not a firewarrior, while there
  are fewer than 6; with 3 or more of its tribe there it joins only by replacing the one of lowest rank below its
  own (released). Then, both sides with 2 or more, the last of each leaves for a new one-to-one melee at the same
  place: a melee stays one against several ("Fight" [exe, measured]).
- Not yet: the ground check moving the middle off unsuitable ground, the winner's
  count, the move's sounds.

## Attacking a building
`crate::wreck` ("Attacking a building" [exe]); the attacker's own work, no coordinator except the pull-out.
- `Order::Attack` on an enemy building walks the attacker to its door. Arrived (`ARRIVED` 32 per axis): it hacks
  (`Action::Hacking`), `HACK_AT_DOOR` (16) turns at the door, the building shaking. Then, if people are inside, the
  first is pulled out: thrown from the door outwards at `PULL_SPEED` (100) with a small lift `PULL_LIFT` [guess: ours
  does not slide], and a challenge (`Approach::PulledOut`) starts with it already `Shoved`; the attack on the
  building is chained after it. Empty: it hacks for 8 + a draw below 16 turns, adding its kind's `sabotage` (brave,
  preacher, spy 7; warrior 12; shaman 10; firewarrior 16) to the building's `wear` each turn, then walks to a spot
  `WANDER` (360) from the building's middle (outside the walls, else the door) and hacks again.
- The building (`Building::tend_wear`, every turn, `GameMap::tend_building_wear` after the fires): it shakes
  `SHAKE_TICKS` (6) after each blow; its wear fades by one every 4 turns; at its limit (`wear_limit`: 2000; walls
  and gates 16 000; the prison 22 000; a vault never) the wear drops by the limit and it loses a piece of wood
  (`lose_a_piece`): damage smoke, a large building's people thrown out [which kinds are large: guess], no repairs
  for `REPAIR_DELAY` (1200 turns); with no wood left it is destroyed (its people thrown out) and leaves ashes.
- Not yet: enemies standing on a plan counting as its people, blasts' and firewarriors' wear, the flying faces of
  the partial explosion, the player warned once, the attack going on from where it stands when no way leads to its
  spot.

## Ashes
`crate::ashes` ("Building ashes" [exe]): a building destroyed by losing its last piece of wood (worn down by
attackers, burnt down) is removed and leaves its footprint cells (`walled_cells`) in ashes (`GameMap::burn_down`);
one removed any other way (a plan cancelled) leaves none.
- `GameMap::ashes`: up to `ASHES_SLOTS` (100) patches, oldest first; a new one when full clears the oldest.
- On `ASHES_EVERY` (every 16 turns, phase 0) every patch ages by 16; past `ASHES_FULL_TICKS` (6000) its darkness
  (127 when fresh) drops by 2 each time; at 92 or below the patch is gone: 6000 turns dark, then 18 steps (288 turns).
- Nothing is built on a cell with ashes (`placement::Blocked::Ashes`).
- Drawn: the terrain texture chars those cells (`terrain_texture::char_ashes`, our colour), less as they fade; the
  map's dirty rects redraw them when a patch comes, fades a step or goes.
- Not yet: the hover text over ashes; the long damage smoke at the ashes; the whirlwind's destruction; lava and
  earthquake ground, which share the mark in the original.

## Spotting
`crate::spotting` ("Auto-attack" [exe]): people who see an enemy attack it on their own, before the attacks are
followed each turn (`GameMap::spot_enemies`).
- Who: braves and warriors (`looks_for_melee`); the preacher's conversion, the firewarrior's fire and the shaman's
  look during some orders are TODO; spies and wildmen never look. On its kind's `look_every` (4 turns; preacher 2),
  phase 0.
- How far (`sight_now`, cells; per kind `sight` and `sight_at_work`: brave and warrior 5 and 1, preacher and shaman
  3 and 1, firewarrior 11 and 5, spy and wildman 0): idle, holding wood or hacking at a building, its full sight;
  walking (every walk is on an order, the original's "going to the place") or attacking on its way, nothing;
  working (cutting, picking up, flattening, building, hammering, round a fire, praying), `sight_at_work`, its own
  cell; busy (fighting, panicking, thrown, casting, drowning, entering, inside, dead), no look at all [the order
  kinds' split is the analysis' guess].
- What: the square of cells (sight rounded down to even, + 1) around it, row by row from a corner (`square`); the
  first match wins, not the nearest: an enemy person it may attack, alive, outside, on the ground and not in a melee;
  when idle, also someone in a melee (to join it) or an enemy building it can wear down. It then attacks it
  (`Order::Attack`'s path) [ours: the original gives itself an "attack around this cell" order].
- `UnitGrid`: the living people outside by cell, sorted, built only on a look turn.
