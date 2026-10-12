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
  (`attack::may_attack`): both fight (never a wildman), other tribes, the target alive. Firewarriors take no attack
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
  damage. Still again, it waits. The attacker charges again; reaching it again ends the challenge (the melee comes
  next).
- Already in reach when it starts, the attacker shoves at once.
- Not yet: the target facing the attacker and its taunts; the original's option with no shove; a target at its
  building (the third approach, unverified).

