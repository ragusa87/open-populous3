# Praying at vaults and totems (design, not implemented)

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

## Totems (scenery 9)
- Followers are assigned to pray at it (type 0 triggers), or only the shaman (type 3) [files].
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
- She prays (sits) at the door until its gauge is full [player].
- The door then opens, sliding up (points 192 -> 191), for a short time; she walks in; reaching the middle grants
  the reward (permanent spell or building) [player]. She walks out, the door closes.
- The top (petals) stays open the whole time, through the door opening for her [player].
- Once she has the reward and is out, the door closes and the top folds at the same time, one animation
  (191 -> 193: door down and petals folded together) [player]. The vault stays like that: spent, nobody can go in
  any more (its capacity becomes 0) [player].
- So: fresh 192 (door closed, top open); door opening 192 -> 191; spent 193 (door closed, top folded).
- What it gives is drawn: an icon at its top, from the cursor icons (sprites.md: 38-57 spells, 58-65 buildings)
  [player].

## Open
- Whether a vault's gauge drains like a totem's when the shaman stops praying, or starts over.
- `PrayTime`'s unit, the drain speed, and the speed with fewer than `TriggerCount` praying.
