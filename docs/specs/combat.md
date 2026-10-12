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
  is a cell off; a target gone, dead or no longer an enemy stops the attack (Idle). What happens once there comes
  with the challenge and the melee.
