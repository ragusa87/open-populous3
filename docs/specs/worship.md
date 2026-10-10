# Praying at vaults and totems (done; rewards beyond spells and buildings only logged)

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
- The gauge fills at full speed with `TriggerCount` people praying [player]. Fewer pray slower, on a soft curve:
  speed `(n / TriggerCount)²` with `n = min(praying, TriggerCount)` [ours, to check]. A totem of 8: 8 praying 100 %,
  6 about 56 %, 4 25 %, 2 about 6 %, 1 about 1.6 %. In integers: the gauge counts 1/256 steps and gains
  `256 * n² / TriggerCount²` a tick (a lone prayer on a totem of 16 still gains 1 step). Done:
  `game_core::gauge` (`gain`, `step`, `STEP`, `DRAIN`), used by the vaults; totems will use it too.
- Nobody praying: the gauge drains, quite quickly, **linearly**: a constant amount each tick, whatever the totem
  needs or how full it is [player]. The rate is to measure; until then as fast as it fills at full speed, 256
  steps (one `PrayTime` unit) a tick [ours].

## Sandbox
Sandbox > Worship (`POP3_START=sandbox-worship`, `GameMap::sandbox_worship`): south of the player's site a
neutral pyramid of knowledge teaching the temple, its door facing the shaman; north a row of totems, one of each
look (`TotemKind::ALL`: totem, winged death totem with its bird, prayer totem, stone head, the three totem poles,
since which one the levels' scenery 9 uses is not known), and 8 of the player's braves in front of them.
Totems (`game_core::totem`, client `totems.rs`): their original objects with animated flames, or stone blocks
without the original files. The stone totem gives once (`occurrences` 1), to show it turning, sinking and gone;
the others give again and again. Each asks for something else: one unit; the shaman with an
Angel of Death (the winged death totem); 6, 8, 2 units; the shaman; 4 units (`map::SANDBOX_TOTEMS`).

## Totems (scenery 9)
- Praying (done: `game_core::worship`): a click on a totem sends the selected units that may pray there
  (`GameMap::totem_orders`: the shaman only for a shaman-only totem, else the followers and the shaman; none
  at an exhausted one). Each walks to a free spot around it and prays (`Action::Worshipping` with the totem's
  centre, the prayer pose). `Totem::queue` keeps them in the order they started; each tribe's first
  `prayers` count. Each tick (`GameMap::tend_totems`) each tribe's gauge (`Totem::gauges`) fills on the soft
  curve with its counted prayers and drains linearly without any (`gauge::step`). Full, the tribe gets the
  totem's gifts (`Totem::gifts`, logged in `GameMap::granted`), its gauge starts again; once it gave
  `occurrences` times (0: no limit, a guess) its prayers are sent away (stop) and it takes no more.
- The stone totem (object 1, "Totem rocks" in objects.md) [player]: its rock layers are out of line on purpose
  (each turned about 20° from the one under it). Once completed, every slab's bottom ring turns about the
  centre axis (the way object 3 turns it) until it is turned **64°** from object 1 [player, picked with a slider;
  object 3 is at 19.6°, straight], its radius going to object 3's; then, once it has given every time it can (`occurrences` > 0 and all given), it
  sinks slowly under the ground, giving off smoke, and disappears: removed from the map once sunk, its cell
  free again [ours]. What it does after a completion that is not its last (or with no limit) is not known: it
  turns and stays so [ours, to check]. The generated stand-in (a stack of blocks) does the same.
  - The turn, chosen for now from side-by-side tests (local HTML previews of the original model, not kept):
    every slab's bottom ring turns together, smoothly (eased in and out), by 64° plus one full turn (424°) over
    2 s; only the bottom rings move, so the slabs twist through themselves while turning. Then a 1 s hold, the
    sinking over 4 s with smoke puffs rising round its base, gone [ours: the timings and the smoke].
  - Other ways tried or worth trying, to revisit against the game: the slabs turning one after another, bottom
    up or top down; the whole slabs spinning as blocks (1-3 turns) while their rings ease to 64°; more full turns
    of the rings (2 or 3, over a longer time); spinning fast and slowing down into place; overshooting 64° and
    springing back; a ratchet of short clicks (4 × 16°) with pauses; a rumble (the totem shaking) while it sinks.
  Done: the timeline is in the simulation (`Totem::since_given`; `TURN_TICKS` 2 s, `HOLD_TICKS` 1 s, `SINK_TICKS`
  4 s; `Totem::is_gone`, then nothing finds it: `GameMap::totem_at`); the client poses the rings from objects 1
  and 3 (`totems::twisted`, `turn_fraction`, eased and smoothed between ticks), lowers the body
  (`sink_fraction`) with puffs of smoke (`effects::SINKING`) and hides it once gone. The generated stack of blocks turns its
  blocks above the base by the same amount.
- While it turns and sinks (its slots all used: exhausted) no unit can be sent to it [player]: done, an
  exhausted totem takes no order (`GameMap::totem_orders` gives none, `Order::Worship` there does nothing) and
  sends away those still praying.
- The stone head (object 82) [player]: its head nods at all times, completed or not: it tips forward and back
  about a horizontal axis through its neck. Chosen from a side-by-side test of four nods ("A, gentle"): the head
  is every face above the pedestal (all its points at height 324 or more); it turns about the horizontal axis
  across the face's direction, through the top of the pedestal (height 324, centre axis), by `6° * sin(2π t / 1.6 s)`
  (±6°, one nod every 1.6 s, smooth both ways). Done: `totems::split_head`, `nod_angle`; the generated stand-in
  is a pedestal block with a head block nodding the same way.
- The other looks do not move (to check in the game).
- Gifts (`totem::gift`, from the trigger's targets): a spell (`Reward::Spell`, a "once" discovery
  `Reward::OneShot`: one more cast, `worship::one_more_shot`), a building, mana (`Reward::Mana`, only logged:
  no mana yet), anything else `Reward::Unhandled { kind, model }` (effects, revealed things, the Angel of
  Death), only logged. The Worship sandbox's totems give one cast of a spell, mana, or (the winged death
  totem) an unhandled effect (`map::sandbox_gift`). Dev: `BRAVES=totem:N`.
- Loaded (done: `totem::totems_from_level`): every scenery 9 of a level becomes a `Totem` with `prayers`
  (`TriggerCount`), `shaman_only` (types 3 and 5), `summons_angel` (type 5) and `pray_time` from the prayer trigger on
  its cell (`totem::triggers_on_cell`, shared with the vaults); drawn as the stone head (its real model is not
  known).
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
- Its reward is not drawn: the player does not know what it gives before it is granted [player].
- Tooltip (tooltips.md, done but for the bar and the filled places): its name, a people row of `TriggerCount`
  places, brave shapes or the shaman's for shaman-only totems, filled by the counted prayers (the others assigned
  do not show), and the prayer bar. Rewards: spells
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
- Praying (done: `game_core::worship`): a click on the vault with the shaman selected sends her
  (`Order::Worship`, `GameMap::worship_orders`; other units ignore it); she walks to its door and prays
  (`Action::Worshipping`, the prayer pose). Each tick (`GameMap::tend_vaults`) its gauge fills while she prays at the
  door or is inside (one prayer out of the one it needs: full speed), and drains linearly otherwise: another
  order, death. Full, she walks in to its middle
  (`Unit::enter`); there the reward is granted (`Vault::grant`): logged in `GameMap::granted` with her tribe,
  given to the level's books, and the client makes it available on its panels (`hud::build::apply_rewards`).
  Then she walks out by the door (well within `OPEN_TICKS`). From going in until she is out she is locked: no
  order of the player reaches her (units.md "Locked units"). A spent vault does not take her. Dev:
  `SHAMAN=worship` sends her to the map's first vault.
- Drawn (done: client `vault.rs`): the original pyramid's frames blended point by point, the door's points from
  192 towards 191, the top's from 192 towards 193 (points moving over 10 units between frames; the others are
  export noise). The generated pyramid slides a stone slab up its doorway (its top does not fold yet). Its phase
  does not rebuild the building views (`buildings::same_look`). Dev: `VAULT=progress:N|granted:T|spent`.
- Tooltip (tooltips.md): the name of its reward, a people row of one shaman slot, and the prayer bar of the
  player's tribe.
- What it gives is drawn: an icon at its top, from the cursor icons (sprites.md: 38-57 spells, 58-65 buildings)
  [player]. The icon goes away once the reward is granted [player].

## Open
- `PrayTime`'s unit, the drain rate (linear; as fast as a full fill [ours]), and whether the soft curve
  matches the game.
