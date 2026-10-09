# Original engine internals (`pop3-rev` analysis)

What the reverse engineering of the original executable says about game logic, spells, combat, mana, wood, huts
and prayer, laid out for implementation: the rules first, then what they change for us, then the addresses and
layouts as reference.

Sources:
- [exe]: disassembly of `D3DPOPTB.EXE` (French release, 1998, Direct3D build) with `objdump`.
- [ghidra]: the `pop3-rev` Ghidra project. Function names in `code style` are its names, names in quotes are our
  proposed renames.

## Takeaways for the implementation
- **12 turns per second** [exe, measured]: a fixed timestep of 83 ms. Every "turns" value below converts at that
  rate. Our simulation runs 10 ticks per second.
- **Lockstep is the original design** [ghidra]. Orders are queued tribe commands applied on the network turn, with
  one global pseudo-random generator. Our `Command` + `map::Lcg` model (multiplayer.md) matches it.
- **Everything is a unit** [ghidra]. People, buildings, scenery, shots, spells and invisible controllers share one
  record and one fixed pool, each class with its own per-turn handler. A spell is a unit that spawns effect units,
  and the effects do the work. Fights, formations and guard posts are controller units that people link to, not
  person states.
- **Mana is a per-tribe pipeline run every turn** [exe]: income every 16 turns, then spell charging first, the
  surplus banked, a delayed reservoir for gifts. See "Mana rules".
- **Income is busy/idle per person**, from constants that `constant.dat` cannot change. It is not the
  `MANA_F_<PERSON>` × activity % model of huts-and-training.md. See "Impact on our specs".
- **Chopping is a countdown, then one transfer** [exe, measured]: 20 turns next to the tree, then a whole load
  (100 wood, one of our pieces) moves at once. A wood pile takes 3 turns. See "Wood rules".
- **A hut has two independent counters** [exe]: breeding (a brave at the door, slower as the tribe grows) and
  growth (the next size, only with people inside, independent of population). See "Huts: breeding and growth".
- **Prayer gauges fill in `PrayTime / 3` seconds at full count** [exe]: one `PrayTime` unit is 4 turns, and each
  missing prayer slows the gauge sharply (`C² / (C − n + 1)²`). See "Prayer at triggers".
- **A spell is a recipe of reusable components** [exe]: the shaman fires a shot, and on arrival the spell spawns
  effects (a shockwave, fire, a status on the N nearest...) that do the work. See "Spells".
- **Constants**: `constant.dat` overrides the exe defaults, which differ a lot. Percent constants are 8.8 fixed
  point at run time (`value * 256 / 100`), which fits our integer-only simulation.

## Turns and timing [exe]
```text
step_ms = 1000 / turns_per_sec                    # 1000 / 12 = 83 (integer)
loop:
    if GetTickCount() > next_turn_time:
        repeat 1 + extra_turns times: run one game turn (turn += 1)
        next_turn_time += step_ms                 # at most 4 catch-up steps per frame
    render
```
- `turns_per_sec` is 12 in a normal game. It is copied from a saved option, and one code path forces 14 for a
  while (not traced).
- `extra_turns` is 0 in a normal game. It is clamped to 0..4, or 0..32 with a flag set: probably a fast-forward or
  cheat (unverified).
- Measured under Wine by reading the turn counter from `/proc/<pid>/mem` once a second: +12 per second in a level,
  `turns_per_sec` 12, `extra_turns` 0. Before the level ran normally (loading, intro), the counter moved 1-3 per
  second.
- A turn is therefore 1/12 s. 16 turns (one mana income) is 1.33 s, and 1000 turns is 83 s.

## Mana rules [exe]
A tribe holds:
- `bank`: the mana the player has, clamped to `0..MAX_MANA`;
- `incoming`: mana gained this turn, not yet distributed;
- `reservoir`, `delay`, `rate`: delayed mana (gifts, shaman kills);
- `charge[spell]`: per spell, the mana still needed for its next charge;
- `needed`: the sum of `charge` over the available spells;
- `last_income`.

### Every turn, per tribe
```text
if income turn: incoming += income(tribe)            # see Income
release_reservoir(tribe)                               # see Reservoir
distribute(tribe)                                      # see Distribution
```

### Income
- Only on turns where `turn & MANA_UPDATE_FREQ == 0` (the file's 15: every 16 turns), and never when level flag
  0x20 is set.
- `sum` over the tribe's people of:

| Person | Busy | Idle | Constants |
|---|---|---|---|
| brave | 15 | 4 | `MANA_BUSY_BRAVES`, `MANA_IDLE_BRAVES` |
| warrior, spy, firewarrior | 5 | 4 | `MANA_BUSY_SPECIALS`, `MANA_IDLE_SPECIALS` |
| preacher | as specialists, with one extra check (maybe "is preaching") | | same |
| shaman | 30 | 30 | `MANA_F_SHAMEN` (file) |
| wild, angel of death | 0 | 0 | |

  The busy/idle values are exe defaults; those constants are not in `constant.dat`. "Busy" is probably "inside a
  building" or "working" (unverified).
- `gain = sum * adjust >> 8`, `adjust` being `HUMAN_MANA_ADJUST` for a human tribe and `COMPUTER_MANA_ADJUST`
  otherwise, in 8.8 (file: 125 % and 50 %; exe: 200 % and 25 %).
- `incoming += gain`, `last_income = gain`.

Example: 20 busy braves, 4 idle warriors and the shaman make 300 + 16 + 30 = 346, ×1.25 for a human = 432 mana every
16 turns (1.33 s), about 324 mana per second.

### Reservoir
- `give_mana_delayed(tribe, amount)`: `reservoir += amount`, `delay = 1000` turns, 83 s (50 turns, 4 s, with a
  global flag set).
- While `delay` counts down, nothing is released. Then `reservoir` flows into `incoming` at `rate` per turn.
- `rate` is set when the release starts, from the amount left:

| Amount left | Rate per turn |
|---|---|
| below 36 000 | amount / 24, at least 1 |
| 36 000 .. 361 500 | 1500 |
| above 361 500 | amount / 240 |

### Distribution
1. `incoming` charges the available spells (bitmask in the level header, per tribe): each `charge[spell]` is
   decreased by what it receives, clamped at 0. When a charge is filled, the loop runs again. The mana is shared
   evenly between the learned, unblocked spells that are not full (0x41AD70).
2. The rest goes to `bank`, clamped to `0..MAX_MANA`.
3. `incoming = 0`; `needed` = sum of `charge` over the available spells.

So spells charge first and only the surplus is banked: the bar fills spell by spell.

### Charges
- On a reset (two callers, see Reference), each available spell's `charge` is cleared and its charge count
  set to `SP_1_OFF_MAX` (the second count with level flag 0x20).
- A tribe of controller type 1 gets `bank = MAX_MANA` instead (probably a "full mana" mode, unverified).

### Spending and other sources
- Casting: `incoming -= cost` through `add_mana`, so it reaches the bank at the next distribution, unless the
  tribe's "free spells" flag is set. Costs are `SPELL_<X>` (spells.md "Original balance").
- A tribe command adds a signed amount, and a PopScript opcode adds a script expression (ai-scripts.md).
- Buildings hold a stored value: one routine sums it over buildings of a flagged type, another adds it to a
  type 1 tribe. Maybe the mana stored by training huts (unverified).

### Shaman death
- `base = bank`; for a human tribe (controller type 2), `base = needed + stored building mana`.
- The tribe loses `base * SHAMEN_DEAD_MANA_%_LOST`.
- If `SHAMEN_DEAD_MANA_%_GAIN` is non-zero and the shaman's last attacker is another tribe, that tribe gets
  `base * %_GAIN` through `give_mana_delayed`, so after a 1000-turn delay.
- File: 25 % and 25 %. Exe: 25 % and 0 %.

### Start
- `bank = START_MANA` (file 30 000, exe 42 000).
- `CONVERT_MANA` (exe 6000) and `SOUL_GRAB` (exe 30 000) exist, but no reader is found yet.

## Wood rules [exe]
Wood is an amount, in units of 100 per load (one of our pieces). Four kinds of units hold it:

| Holder | Field | Cap |
|---|---|---|
| person | `carried` | `WOOD_<PERSON>` |
| tree (scenery) | `left` | `TREE<N>_WOOD_VALUE` |
| shape (building plan) | `received` | the planned building's `WOOD_<BUILDING>` |
| building | `stored` | a per-type cap, only for building types with a flag set (others take none) |

### Transfer
`transfer_wood(src, dst, max)` moves `min(src wood, max, dst cap - dst wood)` in one step. A tree source goes
through a separate routine that removes its wood, with the cutter's tribe.

### Chopping (a person's gather-wood state)
```text
on arrival:   play the chop animation; timer = CHOP_TIME (20 at a tree, 3 at a wood pile)
every call:   if not within 0x70 world units of the tree on both axes: walk to it again
              else timer -= 1; if timer == 0 and carried < WOOD_<PERSON>:
                  transfer_wood(tree, person, WOOD_<PERSON>)
```
- A full load moves at once, at the end of the countdown. Wood does not trickle in during it.
- Measured live on 5 braves: the timer reads 19, 18, ..., 1 on consecutive turns (it is set to 20 and decremented
  in the same turn), and `carried` jumps from 0 to 100 on the next turn. That is 20 turns per load with no
  every-8th-turn gate.
- A person already full skips the transfer.
- `CHOP_TIME` is exe-only (not in the constant table): 20 for all six person types that can carry wood, 3 from a
  wood pile (scenery type 0x0B), so picking up is about 7× faster than cutting.
- Only braves carry wood with the file's values (`WOOD_<SPECIALIST>` is 0).

### Regrowth
A tree regains `TREE<N>_WOOD_GROW` every 16 turns, up to `TREE<N>_WOOD_VALUE`. `TREE<N>_DORMANT_TIME` is read
next to them; what it does is open.

### Values

| Value | Exe | File |
|---|---|---|
| `CHOP_TIME` (tree / wood pile) | 20 / 3 | - |
| `WOOD_BRAVE` (one load) | 100 | 100 |
| `WOOD_WARR`, `_PREACH`, `_SPY`, `_SWARR`, `_SHAMAN` | 100 | 0 |
| `WOOD_HUT_1` | | 300 |
| `TREE1..6_WOOD_VALUE` (full tree) | 200 | 400 |
| `TREE1..6_WOOD_GROW` (per 16 turns) | 6 | 2 |
| `TREE1..6_DORMANT_TIME` | 855 968 .. 1 183 648 | - |

Example with the file's values, at 12 turns per second:
- one load is 20 turns of chopping, 1.67 s (measured); a wood pickup is 3 turns, 0.25 s;
- a full tree is 4 loads, 80 turns (6.7 s) of chopping plus the walks; a small hut is 3 loads;
- an emptied tree regrows one load in 800 turns (67 s), and is full again after 3200 turns (4 min 27 s).

## Huts: breeding and growth [exe]
A hut (building types 1-3, small, medium, large) has two counters, updated by the building handler on its own
rhythm. `inside` is the number of people in the hut.

| | Breeding (`breed`) | Growth (`grow`) |
|---|---|---|
| Runs | every 4 turns (`class_counter & 3 == 0`) | every 16 turns (`class_counter & 0xF == 0`) |
| Step | `2 × (inside + 1)` | `8 × inside`: nothing when empty, but not reset either |
| Target | `HUT<n>_SPROG_TIME × SPROG%[band]` | 2400 for small and medium, exe-only (not in `constant.dat`) |
| Depends on population | yes, through `band` | no |
| When full | a brave is born at the door, `breed = 0` | the hut starts growing into the next size |
| When the hut is completed | `breed = target - 54`: the first brave comes quickly | `grow = 0` |

### Breeding
```text
every 4 turns, per hut:
    if not can_breed(tribe): breed = 0; stop
    target = HUT<n>_SPROG_TIME * SPROG%[band] >> 8          # SPROG% in 8.8
    breed += 2 * (inside + 1)
    if breed >= target: alloc a brave (class 1, type 2) at the door, breed = 0     # same pass
```
- `P` is a weighted head count: `sum(count[type] * CONV_<type>)` over braves, warriors, preachers, spies and
  firewarriors. The file's `CONV_<PERSON>` are all 1, so `P` is the number of those people, without the shaman or
  wildmen. The exe defaults are 1 for a brave, 3 for a warrior, preacher or spy, and 6 for a firewarrior.
- `band = (P + 1) / 10`, clamped to 0..19: bands of 10 people, i.e. 5 % of the 200 maximum. It is not a % of the
  tribe's cap.
- `SPROG%` rises with the band (file: 30 % for band 0 up to 200 % for band 19), so a bigger tribe breeds slower.
- `can_breed`: `P + 1 < cap`, where `cap = sum over huts of MAX_POP_VALUE__HUT_<n>` (3, 5, 7) plus a per-tribe base
  (tribe+0x921), at most 200. A tribe flag (tribe+0x941 bit 6) also stops it. At the cap, `breed` is reset to 0,
  not kept full.

Time to one brave, at 12 turns per second, with the file's values (`SPROG_TIME` 4000 / 3000 / 2000):

| `P` (band) | Small, empty | Small, 3 inside | Medium, 5 inside | Large, 7 inside |
|---|---|---|---|---|
| 0-8 (30 %) | 2376 turns, 198 s | 596, 50 s | 300, 25 s | 152, 13 s |
| 39-48 (60 %) | 398 s | 100 s | 50 s | 25 s |
| 89-98 (110 %) | 732 s | 183 s | 92 s | 46 s |
| 189+ (200 %) | 1333 s | 333 s | 167 s | 83 s |

The `SPROG_TIME` comment in the file ("X 0 - 0.5, 1 - 1.0, 2 - 1.5, 3 - 2.0") matches the step: it is
`(inside + 1) / 2` per turn on average.

### Growth
```text
every 16 turns, per hut whose type has a next size (small -> medium -> large):
    if inside == 0: stop                                   # grow is kept
    grow += 8 * inside
    if grow >= 2400:
        grow = 2400
        if placement_check(hut):                           # 0x40B4F0, probably the bigger footprint
            create the next size on the spot as a construction site (class 2, its shape seeded with 100 wood)
            move the people inside into it
```
- The new site starts with one load (100 wood). With `WOOD_HUT_2` / `_3` 300, braves bring 2 more loads.
- When the placement check fails, the counter stays full, and another branch runs every 128 turns, probably a
  retry (not traced).
- A large hut has no next size and never grows.

Time to start growing, at 12 turns per second:

| Inside | 1 | 2 | 3 (small full) | 5 (medium full) | 7 |
|---|---|---|---|---|---|
| Turns | 4800 | 2400 | 1600 | 960 | 688 |
| Seconds | 400 | 200 | 133 | 80 | 57 |

## Prayer at triggers [exe]
Totems, stone heads and other prayer places are driven by the level's trigger on their cell (class 6, type 6;
level-format.md "General (6), model 6, trigger"). The trigger handler (`FUN_004fb270`) switches on `TriggerType`:

| `TriggerType` | Path |
|---|---|
| 0 proximity, 3 shaman proximity, 5 shaman + angel of death | prayer gauge, below (3 and 5 count only shamans) |
| 1 timed | a counter +1 per call, fires at `TriggerCount` |
| 2 player death | its own check (0x419480 with `TriggerCount`) |
| 4 library (vaults) | its own branch (0x4FB813), not traced |

### The gauge
```text
every 4 turns (class_counter & 3 == 0):
    count[tribe] = people of each tribe praying within (2 * CellRadius + 1)² cells of the trigger
    n = min(max over tribes of count, C)                  # C = TriggerCount
    if n > 0:
        gauge += C² / (C − n + 1)²                        # integer division
        if gauge >= PrayTime * C²: gauge = PrayTime * C²; the trigger fires
    else:
        gauge = max(0, gauge − C²)                        # linear drain
```
- A unit counts if it is a person of a tribe and in the prayer state (0x4F62C0 with 0x1B); for types 3 and 5,
  only shamans.
- With `PrayTime` 0 the trigger does not use the gauge: it fires on presence alone (walk-in triggers).
- There is one gauge per trigger, not per tribe. The tribe with the most prayers drives it (the lowest tribe
  index on a tie), and its index is kept at trigger+0xA0.
- Speed: at full count (`n >= C`) the gauge gains `C²` per update, so it is full after `PrayTime` updates, i.e.
  `PrayTime × 4` turns or **`PrayTime / 3` seconds**. Each missing prayer divides the speed by `(C − n + 1)²`.
- Draining takes the same time as filling at full count.

Example, a totem needing 8 (C 8) with `PrayTime` 64 (level 3's stone head):

| Praying | Gain per update | Speed | Time to fill |
|---|---|---|---|
| 8 or more | 64 | 100 % | 256 turns, 21 s |
| 7 | 16 | 25 % | 85 s |
| 6 | 7 | 11 % | 3 min 15 s |
| 5 | 4 | 6 % | 5 min 41 s |
| 4 | 2 | 3 % | 11 min 23 s |
| 1 | 1 | 1.6 % | 22 min 45 s |

At full count, the levels' totems (`PrayTime` 15-1000) take 5 s to 5 min 33 s, and the shaman-only triggers
(5-35) take 1.7 s to 12 s.

## Spells [exe]
Traced from the disassembly for all 21 spell ids. Values: "file" is `constant.dat` (wins at run time), "exe" the
built-in default; costs below are the file's. Times at 12 turns per second.

### From cast to effect
A cast creates a spell unit (class 11) that only carries the spell to its target; the work is done by the
**component** units it spawns there: effects (class 7) and shots (class 8). The same components are reused by
several spells (and by other game code), so a spell is a small recipe.

```text
cast:       shaman in cast state 0x16 for 10 turns (0.8 s), facing the target
            human tribe: 12-turn (1 s) cooldown on every cast command
+6 turns:   a shot flies from the shaman (z + 0x60) to the target: record +0x27
            (fireball 8/4 for blast, 1000 units/turn; standard shot 8/1 for the others, 1400 units/turn)
            homing on the target unit for spells with record +0x1B bit 0x40
arrival:    if record +0x2F (burn, blast, lightning, whirlwind, insects) and the target cell holds a shielded
            enemy: the shot is deflected to a random point 8 cells away, owned by the shield's tribe; repeat
            else: spawn each effect of record +0x28..+0x2C at the target, delete the spell unit
```
- Nothing appears directly at the target: every spell flies first. Without a shaman the spell fizzles (a debug
  tribe flag instead drops record +0x26 from the tribe's base).
- Whirlwind and land bridge also receive the caster position (land bridge is created at the caster, aimed at the
  target).

### Range
`range = SP_W_RANGE × altitude %`. The caster's band is `clamp(z >> 7, 0, 7)` (128 height units per band),
interpolated linearly to the next band; `ALT_BAND_0..7_SPELL_INCR` is 80, 90, ..., 150 % (file; exe all 100 %).
A shaman in a finished drum tower gets +1/3. The range is checked by the UI and the shaman's walk-to-cast state,
not again when the command is executed.

### Charges and mana
- Per player (0x96070A + player × 0x38): a learned-spell bitmask, a blocked bitmask, and per spell a byte whose
  low nibble is the charges and high nibble how many of them are gifted one-shots.
- Mana income is shared evenly between the learned, unblocked spells that are not full; a spell gains a charge
  when its share reaches its cost, up to `SP_1_OFF_MAX` (see "Mana rules").
- Casting from a charge costs no mana. No per-spell cooldown exists; `SPELL_<X>_OPT_S` is only read by the
  advisor's mana bookkeeping.
- Hidden spells (record +0x00 = 2: armageddon, bloodlust, teleport) are not offered in normal play. Burn is not
  castable (record +0x1A bit 0 clear).
- A timed "landscape shaping" mode (global 0x89D17C bit 0x20, `LSME_DURATION_SECS`, file 120 s) uses record
  +0x2E as the charge cap (5 for erosion, land bridge and flatten, 0 = not offered) and +0x22 as the range
  (7168). What starts it in normal play is not traced.

### Components
| Component | Effect | What it does |
|---|---|---|
| Simple blast | 7/0x01, 0x4E (spell blast), 0x38 | 3 turns; radius 2, 4, 5 cells. Each turn every non-allied person within the radius takes `BLAST_DAMAGE_PERSON` (exe 50, not in the file), so up to 150; pushed within 2.5 cells (140 horizontal, +98 up, × (1280 − d) / 1280). Buildings take `BLAST_DAMAGE_BLDG` (exe 40) once, vehicles 50 per turn. Allies: no damage, at most `BLAST_FRIEND_AFFECT` (5) pushed, on the last turn. Not pushed: own shaman, angel of death; immune: shielded units. Ghosts in the centre cell are removed. |
| Burn cell | 7/0x05 | Once, on the target cell: trees, plants, wood piles and fires start burning. No damage to people. |
| Lightning | 7/0x1E | On the target cell: sets the building on fire, burns trees (or lights a fire), and a simple blast. |
| Fireball shots | 8/4 (blast, firestorm), 8/7 and 8/8 (volcano) | Fly or fall to a point; on impact a simple blast (firestorm's also panics everyone it hits), smoke, and a short fire. |
| Smoke, explosions, sparkles, strands, gloops | 7/0x03, 0x26, 0x27, 0x2A, 0x3C, 0x3E, 0x3F, 0x40 | Visual only. |
| Nearest-N selector | function 0x515E30 | The 3 × 3 cells around the target; applies an action to the N closest units passing a filter. |
| Lava | 7/0x22 flow, 0x28 square | Hot cells set people on fire (state 31) and set buildings and trees burning; the caster's own people too, only the caster's shaman is spared. |

Shared person and building states:
- **Panic** (person state 0x1A): runs at a random heading at speed 110, screaming, for 64 turns (5.3 s).
- **On fire** (person state 31): runs about and loses `LIFE_<type>` / 32 per turn for 70 turns, so dies in about
  32 turns (2.7 s).
- **Fall**: landing at vertical speed ≤ −200 costs `FALL_OUT_OF_WW_DAMAGE` (exe 700, not in the file).
  Gravity 32 per turn.
- **Burning building**: 127 turns (10.6 s); occupants thrown out and panicked; one structural hit of 100 wood; a
  building with no wood left is destroyed.

### Per spell
| # | Spell (file cost, range, charges) | Mechanism |
|---|---|---|
| 1 | Burn (exe 12 000; not castable) | Standard shot, burn cell, smoke, explosion. No damage to people. |
| 2 | Blast (10 000, 3072, 4) | Fireball shot, then simple blast, burn cell, smoke, explosion. |
| 3 | Lightning (80 000, 6144, 4) | Kills outright up to `LIGHTNING_NUM_KILLS` + 1 (file 7) people in the single target cell, any tribe except the caster's shaman, angels of death and shielded units: own followers and enemy shamans die too. Plus the lightning component. |
| 4 | Whirlwind (90 000, 4096, 3) | 200 + 24 turns (18.7 s), 120 units per turn. Wanders around the target for 100 turns, then drifts away. Lifts every person in its own cell, any tribe, for 12-19 turns, then throws them: the landing costs 700. Each turn a building in its cell has a 25 % chance to lose 100 wood (a hut survives 3, a temple 8); trees lose 100 wood (50 %); library and prison immune; vehicles get a simple blast. |
| 5 | Insect plague (40 000, 6144, 4) | 60 insects for 200 turns (16.7 s), at most 6 cells from the target. Every 8 turns, enemies in its 3 × 3 cells take `SWARM_PERSON_DAMAGE` (100) and panic; shamans and angels immune; ghosts destroyed. Up to 3 times it raids an enemy building: occupants thrown out, damaged and panicked. |
| 6 | Invisibility (50 000, 4096, 4) | The `INVIS_NUM_PEOPLE` (file 6) nearest own followers (not the shaman), for `INVISIBLE_COUNT_X8` × 8 turns (120 s). Enemy auto-targeting skips them; some actions end it early. |
| 7 | Hypnotism (85 000, 4096, 3) | The `HYPNO_NUM_PEOPLE` (file 6) nearest persons of other tribes (not wildmen, shamans, angels) are recreated in the caster's tribe for `HYPNO_COUNT_X8` × 8 turns (file 440, 36.7 s), then return. If their tribe is gone, they stay. Ghosts are destroyed. |
| 8 | Firestorm (400 000, 4096, 2) | `FIRESTORM_DURATION` (file 220 turns, 18.3 s); one fireball every 4 turns (about 55) falling on a random point of a 6 × 6-cell square around the target. Each: simple blast that panics, a short fire, and sets fire to a building on that cell. |
| 9 | Ghost army (18 000, 4096, 4) | Copies the best own unit type near the target (shaman > firewarrior > preacher > spy > warrior > brave): 6 brave ghosts, 3 specialists or 1 shaman; at most 60 per tribe. No expiry found. They attack only enemy ghosts and shamans, their shots do nothing; melee untested. Blast and insects destroy them. |
| 10 | Erosion (210 000, 4096, 2) | 63 hydraulic-erosion passes, one per turn (5.25 s), on an 8 × 8-cell window jittered around the target: ground washed downhill. No direct effect on units. |
| 11 | Swamp (100 000, 4096, 3) | No terrain change. Lasts 32 000 turns (44 min). Every 4 turns it kills outright every person on its 3 × 3 cells, any tribe, until `SWAMP_NUM_PEOPLE` (exe 10) are dead. At most 30 per tribe (the oldest goes). Removed when most of its cells are unsuitable. |
| 12 | Land bridge (70 000, 5120, 4) | From the caster to the target, 4 cells wide, heights on a straight slope between both ends (at least 90), reached over 62 turns (5.2 s), at most `LAND_BRIDGE_MAX_CHANGE` per step; sea cells raised too. |
| 13 | Angel of death (510 000, 3072, 1) | A meteor, then an angel (person type 8, `LIFE_AOD` 10 000) hunts the nearest enemy on the whole map, shamans included, pulling people out of buildings; carries and throws each victim to death, about one per 28 turns (2.3 s). Ends after `AOD_KILL_COUNT` (40) kills or `AOD_DURATION` (2500 turns, 208 s). Two angels fight by health. |
| 14 | Earthquake (175 000, 4096, 2) | 120 turns (10 s) of shake. At turns 24, 56 and 88 each building in 16 × 16 cells has a 20/128 chance to be shaken (occupants out, partial explosion). At turn 60, a half-disc of radius 8 cells sinks by up to 600 (`600 × (4096 − d) / 4096`), with a lava fissure of short-lived lava squares and possibly two lava flows. |
| 15 | Flatten (125 000, 4096, 3) | 15 turns (1.25 s): a 45-cell disc (about 3.5 cells radius) converges to the ground height of the target point. |
| 16 | Volcano (800 000, 3072, 1) | 160 turns (13.3 s): 80 turns of rumbling, then a cone rises over 59 turns (rim 1024 at 2.5 cells, slopes to 5 cells, a plug in the crater). At turn 110 every building in 6 × 6 cells is destroyed and every person there (but the caster's shaman) set on fire; lava flow (up to 16 s); about 22 small and 6 big fireballs (simple blasts) and 26 rocks. |
| 17 | Convert wild (10 000, 8192, 4) | Every 8 turns the nearest wildman within 2 cells becomes a brave of the caster (if the tribe is under 200); ends 18 turns after the last conversion. |
| 18 | Armageddon (hidden) | Clears the map, turns every building into one person, gathers every tribe on an arena around the target and makes them fight; the weakest shaman is struck by blast or lightning at intervals; anyone more than 9 cells away dies. |
| 19 | Shield (60 000, 4096, 4) | The `SHIELD_NUM_PEOPLE` (file 6) nearest own followers (not shaman, wildmen, angels, ghosts) for 120 s: immune to blast and whirlwind, other damage ignored too, except melee; deflects burn, blast, lightning, whirlwind and insect shots. |
| 20 | Bloodlust (hidden) | 6 nearest own followers for `BLOODLUST_COUNT_X8` × 8 turns (file 120 s): melee and firewarrior damage × `BLOODLUST_DAMAGE_X` (3), damage taken >> `BLOODLUST_HEALTH_X` (3, i.e. / 8), firewarrior reload >> `BLOODLUST_SW_BLAST_X`. |
| 21 | Teleport (hidden, 65 536) | Moves the caster's own shaman to the target 8-12 turns after arrival (out of any vehicle or building; boards a friendly or empty vehicle there). |

Friendly fire: lightning, whirlwind, swamp, volcano and lava hurt the caster's own people; blast, firestorm and
insects do not damage allies. Lightning and swamp kill regardless of health.

## Combat [ghidra]
- Internal controller units (class 10):

| Type | Role |
|---|---|
| 10/8, 10/9 | fight, pre-fight |
| 10/1, 10/6 | formations (`process_formation_unit`, `add_unit_to_formation`) |
| 10/0xA | guard control |
| 10/4, 10/0xC | soul convert |

  A melee is probably one fight controller that its participants link to. Melee damage per hit is
  `max(32, FIGHT_DAMAGE_<type> × hp / max_hp)` (× `BLOODLUST_DAMAGE_X` when bloodlusted, 0x4A39C0) [exe];
  the rest of the fight is not traced.
- Per person type, the constants hold `LIFE_`, `FIGHT_DAMAGE_`, `CONV_`, `SW_BLAST_DAMAGE_` and the tower detection
  radius (see "Constant table").

## Impact on our specs
- huts-and-training.md rule 7 (`MANA_F[kind]` × `HOUSED`/`WORKING`/`TRAINING` %, × `HUT_LEVEL`) is not what the
  income routine does: it uses the busy/idle constants above. The `MANA_F_<PERSON>` and activity % values are read
  somewhere else, not found yet. Brave 15 and specialists 4 happen to equal the busy and idle defaults.
- spells.md "Charges": the charge count is read from `SP_1_OFF_MAX`, so `SpellKind::max_charges()` should read it
  rather than derive it from the cost.
- TODO "Mana", "split between spells and training": the original charges spells first and banks the surplus.
  Whether training takes from the bank is open.
- The shaman kill reward goes through the delayed reservoir. Which path totem and discovery mana gifts take is
  not traced.
- Wood, mapped to ours: one load (100) is one piece, a tree's 400 is our size 4, a small hut's 300 is our
  `wood_cost` 3. Our model (one piece moved at the end of the chop) already matches the single transfer.
- units.md `Chopping`: the original is 20 turns per load, 1.67 s; our `CHOP_TICKS` is 60 ticks (6 s), a guess,
  so about 17 ticks would match. Picking up from a pile is 3 turns (0.25 s); ours is instant on arrival.
- trees.md "Growth": ours regrow one piece every 600 ticks (60 s); the original regrows 2 wood per 16 turns, one
  piece in 800 turns (67 s), so about 667 ticks.
- Spell durations in turns convert the same way, e.g. `SHIELD_COUNT_X8` 180 is 1440 turns, 120 s.
- Specialists cannot carry wood with the file's values, as in our braves-only rule.
- huts-and-training.md rule 5 (breeding): our `occupants + 1` per update against `2 × SPROG_TIME × band` gives the
  same time as the original if one update is one turn (original: `2 × (inside + 1)` every 4 turns against
  `SPROG_TIME × band`). The differences:
  - the band comes from the weighted head count in steps of 10 people (out of 200), not from the population as a
    % of the cap;
  - at the cap the original resets `breed` to 0, where ours keeps it full and releases a brave as soon as there is
    room;
  - a new hut starts at `target - 54`, nearly full, which is close to our "full".
- huts-and-training.md rule 6 (growing): the original only grows with people inside (`8 × inside` every 16 turns
  to 2400), not "even when the hut is empty". No population check was seen in the growth code, only a placement
  check. The new site starts with 100 wood (one piece), so 2 more pieces are brought, not 3 with 1 consumed.
  `GROW_STEP` / `GROW_BONUS` can be replaced by these values.
- worship.md "Shared: the prayer gauge":
  - one `PrayTime` unit is 4 turns (1/3 s), not one of our ticks (0.1 s), so our gauges fill about 3.3× too fast;
    `PrayTime` × 10 / 3 ticks would match;
  - the original's speed is `C² / (C − n + 1)²`, much harsher than our `n² / C²`: on a totem of 8, 6 praying is
    11 % (ours 56 %) and 4 praying is 3 % (ours 25 %); `game_core::gauge::gain` can use it, still in integers;
  - the linear drain at the full-speed rate matches our guess;
  - the original has one gauge per trigger, driven by the tribe with the most prayers, where ours has one per
    tribe.

- spells.md, the original spells:
  - every spell flies from the shaman as a shot first (6 turns after the cast starts, 1000 or 1400 units per
    turn) and takes effect on arrival; burn, blast, lightning, whirlwind and insects can be deflected by a
    shielded enemy on the target cell;
  - range scales with the caster's altitude band `z >> 7` (128 height units per band, interpolated), which
    answers the "how heights map to the 8 bands" question of "Original balance";
  - `SPELL_<X>_OPT_S` is not a recharge time: no per-spell cooldown exists, only a 1 s cooldown after any
    human cast;
  - Land bridge: 4 cells wide, minimum height 90 (ours: min 32); Flatten: a 45-cell disc of about 3.5 cells,
    15 turns (ours: radius 4, instant); Erosion is a hydraulic-erosion simulation over 63 turns on 8 × 8 cells
    (ours: radius 4 lowered by 120); Swamp, Earthquake and Volcano are now described in enough detail to
    implement (see "Per spell");
  - Teleport moves the caster's own shaman, as our sandbox teleport does; its range is effectively unlimited.

## Open questions
- Where `MANA_F_<PERSON>`, `MANA_F_HOUSED/WORKING/TRAINING` and `MANA_F_HUT_LEVEL_1..3` are applied. Only the max
  of the three activity percentages is stored, and its readers are not traced.
- What "busy" means (unit+0xE bit 7, unit+0xA7), and the preacher's extra check.
- The controller types of tribe+0xC1F (2 is human; 1 gets full mana).
- Spell order and share inside the distribution loop.
- Combat: how the fight controllers apply `FIGHT_DAMAGE_<P>` to `LIFE_<P>`.
- What the per-building stored mana is.
- What sets `turns_per_sec` to 14, and whether game options or multiplayer change it.
- Which field holds the gather sub-state: +0xA8 stays 0 while chopping, so the handler's state byte is elsewhere.
- What `TREE_DORMANT_TIME` does (maybe the time before a cut tree regrows or a new tree appears).
- Huts: whether `class_counter` advances once per turn (the 4- and 16-turn rhythms assume it), what the growth
  placement check tests (footprint, population?), the every-128-turns branch when it fails, and the per-tribe base
  capacity (tribe+0x921).
- Prayer: the vault (library trigger) branch, what the "contest" countdown at trigger+0x6F does when several tribes
  pray at once, and whether the gauge resets after the trigger fires when `NumOccurences` allows more.

- Spells: burning trees (scenery state 5), the partial building explosion of the earthquake, whether ghosts
  expire, what starts the landscape-shaping mode, how people react to the ground moving or flooding under them,
  and the melee fight itself.

## Reference

### Executable layout [exe]
Addresses are virtual addresses in that build. `.text` 0x401000 is at file offset 0x400, `.rdata` 0x58F000 at
0x18D800, and `.data` 0x598000 at 0x195E00.

| Address | Function |
|---|---|
| 0x4ECAC0 | `init_tribe_struct`, "tribe_recount_and_mana_income": rebuilds unit lists, income |
| 0x41AF80 | `get_unit_mana(person)` |
| 0x4DF0E0 | preacher's extra check in income |
| 0x41A4F0 | `add_mana`: `tribe->mana_in += amount` |
| 0x41A500 | "give_mana_delayed(tribe_index, amount)" |
| 0x41A550 | calls the distribution for each active tribe, every turn |
| 0x41A590 | "tribe_distribute_mana"; its refill loop at 0x41ACCB |
| 0x41AEB0 | charge reset (callers 0x44139B, 0x50AFAC) |
| 0x4C2D50 | `charges(spell)`: `SP_1_OFF_MAX` |
| 0x41AE60 | sum of stored building mana |
| 0x406218 | adds a building's stored mana to a type 1 tribe |
| 0x4C16E5 | cast cost in `init_unit_type_11`; parameter queue at 0x892443 |
| 0x43F21F | `process_tribe_cmd` mana command |
| 0x491A82 | AI-script mana opcode, through `get_tribe_data` |
| 0x4D5FE0 | shaman death |
| 0x42B752 | `clear_tribe`: `START_MANA` |
| 0x49C1A0 | `read_levels_const` |
| 0x4D32B0 | `unit_processing_class_1_person` |
| 0x4C1940 | `unit_processing_class_11_spell` |
| 0x4A7860 | `FUN_004a7860`, "transfer_wood(src, dst, max)" |
| 0x4A79F0 | removes wood from a tree (tree, -amount, tribe) |
| 0x495D70 | `FUN_00495d70`, a person's gather-wood state; chop step at 0x4960CC |
| 0x4342BF | wood pile pickup: timer 3 (scenery type 0x0B) |
| 0x4A5590 | main loop; `step_ms = 1000 / turns_per_sec` at 0x4A5595, fixed-step catch-up at 0x4A57B4 |
| 0x4EC6F0 | one game turn: `turn += 1` at 0x4EC703 |
| 0x43EC3C | changes `extra_turns` (clamped 0..4, or 0..32 with bit 7 of 0x89C662) |
| 0x404C80 | hut breeding, every 4 turns; spawns the brave at 0x404E2C |
| 0x404BEE | hut completion: `grow = 0`, `breed = target - 54` |
| 0x41B3F0 | "sprog_target(tribe, hut_type)": `SPROG_TIME × SPROG%[band] >> 8` |
| 0x41B240 | "can_breed(tribe)": weighted head count against the hut capacity |
| 0x4050C0 | hut growth, every 16 turns; creates the next size at 0x4051FE |
| 0x40B4F0 | growth placement check |
| 0x4FA8F0 | `unit_processing_class_6_general` |
| 0x4FB270 | trigger handler; prayer count at 0x4FB48D, gauge at 0x4FB71F, drain at 0x4FB788 |
| 0x4FB1D0 | trigger defaults when created without a level record (`TriggerCount` 1, `PrayTime` 192) |
| 0x485D00 | level loader: trigger record fields to the unit |
| 0x4C14C0 | `init_unit_type_11`: spell unit, mana, cooldown; cast setup 0x4C1B80 |
| 0x4C1D10 | spell unit per turn: sub-state 3 waits 6 turns, launches the shot (0x4C21E0), sub-state 2 deflects or spawns the effects (0x4C1EA0) |
| 0x4C2E30 | cast range with altitude bands; 0x4C24F0 can-cast check |
| 0x4BAF00, 0x4BB440 | standard shot and fireball shot per turn |
| 0x50A750 | effect per turn: switch on +0x2C (table 0x50AFD4); effect init by type: table 0x50A59C |
| 0x50B630, 0x50B6F0, 0x50B740 | simple blast: default init, init with parameters, per turn |
| 0x515E30 | nearest-N selector (invisibility, shield, bloodlust, hypnotism) |
| 0x4DA080 | person damage (unit, tribe, amount, ignore_shield) |
| 0x4E6D00 | person physics: landing and fall damage |

Timing globals: turn counter 0x89D188 (u32), `turns_per_sec` 0x89D161 (u8, copied from the option at 0x98F712),
`next_turn_time` 0x5CD92C, `step_ms` 0x5CD930 (83 in the exe image), `extra_turns` 0x895DAC (i8). Under Wine the
exe is mapped at its image base 0x400000, so these addresses can be read as they are.

### Unit record [ghidra]
- `unit_struct`, 0xB3 bytes. Pool of 2000 (`unit_array` 0x8E0428), two free lists split at index 640
  (`alloc_unit`, `move_unit_to_free_list`).
- Fields: `unit_class` +0x2A, `unit_type` +0x2B, `state` +0x2C, `state_2` +0x2D, `tribe_index` +0x2F, `unit_obj`
  (model or sprite) +0x33, `pos` +0x3D, building stored value +0x98, last attacker's tribe +0xB0.
- Wood fields: a person's state timer +0x70 and carried wood +0x78, a tree's wood left +0x84, a shape's wood +0x96,
  and +0xA4 for the building types that hold wood. The gather handler switches on a byte read as +0xA8, but that field stays 0 while
  chopping (measured), so the offset is unconfirmed.
- Hut fields: `grow` +0xA0, `breed` +0xA4 (the same offset as wood in other building types), people inside +0xA6,
  "brave ready" flag bit 14 of the word at +0x9C. `class_counter` +0x2E gives each unit its 4- or 16-turn phase.
- Trigger fields (from the level record, 0x485D00): `TriggerType` +0x68, `CellRadius` +0x69, `RandomValue` +0x6A,
  `NumOccurences` +0x6B, `StartInactive` +0x70, `ThingIdxs` +0x72 (10 × u16), `TriggerCount` +0x8E,
  `InactiveTime` +0x94, `PrayTime` +0x9A (i32). Run time: prayers per tribe +0x86 (4 × u16), `gauge` +0x96 (i32),
  leading tribe +0xA0, flags +0x6D, contest countdown +0x6F.
- Effect state (+0x2C) per component, for the per-turn table: simple blast 1, smoke 3, burn cell 6, ghost army
  13, invisibility 14, earthquake 15, volcano 16, hypnotism 17, lightning bolt 18, swamp 19, angel of death 20,
  whirlwind 21, insect plague 22, firestorm 23, erosion 24, land bridge 25, lightning 30, flatten 31, lava flow
  33, lava square 38, convert wild 60, shield 68, armageddon 71, bloodlust 72, teleport 73.
- Person statuses: invisible +0x10 bit 0x1000 (timer +0xA3), shield +0x14 bit 0x8000 (timer +0xA5), bloodlust
  +0x14 bit 0x80000 (timer +0xB1), hypnotised +0x10 bit 0x4000 (timer +0xAC, original tribe +0xAD), ghost +0x10
  bit 0x800; the timers count down every 8 turns. On fire: +0xC bit 0x8 leads to state 31.
- Per-cell index `unit_land_array` (the original "mapwho"), filled by `insert_unit_into_land_tile`.
- Global lists: `wild_units`, `fight_units`, `pre_fight_units`, `guard_control_units`, `boat_units`,
  `airship_units`, `trigger_units`, `head_units`, `swamp_effect_units`.
- Each class has `init_unit_type_N`, `init_unit_class_N` and a per-turn `unit_processing_class_N_*`.

| Class | Kind | Class | Kind |
|---|---|---|---|
| 1 | person | 7 | effect (~93 types) |
| 2 | building | 8 | shot |
| 3 | creature | 9 | shape (building plan) |
| 4 | vehicle | 10 | internal (controllers) |
| 5 | scenery | 11 | spell (21 types) |
| 6 | general (lights, triggers, discoveries) | | |

Person types: 1 wild, 2 brave, 3 warrior, 4 preacher, 5 spy, 6 firewarrior, 7 shaman, 8 angel of death. Types per
class are in `pop3-rev/docs/unit_types`.

### Commands [ghidra]
`set_tribe_command`, `tribe_commands`, `process_tribe_cmd`: a 15-byte `cmd_pop` record (opcode, two args), run with
`mld_turn` (MLDPlay network turn) and the global `pseudo_random`.

### Tribe record [ghidra][exe]
4 tribes at `tribes_array` 0x89D1C8, 0xC65 bytes each. The player's tribe index is at 0x89C6F0.

| Offset | Meaning |
|---|---|
| +0x881 .. +0x899 | heads of the person, building, shape, formation and beacon lists |
| +0x89D | shaman |
| +0x91D | number of people (reaching 200 calls 0x499D90, probably an error or limit report) |
| +0x93D | flags; bit 3: spells are free |
| +0x94D | `bank`, clamped to `MAX_MANA` |
| +0x951 | `reservoir` |
| +0x955 | `incoming` |
| +0x959 | `needed` |
| +0x95D | `last_income` |
| +0x969 + 4·spell | `charge[spell]`, spells 1-21 |
| +0xA27 + 2·type | number of people per person type (rebuilt each turn) |
| +0xB7D + 2·type | number of huts per hut type (1-3), for the capacity |
| +0x921 | base population capacity, added to the huts' |
| +0x941 | flags; bit 6: no breeding |
| +0xA05, +0xA07 | reservoir `delay` and `rate` |
| +0xC1F | controller type: 2 human, 1 full mana, others unknown |
| +0xC22 | tribe number |

Other globals: the per-tribe level header `struct_56B` (0x38 bytes per tribe at 0x96070A) holds the spells:
learned bitmask +0x00, blocked bitmask +0x10, and per spell a byte at +0x14 + spell (low nibble charges, high
nibble gifted one-shots; `set_struct_56B_array_spell_val`); the reservoir short-delay flag is bit 3
of 0x89C661.

### Constant table [exe]
- At 0x5AA60F, 343 records of 31 bytes: `char name[25]`, `u8 size` (1, 2 or 4), `u8 flags`, `u32 value_ptr`.
  Read by `read_levels_const`.
- Flag bit 0, percent: stored as `value * 256 / 100` (0x49C4C1). The exe defaults are already in that form
  (512 = 200 %).
- Flag bit 1 (`WALK_ALT_DIFF2` only): the value is also copied into a per-type table.
- After loading, `max(MANA_F_HOUSED, MANA_F_TRAINING, MANA_F_WORKING)` is stored at 0x5AA448.
- Per-type records the constants point into:

| Table | Base | Stride | Fields |
|---|---|---|---|
| person types | 0x5A7060 | 0x32 | `MANA_F_` +0xE, `LIFE_` +0x10, `WOOD_` +0x14, `FIGHT_DAMAGE_` +0x1B, chop time (u8, not a named constant) +0x1D, `CONV_` +0x1F, tower detection radius +0x22, `SW_BLAST_DAMAGE_` +0x2C |
| building types | 0x5A7228 | 0x4C | `WOOD_` +0x1A, next size +0x34 (u8), growth target +0x36 (2400, not a named constant), mana factor +0x38, `MAX_POP_VALUE__HUT_` +0x3A, `HUT_SPROG_TIME` +0x3C, flags +0x48 (bit 10: breeds; bit 6: holds wood) |
| scenery types | 0x5A79B0 | 0x18 | `TREE_WOOD_VALUE` +0x4, `TREE_WOOD_GROW` +0x6, `TREE_DORMANT_TIME` +0x8, flags +0x14 |
| spells | 0x5A80D0 | 0x3E | kind +0x00 (1 normal, 2 hidden), cost (u32) +0x4, id +0x8, flags +0x1A (bit 0 castable) and +0x1B (0x40 homing), `SP_W_RANGE_` +0x1E, mode-0x20 range +0x22, shot without shaman +0x26, shot from shaman +0x27, effects +0x28..+0x2C, `SP_1_OFF_MAX_` +0x2D, mode-0x20 charges (`LSME_1_OFF_MAX_`) +0x2E, deflectable +0x2F, sound +0x30, enemy-cast alert +0x35/+0x36, `OPT_S` +0x3A |

- The shipped `constant.dat` wins over the exe defaults, e.g. `SPELL_BLAST` 18 000 in the exe and 10 000 in the
  file, `LIFE_BRAVE` 1400 and 1000.
- Not in the table: 0x5AA44C (default 3) is masked with the turn number like `MANA_UPDATE_FREQ` should be, so it is
  probably that constant (unverified). The `MANA_IDLE_*`/`MANA_BUSY_*` values are exe-only too.
