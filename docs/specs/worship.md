# Praying at vaults and totems (design; the vault's state and look are done)

Pyramids of knowledge (vaults, building 18) and totems (scenery 9) give their tribe a reward once prayed at long
enough. What each one gives, and how long, comes from the level's trigger on its cell (level-format.md "Vault of
knowledge and its reward", "What triggers stand on").

Tags: **[files]** decoded from the levels, **[player]** how the game plays, as told by someone who played it,
**[ours]** a choice made here, to check in the game.

## Shared: the prayer gauge
- One gauge per place and per tribe; it fills while that tribe prays there. Full at the trigger's `PrayTime`
  [files]; then the reward is granted. Its unit is unknown (game turns?); until measured, one `PrayTime` = one of our
  ticks [ours].
- Praying is slow [player].
- The gauge fills at full speed with `TriggerCount` people praying [player]. Fewer pray slower: speed in
  proportion, `min(praying, TriggerCount) / TriggerCount` [ours, to check].
- Nobody praying: the gauge drains, quite quickly (TODO.md) [player, speed to measure].

## Sandbox
Sandbox > Worship (`POP3_START=sandbox-worship`, `GameMap::sandbox_worship`): south of the player's site a
neutral pyramid of knowledge teaching the temple, its door facing the shaman; north a row of totems, one of each
look (`TotemKind::ALL`: totem, winged death totem with its bird, prayer totem, stone head, the three totem poles,
since which one the levels' scenery 9 uses is not known), and 8 of the player's braves in front of them.
Totems (`game_core::totem`, client `totems.rs`) only stand there for now: their original objects with animated
flames, or a stone pillar without the original files. Each asks for something else: one unit; the shaman with an
Angel of Death (the winged death totem); 6, 8, 2 units; the shaman; 4 units (`map::SANDBOX_TOTEMS`).

## Totems (scenery 9)
- Loaded (done: `totem::totems_from_level`): every scenery 9 of a level becomes a `Totem` with `prayers`
  (`TriggerCount`), `shaman_only` (types 3 and 5), `summons_angel` (type 5) and `pray_time` from the prayer trigger on
  its cell (`totem::triggers_on_cell`, shared with the vaults); drawn as the stone head (its real model is not
  known). Not prayed at yet.
- Who may pray and how many are needed come from the level, per totem, from the trigger on its cell [player,
  files]: type 0 takes any of the tribe's units, followers and the shaman alike, each counting towards
  `TriggerCount`; type 3 takes only the shaman [player]. Type 5 takes only the shaman too, and when it fires an
  Angel of Death appears [player] (one in level 5, cell 83, 65; its targets are effects 91 and 88 [files], one of
  them probably the Angel of Death).
- In the levels [files]: 96 of the 98 totems have exactly one trigger on their cell; level 23 has two totems and
  two triggers on one cell (30, 84), paired in slot order [ours]. Type 0 needs 1-8 units (6 for 41 totems, 1 for
  17); the 21 type 3 totems need 1.
- More may be assigned than `TriggerCount`: e.g. 12 on a totem of 8. Only `TriggerCount` count; the gauge goes
  no faster with more [player].
- Every assigned unit takes the prayer pose, counted or not; none of them is idle [player].
- A unit given another order stops praying and leaves; an assigned unit not counted yet takes its place, so the
  count stays at `TriggerCount` while enough are assigned (12 assigned, one moved away: 11 praying, still 8
  counted) [player].
- Its reward is not drawn: the player does not know what it gives before it is granted [player]. Rewards: spells
  for one use, mana, effects, a vehicle, revealed scenery and triggers (level-format.md) [files].
- `NumOccurences` (0-4) is probably how many times it can be prayed at; 0 unlimited? [files, meaning unverified]

## Pyramids of knowledge (vaults)
- Holds one unit, and only the shaman: she is ordered in like followers into a hut (`game_core::enter`) [player].
- Once placed, the door is closed: object 192 [player, objects.md "Pyramid of knowledge frames"].
- She prays (sits) at the door until its gauge is full [player]. Until the reward, the gauge behaves exactly like a
  totem's: it builds up while she prays, stops when she is attacked or leaves, and goes down while nobody prays
  [player].
- The door then opens, sliding up (points 192 -> 191), for a short time; she walks in; reaching the middle grants
  the reward (permanent spell or building) [player]. She walks out, the door closes.
- The top (petals) stays open the whole time, through the door opening for her [player].
- Once she has the reward and is out, the door closes and the top folds at the same time, one animation
  (191 -> 193: door down and petals folded together) [player]. The vault stays like that: spent, nobody can go in
  any more (its capacity becomes 0) [player].
- So: fresh 192 (door closed, top open); door opening 192 -> 191; spent 193 (door closed, top folded).
- Before the reward, the door follows the gauge [player]: if she dies or leaves while the door is open (gauge
  full, not yet at the middle), the gauge goes down and the door slides back down with it. The door's opening is
  a function of the gauge, not a timer of its own: closed below the top band of the gauge, opening through it,
  fully open when full [ours: the band's size].
- The reward is granted the moment she reaches the middle, and kept whatever happens to her next (attacked,
  killed) [player].
- From the reward on, the vault's sequence (her walk out, the door closing with the top folding, spent) plays
  the same whether she is alive or not [player]: it runs on the vault's own clock from the reward, not on her
  progress [ours]. If she is killed inside, the door still closes on time [ours, to check].
- State (done: `game_core::vault`): `Building::vault` holds the reward, the gauge length (`pray_time`) and a
  phase that only goes forward: `Praying { progress }` -> `Granted { ticks }` -> `Spent`. The reward is given once,
  on entering Granted (`Vault::grant`); Granted counts ticks on its own (`OPEN_TICKS` 20 open, then `CLOSE_TICKS`
  10 closing) whatever happens to the shaman, then Spent. The progress is not reused to close the door [ours].
  `Vault::door_and_top` gives the door and top positions in thousandths: Praying, the door opens over the last
  tenth of the gauge and the top is open; Granted, open, then both close together; Spent, both closed.
- Drawn (done: client `vault.rs`): the original pyramid's frames blended point by point, the door's points from
  192 towards 191, the top's from 192 towards 193 (points moving over 10 units between frames; the others are
  export noise). The generated pyramid slides a stone slab up its doorway (its top does not fold yet). Its phase
  does not rebuild the building views (`buildings::same_look`). Dev: `VAULT=progress:N|granted:T|spent`.
- What it gives is drawn: an icon at its top, from the cursor icons (sprites.md: 38-57 spells, 58-65 buildings)
  [player]. The icon goes away once the reward is granted [player].

## Open
- `PrayTime`'s unit, the drain speed, and the speed with fewer than `TriggerCount` praying.
