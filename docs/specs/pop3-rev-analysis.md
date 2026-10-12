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
  rate. Our simulation runs at the same rate: one tick is one turn (`game_core::time`).
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
- **The planet bend is a parabola of 0.011 × d² cells** [exe, measured], applied in camera space; heights are
  256 units per cell (1024 = 4 cells) and the camera looks down at a fixed 20°. See "Landscape projection".
- **Walking speed is per type, in world units per turn** [exe]: `<TYPE>_SPEED` plus up to 25 % at random per
  walk; a brave walks 1.6-2.0 cells per second. See "Walking speed".
- **A melee is a pre-fight, then a fight** [exe]: the attacker runs up, kicks the target away without damage,
  runs up again; then a ring fight of one against up to five, where every blow costs
  `max(32, FIGHT_DAMAGE × life / max_life)` and most blows hit both fighters. See "Combat".
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

## Reincarnation sites [exe, measured]
No level file has pillars (scenery 12): the game creates them when the level loads, for each tribe that has a
site. The site (tribe+0x911) is on a cell centre; its height is kept at tribe+0x915.

### Pillars
| Slot | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|---|
| Offset (cells x, z) | 0, 3 | 2, 2 | 3, 0 | 2, −2 | 0, −3 | −2, −2 | −3, 0 | −2, 2 |
| Angle (2048 = full turn) | 0 | 256 | 512 | 768 | 1024 | 1280 | 1536 | 1792 |

- An octagon: 3 cells out on the axes, (2, 2) on the diagonals (2.8 cells). The offsets are a table at 0x5A9F10
  (in map-index units, 2 per cell). Each pillar sits on the centre of its cell, at that cell's ground height.
- Angle: `atan2(dx, −dz)` of the pillar's offset from the site (0x4A7D80), so each pillar faces the line from the
  centre; it comes out as slot × 256.
- Model: object 30 (the reincarnation stone, objects.md), from the scenery-12 record (`+0xA` = 30). The pillar
  belongs to its tribe; between tribes only the low byte of its model flags differs (3 for blue, 2 for yellow),
  probably the tribe colour (unverified).
- A new pillar rises out of the ground: it starts 256 below and climbs 16 per step over 8 steps with rock debris
  (7/0x33) and a sound (0x4A7EB0). "delete rs pillars" (7/0x4D) removes them; "move rs pillar" (7/7,
  0x50C690) replaces a slot's pillar, with an explosion at the old and the new place.
- Measured on a level (live memory): blue and yellow sites, 8 pillars each at exactly these offsets and angles;
  heights 127-128 around a site at 128, except one pillar at 107 on lower ground.

### Levelling ("prepare rs land", 7/8, 0x50C840)
Over about 21 turns (1.75 s) a disc grows by 160 world units per turn up to 2560 (5 cells), and every cell inside is
set to the site height. Inside it, enemy braves and specialists panic and lose half their `LIFE`, wildmen are
converted (when the effect's flag 1 is set), trees catch fire and swamps are removed; 32 sparkles orbit the site.

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
| Simple blast | 7/0x01, 0x4E (spell blast), 0x38 | 3 turns; radius 2, 4, 5 cells. Each turn every non-allied person within the radius takes `BLAST_DAMAGE_PERSON` (exe 50, not in the file), so up to 150; pushed each turn within 2.5 cells, cumulatively (140 outward, +98 up, × (1280 − d) / 1280, scaled by the slope: "Blast push and flight"). Buildings take `BLAST_DAMAGE_BLDG` (exe 40) once, vehicles 50 per turn. Allies: no damage, at most `BLAST_FRIEND_AFFECT` (5) pushed, on the last turn. Not pushed: own shaman, angel of death; immune: shielded units. Ghosts in the centre cell are removed. |
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
- **Fall**: landing at vertical speed ≤ −100 raises dust and a sound; ≤ −200 costs `FALL_OUT_OF_WW_DAMAGE` (exe 700,
  not in the file), except on cells whose terrain has flag 2 (probably water). Gravity 32 per turn.
- **Burning building**: 127 turns (10.6 s); occupants thrown out and panicked; one structural hit of 100 wood; a
  building with no wood left is destroyed.

### Blast push and flight
Each turn of a simple blast (0x50B740), for every person it may push in its current radius:
```text
d     = horizontal distance to the centre (2D, wrapping, square root; 0x4503F0), clamped to 0..R    R = 1280
f     = (R − d) / R                               # 1 at the centre, 0 from 2.5 cells on: no push, damage only
angle = direction from the centre to the unit     # straight outward, no randomness
len   = 140 × f, then the slope rule              # 0x4E93F0
h     = ground(unit) − ground(unit + 32 units along the push)
len  += len × clamp(|h|, 1, 16) / 16 if h ≥ 0 else −that; at least 3
vel.x += len × sin(angle); vel.z += len × cos(angle)        # added to the velocity, not replacing it
vel.y += 98 × f
flags  : airborne (+0xC |= 0x2000 | 0x80000)
```
- The slope rule gives ×1.06 on flat ground, up to ×2 when the ground falls away along the push, and almost
  nothing (3) against a rising one.
- The pushes add up: a person still within 2.5 cells on the 2nd and 3rd turn is pushed again, less as it got
  farther. Not pushed: the caster's shaman, angels of death, shielded units, units inside something (+0x9F);
  allies only on the last turn, at most `BLAST_FRIEND_AFFECT` (5).

Then the person physics moves it every turn ("Person physics", blown mode).

Worked examples (hand estimates, flat ground, not measured):
- Hit once at the centre: 149 outward and 98 up; it rises about 200, lands after about 8 turns at about −126
  (dust, no damage), having travelled about 2.2 cells in the air and 0.8 sliding: about 3 cells.
- At the centre on all 3 turns: the vertical kicks add up (98, then about 153, then about 196); it rises about
  950 and lands at about −220: 700 fall damage on top of the 150 blast damage, 850 of a brave's 1000. Horizontal
  speed reaches the 192 cap.

### Per spell
| # | Spell (file cost, range, charges) | Mechanism |
|---|---|---|
| 1 | Burn (exe 12 000; not castable) | Standard shot, burn cell, smoke, explosion. No damage to people. |
| 2 | Blast (10 000, 3072, 4) | Fireball shot, then simple blast, burn cell, smoke, explosion. No thrown state: people are pushed (see "Blast push and flight"), and only a hard landing hurts them. |
| 3 | Lightning (80 000, 6144, 4) | Kills outright up to `LIGHTNING_NUM_KILLS` + 1 (file 7) people in the single target cell, any tribe except the caster's shaman, angels of death and shielded units: own followers and enemy shamans die too. Plus the lightning component. |
| 4 | Whirlwind (90 000, 4096, 3) | 200 + 24 turns (18.7 s), 120 units per turn. Wanders around the target for 100 turns, then drifts away. Lifts every person in its own cell, any tribe, for 12-19 turns, then throws them: the landing costs 700. Each turn a building in its cell has a 25 % chance to lose 100 wood (a hut survives 3, a temple 8); trees lose 100 wood (50 %); library and prison immune; vehicles get a simple blast. |
| 5 | Insect plague (40 000, 6144, 4) | 60 insects for 200 turns (16.7 s), at most 6 cells from the target. Every 8 turns, enemies in its 3 × 3 cells take `SWARM_PERSON_DAMAGE` (100) and panic; shamans and angels immune; ghosts destroyed. Up to 3 times it raids an enemy building: occupants thrown out, damaged and panicked. |
| 6 | Invisibility (50 000, 4096, 4) | The `INVIS_NUM_PEOPLE` (file 6) nearest own followers (not the shaman), for `INVISIBLE_COUNT_X8` × 8 turns (120 s). Enemy auto-targeting skips them; some actions end it early. |
| 7 | Hypnotism (85 000, 4096, 3) | The `HYPNO_NUM_PEOPLE` (file 6) nearest persons of other tribes (not wildmen, shamans, angels) are recreated in the caster's tribe for `HYPNO_COUNT_X8` × 8 turns (file 440, 36.7 s), then return. If their tribe is gone, they stay. Ghosts are destroyed. |
| 8 | Firestorm (400 000, 4096, 2) | `FIRESTORM_DURATION` (file 220 turns, 18.3 s); one fireball every 4 turns (about 55) falling on a random point of a 6 × 6-cell square around the target. Each: simple blast that panics, a short fire, and sets fire to a building on that cell. |
| 9 | Ghost army (18 000, 4096, 4) | Copies the best own unit type near the target (shaman > firewarrior > preacher > spy > warrior > brave): 6 brave ghosts, 3 specialists or 1 shaman; at most 60 per tribe. No expiry found. They attack only enemy ghosts and shamans, their shots do nothing; melee untested. Blast and insects destroy them. |
| 10 | Erosion (210 000, 4096, 2) | 63 hydraulic-erosion passes, one per turn (5.25 s), on an 8 × 8-cell window jittered around the target: ground washed downhill. No direct effect on units. |
| 11 | Swamp (100 000, 4096, 3) | No terrain change. Lasts 32 000 turns (44 min). Every 4 turns it kills outright every person on its 3 × 3 cells, any tribe, until `SWAMP_NUM_PEOPLE` (exe 10) are dead. At most 30 per tribe (the oldest goes). Removed when most of its cells are unsuitable. |
| 12 | Land bridge (70 000, 5120, 4) | From the caster to the target, 4 cells wide (−1, 0, +1, +2 across the line), heights on a straight slope between both ends (at least 90): each turn `h += clamp(T − h, ±LAND_BRIDGE_MAX_CHANGE) / (64 − turn)` (`LAND_BRIDGE_MAX_CHANGE` 256, 64 being `LAND_BRIDGE_DURATION`), so every cell reaches its height after 62 turns (5.2 s) when the gap is small: the cap makes large gaps fall short (gap 256: 251 reached, 500: 488, 800: 759, 1000: 900); sea cells raised too; a smoke puff on every bridge cell every turn. Buildings left on ground uneven by more than 200 collapse ("Terrain changes: what reacts"). |
| 13 | Angel of death (510 000, 3072, 1) | A meteor, then an angel (person type 8, `LIFE_AOD` 10 000) hunts the nearest enemy on the whole map, shamans included, pulling people out of buildings; carries and throws each victim to death, about one per 28 turns (2.3 s). Ends after `AOD_KILL_COUNT` (40) kills or `AOD_DURATION` (2500 turns, 208 s). Two angels fight by health. |
| 14 | Earthquake (175 000, 4096, 2) | 120 turns (10 s) of shake. At turns 24, 56 and 88 each building in 16 × 16 cells has a 20/128 chance to be shaken (occupants out, partial explosion). At turn 60, a half-disc of radius 8 cells sinks by up to 600 (`600 × (4096 − d) / 4096`), with a lava fissure of short-lived lava squares and possibly two lava flows. |
| 15 | Flatten (125 000, 4096, 3) | 15 turns (1.25 s): a 45-cell disc (about 3.5 cells radius) converges to the ground height of the target point. |
| 16 | Volcano (800 000, 3072, 1) | 160 turns (13.3 s): 80 turns of rumbling, then a cone rises over 59 turns (rim 1024 at 2.5 cells, slopes to 5 cells, a plug in the crater). At turn 110 every building in 6 × 6 cells is destroyed and every person there (but the caster's shaman) set on fire; lava flow (up to 16 s); about 22 small and 6 big fireballs (simple blasts) and 26 decorative rocks ("Volcano rocks"). |
| 17 | Convert wild (10 000, 8192, 4) | Every 8 turns the nearest wildman within 2 cells becomes a brave of the caster (if the tribe is under 200); ends 18 turns after the last conversion. |
| 18 | Armageddon (hidden) | Clears the map, turns every building into one person, gathers every tribe on an arena around the target and makes them fight; the weakest shaman is struck by blast or lightning at intervals; anyone more than 9 cells away dies. |
| 19 | Shield (60 000, 4096, 4) | The `SHIELD_NUM_PEOPLE` (file 6) nearest own followers (not shaman, wildmen, angels, ghosts) for 120 s: immune to blast and whirlwind, other damage ignored too, except melee; deflects burn, blast, lightning, whirlwind and insect shots. |
| 20 | Bloodlust (hidden) | 6 nearest own followers for `BLOODLUST_COUNT_X8` × 8 turns (file 120 s): melee and firewarrior damage × `BLOODLUST_DAMAGE_X` (3), damage taken >> `BLOODLUST_HEALTH_X` (3, i.e. / 8), firewarrior reload >> `BLOODLUST_SW_BLAST_X`. |
| 21 | Teleport (hidden, 65 536) | Moves the caster's own shaman to the target 8-12 turns after arrival (out of any vehicle or building; boards a friendly or empty vehicle there). |

Friendly fire: lightning, whirlwind, swamp, volcano and lava hurt the caster's own people; blast, firestorm and
insects do not damage allies. Lightning and swamp kill regardless of health.

### Volcano rocks [exe]
Purely decorative: the volcano's per-turn function (0x50E060) makes one scenery 13 ("rock", `create_unit(5, 13)`
0x4ED8A0, at 0x50E2A2) per turn on volcano turns 105-130 (26 in all). No level places one; the volcano is their only
creator.
- Placement: a random angle (`rand & 0x7FF`) and distance `(rand & 0x1FF) × 2 + 0x600` (1536-2558 units, 3-5 cells:
  the cone's outer slope) from the centre, then a random point in that cell (scenery init 0x4A5EF0, model 13 at
  0x4A604A), at the crater's height when cast.
- Motion: no initial velocity. State 7 (0x4A650C) runs the generic mover 0x4E7A80 with movement profile 11 (speed 4,
  caps 192 / 512, gravity 32, ground friction 8, 72 on terrain flags 0x3E): it falls, and slides downhill while the
  slope (0x44F750, max − min height of its triangle) is above 24; blocking cells (flags 0x80200) push it out and
  reflect it. No bounce, no damage, no sound, no effect spawned, no terrain change (0x517F10 in the collision
  helper not checked).
- Stop: not sliding, its speed is scaled by 22/32 per turn; at 2 or below it goes to state 1 with 128 turns to
  live (0x4A6F40), then is deleted (0x4A6E20 → 0x4EDCF0), about 10.7 s at rest. "Ground moved" in state 1 (0x4A80B0)
  sends it sliding again.
- Drawn: a camera-facing sprite of `hfx0-0.dat` (sprite table `[0x59DF14]`, draw type 0 of the table 0x5A6AF8, draw
  kind 1 at 0x46895E), bottom-centred on its position, no rotation, opaque: frame `346 + ((x >> 6) + (z >> 6)) & 3`
  (scenery flag 0x20), so it seems to roll, changing every 1/8 cell. Its ground shadow is hfx 22 (draw kind 15,
  0x46ACF6). Which palette the game uses for hfx in a level is open.
- Related: "volcano explosions" 7/0x23 is deleted at init (0x509EE9). "Rock debris" 7/0x33 (init 0x5137C0) is not a
  rock but a 20-turn dust burst at ground − 112 (alpha sprites hfx 1160-1179, draw type 0x25, 20
  frames, animator 0x4EE7B0), made by the rising reincarnation pillars. "Rock explosion" 7/0x3F (init 0x50A278) is the
  16-turn fire blast where a big volcano fireball 8/8 lands (alpha hfx 1194-1209, sound 0xB2); small 8/7 ones make
  7/0x3E instead.

### Status markers over people [exe]
All drawn with the person's sprite in `draw_polygons` (0x468B3F-0x4694F2), straight from the status flags: no
effect unit. Sprites are `hfx0-0.dat` (table `[0x59DF14]`); `h` is the height of the person's current frame (VFRA
record byte +3), offsets go up the screen from the sprite's anchor; `n` is `frame_counter` (u32 0x897981).
```text
translucent = person[+0x36] & 0x40                  # whole person sprite alpha-blended (draw flag 0x5DA074 bit 3)
if own_tribe and life < max_life and not low_health and not ghost and health_bars_on:   # 0x468E1C
    health_bar(x, y - 0.75 h)                        # 0x525450, lines, not a sprite
if low_health:                                        # +0x14 bit 0x1000 (0x468EF5)
    draw hfx[1489 + n % 10] at y - 1.11 h, alpha, colour table 0x87F000; start sound 0xCF (0x48A050, mode 2)
elif person[+0x10] & 0x10: stop sound 0xCF (0x48A770)
if shielded and (timer +0xA5 >= 16 or class_counter & 2 == 0):           # +0x14 bit 0x8000 (0x469021)
    colour table = 0x87D000 + tribe_colour[tribe] << 12 (byte table 0x5A89CB, stride 5)
    for ring k in 0, 1, 2:                            # three rings, each one step behind the previous
        i = ((person[+0x24] + n) & 15 - k) & 15
        (s, f) = SHIELD_PATH[i]                       # table 0x59D9D8, 16 × (i16 sprite, i16 height × 256)
        draw hfx[s + 5k] at y - h / 8 - f × 7h / 8, alpha
if bloodlust and (timer +0xB1 >= 16 or class_counter & 2 == 0):         # +0x14 bit 0x80000 (0x4691F4)
    draw hfx[1478 + n % 10] at y - 1.11 h, alpha, colour table 0x87F000
if hypnotised:                                        # +0x10 bit 0x4000 (0x46930B)
    draw hfx[1553 + n % 11] at y - 1.25 h, alpha, colour table 0x87F000
if own_tribe and class == 1 and person[+0x7A] & 0x80:                    # selected (0x469415)
    draw hfx[53] at y - h, opaque
```
All markers are centred horizontally on the person. When zoomed (`[esp+0x3C]` set) they are scaled (0x476090,
drawn by 0x516430), otherwise drawn 1:1 (0x5162E0).

| Status | Flag | Marker | Sprites (`hfx0-0.dat`) | Above the feet | Blinks |
|---|---|---|---|---|---|
| Low health | +0x14 bit 0x1000 | white sparkles circling, plus the looping sound 0xCF | 1489-1498 (10 frames, 41 x 29) | 1.11 h | no |
| Shield | +0x14 bit 0x8000 | three rings in the tribe's colour sliding up and down the body | 1538-1552 (3 rings x 5 sizes, 32 x 16) | h/8 to h | last 15 steps (10 s) |
| Bloodlust | +0x14 bit 0x80000 | red bubbles | 1478-1487 (10 frames, 30 x 25) | 1.11 h | last 15 steps (10 s) |
| Hypnotised | +0x10 bit 0x4000 | a yellow ring turning, a halo | 1553-1563 (11 frames, 27 x 19) | 1.25 h | no |
| Invisible | +0x10 bit 0x1000 | no marker: own people drawn translucent, enemies not drawn | - | - | own, last 23 timer steps |
| Ghost | +0x10 bit 0x800 | no marker: own ghosts drawn translucent, enemy ones look normal | - | - | no |
| Selected | +0x7A bit 0x80 | yellow down arrow, own people only | 53 (9 x 7, opaque) | 1.0 h | no |

- All marker sprites are alpha sprites (`hfx0-0.dat` 1090..1499 and 1538..1592), except the arrow. Sparkles and
  rings are cream/white, bubbles red, the halo yellow at about half strength. What the colour tables do to an alpha
  sprite is open; for the shield it is the only thing that makes the rings tribe-coloured.
- `SHIELD_PATH` (index: sprite offset, height): 0: 0, 0; 1: 1, 1/8; 2: 2, 2/8; 3: 3, 3/8; 4: 4, 4/8; 5: 3, 5/8;
  6: 2, 6/8; 7: 1, 7/8; 8: 0, 1; then back down, 9: 1, 7/8 ... 15: 1, 1/8. A ring rises from the feet to the head
  over 8 frames and comes back, largest (offset 4) at mid-height; rings 0, 1, 2 are bright, medium and faint. The
  phase is offset per person by `+0x24` (probably its index), so shielded people do not move in step.
- Blinking (shield, bloodlust): the timer (+0xA5 / +0xB1, a byte) starts at `SHIELD_COUNT_X8` / `BLOODLUST_COUNT_X8`
  (0x5156F0 / 0x515690; file 180 each, exe 180 / 90) and loses 1 on the turns where `class_counter & 7 == 0`
  (0x4D3C74), the flag being cleared when it reaches 0. While it is 15..1 the marker is drawn only when
  `class_counter & 2 == 0`. `class_counter` +0x2E goes up once per turn for every active unit, just before its
  handler (`main_loop_inner` 0x4EC8B3 → 0x4ED700), so:
  - full duration 1433-1440 turns (179 steps of 8, the first one 1-8 turns after the cast): 119.4-120 s;
  - blinking for the last 15 steps, exactly 120 turns = 10 s;
  - 2 turns on, 2 off (167 ms each), 3 blinks per second, 30 blinks, in phase with the timer steps (each step is
    two full blinks) and phased per person by its `class_counter`.
- Low health (0x4D43A0, every 8 turns, person types with flag 8 at person table +0x30): life +0x6E regains the
  type's byte +0x1A while below the maximum +0x6C (when full, the last attacker +0xB0 is reset to 0xFF); the flag
  is set when `max_life / 4 >= life` (a quarter or less), cleared otherwise. Buildings use the same bit for
  something else (set in the mana distribution 0x41A7CA and hut growth 0x4055BA).
- Health bar (0x525450): a 7 x 26 px box (lines in palette 0x9A, 0x9D, 0x96) filled from the bottom with
  `24 × life / max_life` px in colour `[0x89C6F4]`, at 1:1 whatever the zoom. Only the player's damaged people,
  only while display flag 0x87CA1C bit 3 is on: player commands 100 (on) and 101 (off) in `process_cmd`
  (0x4AB4EA, 0x4AB4F6); which key sends them is not traced.
- Translucency (+0x36 bit 0x40, decided when the sprite is chosen, 0x4D41F2): a ghost of the player's tribe; an
  invisible person of the player's tribe, or any invisible person when the player's tribe has flag 0x8 at
  +0x93D (meaning not traced); other invisible people get +0x35 bit 0x10 and are not drawn. Near the end of
  invisibility the player's people flicker (0x4D3BA4): timer 14-23 toggles every 4 turns, 6-13 every 2, 0-5 every
  turn (`turn & mask`). Ghost army sets the same bit on its ghosts (0x50D84F).
- `frame_counter` is incremented once per pass of the loop at 0x4A4710 (`main_3`), not by the game turn: if that
  loop runs per rendered frame, the markers animate at the frame rate (unverified).
- The hypnotised halo lasts as long as the flag: 0x4D92B0 counts the timer +0xAC down every 8 turns, then
  recreates the person in its original tribe +0xAD; the flag is cleared at once if that tribe is gone.

### Terrain changes: what reacts
Every terrain-changing effect (land bridge, flatten, erosion, earthquake, volcano...) ends each cell change with the
same two calls:
- `0x44DDF0(cell, r, 1)`: the landscape around the cell is recomputed (radius r);
- `0x44F2F0(1, cell, r, −1)`: every unit and building on the (2r + 1)² cells around gets the "ground moved" flag
  (+0xC bit 2), and building cells the cell flag 0x20000. Mode 2 of the same call is the lava's "set on fire".

Land bridge example (0x50EE00, per cell 0x50ECC0, r = 2 so 5 × 5 cells): a smoke puff (7/3, 2 turns) on every
bridge cell every turn, which is the dust seen along it (there is no sparkle effect); the height step; then both
calls. Two sounds (0xAB, 0x29) when it starts.

| Unit | Reaction to "ground moved" |
|---|---|
| Person | Nothing beyond clearing the flag at the end of its turn (0x4D3CEA): the person physics follows the ground every turn, lifting people when it rises and letting them fall when it drops ("Person physics"). No push, no damage. |
| Building (0x408080) | The base becomes the average height of its footprint. Footprint cells within 200 of it (building record +0x46, every type) are pulled back towards it (0x44FDE0, rate +0x44 = 2): the building re-levels its own ground. If any footprint cell is more than 200 away, the building is destroyed (state 3, sub-state 1). Boat huts (types 13, 14) keep their base. A second destroy reason (sub-state 2) exists, not traced. |
| Boat (vehicle types 1, 2) | One forced movement pass (0x4E7FB0); no damage or destroy call in it. What a boat does on raised land is not traced. |
| Balloon (vehicle types 3, 4) | Never explodes from it. Each turn (0x4E8F00) it samples the ground ahead, takes the highest point plus a hover height of its type (another one over buildings: vehicle table 0x5A7938, stride 0x17, +0xD / +0xF) and climbs or sinks towards it at its profile's vertical rate: a bridge under it lifts it. |

## Landscape projection [exe, measured]
The landscape is drawn as a grid of vertices around the camera: ±0x6E00 render units (±55 cells), built in 220
strips (`draw_land` 0x467130, `main_landscape_mesh_generation` 0x46CB90). Render units are world units / 2, so
**256 per cell**; a vertex stores its camera-relative x, z and the cell height unscaled (0..1024).

Each vertex is then projected (0x46DE00; the same code at 0x46DC40 for the other buffer):
```text
(x', y', z') = M · (x, height, z) >> 14             # camera rotation, 14-bit fixed point (0x74A354..0x74A374)
y'  -= ((2x')² + (2z')²) · K >> 32                    # the planet bend, K at 0x87CA5C
z'  += D                                             # camera distance, 0x87CA64
screen = (x', y') · (1 << (S + 16)) / z'             # S = 0x87CA68, then a per-axis screen scale
```

| Value | Where | Default / measured | Meaning |
|---|---|---|---|
| K | render block +0x0 (0x88F004), copied to 0x87CA5C | 46 000 (0xB3B0), constant while playing | bend: drop = `4 d² K / 2³²` render units = **0.01097 × d² cells** (d in cells) |
| D | render block +0x10 | 6500 | camera 6500 render units (25.4 cells) behind the focus |
| S | render block +0x14 | 11 | perspective shift |
| M | 0x74A354 | pure rotation (column norms 16 384) | yaw follows the player; **pitch fixed at about 20.4°** (cos 15 357/16 384, sin 5709/16 384) |

- The bend is measured in view space (the camera's horizontal and depth axes after rotation), not around the
  focus point on the ground; with a 20° pitch the two are close.
- Examples: 1.1 cells of drop at 10 cells, 4.4 at 20, 33 at the 55-cell edge of the grid.
- Heights share the horizontal scale (the matrix is a pure rotation), so the highest ground (1024) is **4 cells**
  above the sea, assuming the screen scales both axes alike (per-axis shifts at 0xAFC2F4 + 0xCF8 / 0xCFC, not
  checked).
- K can change: camera transitions (0x417510, 0x41C700) blend the whole render block, K included, towards a
  target block. During normal play it stayed at 46 000, with the same distance, shift and pitch, on three levels
  (including levels 3 and 24), while rotating and zooming: it is not a per-level setting.
- Defaults set by 0x416F50 (render block) and 0x46DFB0 (draw state); `clear_land_draw_state` 0x46E700 copies the
  block into the draw state before each frame.

## Walking speed [exe]
Each person type has a movement profile (table 0x5A7B90, 26 bytes each, picked by unit+0x30) whose speed at +4 is
a named constant. When a person starts a walk (0x4D4F40, 0x4D6660):
```text
speed = SPEED + rand % max(1, SPEED / 4)          # 100 to 125 % of the constant, kept at unit+0x5F
if bloodlusted: speed *= 2
```
The speed is in world units per turn (512 per cell): the move helper 0x4E6A70 adds `speed × sin/cos(angle)`
(table 0x5DDDE8, 2048 = full turn, 16.16) to the position. A walking person moves through its velocity vector
(+0x49, +0x4D), applied by the person physics step 0x4E6D00; the walk code checks that vector against `speed²`
(0x4E9207).

| Type | Constant | Exe | File | Per turn (file) | Cells per second |
|---|---|---|---|---|---|
| brave | `BRAVE_SPEED` | 64 | 70 | 70-87 | 1.64-2.05 |
| spy | `SPY_SPEED` | 64 | 66 | 66-82 | 1.55-1.93 |
| warrior | `WARRIOR_SPEED` | 48 | 59 | 59-73 | 1.38-1.72 |
| preacher | `RELIGIOUS_SPEED` | 48 | 58 | 58-72 | 1.36-1.70 |
| shaman | `MEDICINE_MAN_SPEED` | 56 | 58 | 58-72 | 1.36-1.70 |
| firewarrior | `SUPER_WARRIOR_SPEED` | 32 | 55 | 55-68 | 1.29-1.60 |

Other speeds seen in the spell code, same unit: panic and burning runs 110 (2.6 cells per second), the angel of
death's throw 400, whirlwind 120, insect swarm 80.

Not traced: whether slopes change the walking speed (no slope factor seen in a first pass over 0x4E6D00), and the
movement profiles of wildmen, the angel of death and creatures.

## Person physics [exe]
Every person, every turn, goes through one physics step (0x4E6D00), with the numbers of its movement profile
(table 0x5A7B90, see "Walking speed"). It has three modes:

```text
blown (+0xC bit 0x80000, set by a blast push):           # 0x4E7307
    if ground(pos) < height:  in the air: scream once on take-off, gravity, air drag, spin, move
    else:                     landing (dust, fall damage), then slide downhill with friction until still
otherwise:
    ground = ground(new position)
    if ground >= height:      height = ground              # rising ground lifts it: slopes, bridges, hills
    else:                     falling (0x4E71CD): gravity, air drag, move, until the ground is reached (0x4E72E9)
```

| Rule | Value (persons: braves and specialists) |
|---|---|
| Velocity caps | 192 on each horizontal axis, 512 vertical (profile +0x8 / +0xA and +0xC / +0xE), applied every step |
| Gravity | 32 per turn off the vertical speed (profile +0x12), while above the ground |
| Air drag | 2 per turn off each horizontal axis, towards 0 (falling and blown) |
| Blown, in the air | the unit spins: its heading turns by 147 (of 2048, about 26°) per turn; a scream when it takes off (one sound for the player's shaman, another for other shamans, a third for others rising faster than 90) |
| Blown, landing | vertical speed ≤ −100: dust (7/3) and a sound; ≤ −200: `FALL_OUT_OF_WW_DAMAGE` (700, ignoring the shield), not on terrain with flag 2 |
| Blown, on the ground | a slope vector (gradient over ±76 units, 0x4EBC20) is added: the unit slides downhill; friction takes 28 per turn off each horizontal axis (profile +0x14), 72 on terrain with flags 0x3E (+0x16); the unit faces where it slides |
| Ordinary fall | ground dropping under a person (earthquake, erosion, flatten lowering) makes it fall with gravity and drag; the fall damage above is only in the blown mode, so an ordinary fall seems harmless (unverified: the grounded path also calls a landing routine, 0x4E9950, not read) |

- People do not need the terrain's "ground moved" flag: this step follows the ground every turn ("Terrain
  changes: what reacts").
- Example: an earthquake's drop of up to 600 under a person: it falls for about 6 turns (0.5 s) and lands at
  about −190, below the fall-damage threshold even in the blown mode.

## Combat [exe]
- Internal controller units (class 10):

| Type | Role |
|---|---|
| 10/8, 10/9 | fight, pre-fight |
| 10/1, 10/6 | formations (`process_formation_unit`, `add_unit_to_formation`) |
| 10/0xA | guard control |
| 10/4, 10/0xC | soul convert |

- Per person type, the constants hold `LIFE_`, `FIGHT_DAMAGE_`, `CONV_`, `SW_BLAST_DAMAGE_` and the tower detection
  radius (see "Constant table").
- A melee is two controllers in sequence. A **pre-fight** (10/9) pairs one attacker with one target: the attacker
  runs up, kicks the target away without damage, runs up again, then the pre-fight turns into a **fight** (10/8).
  A fight holds up to 6 people in a ring and is where all the health is lost. People in a pre-fight are in person
  state 29 (0x1D), in a fight in state 25 (0x19); both link to their controller by `+0x9D`.
- Ranged fighting is not part of these controllers: firewarriors (type 6, person-type flag `+0x30` bit 0x40) can
  never take a fight slot, and their attack order uses its own approach sub-states (10, 11) instead of 2, 3
  (0x51BC2E), presumably to throw fire from a distance (not traced).

### Starting a melee (the attack-order handler, 0x51A2A0)
```text
attacker within reach of the target (0x439850(person, 224)):
  target already in a fight (+0x9D is a 10/8)  → join it (below)
  target already in a pre-fight (10/9)         → attacker sub-state 8 (not traced further)
  else                                         → new pre-fight(attacker, target, mode)
mode 0: both in the open
mode 1: the attacker is in an enemy building and pulls one occupant out (0x407490)
mode 2: the target is at its building (target +0x72 is the building on its cell)   (meaning unverified)
```
Both people take state 29 (their previous state saved in `+0x7D`); the pre-fight keeps the attacker (`+0x6A`),
the target (`+0x6C`), their tribes (`+0x6E`, `+0x6F`) and, for modes 1-2, the building (`+0x68`). A pre-fight or
a fight involving the player's tribe sets 0x89D167 = 2 (probably the fight music or alert, unverified), and
0x431EA0 is called on every new controller (an alert to both tribes, unverified).

### Pre-fight (per turn 0x518630; person sub-state `+0x2D`, timer `+0x70`)
```text
ends (both freed, +0x9D = 0) when a person is gone, both are of one tribe, a person leaves state 29,
or the attacker's approach timer runs out
mode 0: attacker sub 5, target sub 4
4 target stands: stops, faces the attacker, idle (6); while idle, 1 chance in 8 per turn of a taunt
    (48, 49 or 50 at random); waits as long as the pre-fight lasts
5 attacker runs: walks (anim 5, 9 carrying wood; walking speed + rand(speed / 4)), 40-turn timer (3.3 s)
    reached when |dx| < 315 and |dz| < 315 (0.6 cell) and the target is in sub 4 → attacker sub 6
6 attacker kicks: speed 0, anim 15 (shaman 57), faces the target; 6 turns (shaman 4)
    2 turns before the end (turn 4 of 6): target sub 7; at the end: sound 13, attacker sub 8
7 target kicked away: anim 16 (one tilted frame; a shaman stays idle, 53) for 2 turns, then
    velocity 100 + rand(100) units per turn at (heading to the attacker) + 120°..240°
    (+0x2AB + rand(0x2AA) of 2048: away from the kicker, ±60°); no damage
    back on the ground (+0xE bit 0x8 clear) → sub 4
8 attacker runs again (as 5, a new 40-turn timer); reached and the target in sub 4 → the fight starts:
    a fight 10/8 at the target's place, both in state 25, placed by 0x519D10 (below)
global flag 0x89D17C bit 0x40 (an option, not traced): no kick, reaching the target starts the fight
mode 1 (pulled out of a building): both sub 3; the target is thrown out of the door (0x4044B0, 0x404420):
    speed 100, ×12/16 per turn while ≥ 40 (floor 30), from the door outwards; after 6 turns or on landing
    → target sub 4, attacker sub 5 (then as mode 0)
mode 2: both sub 1: wait up to 100 turns (8.3 s) for 0x439030, then sub 2: walk to the building door, up to
    100 turns; both in sub 2 → sub 3 as mode 1                                   (meaning unverified)
```
So the opening kick exists: a knockback of 100-199 units per turn, without damage, once per pair, before the first
exchange. Nobody is gathered: a pre-fight is always one attacker and one target.

### Fight (per turn 0x518FB0)
Controller fields: count `+0x68` (≤ 6), the two tribes `+0x69` / `+0x6A`, winner tribe `+0x6B`, the centre
person `+0x6C`, the 6 people `+0x70` (u16 ids), ring angle `+0x26`. A person's opponent is in its `+0x72`.
```text
every turn:
  drop a person who is gone, not linked any more, dead, in a state without flag 0x10 (state table 0x5A6F79),
    or more than 2048 units (4 cells) from the controller
  fewer than 2 people, or one tribe left → the fight ends: everyone freed, the remaining tribe wins
    (stats[winner][0] += 1, per-tribe counters at 0x969DC6)
  centre = the person alone on its side (0x5199F0); the controller moves onto it, the ring angle turns 180°
  every 32 turns, 1 chance in 2: ring angle ± (113 + rand(341)) of 2048 (20°-80°): the ring turns
  every 32 turns (and on creation): unsuitable ground under the controller (0x51E4B0) → move it to the nearest
    suitable cell (spiral search, 0x49A2F0); on a building cell → to its door
slots (0x51E3D0): the centre at the controller, the others on a circle of radius 180 units (0.35 cell):
  2 people: one slot at the ring angle; 3: angle, angle + 90°; 4: angle, +120°, +240°; 5th and 6th: the centre
per person (state 25):
  0 walk to its slot; within 12 units → 1
  1 ready: faces the controller (ring people); r = rand & 15
    opponent: the centre picks a random ring person, a ring person picks the centre
    opponent ready (state 25, sub 1):
      shaman:  r ≤ 6 → move 2, else move 4
      others:  r ≤ 4 → move 2, r ≤ 13 → move 4, else move 3
    opponent busy, r ≤ 3, opponent within 360 units of its slot:
      opponent in state 25 and more than 2 people → move 4 (r even) or 3 (r odd)
      opponent not in state 25 → move 4
    away from its slot → 0
  2, 3, 4 attack: speed 0, anim 15 / 13 / 25 for 6 / 7 / 7 turns (shaman: 57 / 57 / 69, 4 / 4 / 5 turns);
    both turn to face each other
    first turn: hit(target, self, one_sided = (move == 4))
    a ready target reacts: sub 5 if the move is 2 and the target is not a shaman (and flag 0x40 clear), else 6
    at the end: sound 13 (moves 2, 4) or 14 (move 3; a warrior's move 3: 0x27 against a warrior, else 0x2B
      and 0x32 on the target); → 0
  5 hit and knocked: anim 14 for 4 turns (cut short), → 7
  6 hit: anim 14 for its 7 turns (a shaman: idle 53, 7 turns), → 0
  7 knocked back: velocity 35 + rand(70) units per turn backwards; after 2 turns walks (anim 5, 9 when
    carrying wood); on landing → 0
hit(target, attacker, one_sided):                                                (0x4A39C0)
  strength(p) = max(32, FIGHT_DAMAGE_<p> × life / max_life), × BLOODLUST_DAMAGE_X when bloodlusted
  target.life -= strength(attacker)
  unless one_sided: attacker.life -= strength(target)                  (both hit at once)
  each one at life ≤ 0 dies (0x4F3190, killer = the other); stats[its tribe][1] += 1
join(person, fight)                                                    (0x51DDC0, 0x51DCC0)
  only a person of one of the fight's two tribes, never a firewarrior
  its tribe already has 3 or more there → replaces one of its own tribe with a lower rank
    (person table +0x17: brave, preacher, spy 1; warrior, shaman 2; firewarrior 3)
  else the first free slot; state 25
  then split (0x51DF90): both tribes with 2 or more people → the last of each leaves for a new 1-to-1 fight
    at the same place
```
- The fight is a series of exchanges, not a kick/guard toggle: the attacker plays one of three attack moves and
  the target one of two hit reactions. Move 2 (anim 15, also the pre-fight kick) knocks a non-shaman target back;
  moves 2 and 3 trade blows (both lose health); move 4 lands without a return blow. A shaman is never knocked
  back in a fight and never uses move 3.
- **A fight is always one against several** (up to 5): joining and splitting keep one side at one person, who
  stands in the centre and takes every ring person's blows.
- Animations [exe]: `0x4D3FF0(person, action)` reads an object id from 0x5A6D50 `[action × 9 + type]`, and
  object `o` is the pair at 0x5A6858 + 4·o: (`VSTART` index × 8 → `+0x33`, draw class → `+0x3A`). So the
  `VSTART-0.ANI` animation is the first value / 8 (checked on idle 6/53, walk 5/77, pray 8, chop 11, flung 19/61).
  Types 2-6 share the body and differ by draw class (14-18, the outfit).

| Use | Action | Followers (types 2-6) | Shaman | Turns (frames from `VFRA-0.ANI`) |
|---|---|---|---|---|
| move 2 and the pre-fight kick | 10 | 15 | 57 staff strike | 6 (shaman 4) |
| move 3 (trade) | 8 | 13 | 57 staff strike | 7 (shaman 4) |
| move 4 (one-sided) | 16 | 25 kick | 69 kick | 7 (shaman 5) |
| hit (fight) | 9 | 14 | 53 idle | 4 (knocked), else 7 |
| kicked away (pre-fight) | 11 | 16 tilted (1 frame, held 2 turns) | 53 idle | 2, then the flight |
| taunt while waiting (0x4D4DA0, table 0x5A6ECA) | | 48, 49 or 50 | 53 idle | its frames (8, 9, 7) |
| running up, back to the slot | 1 / 5 | 5 walk / 9 walk with wood | 77 walk | |

- Timings: one turn per frame for every draw class used (0x5A6AFB[class] = 0; 1 for the firewarrior's, 2 turns
  per frame), so an attack is `frames` turns, counted on the direction-0 loop (`+0x33` = anim × 8; anim 15 has
  6 frames there, 7 in the other directions): a brave's exchange is 6-7 turns (0.5-0.6 s), a shaman's 4-5. An
  idle person in sub 1 attacks again at once if its opponent is ready, so the rhythm is about one blow per
  0.6 s per pair (estimate, not measured).

Strength per type (exe default / `constant.dat`); it falls with the attacker's health, never under 32:

| Type | `LIFE_` | `FIGHT_DAMAGE_` | Full-health blows to kill a full brave (file, estimate) |
|---|---|---|---|
| brave | 1400 / 1000 | 60 / 60 | about 21 (60 falling to 32 as the attacker weakens) |
| warrior | 1800 / 1800 | 180 / 360 | 3 |
| preacher | 1400 / 1100 | 60 / 60 | about 21 |
| spy | 1400 / 600 | 60 / 60 | about 21 |
| firewarrior | 1200 / 700 | 180 / 60 | never in a fight |
| shaman | 900 / 2000 | 60 / 60 | about 21 |

- The file spells the shaman's constant `FIGHT_DAMAGE_SHAMAN`, the exe table `FIGHT_DAMAGE_SHAMEN`; if
  `read_levels_const` matches names exactly, the file's line is ignored (both are 60: no effect).

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
- units.md `Chopping`: the original is 20 turns per load, 1.67 s; our `CHOP_TICKS` is the same 20 ticks. Picking
  up from a pile is 3 turns (0.25 s); ours is instant on arrival.
- trees.md "Growth": the original regrows 2 wood per 16 turns, one piece in 800 turns (67 s); ours regrow one
  piece every 800 ticks, the same time, in one step.
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
  - one `PrayTime` unit is 4 turns (1/3 s), not one of our ticks (1/12 s), so our gauges fill 4× too fast;
    `PrayTime` × 4 ticks would match;
  - the original's speed is `C² / (C − n + 1)²`, much harsher than our `n² / C²`: on a totem of 8, 6 praying is
    11 % (ours 56 %) and 4 praying is 3 % (ours 25 %); `game_core::gauge::gain` can use it, still in integers;
  - the linear drain at the full-speed rate matches our guess;
  - the original has one gauge per trigger, driven by the tribe with the most prayers, where ours has one per
    tribe.

- Reincarnation sites (game-client `sites.rs`, game-core `site.rs`): ours put 8 stones on a round ring of 1.3
  cells, not turned, and flatten radius 3 (blend 4). The original: the octagon above (3 cells, 2.8 on the
  diagonals), on cell centres, each stone turned to face the centre, object 30; the ground levelled to the site
  height over a 5-cell disc, in about 21 turns.
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

- spells.md Blast and units.md "Physics" (`blast::effect`, `Unit::fling`, `physics::step`): ours flings each person
  once; the original pushes again on each of the 3 turns while the person is within 2.5 cells (cumulative), scales
  the horizontal push by the slope, pushes allies only on the last turn (at most 5), never pushes angels of death
  or units inside something. Ours has no velocity caps, air drag, spin, ground sliding or friction yet (192 / 512
  caps, drag 2, a 26° turn per turn in the air, friction 28 or 72 per turn, downhill slide), no dust below −100 and no fall damage. Our speeds are per
  tick (10 per second) where the original's are per turn (12 per second): the same numbers fly about 17 % slower
  and lower in time. The original does not kill a unit landing in the sea outright (no fall damage on water
  cells; what follows is not traced). Ordinary falls (ground dropping under a person) use the same gravity and
  drag but no fall damage ("Person physics").
- Terrain changes (spells.md, buildings.md, TODO "Destruction of buildings on uneven/flooded ground"): in the
  original a building re-levels its footprint towards its average height and is destroyed when a footprint cell is
  more than 200 away; people ride the ground (no push, no damage); balloons keep a hover height over the highest
  ground ahead. The land bridge raises smoke on its cells every turn; ours has no such dust.
- units.md `Walking`: ours walks at 64 units per tick, 10 ticks per second, 1.25 cells per second for every kind.
  The original brave walks 1.64-2.05 cells per second (`BRAVE_SPEED` 70 per turn, + up to 25 % per walk), and
  each type has its own speed (firewarriors the slowest). About 82-102 of our units per tick would match a brave.
  Our slope factor (uphill and downhill) is our own design; the original's is not traced.
- terrain.md "Planet illusion" and ui-and-editor.md view tuning: the original bend is k = 0.011 per cell² (our
  default `POP3_CURVATURE` 0.008 is too gentle, the earlier 0.012 was close), measured in camera space. The
  original height scale is 1/256 cell per unit (max 1024 = 4 cells), not "about 2 cells"; our 1/384 (2.7 cells)
  is flatter than the original. The original camera pitch is fixed at about 20°, 25 cells behind the focus.
- units.md `statuses` / unit drawing: our drawing has no status marker yet. The original shows low health
  (sparkles, `hfx0-0.dat` 1489-1498), shield (three tribe-coloured rings, 1538-1552), bloodlust (red bubbles,
  1478-1487) and hypnosis (a halo, 1553-1563); invisible and ghost people of the player are drawn translucent,
  invisible enemies not at all; own damaged people can show a health bar; the selection arrow is hfx 53
  ("Status markers over people").
- coordinators.md models 8 and 9 and TODO "Combat": the original's melee is a pre-fight coordinator (one attacker,
  one target: approach within 40 turns, a damage-free kick knocking the target 100-199 units per turn away, a
  second approach) handing over to a fight coordinator (a ring of radius 180 around one centre person, up to 6,
  one side always alone, re-split when both sides have 2). Damage is per exchange, mutual except for move 4, and
  scales with the hitter's health ("Combat").
- TODO "Warriors: about 3-4x a brave's combat strength": with `constant.dat` a warrior hits 6 times as hard
  (360 against 60) with 1.8 times the life (1800 against 1000); the exe defaults give 3 times (180).
- TODO "Firewarriors throw fire": firewarriors never take a melee slot; their attack is a separate ranged path.

## Open questions
- Where `MANA_F_<PERSON>`, `MANA_F_HOUSED/WORKING/TRAINING` and `MANA_F_HUT_LEVEL_1..3` are applied. Only the max
  of the three activity percentages is stored, and its readers are not traced.
- What "busy" means (unit+0xE bit 7, unit+0xA7), and the preacher's extra check.
- The controller types of tribe+0xC1F (2 is human; 1 gets full mana).
- Spell order and share inside the distribution loop.
- Combat: a live check of the exchange rhythm (`fightwatch.py`), whether a taunt (48-50) plays to its end (0x4D4DA0
  resets a non-idle person to idle the next turn; the per-person `+0x7E` it sets may hold it),
  what option flag 0x89D17C bit 0x40 is (no kick, no knockback),
  the meaning of pre-fight mode 2, the ground test of a fight's place (0x51E4B0), the attacker's sub-state 8 when
  its target is already in a pre-fight, and the firewarriors' ranged attack (attack-order sub-states 10, 11).
- What the per-building stored mana is.
- What sets `turns_per_sec` to 14, and whether game options or multiplayer change it.
- Which field holds the gather sub-state: +0xA8 stays 0 while chopping, so the handler's state byte is elsewhere.
- What `TREE_DORMANT_TIME` does (maybe the time before a cut tree regrows or a new tree appears).
- Huts: what the growth placement check tests (footprint, population?), the every-128-turns branch when it fails, and the per-tribe base
  capacity (tribe+0x921).
- Prayer: the vault (library trigger) branch, what the "contest" countdown at trigger+0x6F does when several tribes
  pray at once, and whether the gauge resets after the trigger fires when `NumOccurences` allows more.

- Physics: what the grounded landing routine 0x4E9950 does, so whether ordinary falls ever hurt.
- Blast: what happens to a person landing in water, and measured flights (position, velocity and life per turn of
  a blown brave) to check the estimates of "Blast push and flight".
- Walking: whether slopes change the speed, and the speeds of wildmen, the angel of death and creatures.
- Projection: the per-axis screen scale (0xAFC2F4 + 0xCF8 / 0xCFC), which decides the on-screen height ratio, and
  which views (zoom levels, cut scenes) change the render block.
- Sites: which code creates the pillars at level load, and whether the levelling forces every cell of its disc.
- Spells: burning trees (scenery state 5), the partial building explosion of the earthquake, whether ghosts
  expire, what starts the landscape-shaping mode, what a person does when the ground under it floods, the
  building's second destroy reason (sub-state 2), and boats on raised land.
- Status markers: whether `frame_counter` 0x897981 advances per rendered frame or per turn, the anchor of the
  screen position the height offsets are taken from, what the colour tables (0x87D000 + n × 0x1000) do to an alpha
  sprite, the key behind commands 100/101 (health bars), tribe flag +0x93D bit 0x8 (sees invisible people?), and
  what +0x3C (a colour table index −15..15, 0x468B50) tints a person for.

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
| 0x51A2A0 | a person's attack order: reach, then join a fight or start a pre-fight |
| 0x51E150 | "start_pre_fight(attacker, target, mode)" |
| 0x518630 | "pre_fight_turn": approach, kick, knockback, then the fight |
| 0x51DE60 | "start_fight(a, b)" from a pre-fight |
| 0x518FB0 | "fight_turn": slots, opponents, moves, hit reactions |
| 0x519A70 | fight roster check (drops gone, dead, unlinked, > 4 cells), winner |
| 0x5199F0 | picks the fight's centre (the side with one person) |
| 0x519D10 | moves a fight to suitable ground, every 32 turns |
| 0x51E3D0 | ring slot position (radius 180) |
| 0x51DDC0, 0x51DCC0 | join a fight; slot choice and rank replacement |
| 0x51DF90 | split a fight into a new 1-to-1 when both sides have 2 or more |
| 0x4A39C0 | "melee_hit(target, attacker, one_sided)" |
| 0x41B550 | per-tribe combat counters at 0x969DC6 (`[tribe × 12 + k]`: 0 fights won, 1 people killed) |
| 0x4D3FF0 | a person's action animation (`[action × 9 + type]` at 0x5A6D50) |
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
| 0x4E6D00 | person physics, every turn: follow the ground, falling (0x4E71CD), blown mode with landing and sliding (0x4E7307) |
| 0x4A7D80 | reincarnation pillar init: snap to the cell centre, angle from the site, rise (0x4A7EB0) |
| 0x50C690 | "move rs pillar" (7/7): replace a slot's pillar |
| 0x467130, 0x46CB90 | `draw_land`, `main_landscape_mesh_generation`: the vertex grid |
| 0x46DE00 | per-vertex projection with the planet bend |
| 0x416F50, 0x46E700 | render block defaults (K 46 000), copy into the draw state |
| 0x4D4F40, 0x4D6660 | walk speed: `SPEED + rand % (SPEED / 4)`, doubled when bloodlusted, to unit+0x5F |
| 0x4E6A70 | move a position by length and angle |
| 0x50EE00, 0x50ECC0 | land bridge: per turn, per cell (smoke, height step, refresh, notify) |
| 0x44DDF0 | recompute the landscape around a cell after a height change |
| 0x44F2F0 | notify units and buildings around a cell: mode 1 "ground moved", mode 2 "set on fire" |
| 0x408080 | building foundation check: re-level the footprint or collapse |
| 0x4E7FB0, 0x4E8F00 | boat and balloon movement passes (balloon hover height) |
| 0x4E93F0 | add a push to a velocity, scaled by the slope along it |
| 0x4EBC20 | slope vector at a position (ground sliding) |
| 0x4503F0 | horizontal distance between two positions (wrapping, square root) |
| 0x50C780, 0x50C840 | "prepare rs land" (7/8): init (site height at tribe+0x915), levelling per turn |
| 0x41C140, 0x41C520 | landscape-shaping mode start and end: sites rebuilt at the shamans, pillars from 0x5A9F10 |
| 0x468E1C-0x4694F2 | in `draw_polygons`: health bar, low health, shield, bloodlust, hypnosis and selection markers |
| 0x525450 | health bar of a person |
| 0x4D43A0 | person regeneration and the low-health flag, every 8 turns |
| 0x4D41F2 | translucent / hidden flags for ghosts and invisible people |
| 0x4D92B0 | "hypnosis_tick": every 8 turns counts +0xAC down, then restores the person to tribe +0xAD |
| 0x516270 | select the colour table for the next sprite draw (0x974560 = none) |

Timing globals: turn counter 0x89D188 (u32), `turns_per_sec` 0x89D161 (u8, copied from the option at 0x98F712),
`next_turn_time` 0x5CD92C, `step_ms` 0x5CD930 (83 in the exe image), `extra_turns` 0x895DAC (i8). Under Wine the
exe is mapped at its image base 0x400000, so these addresses can be read as they are.

### Unit record [ghidra]
- `unit_struct`, 0xB3 bytes. Pool of 2000 (`unit_array` 0x8E0428), two free lists split at index 640
  (`alloc_unit`, `move_unit_to_free_list`).
- Fight links: controller id +0x9D (pre-fight or fight), state saved on entering one +0x7D, fight opponent +0x72.
- Fields: `unit_class` +0x2A, `unit_type` +0x2B, `state` +0x2C, `state_2` +0x2D, `tribe_index` +0x2F, `unit_obj`
  (model or sprite) +0x33, `pos` +0x3D, building stored value +0x98, last attacker's tribe +0xB0.
- Wood fields: a person's state timer +0x70 and carried wood +0x78, a tree's wood left +0x84, a shape's wood +0x96,
  and +0xA4 for the building types that hold wood. The gather handler switches on a byte read as +0xA8, but that field stays 0 while
  chopping (measured), so the offset is unconfirmed.
- Hut fields: `grow` +0xA0, `breed` +0xA4 (the same offset as wood in other building types), people inside +0xA6,
  "brave ready" flag bit 14 of the word at +0x9C. `class_counter` +0x2E gives each unit its 4- or 16-turn phase:
  +1 per turn for every unit whose state is not 0, just before its handler (0x4EC8B3).
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
  bit 0x800, low health +0x14 bit 0x1000; the timers count down every 8 turns. On fire: +0xC bit 0x8 leads to
  state 31. Life +0x6E, maximum life +0x6C. Drawing: translucent +0x36 bit 0x40, hidden +0x35 bit 0x10,
  selected +0x7A bit 0x80.
- Movement: profile index +0x30 (into 0x5A7B90), walk speed +0x5F, heading +0x5D, velocity +0x49 (x),
  +0x4B (vertical), +0x4D (z).
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
| +0x911, +0x915 | reincarnation site position (x, z) and height |
| +0xA0D + 2·slot | the site's 8 pillar unit ids |
| +0xC1F | controller type: 2 human, 1 full mana, others unknown |
| +0xC22 | tribe number |

Other globals: the per-tribe level header `struct_56B` (0x38 bytes per tribe at 0x96070A) holds the spells:
learned bitmask +0x00, blocked bitmask +0x10, and per spell a byte at +0x14 + spell (low nibble charges, high
nibble gifted one-shots; `set_struct_56B_array_spell_val`); the reservoir short-delay flag is bit 3
of 0x89C661.

### Constant table [exe]
- At 0x5AA60F, 340 records of 31 bytes: `char name[25]`, `u8 size` (1, 2 or 4), `u8 flags`, `u32 value_ptr`.
  Read by `read_levels_const`.
- Flag bit 0, percent: stored as `value * 256 / 100` (0x49C4C1). The exe defaults are already in that form
  (512 = 200 %).
- Flag bit 1 (`WALK_ALT_DIFF2` only): the value is also copied into a per-type table.
- After loading, `max(MANA_F_HOUSED, MANA_F_TRAINING, MANA_F_WORKING)` is stored at 0x5AA448.
- Per-type records the constants point into:

| Table | Base | Stride | Fields |
|---|---|---|---|
| person types | 0x5A7060 | 0x32 | `MANA_F_` +0xE, `LIFE_` +0x10, `WOOD_` +0x14, `FIGHT_DAMAGE_` +0x1B, chop time (u8, not a named constant) +0x1D, `CONV_` +0x1F, tower detection radius +0x22, `SW_BLAST_DAMAGE_` +0x2C |
| building types | 0x5A7228 | 0x4C | `WOOD_` +0x1A, next size +0x34 (u8), growth target +0x36 (2400, not a named constant), mana factor +0x38, `MAX_POP_VALUE__HUT_` +0x3A, `HUT_SPROG_TIME` +0x3C, flags +0x48 (bit 10: breeds; bit 6: holds wood), foundation re-level rate +0x44 (2) and tolerance +0x46 (200) |
| scenery types | 0x5A79B0 | 0x18 | object +0x0A (12: 30), `TREE_WOOD_VALUE` +0x4, `TREE_WOOD_GROW` +0x6, `TREE_DORMANT_TIME` +0x8, flags +0x14 |
| movement profiles | 0x5A7B90 | 0x1A | speed +0x4 (`<TYPE>_SPEED`: brave profile 2, warrior 14, preacher 15, spy 16, firewarrior 17, shaman 18), velocity caps +0x8 / +0xA and +0xC / +0xE (horizontal / vertical: 192 / 512 for persons), `WALK_ALT_DIFF2` +0x10 (copied for flagged profiles), gravity +0x12 (32), ground friction +0x14 (28) / +0x16 (72), flags +0x18 |
| spells | 0x5A80D0 | 0x3E | kind +0x00 (1 normal, 2 hidden), cost (u32) +0x4, id +0x8, flags +0x1A (bit 0 castable) and +0x1B (0x40 homing), `SP_W_RANGE_` +0x1E, mode-0x20 range +0x22, shot without shaman +0x26, shot from shaman +0x27, effects +0x28..+0x2C, `SP_1_OFF_MAX_` +0x2D, mode-0x20 charges (`LSME_1_OFF_MAX_`) +0x2E, deflectable +0x2F, sound +0x30, enemy-cast alert +0x35/+0x36, `OPT_S` +0x3A |

- `LAND_BRIDGE_MAX_CHANGE`: record size 4, flags 0, value at 0x5AA518, exe default 256 (the file's is 256 too,
  spells.md "Effect constants").
- The shipped `constant.dat` wins over the exe defaults, e.g. `SPELL_BLAST` 18 000 in the exe and 10 000 in the
  file, `LIFE_BRAVE` 1400 and 1000.
- Not in the table: 0x5AA44C (default 3) is masked with the turn number like `MANA_UPDATE_FREQ` should be, so it is
  probably that constant (unverified). The `MANA_IDLE_*`/`MANA_BUSY_*` values are exe-only too.
