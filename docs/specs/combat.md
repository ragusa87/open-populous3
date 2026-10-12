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
