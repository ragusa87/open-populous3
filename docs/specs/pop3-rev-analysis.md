# Original engine internals (`pop3-rev` analysis)

What the reverse engineering of the original executable says about game logic, spells, combat, mana, wood, huts
and prayer, laid out for implementation: the rules first, then what they change for us, then the addresses and
layouts as reference.

Sources:
- [exe]: disassembly of `D3DPOPTB.EXE` (French release, 1998, Direct3D build) with `objdump`.
- [ghidra]: the `pop3-rev` Ghidra project. Function names in `code style` are its names, names in quotes are our
  proposed renames.
- [measured]: live memory reads of the running game, on `D3DPOPTB.EXE` or on `POPTB.EXE` (software build of the
  same release, same game code).

Every address in this file is a `D3DPOPTB.EXE` address. `POPTB.EXE` runs the same rules at other addresses
(`pop3-rev/docs/poptb_map.tsv`: game state shifted by −0x17A68, per-type tables by −0x7220, functions one by one),
so a live measurement may run on it with the addresses translated.

## Takeaways for the implementation
- **12 turns per second** [exe, measured]: a fixed timestep of 83 ms. Every "turns" value below converts at that
  rate. Our simulation runs at the same rate: one tick is one turn (`game_core::time`).
- **Lockstep is the original design** [ghidra]. Orders are queued tribe commands applied on the network turn, with
  one global pseudo-random generator. Our `Command` + `map::Lcg` model (multiplayer.md) matches it.
- **Everything is a unit** [ghidra]. People, buildings, scenery, shots, spells and invisible controllers share one
  record and one fixed pool, each class with its own per-turn handler. A spell is a unit that spawns effect units,
  and the effects do the work. Fights, formations and guard posts are controller units that people link to, not
  person states.
- **An order is a shared "command group", not a per-person copy** [exe]: selecting several units and giving one
  order makes them all point at one 10-byte record (kind, flags, target, a reference count) in a fixed array of
  800; a person just resolves and runs whichever group it currently points to. See "Command groups".
- **Mana is a per-tribe pipeline run every turn** [exe]: income every 16 turns, then spell charging first, the
  surplus banked, a delayed reservoir for gifts. See "Mana rules".
- **Income is busy/idle per person**, from constants that `constant.dat` cannot change. It is not the
  `MANA_F_<PERSON>` × activity % model of huts-and-training.md. See "Impact on our specs".
- **Chopping is a countdown, then one transfer** [exe, measured]: 20 turns next to the tree, then a whole load
  (100 wood, one of our pieces) moves at once. A wood pile takes 3 turns. See "Wood rules".
- **A hut has two independent counters** [exe]: breeding (a brave at the door, slower as the tribe grows) and
  growth (the next size, only with people inside, independent of population). See "Huts: breeding and growth".
- **Training deletes the trainee and creates a new person** [exe]: a training building (not a house) stores mana
  taken from each income (at most `cost / 32` per pass, before the spells), and when the store reaches the cost
  the trainee is replaced by a new unit of the building's kind. See "Training buildings".
- **Training buildings take up to 5 at once** [exe]: queued people (a wall-hugging queue from the door) go in
  one every 16 turns while fewer than 5 are inside, and train together for `n × cost`. See "The queue at the door".
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
- **A firewarrior throws two homing fire bolts, one hits** [exe]: 1 cell per turn, 100 to every enemy in the
  impact cell plus `SW_BLAST_DAMAGE` × a per-type % to the target, every 25-28 turns, at 9.2-17.2 cells by altitude.
  See "Firewarrior fire".
- **Preaching rerolls a 1-in-`PREACHEE_CONV_CHANCE` (5) chance every `PREACHEE_CONV_FREQ` (500 turns, 41.7 s)**
  [exe]: on success the preachee is destroyed and a person of its own type, in the preacher's tribe, takes its
  place (capped by the preacher's tribe population); instant for a wildman or other "no breeding" tribe. The
  preacher's own look scan (0x51E7B0) and preach order (0x43C7A0) are now traced, but neither visibly sets a
  target's state to 3 (the state this reroll runs in); the likely missing link is a flag bit on the attack
  order itself, not yet pinned to its setter. See "Preacher conversion".
- **A vehicle's life is also its abandon timer** [exe]: 5000, −1 per turn, refilled every turn while it moves or
  carries anyone; blasts (50 per turn) and bolts (100) only sink an empty, still one. A boat whose cell turns to
  land is emptied, keels over and explodes 13-24 s later. Both are 3D objects 143 / 144. See "Vehicles".
- **A group order is one shared target, not spread units** [exe]: every unit in a selected group gets the
  identical point or target id; the only thing that can space units out is the separate "formation" system,
  wired to the move order (kind 3) but not the attack order (kinds 19/21). See "Order kinds and formations".
- **"Idle" is not one state** [exe]: a person cycles a short state-1 hand-off into state 10 ("carrying out an
  order, or nothing"), which does nothing at all - no wandering, no autonomous wood-gathering was found - when
  no command group is assigned. Only wildmen are confirmed to cycle idle-fidget animations (state 8 -> 5) on
  their own; this was not found for ordinary followers. See "Idle behaviour".
- **Lava flow spreads downhill, one cell at a time, for a fixed 16 s** [exe]: a per-cell precomputed downhill
  direction decides where each "mature" lava cell pushes next; it never kills outright, only sets people on
  fire (state 31) every turn a cell is hot; after growth it fades for 16 more turns, then becomes ordinary
  building-ash scorch. The direction field itself is a general terrain property (map-cell +0xE), frozen at the
  last height edit rather than read live, shared with ordinary terrain-shaping and level load, not lava-only.
  See "Volcano lava flow".
- **Wildmen seek a tree first, the shore otherwise, and never respawn** [exe]: unable to take orders, they search
  for a nearby tree/plant to idle beside (state 5, "eating") and, failing that, deliberately walk to a shore cell
  to idle there instead (states 6/16, "drinking") - confirmed from the exe's own scenery-type and coastal-shape
  tables, not a random destination. No hunger/thirst/age stat exists even so, and the only way to create a
  wildman is a one-shot level-placement routine, never a per-turn spawn. They are also immune to ordinary units'
  auto-attack (tribeless, tribe 0xFF). Idling at the shore genuinely drowns them sometimes (the shore search is
  only cell-grained, drowning is checked pixel-grained, every turn, no timer), and whatever kills a wildman, its
  death always plays the rare "climbing + spinning" spirit-rise variant everyone else only gets by drowning too -
  never the calm rising-ghost pose. See "Wildmen", "Death and the spirit rise".
- **Land colour's brightness is a precomputed per-cell byte fed by a sun vector that, live-confirmed, is a flat
  constant in normal play - not the level's sunlight block; its row is pure height + blend, no sunlight** [exe,
  measured]: `bigf0-X.dat`'s row comes from `height + 75 (+75 again if the cell isn't perfectly flat)`, clamped
  to 1022, jittered by `disp0-X.dat` noise scaled by a fixed altitude-dependent sensitivity curve (320 on
  low/flat ground, up to 1024 high up) - no per-theme constant; the column comes from a map-cell byte (+0xD)
  precomputed from each cell's own height gradient dotted against a sun vector, recomputed live on every height
  edit - but that vector is `sunX=sunZ=sunBase=147` always (`sunlight_init_default` 0x401040's hardcoded
  constant), NOT derived from the level's `ShadeStart`/`ShadeRange`/`Inclination`, confirmed live on two levels
  with different on-disk `Inclination`. See "Terrain land colour".
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
0. Training buildings take their share first: up to half of `incoming`, at most `cost / 32` each (see
   "Training buildings"); those not training give their store back, 100 per turn.
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
  tribe's "free spells" flag is set (confirmed this session at the cast-cost site, `init_unit_type_11`, 0x4C16D6:
  `test tribe+0x93D,8`, skips the `add_mana` call when set; a second, unread gate on the same bit sits at
  0x4C2947 against the per-spell constant table). Costs are `SPELL_<X>` (spells.md "Original balance"). The flag
  is set by a player command (id 0x41 in `process_tribe_cmd`, `or tribe+0x93D,8`) - presumably a cheat/debug
  command, never cleared by any code found, not something our implementation needs.
- A tribe command adds a signed amount, and a PopScript opcode adds a script expression (ai-scripts.md).
- Training buildings hold the mana they collect (+0x98, see "Training buildings"); 0x41AE60 sums it over the
  tribe's training buildings (used by the shaman death loss), and a controller type 1 tribe gets it back when a
  training ends.
- The bank (+0x94D) is written in the distribution only on the controller type 1 path (0x41AD1C); the other tribes'
  path ends with `incoming = 0` (0x41AD57) after the spells and training buildings. How their surplus reaches the
  bank is not traced.

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

## Training buildings [exe]
Training buildings (building types 5-9, type flags +0x48 bit 0; not houses, types 1-3) train the people inside
into the type's output person (+0x31):

| Building type | Output | Slots (+0x20) | Cost constant (human / computer) |
|---|---|---|---|
| 5 temple | 4 preacher | 5 | `HUMAN_TRAIN_MANA_PREACH` 3500 / `CP_` 1000 |
| 6 spy training | 5 spy | 5 | `HUMAN_TRAIN_MANA_SPY` 4000 / 1000 |
| 7 warrior training | 3 warrior | 5 | `HUMAN_TRAIN_MANA_WARR` 3500 / 1000 |
| 8 firewarrior training | 6 firewarrior | 5 | `HUMAN_TRAIN_MANA_SWARR` 4000 / 1000 |
| 9 reconversion | 2 brave | 1 | none (person type 2 has no cost: free) |

Trainees queue outside and go in one by one, at least 16 turns apart, until the 5 slots are full: up to 5
people train together and come out together (see "The queue at the door"). This contradicts the [player] note
"one at a time" that our spec follows; settle it in the game.

Values are the file's; the exe defaults of `HUMAN_`/`CP_TRAIN_MANA_*` and `TRAIN_MANA_BAND_*` are all 0.

### The trainee is replaced, not changed
Training never changes a person's type. When the training building has the mana, every occupant that is not
already of the output type is deleted (`remove_person_from_hut` 0x407490, then 0x436CA0, then `delete_unit`
0x4EF180), and new persons are allocated (`alloc_unit(1, type, tribe, door)`, 0x406190): new unit ids, new
records with the type's full life, at the door. So nothing of the old person (life, statuses, kills) carries
over, except:
- its queued orders: if an occupant has queued command groups (+0x8B, 0x43B120), they are given to the new units
  (0x436D00); otherwise each new unit gets a fresh group with one move order (kind 3) to a point one cell
  (0x200) in front of the door (0x436C20, 0x438730);
- the ghost flag: if any occupant is a ghost (+0x10 bit 0x800), every new unit is a ghost (and translucent for the
  player's tribe, +0x36 bit 0x40).
If the unit pool is full before all are allocated, the ones made are deleted and nothing happens (retried later).

### Cost
```text
points = sum over occupants whose type != output of CONV_<type>     # person table +0x1F
n      = points / CONV_<output> if points >= CONV_<output> else 0    # people made of the output type
extra  = points - n * CONV_<output>                                  # braves made from the leftover points
cost   = TRAIN_MANA[output] * n * BAND%[count[output]] / 100         # min(cost, 0xFFFF)
TRAIN_MANA = HUMAN_TRAIN_MANA_* if tribe controller (+0xC1F) == 2 else CP_TRAIN_MANA_*
count[output] = the tribe's people of the output type (tribe +0xA27, rebuilt each turn)
```
- `0x41B0C0` "train_cost(building, &cost, type, n)": returns `stored >= cost`.
- Bands (`TRAIN_MANA_BAND_*`, percent constants): 0-3 → 100 %, 4-7 → 125 %, 8-11 → 150 %, 12-15 → 175 %,
  16-20 → 200 %, 21+ → 250 %, by the number of that specialist the tribe already has.
- File `CONV_<PERSON>` are all 1, so `n` is the number of occupants not already of the output type and `extra`
  is 0: one brave in, one specialist out. With the exe defaults (brave 1, warrior / preacher / spy 3, firewarrior
  6) it would take 3 braves for a warrior and 6 for a firewarrior.
- With one trainee inside, `n` is 1: a human's warrior costs 3500 while the tribe has 0-3 warriors, 4375 with 4-7.
- It is recomputed when someone enters (0x407150) and every 16 turns (`class_counter & 0xF == 0`) while training.

### Mana flows into the building, then is spent at once
The training building has its own store (`stored` u16 +0x98) and a target (`cost` u16 +0x96). Training is on
while bit 0x80 of +0x9C is set: set on entry when `n > 0`, cleared by `remove_person_from_hut` (0x4075F1) and on
entry when `n == 0` (0x4073FC).
```text
distribution (every turn, per tribe, 0x41A590), before the spells:
    for each training building (type flag bit 0) of the tribe:
        if not training: give min(stored, 100) back to incoming          # 100 per turn, until empty
        else: list it
    share = (incoming / 2) / listed                                       # half of incoming, split evenly
    per listed building: give = min(share, cost / 32, cost - stored)      # negative when stored > cost
                         stored += give; incoming -= give
                         if give <= 32: +0x14 bit 0x1000 (probably "short of mana" for the panel)
    ... spells charged from incoming (see Distribution) ...
    if no spell needs mana: second pass, over the training buildings with stored < cost,
        share = incoming / count, give = min(share, cost / 32, cost - stored)
    if incoming < 0 (casts): training buildings give back up to -share each

training building, every turn (0x405B80):
    if training and stored >= cost:
        stored = cost; make n specialists and `extra` braves (see above); delete the converted occupants
        unless an occupant is a ghost:
            stored = 0 (a controller type 1 tribe gets it back: add_mana(stored), 0x406218)
        cost = 0; +0x9D bit 0x4; +0x7E = turn
```
- So the mana is taken along the training, from each income, never from the bank. Ghost trainees still wait
  for a full store but do not empty it.
- Cancelling (everyone leaves): the stored mana goes back to the tribe at 100 per turn; nothing is lost.
- Time: incoming is non-zero only on income turns (every 16 turns) and reservoir turns, and a training building
  gets at most `cost / 32` per pass. So at least 32 income turns, 512 turns (43 s), while spells are still
  charging; 16 income turns, 256 turns (21 s), when every spell is full and income is large enough. A slower income
  (half of it shared between training buildings) stretches it. Estimate, not measured.
- Worked example, estimate: a human with 432 income per 16 turns and one warrior training building at work (3500):
  the first pass gives min(216, 109) = 109 per income, so 33 incomes, 528 turns (44 s), while spells charge.
- `CONV_TIME_TEMPLE/SPY/WARRIOR/SUPER/RECONV` (building table +0x2C, exe 32) have no reader found. The file's
  `CONV_TEMPLE`, `CONV_SPY`, `CONV_WARRIOR`, `CONV_SUPER` (4000) and `CONV_RECONV` (128) match no exe constant
  name, so they are probably ignored.
- A shaman entering a full building pushes one occupant out (0x4071AD).

### The queue at the door
The "go into a building" order (person state 0xA or 0xE, current command kind 8 on the building) runs 0x434610,
sub-state +0x2D. Buildings with type flag +0x48 bit 0x800 have a queue: the training buildings (types 5-9) and the
boat and airship huts (13-16), not the houses. The queue is a linked list: head person id at building +0xA2, next
at person +0x85, the person's rank at +0xAA, person +0x14 bit 0x20 while listed.
```text
sub 0 (start): target = building (+0x89), timeout +0x70 = 256
    already listed (+0x14 bit 0x20)     -> sub 3 at its rank
    standing at the building, queue empty, inside (+0xA6) < slots and entering (+0xAD) < slots -> sub 4
    else walk to the door point (0x4044B0) -> sub 1
sub 1 (walking), every 2 turns:
    queue building, within 0x538 (2.6 cells, per axis) of the door point,
    and (queue not empty or inside >= slots or entering >= slots) -> sub 2
    within 0x70 of the door point -> sub 4
sub 2: append at the end (0x409B10), rank = last rank + 1, walk to the spot of that rank -> sub 3
sub 3 (queued, +0xC bit 0x200000):
    if inside < slots and building gate +0xAB == 0 and I am the head and not walking:
        +0xAB = 16; -> sub 6 (walk to the door point; within 0x70 -> sub 4)
    moving up: when the countdown +0xA9 reaches 0, the one behind gets +0xA9 = 2, my rank = rank of the
        one ahead + 1, walk to that spot: the queue moves up as a ripple, 2 turns per rank
    at my spot, every 16 turns: if I am already of the output type (+0x31) and the one behind is not,
        swap places (so people to convert pass those who would come out unchanged)
    not at the spot: re-walk; while walking +0x70 counts down, at 0 restart (sub 0): 256 turns (21 s)
sub 4/5 (entering): inside < slots: walk in (0x404420), entering count +0xAD += 1 (up to 120; more than
    slots at once sets building +0x14 bit 0x40); every 2 turns, within 0x70: enter_building (0x407150)
    building full: queue building -> sub 8, 9 (back to the door point, then sub 0: queue again)
```
- Admission gate +0xAB: set to 16 when a head is admitted, 12 when anyone leaves (`remove_person_from_hut`
  0x40778C, also the converted occupants of a finished training), minus 1 per building turn (0x4032E0). So one
  person every 16 turns (1.3 s) while there is room, 12 turns (1 s) after someone came out.
- Capacity: `inside < slots` (type +0x20, 5 for types 5-8, 1 for the reconversion building 9), checked by the
  head (0x434AC1) and again on entry (0x4071A5). So 5 trainees can be inside, admitted 16 turns apart, and the
  cost is recomputed on each entry with `n` = all of them (`TRAIN_MANA × n × BAND%`); they all come out at
  once. Not seen in game: with a training at 500 turns, a queue of 5 fills the building in 64 turns.
- Leaving the queue: any other order ends state 0xA/0xE; the queue rebuild (0x409580, run every turn by queued
  people not in sub 3) unlinks anyone no longer queued for this building, stores the first freed rank in
  building +0xAC and sets +0x9C bit 0x2000; the next queued person to run sees the bit and starts the ripple at
  that rank (0x43475D). A finished training sets the bit as well (0x4065E4).
- Entering jam: +0xAD counts people walking in; +0xAE (16 turns) resets it to 0; while +0x14 bit 0x40 is set it
  drops by 1 every 8 turns instead, and newcomers queue instead of going straight in.
- Spots (0x409710(building, rank, &pos)): start at the door point (building pos − shape origin × 256 + door
  offset × 64, 0x4044B0), snapped to 64 units inside the edge of its cell, facing along the wall (the quadrant
  from the door to the building, turned by 90°). Then `rank` steps of 0x80 (a quarter cell, 4 spots per cell):
  each step turns towards the building when that cell is not footprint (shape mask bit 0x1), otherwise goes
  straight, and turns away when the next cell lacks mask bit 0x4 (the ring around the footprint). So the queue
  hugs the building, going round it from the door. What happens past one lap, and off the mask grid: not traced.

### The panel's training bar
The building panel (`draw_ui_panel` 0x504BC0, switch on the panel kind: training buildings are kind 5, case
0x504FDD) shows the building's slot icons and one progress bar:
```text
if building.training (+0x9C bit 0x80):
    value = stored << 12, max = cost << 12                # +0x98, +0x96: the bar is stored / cost
    blocked = building +0x14 bit 0x1000                   # set when its last share was 32 or less
else: no bar
```
- The bar (0x505604): a 5 px tall box as wide as the panel's content (the slot icons row), filled with palette
  0x9A, edged with 0x9D (top, left) and 0x96 (bottom, right), the health bar's colours. The fill is 3 px tall.
- Fill: `value × W / max` pixels in the player's tribe colour (0x5A89CC + 5 × tribe), W the inner width.
- When `max` is far above `W`, finer layers are drawn first (colours 0xF0 − k, up to 0xEF): layer periods
  `p, p × W, ...` below `max`, each `(value mod p) × W / p` long. They run ahead of the main fill, so the bar
  visibly moves even when one income adds less than a pixel (reading of 0x50575A-0x505817, not seen in game).
- Blocked: the fill is drawn only on the frames where the global blink 0x897989 is set, so the bar blinks while
  the building gets 32 mana or less per pass: a cost below 1056 (any computer player, 1000), or a small income
  split between many training buildings and spells.
- The bar jumps on income turns (every 16 turns) and stays still between them.

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
  (7/0x33) and a sound (0x4A7EB0). So it ends 128 below the ground (256 − 8 × 16), partly buried. Hint, unverified: a
  stone at full height would probably reach a balloon floating 200 over the site (vehicle table +0xF, "Vehicles"),
  so it is sunk to stay under one. "delete rs pillars" (7/0x4D) removes them; "move rs pillar" (7/7,
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
| Lightning | 7/0x1E (0x511AE0) | On its first turn, at the strike point: sets the building on that cell on fire (0x408CB0, so its occupants come out 8 turns later), burns the trees of the cell or else lights a fire (5/0xA, a scenery object - corrects this row's own earlier "7/0x0A"), and a simple blast. |
| Fireball shots | 8/4 (blast, firestorm), 8/7 and 8/8 (volcano) | Fly or fall to a point; on arrival a caller-configured impact effect (not hardcoded in the shot itself - "Fireball impact"): Firestorm's own is a land/water branch (ground-fire 5/0xA plus a 5-cell simple blast, or an unnamed splash 7/0x2B) and, if a building is there, 0x408CB0 same as Lightning. A second, shot-flag-gated burst of 4 smoke puffs (7/3) at the landing point exists in the same code but is OFF for Firestorm's own shot. |
| Smoke, explosions, sparkles, strands, gloops | 7/0x03, 0x26, 0x27, 0x2A, 0x3C, 0x3E, 0x3F, 0x40 | Visual only. |
| Nearest-N selector | function 0x515E30 | The 3 × 3 cells around the target; applies an action to the N closest units passing a filter. |
| Lava | 7/0x22 flow, 0x28 square | Hot cells set people on fire (state 31) and set buildings and trees burning; the caster's own people too, only the caster's shaman is spared. |

Shared person and building states:
- **Panic** (person state 0x1A): runs at a random heading at speed 110, screaming, for 64 turns (5.3 s).
- **On fire** (person state 31): runs about and loses `LIFE_<type>` / 32 per turn for 70 turns, so dies in about
  32 turns (2.7 s). Entered from the person's own life step (0x4D43A0) whenever its status word `+0xC` has bit
  0x8 set and it is not already in state 31 ("Health and healing"). **What actually sets that bit, checked across
  every one of `notify_area`'s (0x44F2F0) 15 call sites exe-wide: only lava (flow mode 3, square mode 2,
  "Volcano lava flow") ever passes the fire bit** - every other terrain-changing effect (land bridge, the
  earthquake's sink, volcano's own ground-moved notify, flatten, erosion) only ever passes mode 1 (ground moved,
  no fire). Lightning's own strike code (0x511AE0, read directly) ignites the struck cell's BUILDING (0x408CB0,
  "Burning building" below - occupants thrown out and panicking, not set alight themselves) or its tree/ground
  fire, never a person's own status bit; this is why "Burn cell" above is separately noted as doing no damage to
  people. **Lava and the volcano's own turn-110 area strike are, so far, the only confirmed ways a person catches
  fire autonomously** - not lightning, not firestorm's fireballs (their "short fire" is the same scenery/building
  ignition, not a person status). The volcano's strike is now traced (confirmed this session, resolving the
  earlier "not yet reconciled" note): `process_volcano`'s per-turn code (0x50E060-0x50E9DE) walks every unit in a
  6×6-cell area once, at a single local-counter value (`ebp == 0x1E`, see "Volcano rocks" below for what `ebp`
  is), and for each person present ORs the same `+0xC` bit `0x8` directly (it does not go through `notify_area`,
  which is why the 15-site survey above missed it) - the caster's own shaman (type 7, same tribe) is the only
  exemption, any other tribe's shaman included. **There is no separate kill path**: this is the ordinary state-31
  fire (32 turns to die), just lit by one direct scan instead of per-turn `notify_area` pressure, contradicting
  the earlier "plausibly a one-shot kill" guess. Buildings in the same scan (unless already in state 3) go
  through the already-documented destruction state 3/sub 1 (0x50E5FB, "partial explosion and a blast 7/1" -
  "Building ashes"), once each, not a direct kill either.
- **Fall**: landing at vertical speed ≤ −100 raises dust and a sound; ≤ −200 costs `FALL_OUT_OF_WW_DAMAGE` (exe 700,
  not in the file), except on cells whose terrain has flag 2 (probably water). Gravity 32 per turn.
- **Burning building** (building state 4, set by 0x408CB0; per turn 0x408AB0): 127 turns (10.6 s), then back to
  state 1. The library and the prison never burn (building-type flag `+0x4A` bit 0x1); a building already burning
  is not lit again (its timer is not reset). The igniter's tribe is kept in `+0xAF`.
```text
on fire:  timer +0xA7 = 127
per turn: timer -= 1
  timer 119 (turn 8, 0.67 s): every occupant (6 slots at +0x86) is thrown out (`remove_person_from_hut`
    0x407490) and panics (person state 0x1A)
  timer > 79: fire sound 0x53
  timer 79 (turn 48, 4 s): one structural hit of 100 wood (0x4BA2C0(shape, −100)) with damage smoke;
    no wood left → the building is destroyed, stats[tribe][7] += 1
  timer 0 (turn 127): state 1
```

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
| 3 | Lightning (80 000, 6144, 4) | Kills outright up to `LIGHTNING_NUM_KILLS` + 1 (file 7) people in the single target cell, any tribe except the caster's shaman, angels of death and shielded units: own followers and enemy shamans die too (0x511F70, walking the cell's unit list). People inside a building are not in that list (leaving a hut puts a person back into it, 0x4D80E0 → `insert_unit_into_land_tile`), so they are not killed (inferred): the building catches fire and throws them out, panicking, 8 turns later. Plus the lightning component. |
| 4 | Whirlwind (90 000, 4096, 3) | 200 + 24 turns (18.7 s), 120 units per turn. Wanders around the target for 100 turns, then drifts away. Lifts every person in its own cell, any tribe, for 12-19 turns, then throws them: the landing costs 700. Each turn a building in its cell has a 25 % chance to lose 100 wood (a hut survives 3, a temple 8); trees lose 100 wood (50 %); library and prison immune; vehicles get a simple blast. |
| 5 | Insect plague (40 000, 6144, 4) | 60 insects for 200 turns (16.7 s), at most 6 cells from the target. Every 8 turns, enemies in its 3 × 3 cells take `SWARM_PERSON_DAMAGE` (100) and panic; shamans and angels immune; ghosts destroyed. Up to 3 times it raids an enemy building: occupants thrown out, damaged and panicked. |
| 6 | Invisibility (50 000, 4096, 4) | The `INVIS_NUM_PEOPLE` (file 6) nearest own followers (not the shaman), for `INVISIBLE_COUNT_X8` × 8 turns (120 s). Enemy auto-targeting skips them; some actions end it early. |
| 7 | Hypnotism (85 000, 4096, 3) | The `HYPNO_NUM_PEOPLE` (file 6) nearest persons of other tribes (not wildmen, shamans, angels) are recreated in the caster's tribe for `HYPNO_COUNT_X8` × 8 turns (file 440, 36.7 s), then return. If their tribe is gone, they stay. Ghosts are destroyed. |
| 8 | Firestorm (400 000, 4096, 2) | `FIRESTORM_DURATION` (file 220 turns, 18.3 s); one fireball every 4 turns (about 55) falling on a random point of a 6 × 6-cell square around the target. Each, on a land cell: a 5-cell simple blast that panics plus a ground-fire scenery object (same as Lightning's); on water: an unnamed splash effect instead, no fire; either way, a building there is set alight (0x408CB0, same as Lightning). No terrain height or blend change ("Fireball impact"). |
| 9 | Ghost army (18 000, 4096, 4) | Copies the best own unit type near the target (shaman > firewarrior > preacher > spy > warrior > brave): 6 brave ghosts, 3 specialists or 1 shaman; at most 60 per tribe. No expiry found. They attack only enemy ghosts and shamans, their shots do nothing; melee untested. Blast and insects destroy them. |
| 10 | Erosion (210 000, 4096, 2) | 63 hydraulic-erosion passes, one per turn (5.25 s), on an 8 × 8-cell window jittered around the target: ground washed downhill. No direct effect on units. |
| 11 | Swamp (100 000, 4096, 3) | No terrain change. Lasts 32 000 turns (44 min). Every 4 turns it kills outright every person on its 3 × 3 cells, any tribe, until `SWAMP_NUM_PEOPLE` (exe 10) are dead. At most 30 per tribe (the oldest goes). Removed when most of its cells are unsuitable. |
| 12 | Land bridge (70 000, 5120, 4) | From the caster to the target, 4 cells wide (−1, 0, +1, +2 across the line), heights on a straight slope between both ends (at least 90): each turn `h += clamp(T − h, ±LAND_BRIDGE_MAX_CHANGE) / (64 − turn)` (`LAND_BRIDGE_MAX_CHANGE` 256, 64 being `LAND_BRIDGE_DURATION`), so every cell reaches its height after 62 turns (5.2 s) when the gap is small: the cap makes large gaps fall short (gap 256: 251 reached, 500: 488, 800: 759, 1000: 900); sea cells raised too; a smoke puff on every bridge cell every turn. Buildings left on ground uneven by more than 200 collapse ("Terrain changes: what reacts"). |
| 13 | Angel of death (510 000, 3072, 1) | A meteor, then an angel (person type 8, `LIFE_AOD` 10 000) hunts the nearest enemy on the whole map, shamans included, pulling people out of buildings; carries and throws each victim to death, about one per 28 turns (2.3 s). Ends after `AOD_KILL_COUNT` (40) kills or `AOD_DURATION` (2500 turns, 208 s). Two angels fight by health. |
| 14 | Earthquake (175 000, 4096, 2) | 120 turns (10 s) of shake. At turns 24, 56 and 88 each building in 16 × 16 cells has a 20/128 chance to be shaken (occupants out, partial explosion). At turn 60, a half-disc of radius 8 cells sinks by up to 600 (`600 × (4096 − d) / 4096`), with a lava fissure of short-lived lava squares and possibly two lava flows. |
| 15 | Flatten (125 000, 4096, 3) | 15 turns (1.25 s): a 45-cell disc (about 3.5 cells radius) converges to the ground height of the target point. |
| 16 | Volcano (800 000, 3072, 1) | 160 turns (13.3 s): 80 turns of rumbling, then a cone rises over 59 turns (rim 1024 at 2.5 cells, slopes to 5 cells, a plug in the crater). At turn 110 every building in 6 × 6 cells is destroyed and every person there (but the caster's shaman) set on fire; lava flow, 16 s growth + 1.3 s fade ("Volcano lava flow"); about 22 small and 6 big fireballs (simple blasts) and 26 decorative rocks ("Volcano rocks"). |
| 17 | Convert wild (10 000, 8192, 4) | Every 8 turns the nearest wildman within 2 cells becomes a brave of the caster (if the tribe is under 200); ends 18 turns after the last conversion. |
| 18 | Armageddon (hidden) | Clears the map, turns every building into one person, gathers every tribe on an arena around the target and makes them fight; the weakest shaman is struck by blast or lightning at intervals; anyone more than 9 cells away dies. |
| 19 | Shield (60 000, 4096, 4) | The `SHIELD_NUM_PEOPLE` (file 6) nearest own followers (not shaman, wildmen, angels, ghosts) for 120 s: immune to blast and whirlwind, other damage ignored too, except melee; deflects burn, blast, lightning, whirlwind and insect shots. |
| 20 | Bloodlust (hidden) | 6 nearest own followers for `BLOODLUST_COUNT_X8` × 8 turns (file 120 s): melee and firewarrior damage × `BLOODLUST_DAMAGE_X` (3), damage taken >> `BLOODLUST_HEALTH_X` (3, i.e. / 8), firewarrior reload >> `BLOODLUST_SW_BLAST_X`. |
| 21 | Teleport (hidden, 65 536) | Moves the caster's own shaman to the target 8-12 turns after arrival (out of any vehicle or building; boards a friendly or empty vehicle there). |

Friendly fire: lightning, whirlwind, swamp, volcano and lava hurt the caster's own people; blast, firestorm and
insects do not damage allies. Lightning and swamp kill regardless of health.

### Volcano rocks [exe]
Purely decorative: the volcano's per-turn function (0x50E060) makes one scenery 13 ("rock", `create_unit(5, 13)`
0x4ED8A0, at 0x50E34B) per turn on volcano turns 105-130 (26 in all). No level places one; the volcano is their
only creator.

`process_volcano`'s own local counter `ebp` (used by this, the fireball windows below, and the turn-110 strike
above) is `160 - (turns left on the effect's own countdown, +0x6C)`: confirmed this session by converting its
windows to absolute cast-turns and matching the figures already on record from elsewhere - `ebp` in `[0x19,0x32]`
(25-50) + 80 reproduces this section's own "105-130", and `ebp == 0x1E` (30) + 80 reproduces "turn 110" above, so
`ebp + 80` is turns since cast (the 80-turn offset itself - most likely the rumbling phase ending before this
handler starts counting - is inferred from the match, not read directly off an init). The fireballs below spawn
on `ebp` in `[0x23,0x46]` (35-70), i.e. cast-turns 115-150, not the same window as these rocks (they overlap but
are not identical - correcting this session's own earlier note that treated all three as one shared window).
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

### Volcano fireballs [exe]
The small (8/7) and big (8/8) fireballs spawned by 0x50E461/0x50E47F (above) share one init function (0x4BC500,
both types' slot in `init_unit_type_8`'s table) and one per-turn function (0x4BC660, state 7, a state neither
Blast's nor Firestorm's own fireball shot - 8/4, state 4, 0x4BB440 - ever enters): the two sizes fly identically
and differ only in which effects they spawn.
```text
init (0x4BC500), both sizes alike:
  state = 7; draw class 29 (a single camera-facing sprite, same class the shot 8/6 ball uses), sprite = hfx 1120
    for type 7 (small - the SAME ball sprite as the firewarrior's own bolt, "The shot 8/6"), hfx 1248 for type 8
    (big) - the only difference at this stage
  launch kick, same formula both sizes: horizontal speed 96-223 (random), random direction (0-2047), vertical
    speed 256-383 (random) - sets off at a random angle and strength from the crater, not aimed; sound 0xBC

per turn (0x4BC660), while flying:
  height -= 32 (gravity, same as elsewhere); land once height <= ground
  every turn, 4 trailing puffs along the path travelled since last turn (interpolated):
    type 7 (small): effect 7/0x03, the plain grey smoke puff (hfx 314-321, 4-turn life - "The shot 8/6",
      the SAME puff the firewarrior's own bolt trails)
    type 8 (big): effect 7/0x42 (newly traced this session: state 53, 16-turn life, draw class 44, hfx 1224) -
      a distinct, longer-lived trail, not just a bigger smoke puff; each of the 4 per-turn instances gets an
      incrementing sprite-frame offset (+4 each) and the "hot" render flag

on landing, both sizes alike:
  effect 7/0x05 (Burn cell) at the impact cell
  one more trail puff at the impact point (7/0x03 small, 7/0x42 big, same as the per-turn trail above)
  a simple blast (0x50B6F0, the same 140/98 magic constants as "Blast push and flight" - size-independent)
  the size-specific burst: 7/0x3E small, 7/0x3F big (16-turn fire blast, alpha hfx 1194-1209, sound 0xB2 -
    "Volcano rocks" above)
  the shot deletes itself (0x4EDCF0) the following turn
```
So **the big fireball is not a scaled-up small one in flight or physics** - same arc, same random launch kick,
same gravity, same 4-puffs-a-turn cadence - it only renders bigger: a different ball sprite (hfx 1248 vs 1120),
a distinct 16-turn trail effect (7/0x42, not traced before this session) instead of the shared 4-turn smoke
puff, and the already-documented bigger impact burst (7/0x3F vs 7/0x3E). Effect 7/0x42's own look (beyond its
sprite index and duration) was not decoded from `hfx0-0.dat` this session.

### Volcano lava flow [exe]
Effect type 34 ("lava flow", state 33, init 0x512A00, per turn 0x512A60) is spawned once when the cone finishes
rising. It is a downhill-following spread, not a disc interpolated between two radii:
```text
init: radius (+0x74) = 3 internal units (1.5 cells), growth timer (+0x70) = 192, accumulator (+0x76) = 0

growing (state_2 = 0), every turn, no rhythm gate:
  scan a (2×radius+1)² window of cells around the cast point
  for each cell with blend > 0xC4 (196, "mature"):
    d = that cell's precomputed downhill direction (map-cell +0xE high nibble, 0 flat, 1-8 the 8 compass dirs)
    flat (d = 0): deepen the cell itself, height += 10 (no spread)
    sloped: push lava one cell in direction d: source height -= 2, target blend += 32, target height += 4,
      both flagged lava + damaged ground; radius grows to cover the farthest point actually reached
    accumulator += 16 per spread step, -= 8 per flat (no-spread) step
  ends (→ state_2 = 1) when the timer hits 0 (192 turns, 16.0 s) OR the accumulator exceeds 0x20000 (131072),
    whichever first - a volcano spreading fast downhill can finish under 16 s; flat ground around the cone
    hits the 16 s cap instead
fading (state_2 = 1), 16 turns (1.3 s): blend -= 8/turn only, no more spread, no more burning
finished: the whole reached footprint becomes an ordinary building-ash scorch record (scorch_add, kind 2
  rectangle, the same 6000-turn hold + 288-turn fade as a destroyed building's ashes - "Building ashes"),
  then the effect frees itself

every turn a cell's blend > 0x9F (159): notify_area(mode 3 = ground-moved + set-on-fire, 0x44F2F0) - the
  caster's own shaman is exempt from the fire bit; every other unit of any tribe (including the caster's own
  people) is set on fire (state 31) every turn it stays on a hot cell; no shield check found in this function
```
Total life: up to 208 turns (~17.3 s) from the cast's end to becoming permanent ash. **It never kills outright**:
only the state-31 fire/evacuation mechanic (already implemented) does. "Lava square" (effect state 38, init
0x5134C0, per turn 0x513570, used e.g. by Earthquake's own fissure) is the same flag/burn mechanics on a single,
static cell: no spreading, blend decays −2/turn below its 0xC4 starting value, self-destructs once it drops
below 0x7F, same per-turn fire-only notify while blend > 0x9F.

The rendered "glowing, linked blobs" look (confirmed this session, see "Building ashes" "Drawn" for the mechanism
in full) is this same blend byte, nothing lava-specific: each cell's +0xA feeds straight into the landscape's
ordinary per-corner colour interpolation, so a hot cell peaks at its own corners and fades toward cold
neighbours (reading as a soft round-ish glow) while two adjacent hot cells keep a bright, undipped shared edge
(reading as linked) - and the brighten-then-fade over a cell's life is +0xA itself rising to 0xFF then cooling
back to 0x7F exactly as traced above, not a separate animation.

**The downhill-direction field is frozen at height-change time, not read live, and is a general terrain
property shared by the whole engine, not computed for lava** (confirmed this session): lava's own spread
(0x512A60) only ever reads map-cell +0xE; it never writes it. The write lives in `level_land_processing_2`
(0x44DF40), the same function that recomputes the blend byte above, reached through `land_level_processing_1`
(0x44DDF0, "recompute the landscape after a height change" - lava's own call to it, right after each spread step,
is what keeps the field current around a growing flow). 0x44DF40 is called from about 32 sites exe-wide having
nothing to do with lava - ordinary terrain-shaping (`landscape_move_2` 0x4043CB, `set_level_height_100` 0x49838F),
level load (`load_level2` 0x484C29), and other effects (firecloud 0x50C618) - so every cell's direction nibble
exists independently of lava and can go briefly stale exactly as our own code worried: it is whatever it was set
to the last time *any* height edit touched that cell or (for untouched cells) at level load, not recomputed every
turn.
```text
for the cell and its 8 neighbours, compass order N, E, S, W, NE, SE, SW, NW (the engine's standard ±2-unit
  3x3 walk, same one the blend recompute above uses):
    start: minimum = the cell's own height, direction = 0 (flat)
    for each of the 8 neighbours in that fixed order:
      if neighbour height < minimum: minimum = neighbour height, direction = that neighbour's number (1-8)
  store: cell +0xE = (cell +0xE & 0xF) | (direction << 4)        # low nibble (another, untraced field) kept
```
No slope-magnitude threshold exists: a neighbour even 1 unit lower than the running minimum wins, so "flat"
means only "no neighbour is strictly lower", not "the terrain here is actually level". Ties are broken purely by
scan order, never randomly: the cell's own height beats every neighbour (a tie keeps "flat"), and among
neighbours of equal height the fixed order above wins - N beats E beats S beats W beats NE beats SE beats SW
beats NW. (A defensive `clamp(direction, ..., 15)` in the same code is never reached: only 9 candidates are ever
compared, so the result is always 0-8.)

Open: **A magical shield does not block catching fire**, now confirmed on both ends: no shield check (+0x14 bit
0x8000) in `notify_area` (0x44F2F0, where the fire flag is set) nor in the person's own life step (0x4D43A0,
where it is consumed into state 31 - "Components"). Still open: the volcano controller's own rock/fireball spawn
windows (0x50E34B/0x50E461/0x50E47F, counted against a local per-function turn counter not yet reconciled with
the absolute cast turn, so the existing "turns 105-130" / "~22 small, 6 big" figures are not yet cross-checked
against this session's `[35,70]`/`[40,160]` windows); an extra, unnamed effect (type 0x40, state 51) spawned both
probabilistically by the lava's own spread (~1/128 per hot cell per turn) and deterministically by the volcano
controller - role not traced.

### Firestorm fireballs [exe]
Per-turn (state 23, per-turn 0x511640 - Ghidra's own name `process_firestorm` is correct here; init pushes
state 23 then loads `FIRESTORM_DURATION` straight from its named address into the duration counter +0x6C, a
direct constant reference, not a coincidental immediate):
```text
every turn: duration -= 1; 0 -> end (0x4EDCF0)
sub-state (+0x2D): 0 -> += 1, return (one turn of nothing right after the cast); 1 -> continue
only when (duration & 3) == 0 (one call in four - this traces, not just infers, "about 55 over 220 turns"):
  two PRNG draws (0x89D178, "x = ror(x*0x9E3D+0x24DF,13)"), each reduced mod 0xC00 (3072) and recentred by
    -0x600 (-1536): an offset up to +-1536 units (+-3 cells) on each axis from the effect's own position - a
    random point in a 6x6-cell square, matching "Per spell" above
  create_unit(class 8, type 4, tribe, point) - the fireball shot ("Fireball shots" in Components); sound 0xB3
  the new shot: speed (+0x5F) = 250, start position (+0x70/+0x74) = the jittered point, aim height (+0x76/+0x7A)
    = ground height there (0x44E940) minus 20 (so the homing check below lands it slightly into the ground);
    flags +0x6E: bit 0 cleared, bit 1 set (the self-attached flame below; NOT the impact smoke burst - see
    "Drawn"); impact spec +0x7C/+0x7D/+0x7E = 7/0x1C/4 and params +0x80/+0x82/+0x84/+0x86 = 80/8/32/0 - what
    effect the shot creates on arrival, read by 0x4BB290 ("Fireball impact" below) - corrects an earlier draft
    of this row, which called these four bytes "movement-arc fields": they were never read as a trajectory, only
    by the impact dispatcher
```
Drawn (the fireball shot itself, 0x4BB440, shared with Blast's own fireball shot - "Fireball shots" in
Components): a dynamic ground light under it while it flies (`sunlight_update_unit_landscape` 0x4010B0, the
same mechanism the firewarrior's own bolt carries, "Drawn" under "The shot 8/6" above); up to 4 trailing
effect 7/0x0A instances, one spawned per turn of flight (clamped 0-4, unconditional) - a distinct ember/particle
trail, not the plain grey smoke puff (7/3) the firewarrior's bolt leaves; and a self-attached effect 7/0x4F
riding the shot itself (a flame/glow, its sprite cached into the shot's own +0x94) when the shot's flags (+0x6E
bit 1) ask for it - true for Firestorm's own shot, confirmed above. A second, separate burst of 4 randomly
jittered plain smoke puffs (7/3) scattered right at the landing point exists in this same function, gated by
+0x6E bit 0 - **Firestorm's own shot has that bit cleared, so it does not get this burst** (corrects "Fireball
shots" in Components, which describes "smoke" as shared by both Blast and Firestorm; whether Blast's own shot
sets bit 0 was not checked this session).

### Fireball impact (Firestorm, Blast) [exe]
On arrival (0x4BB440's own sub-state 2), before the shot deletes itself next turn: 0x4BB290 reads the shot's own
+0x7C/+0x7D/+0x7E (class, type, "has extra params" flag) and, when that flag is set, first copies +0x80/+0x82/
+0x84/+0x86 into the shared creation-parameter queue (0x892443, already known from `init_unit_type_11`'s own
cast-cost parameter) - then `create_unit(class, type, tribe, pos)`, i.e. **the shot does not hardcode what
happens on impact: the caster's own cast code configures it per spell**. For Firestorm (type 0x1C, init 0x511800):
```text
read the impact cell's terrain-type nibble (map-cell +0xC) and look up table 0x5AA328 (14 bytes/record, the
  same coastal-shape table "Wildmen" uses) bit 0x2 - water:
  water: create_unit(7, 0x2B, tribe, pos) - an unnamed splash/hiss effect (state 40, draw class 32, hfx 1152,
    sound 0x55); not traced further
  land: push params [1, 0, 32, 0] to the queue, then create_unit(5, 0xA, tribe, pos) - a ground-fire SCENERY
    object (the exact same object Lightning's own strike creates, "Lightning" in Components - the Components
    table's own "7/0x0A" for that case is a typo for "5/0xA", this class/type), then 0x4A8C60(fire, 0x16) to
    set its own burn-length countdown (fields and exact turns not decoded this session) and sound 0xB6
  either way: create_unit(7, 1, tribe, pos) - a "simple blast" effect, its own fields set directly rather than
    through its usual init: radius (+0x6C) = 5 cells (1280 units, matching R in "Blast push and flight") and the
    same push constants 140/98 from that section - this is the "fireball's own simple-blast-that-panics" already
    named in "Per spell" and spells.md, now pinned to exact numbers
  either way, regardless of the branch above: if the impact cell holds a building, 0x408CB0(building, tribe) -
    the SAME call Lightning's own strike uses to set a building burning (occupants out 8 turns later)
```
**No terrain height or blend write happens anywhere in this chain** - the only read of the terrain is the
water/land check above. "The ground is altered" is, concretely, a scenery object placed on top of it (the
ground-fire) plus whatever that building call or the blast's own push/damage do to what's standing there; the
landscape's own heightmap and blend are untouched by Firestorm, unlike Volcano/Earthquake/lava or the land-change
spells. 0x50CCD0 (called at the very start of the impact effect's own init, before the water check, to read the
queue back into local fields) looked like a shared "spawn N debris fragments from queued params" helper common
to several effects - not followed further, since it only affects a debris burst, not terrain.

This session also decoded `hfx0-0.dat` locally to check sprites.md's "by eye, unconfirmed" notes against the
real art (`pal0-0.dat` is 4 bytes/entry - R, G, B, flag - not 3; decoding it as 3 misreads every colour past
the first few, the pitfall that first made these look wrong):
- hfx 1120-1123 (the "shot" sprite already cited under "The shot 8/6") is now confirmed by eye: a small round
  bright ball, shrinking through 1124-1127 down to a single pixel - the flying fireball itself, not a ground
  patch;
- hfx 90-93 is confirmed by eye: a standing, opaque 4-frame upward flame lick - this is the ground-fire scenery
  object's own sprite (class 5 type 0xA, "Fireball impact" above), not a separate "short fire" effect;
- hfx ~1180-1210 are confirmed by eye as a growing/fading smoke cloud then a separate explosion burst - an
  impact effect, distinct from the flying ball.

Net: the authentic look is dozens of small bright balls, each with its own light, ember trail and self-flame,
falling one at a time to scattered points across a several-cell square over about 18 s, each landing in a
brief burst and leaving a small standing flame behind - never a single persistent ground-fire patch. Impact on
our specs: if our own view renders firestorm as one static fire effect instead of a stream of individually
falling, lit, trailed fireball objects, that view (not the already-correct scheduling in `firestorm`/
`EffectKind::Firestorm`, spells.md) is where the "rain of fire" look is missing.

### Erosion passes [exe]
Per-turn (state 24, per-turn 0x50FF30 - Ghidra's own name `process_erosion` is correct, despite sitting in the
same code area as the shot handlers; init pushes state 24 then sets the duration counter +0x6C to a bare 64,
not a named constant):
```text
every turn: duration -= 1; 0 -> end
  two PRNG draws, each "& 7, - 4": a jitter of -4..+3 half-cells on each axis around the effect's own position
  call 0x4983A0(jittered point, 4) - one hydraulic-erosion pass
```
0x4983A0 (write-back magnitude not traced) scans a window of cells around the jittered point and, per cell,
compares its height against each of its 8 neighbours - the 4 diagonals weighted by 0xB505/0x10000 (~=0.707, a
genuine distance correction) - keeping the steepest downhill one and accumulating a flow amount before
(unread) applying a height change. This is a real steepest-descent slope pick, a different algorithm from
land bridge/flatten's "converge toward a target height": erosion looks at local slope only, confirming
"ground washed downhill" literally rather than as a figure of speech.

Impact on our specs: ours applies one instant, flat, radius-4, -120 height drop (spells.md); the original
instead runs 64 of these small, locally-slope-following passes (one per turn, 5.3 s total), each jittered
within a few cells of the target and touching only a small window, following the real local slope rather than
a fixed radius and depth - a visibly gradual, small-footprint erosion, never a sudden crater. The exact
per-pass height delta is still open (next step: finish tracing 0x4983A0's write-back, or measure live - cast
Erosion on flat ground in a test level and sample the target cell's height every turn with `livemem.py`).

### Swamp: drawing, and the reedy-grass / mist ground dressing [exe]
The Swamp spell's effect (7/18, init 0x511340 → 0x511400 for the kill state, "Per spell" above) sets its own
draw class directly (`mov [unit+0x3A], 0x34`, i.e. 52) without going through the usual drawable setter
`0x4EE700`, so its sprite index `+0x33` stays 0 (the class-7 default, set by the shared init 0x50BCD0 as
`0x4EE700(&unit+0x33, 0, 0)`). It has no in-world ground sprite: its only visible trace is the aerial/overview
map icon `hfx0-0.dat` 81 (a small grey-teal striped shape, read as a headstone; `sprites.md` "swamp (RIP
tombstone)"). This matches "No terrain change" in the per-spell table: casting Swamp does not paint anything
on the ground, it only marks the map and starts killing people in its 3x3 cells (state 19, 0x511400).

The actual marsh/swamp ground look comes from two separate, unrelated class-7 effect types that level
designers place directly (not spawned by casting the spell): 7/84 "reedy grass" and 7/85 "swamp mist"
(level-format.md's effect-model list, "81-90").
- 7/84 "reedy grass" (init 0x513620): a plain opaque billboard, same renderer as the volcano rock (draw class
  0, `0x46895E`): `0x4EE700(&unit+0x33, 0, 0x219 + rand(0x89BC72) % 12)`, i.e. one of 12 `hfx0-0.dat` frames
  537-548 picked at random — olive-green tufts of reed/grass, no flower head (checked by decoding and viewing
  all 12 frames against `pal0-0.dat`).
- 7/85 "swamp mist" (init 0x50C050): an alpha-blended billboard, draw class 0x33 (51):
  `0x4EE700(&unit+0x33, 0x33, 0x626)`, hfx sprite 1574 (within the 1538-1592 alpha range) — a dark muddy-brown
  blotch, not a drifting fog despite the name; it is painted flat, presumably tinting the ground to look wet/
  mired.
- Neither type's sprites (checked: hfx 2-6, 78-85, 537-548, 1574-1576) show anything resembling a flower; no
  "flower" sprite tied to swamp was found. Open: what sprite the user means by "the flower on it" — possibly a
  specific frame not yet checked, or a different system (level editor decoration, generated-map flora) not
  traced here.

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
| Boat (vehicle types 1, 2) | One forced movement pass (0x4E7FB0); no damage or destroy call in it. A boat whose cell becomes all land is stranded at the next 16-turn check (state 8): passengers put off, it keels over, explodes 160-287 turns later ("Vehicles"). |
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

## Terrain land colour: the bigfade row and brightness [exe]
The missing half of `set_texture_landscape_main` (0x4BE330): "Building ashes" "Drawn" already covers the
blend-driven cliff/lava/ash row (`cliff[(blend/4)*128 + colour]`); this is the ordinary land colour that
feeds `colour` into that lookup - the `bigf0-X.dat` row/column pick, terrain-textures.md "bigfade"/"disp"'s
open questions.

**How the original actually draws this, confirmed from the exe (2026-10-20): there is no baked ground texture
at all.** `set_texture_landscape_main` computes each quad's 4 corner values (height/blend/light below), then
dispatches to one of three pixel-fill routines by colour depth - `landscape_texture_32` (0x4BF860, 32-bit
surfaces), `landscape_texture_16` (0x4C0380), `landscape_texture_16_fade` (0x4C0EC0) - that rasterize the quad
at its OWN current on-screen pixel span. `landscape_texture_32` re-reads the same 4 corner cells itself and
sizes its scratch buffer to the quad's live screen extent, not a fixed tile; none of the three makes a single
Direct3D (or any other vtable/COM) call - the whole landscape colour pipeline is CPU software rasterization,
even in this Direct3D-build exe, evaluating the `bigfade`/`disp`/`cliff` formula below **fresh, per rendered
output pixel, every frame**, at whatever resolution the quad is actually drawn at (near or far, zoomed in or
out). There is no intermediate fixed-resolution "landscape texture" surface anywhere in this path to mip-map,
filter, or otherwise manage at draw time - the concept doesn't apply, because the original never samples a
pre-rendered bitmap for land colour; it recomputes the formula per pixel instead. The `disp0-X.dat` noise term
(below) is still a raw, unfiltered point-sample of a small 256x256 tiling table at each pixel's own tile
coordinate - so the original's own grain is always evaluated at native output resolution (reading as fine,
consistent dithering, part of the intended "weathered ground" look) rather than upsampled/downsampled from a
lower-resolution source. Impact on our implementation: see terrain-textures.md "Our bake" and TODO.md "Terrain" -
baking into one coarse, fixed-resolution colour texture up front (ours: 8 px/cell) and then having the GPU
magnify/minify it is a reasonable performance shortcut, but it is the reason that texture needs its own mip/
filtering care at all; the original's own architecture never has that problem in the first place.

Besides height and the already-documented blend byte, two more per-map-cell fields drive it (table 0x8A03E4, 16
bytes/cell, same record as height/blend/downhill):
```text
+0xA blend   (already documented)
+0xD light   - a precomputed BRIGHTNESS byte, 0..255, NOT read live from a sun vector at draw time
+0xE downhill direction, high nibble (already documented); low nibble - also already-precomputed, feeds light
```
`+0xD` is written by a small function family (`compute_cell_light` 0x4BE230, sibling 0x4BE0B0 for the opposite
neighbour pair, a bulk whole-map variant ending at 0x4BDE94 for level load) - called from the SAME per-dirty-cell
pass that recomputes blend and downhill-direction after every height edit (`level_land_processing_2` 0x44DF40,
from 0x44E7C9/0x44E7D1), so lighting tracks terrain edits live, not baked once at load:
```text
dx = height(cell) - height(cell at x+2, same z)        # height drop to the +x neighbour 2 units over
dz = height(cell at same x, z+2) - height(cell)         # height rise to the +z neighbour 2 units over
light = clamp( (sunBase + dz*sunZ - dx*sunX) / 350 + (low_nibble(+0xE) + 8) * 16,  0, 255 )
```
`sunX`, `sunZ`, `sunBase` are three signed 16-bit globals (0x937AA8, 0x937AAA, 0x937AAC), not computed per draw
call - confirming the earlier guess that this, not the bigfade row, is where sunlight feeds in. But **they are
NOT derived from the level's own `ShadeStart`/`ShadeRange`/`Inclination` sunlight block** as a prior pass of this
section assumed: `sunlight_init_default` (0x401040) sets all three to the literal immediate **0x93 (147)**, a
flat hardcoded constant, independent of the level data - `ShadeStart`/`ShadeRange`/`Inclination`
(0x937AAE/0x937AAF/0x937AB0, exe defaults 0x1C=28/0xF=15/0x20=32, level-format.md's own measured values) are
still read from the level and stored, but nothing in the live-measured path uses them to shape `sunX`/`sunZ`/
`sunBase`. **Live-measured this session**, two running levels with different on-disk `Inclination` (0x40 and
0x20, both `ShadeStart`=28/`ShadeRange`=15): `sunX=sunZ=sunBase=147` in both, confirming the constant is
level-independent in practice. `0x401790` (`sunlight_init_2`) does not set the vector either - it only *reads*
the already-set `sunX`/`sunZ`/`sunBase` to build two unrelated 1024-entry angle-keyed shade tables (0x89BC8E,
0x89C08D, still not re-examined). Two other paths exist that could move the vector away from 147, both gated
behind the same flag bit (`0x895DA5 & 0x8`), live-confirmed OFF this session (`Inclination` on both measured
levels sat unchanged at its on-disk value after 77+ turns, 9+ of the 8-turn checks below):
- `0x401350`, a per-turn tick: when the turn counter's low 3 bits are all set (every 8th turn), increments
  `Inclination` by 1 and recomputes `sunX`/`sunBase` from the REAL sin/cos tables (0x5DE5E8/0x5DDDE8, indexed by
  the new `Inclination`) - the exe's actual day/night-style sun-angle animation, not the level's static block.
- `0x401920`: unconditionally forces `Inclination` to 0x40 (64, overwriting the level's own value) and
  `sunX`/`sunBase` from two FIXED scalar memory cells (0x5DE9E8/0x5DE1E8) - not an array indexed by
  `Inclination` despite the name suggesting a per-angle branch (no write to either address exists anywhere in
  the exe's code, so they are build-time data, not computed from the level).
Net effect, confirmed live: for ordinary single-level play, terrain brightness contrast from slope is driven by
a **fixed** sun vector (147/147/147), not one that varies with a level's declared sunlight. The `+0xE` low
nibble (previously unidentified, see "Open questions") is this same precomputed ambient term, untouched by the
downhill-direction write to the high nibble.

The row itself is purely height + blend, same inputs as the already-documented cliff path, no sunlight:
```text
altitude_index = height(cell)                                 # the SAME cell the corner sits on, no averaging
altitude_index += 0x4B                                         # +75, unconditional
if blend != 0: altitude_index += 0x4B                          # +75 again (blend is 0 only on a perfectly flat
                                                                #   cell with all 8 neighbours equal height)
altitude_index = clamp(altitude_index, 0, 0x3FE)                # 1022 of bigfade's 1152 rows
```
Each of a quad's 4 corners gets its own `altitude_index` and `light` this way (no cross-cell averaging at this
step, same as blend); both are then interpolated across the quad with the same Gouraud setup as the cliff path
(value << 16 at the corners, per-step delta << 12). Per rendered pixel:
```text
sensitivity = DISP_SENSITIVITY[round_down(interpolated altitude_index)]   # table 0x5D45B8, 1152 signed i16 entries,
                                                                           #   built once at level load:
                                                                           #   clamp(3 * index - 64, 320, 1024)
                                                                           #   (flat 320 below index 129, a +3/index
                                                                           #   ramp to 1022 at index 362, flat 1024
                                                                           #   above - i.e. disp noise barely moves
                                                                           #   the row on low/flat ground and moves
                                                                           #   it up to ~3.2x as much on high ground)
row_jitter  = (sensitivity * disp_sample) >> 10                 # disp_sample: SIGNED byte from disp0-X.dat (movsx,
                                                                 #   never treated as unsigned or re-centred by a
                                                                 #   theme constant - a theme's own narrow byte
                                                                 #   range, e.g. file 1's 45..127 vs file 5's
                                                                 #   130..193, already IS the per-theme bias once
                                                                 #   read as signed, no separate constant needed)
row_fraction = (interpolated altitude_index fixed-point value) >> 8   # the interpolation's own sub-row precision,
                                                                       #   folded in as extra column units (see below)
brightness  = clamp( ((disp_sample_at_next_tile_row - disp_sample) >> 2) + (interpolated_light_fixed_point >> 16),
                     0, 255 )                                   # `light` (the +0xD field above) interpolated the
                                                                 #   same way, nudged by disp's OWN row-to-row
                                                                 #   difference - a second, smaller use of disp
index = row_jitter + row_fraction*256 + brightness               # one flat offset: a large `brightness` can
                                                                   #   legitimately roll over into the next row,
                                                                   #   this is not guarded against
colour = bigf0-X.dat[index]                                      # 0..111 land colour, same byte then fed into
                                                                   # cliff[(blend/4)*128 + colour] (Building ashes)
```
- No per-theme or per-level constant exists anywhere in this path beyond the loaded `bigf0-X.dat`/`disp0-X.dat`
  files and the sunlight-derived `sunX`/`sunZ`/`sunBase`: `POP3_ROW_SCALE`'s single global multiplier has no
  counterpart - the original's disp sensitivity is a fixed, altitude-dependent CURVE (320..1024, the table
  above), not a flat per-theme or per-level scale, and it is multiplicative on disp, not on height.
- `light`'s divide-by-350 and the `(low_nibble + 8) * 16` ambient term are now live-verified on a flat cell (`dx
  = dz = 0`): measured `light = 128` exactly, matching `clamp(147/350 + 128, 0, 255)` with the `147/350` term
  flooring to 0 - consistent with the formula and the flat 147/147/147 sun vector above. A live spot-check on
  sloped cells did not reproduce the formula from a naive `base + (z*128+x)*16` cell-address read; `compute_cell_
  light` (0x4BE230) builds its map-cell address with `x`/`z` masked by `0xFE`/`0xFE00` before scaling
  (pop3-rev/docs/functions.tsv), which a plain row-major calc does not reproduce bit-for-bit - a live tool that
  reproduces that exact packed-index addressing would be needed to verify sloped cells numerically; the formula
  itself is re-confirmed correct from the disassembly regardless (pop3-rev/docs/functions.tsv 0x4BE230/0x4BE0B0).
- Impact on our specs: see terrain-textures.md "bigfade"/"disp"/"Our bake" and TODO.md "Terrain" - in particular,
  `game_core::shading::SunParams::default` now ships the flat 147/147/147 confirmed above, not the
  `ShadeRange`/`ShadeStart`-scaled guess it used to.
- Re-read 2026-10-20: `set_texture_landscape_main`'s own preamble, before the corner/quad math above, walks the
  mesh's vertex-grid points and for each one reads its cell's coastal shape (`+0xC` low nibble, table 0x5AA328,
  the same field "Parking and the shore" documents) and skips the whole land-colour computation only when the
  shape's class byte has bit 0x2 set ("open sea", shape 1). Confirms land cells AND shore cells (shapes 0, 2-15
  alike) both go through the ordinary blend/bigfade/cliff pipeline above - a coastline's own steep land-to-seabed
  height delta (the heightmap is per-vertex and continues under the water, level-format.md) legitimately pushes
  `blend` high almost everywhere along a shore, which is why the original shows a visibly rockier/"cracked"
  cliff-table colour right at the coastline: it falls out of the existing blend/cliff formula at a steep edge,
  not a separate shore-specific decal or texture. Only pure open sea is excluded from this pipeline and drawn
  separately (its own water surface, not traced this session).

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

## Idle behaviour [exe]
A person's per-turn state dispatch (`unit_processing_class_1_person`, table at 0x4D3D18, keyed by `+0x2C − 1`,
decoded in full: 44 entries, state N's handler at row N−1) was read end to end for the states below.

### The idle hand-off: state 1, then state 10
State 1 is **not** the persistent idle state (correcting the earlier unconfirmed guess). Its handler (0x4D3386)
only counts down `+0x70`; on expiry (or global flag byte 0x89D17C bit 0x2 forcing it early, not traced) it reads
the person's type (`+0x2B`) into a 9-row table at 0x5A7064 (byte 0) for the next state:

| Type | wild | brave..shaman (types 2-7) | angel |
|---|---|---|---|
| Next state | **8** | **10** | 28 (not traced) |

Every ordinary tribe person lands in **state 10**, which is a one-line forward to `order_execute` (0x432590,
"Command groups"): if the person's command group resolves to nothing at all, `order_execute` does only minor
facing/cell housekeeping and returns "no change" - **the person does nothing observable, forever, until
something external assigns it a group**. No wandering and no autonomous order-taking (wood-gathering included)
was found in this path. State 10 completing a group's order returns to state 1, forming a short 1 ⇄ 10 cycle.

Only one non-player, non-combat source of an autonomous command-group assignment was found: a training
building's own per-turn code (0x405B80, "Training buildings") gives a handful of its occupants a kind-3 "go to a
position" order from a per-building-type zone table - read so far as occupant/queue positioning (a training or
worship spot), not a wood-fetch assignment; its own "doing it" handler (kind 4, 0x433800) is not yet read, so
this is inferred, not proven. No caller of the gather-wood state (0x495D70, "Chopping") was found outside the
player's own `Order::FetchWood`/`CutTree`-equivalent command path.

### Idle-fidget animations: confirmed for wildmen, not (yet) for followers
Wildmen (type 1) alone reach **state 8** (0x4D60D0) from state 1. Every turn it runs its own LCG (seed 0x89BC72,
the same `x*0x9E3D+0x24DF; ror 13` shape as the shared 0x89D178 generator) for a ~1-in-256 chance of an ambient
sound; every 4 turns it ticks two sub-timers (`+0x7B`/`+0x7C`); every 16 turns, once either hits 0, it looks for
a nearby "reference unit" (0x4D48E0) and on success enters **state 5** (0x4D6970, already traced as "idle near a
reference unit"), which on its own timer picks a per-type animation/sound id from the same 0x5A7064 table (bytes
3+, the odds table already named "0x5A7067") scaled by its own RNG roll - the confirmed idle-fidget-variant
picker, on a 16-turn (1.33 s) re-roll rhythm. On failure to find a reference unit it instead tries a second
search (0x4D4A00) and may enter state 6 - now fully traced, see "Wildmen".

Whether an ordinary follower (brave, warrior, ...) ever reaches state 5 on its own was **not confirmed**: no
write of state 5 was found inside `order_execute` or its kind-3/kind-6 handlers, only inside the preach-order
handler (0x43C7A0, setting the *target's* state, already flagged as probably unrelated to conversion) and about
seven other call sites across the exe that were not read this session. So the fidget-picking mechanism itself is
real and precisely rhythm-gated, but as traced it is wildman-only; whether followers share it is open.

## Wildmen [exe]
A wildman (person type 1) cannot be given an order: state 1's per-type next-state table ("Idle behaviour") routes
only types 2-7 to state 10 (`order_execute`) - type 1 alone goes to state 8, and no wildman-reachable state ever
touches the command-group machinery. Wildmen are also always tribe `0xFF` (tribeless).

### Two deliberate destinations: a tree/plant first, the shore otherwise
**Correcting an earlier draft of this section, which wrongly called the destination "random, not a deliberate
resource target"** - re-reading both search functions in full, and the scenery-type flags table they key off,
directly from the exe shows both searches are deliberately targeted, not random:

From state 8, it first calls **0x4D48E0** - a ring search, radius 18 cells (per-type table 0x5A7069), over
nearby cells' unit lists, matching the first **scenery (class 5) object whose type has bit 0 set** in a per-type
flags table at **0x5A79C4** (24 bytes/record, read directly from the exe):

| Scenery type (`pop3-rev/docs/unit_types`) | Bit 0 |
|---|---|
| 1-3 mass/special tree | set |
| 4-6 mass/special fruit tree | set |
| 7-8 plant 1/2 | set |
| 9 head, 0xA fire, 0xB wood pile, 0xC pillar, 0xD rock, 0xE portal, 0xF island, 0x10 bridge, 0x12/0x13 scenery | clear |

Only the 6 tree variants and 2 plant types are flagged; nothing else is, and a class-1 person is never matched by
this loop. On success both the wildman and the found tree/plant transition to **state 5** (0x4D6970,
"Idle-fidget animations") - **this is the "eating" stop**, deliberately beside a tree or plant.

Only if no tree/plant is found within 18 cells does state 8 fall back to **0x4D4A00** - a second, separate ring
search, fixed radius 12 cells (scalar byte 0x5A709B), that **rejects** any candidate cell whose coastal-shape
class (table 0x5AA328, 14 bytes/record - the SAME table `is_land_at`/0x44F980 uses for a cell's coast shape,
"Parking and the shore") lacks bits 0x3C (straight-coast/corner/bay/opposite-corners), rejects the
opposite-corners class alone, and requires a parallel sanity table (0x5AA32C, confirmed 1 for shapes 2-13, 0 for
0/1/14/15) to be nonzero, plus a walkability/occupancy flag. **Net effect, confirmed from the raw exe tables:
only straight-coast/corner/bay SHORE cells are ever returned - pure inland and pure open-sea cells are explicitly
rejected.** This is the "drinking" destination: the wildman writes it to `+0x4F` and enters **state 6**
(0x4D342A):
```text
state 6, every turn:
  0x4D66E0 ("notice something nearby", unrelated to the tree/shore search above): pick one random cell at a
    -2/0/+2-per-axis offset, scan its unit list
    scenery class 5 type 9 ("head", docs/unit_types - not a tree) or vehicle class 4 type 3 (identity
      unconfirmed) → accept at once
    a person of a DIFFERENT tribe → accept with 18.75% (24/128) chance; a same-tribe person is always skipped
    on accept: both units' state → 0x22 (34, untraced - the natural next trace), cross-linked via +0x72
      (same reciprocal shape as state 5's binding); state 6 itself does not change this turn
  otherwise, decrement +0x70; while time remains:
    the destination +0x4F is re-checked against the SAME coastal-shape-class test as the 0x4D4A00 search itself
      (0x405050 + table 0x5AA328 bits 0x3C) - not generic walkability: if the cell ever stops being shore-shaped,
      that is treated as "done", same as a timeout, not "keep walking toward now-invalid terrain"
    not yet arrived (0x432DA0, within 56 units both axes) → keep walking there (generic movement system)
    arrived → reroll +0x7C (0x4D4870) → state 16 (below)
    invalid terrain, or +0x70 runs out → give up, fall back to the per-type default next state (0x5A7064
      byte 0) → state 8 for wildmen
```
**State 16** (0x4D6E40, the "at the shore" idle): while its own `+0x70` runs, cycles a sub-state in [8..19] every
few turns through **0x4D4040** - read in full and confirmed to be a generic "set the unit's secondary attached
prop" routine (called from ~80 unrelated combat/order/construction sites exe-wide), **not** an animation-id
table as an earlier draft assumed - toggling between two near-identical descriptor records (table 0x5A6858
entries 3/4, object ids 32/24) by comparing `+0x33` to `ds:0x5A6864`; no terrain/scenery read happens in this
specific toggle. State 16 also rolls its own ~50% per-turn ambient-sound chance (the same 0x89BC72 LCG seed as
state 8); on timeout, the same per-type fallback closes the loop back to **state 8**.

The full cycle observed in play is **1 → 8 → (5 or 6) → 16 → 8 → ...**: an idle tick that checks for a tree/plant
first (state 5, beside it) and only wanders to the shore (state 6 → 16) when none is near. **Confirmed in play
[observed]: it is always fruit-eating at state 5 (the tree/plant stop) and always drinking at state 16 (the
shore stop), never the other way round or ambiguous** - the animation is not a separate choice at all, it falls
out of which search succeeded: state 5 only ever triggers beside a tree/plant, so whatever it plays IS the eat
animation; state 16 only ever triggers at the shore, so whatever it plays IS the drink animation. What is still
not pinned to a specific line of code is the literal VSTART id each state resolves to for wildmen (already known
from sprite inspection: eat-fruit = 2, drink = 4) - `0x4D4040`'s own 3/4 toggle looks too generic/cosmetic to be
it (it is reused ~80 times elsewhere for unrelated props), so that specific animation-id lookup likely lives in
the main VSTART/VFRA system (the per-type `[action × 9 + type]` table at 0x5A6D50, "Fight") rather than this
`+0x33` "attached object" system - open, not confirmed either way.

### No hunger or thirst stat, but drowning at the shore is real and common
No field read by any wildman-reachable state (1, 5, 6, 8, 16) is ever compared against a "starved"/"aged-out"
threshold, and none of them touch the health field `+0x6E` at all - **confirmed absent, not merely untraced**:
visiting a tree or the shore never consumes anything, it only picks where an idle animation plays.

**But a wildman parked at its shore spot (state 16) does genuinely, and fairly often, drown** [observed] - this
is not a special wildman mechanic, it falls out of two mismatched granularities. The shore SEARCH (`0x4D4A00`)
only checks the coarse, per-CELL coastal shape; the generic drowning check (`0x4EEFF0`, "Drowning") re-tests the
FINE, per-pixel shape at the unit's *exact* position every single turn, with no bias toward the dry side -
and a "shore-shaped" cell is by construction part land, part sea at that fine grain. A wildman idling there is
effectively re-rolling whether its exact spot counts as water every turn, with no timer and no special-casing by
type. Besides that, a wildman's health drops the same way anyone else's does: melee (`melee_hit`/0x4A39C0) or an
area-damage spell (0x514579/0x514732, health-gated, no type check) - both call the shared "dies" function
(0x4F3190, 4 call sites in the whole exe). Wildmen remain **harder to reach via ordinary combat**: the
auto-attack melee scan's target-eligibility check (inside 0x51EAB0) explicitly treats a candidate as not-an-enemy
whenever either side's tribe is `0xFF` - since a wildman always is, **no ordinary unit ever auto-initiates combat
against one**; only an explicit player attack order, an untargeted area spell, or drowning at the shore can hurt
it. Whether an explicit attack order itself also respects that tribe filter was not confirmed this session (live
check below).

**Whatever kills it, a wildman's death always looks different from everyone else's** - see "Death and the
spirit rise": it unconditionally gets the long, spinning, straight-up "climbing" spirit variant (reusing the
already-documented "flung in the air" tumble sprite), never the brief calm rising-ghost pose followers normally
get. Death anim 47 ("falling down a slope", TODO.md) is the shared combat-DYING animation group with warrior/
preacher (the fall itself, before the spirit rises) - the spirit-rise AFTER that is where the wildman-specific
difference actually is.

### Population: fixed per level, no respawn
The only code found that creates a wildman is **`effect_place_wild_man`** (0x514330): scatters up to
`LSME_WILDS_PER_SHOT` (exe default 5) wildmen in a circle around a point, via `alloc_unit(class 1, type 1,
tribe 0xFF, pos)` - confirmed the only hardcoded class-1/type-1 allocation among all 188 `alloc_unit` call sites
in the exe (register-passed sites were not individually re-derived, so this is strong but not airtight). Its only
caller is a level/scenario placement-marker dispatch (`init_unit_type_7`, alongside `place_shaman` and
`clear_unit`, paired with the identical "scatter trees" constant family) - a one-shot map-population tool, not a
spell or a per-turn routine. **No periodic or per-turn creator was found anywhere**: a level's wildman population
can only shrink (Convert, Hypnotism's temporary flip, or death), never regrow on its own.

Open: state 0x22 (what "noticing" a 0x4D66E0-matched head/vehicle/enemy actually produces); the identity of
vehicle type 3 in 0x4D66E0's accept list; which exact sub-state(8..19)/toggle in state 16 (or a different,
not-yet-found animation system) actually selects the eat-fruit(2)/drink(4) VSTART ids; whether an explicit
attack order on a wildman is accepted in the original (live check: `livemem.py` watch a wildman's `+0x6E` health
and `+0x2C` state while (a) parking an idle enemy nearby for a long time - health should never move - and (b)
issuing an explicit attack order at it); whether wildman creation can ever be re-triggered mid-level rather than
only at load (live check: watch the unit pool's wildman count, `+0x2A==1 && +0x2B==1` over the pool at 0x8E0428,
across a long session); live confirmation of the tree/shore targeting itself (watch a wildman's `+0x72`
reference-unit field on entering state 5 and read the pointed object's class/type live; watch `+0x4F`'s terrain
coast-shape byte on entering state 6).

## Combat [exe]
- Internal controller units (class 10):

| Type | Role |
|---|---|
| 10/8, 10/9 | fight, pre-fight |
| 10/1, 10/6 | formations (`process_formation_unit`, `add_unit_to_formation`) |
| 10/0xA | guard control |
| 10/4, 10/0xC | soul convert (created at 0x4D5E78 for type 0xC, from the preachee's own state 3): "Preacher conversion" |

- Per person type, the constants hold `LIFE_`, `FIGHT_DAMAGE_`, `CONV_`, `SW_BLAST_DAMAGE_` and the tower detection
  radius (see "Constant table").
- A melee is two controllers in sequence. A **pre-fight** (10/9) pairs one attacker with one target: the attacker
  runs up, kicks the target away without damage, runs up again, then the pre-fight turns into a **fight** (10/8).
  A fight holds up to 6 people in a ring and is where all the health is lost. People in a pre-fight are in person
  state 29 (0x1D), in a fight in state 25 (0x19); both link to their controller by `+0x9D`.
- Firewarriors (type 6, person-type flag `+0x30` bit 0x40) never join an existing fight, and their own attack
  order uses its own approach sub-states (10, 11) instead of 2, 3 (0x51BC2E), presumably to throw fire from a
  distance (not traced). But a firewarrior attacked by someone else goes through the pre-fight and fights in melee
  like anyone, at half speed (its draw class plays 2 turns per frame: 14-turn moves and hits) [measured].

### Auto-attack: people who see an enemy (0x4D4690) [exe]
Every person runs this check in its per-turn update; it decides who starts an attack on its own.
```text
rhythm: when class_counter & mask == 0 (person table +0x2F: 3 → every 4 turns; preacher 1 → every 2)
skipped: busy (0x4D44E0: in a vehicle or building unless a firewarrior, a state with flag 0x40 in 0x5A6F79,
  i.e. 2, 3, 5, 6, 9, 11, 14-16, 20, 22-29 (fight 25, pre-fight 29, panic 26), 31, 36-42, 44; and some orders)
radius in cells (0x51FF60): resolves the person's active command group first (below, "Command groups"); when
  there is none, or its flag byte +0x1 bit 0x20 is set, falls through to the "no order" row
  no order being carried out (state not 10), or no active group, or group flag 0x20 set: person table +0x18
  carrying out an order (state 10): per order kind, byte +3 of its 22-byte record in table 0x5A7DC0 (stride 22,
    corrects an earlier "stride 11" here - confirmed from 0x51FF60's own index arithmetic):
    0 or 4 → no scan at all: kinds 0-5, 7-21 odd, 23, 25-31 odd, 33, 35, 37, 39 and up (4 is not handled)
    2 → +0x19: kinds 6-20 even, 24-30 even, 38
    1 → +0x18: kinds 22, 32, 34, 36
  firewarrior: the same, × ALT_BAND_<b>_SUPER_INCR % by height, + 4 in a drum tower, then made odd
by type: preacher → its own scan (0x51E7B0, conversion); firewarrior → its own scan when reloaded (0x51E9A0);
  shaman → 0x51E7B0 only during some orders; everyone else → the melee scan below
melee scan (0x51E5E0): r = radius rounded down to even; the square of (r + 1) × (r + 1) cells centred on its
  cell, row by row from a corner (0x51EAB0); the first match wins, not the nearest:
    an enemy person (not allied, alive, visible: not invisible +0x10 bit 0x1000, not inside anything, not
      already in a fight other than a pre-fight, not in state 23, not of an excluded state) → 2
    in a state with flag 0x8 (1, 13, 17-19, 21, 30, 57, 58) and outside, also: a fight controller to join → 1,
      an enemy building on a cell → 3
  a match → the person gives itself an order of kind 0x15 (attack around that cell, 0x438730), then the attack
  order runs as in "Starting a melee"
```

Order kinds come in pairs: an odd kind with a sprite (`hfx0-0.dat` index at record +6) and the even kind after it.
Our guess (not traced): the odd kind is going to the order's place, the even one doing it; the text below reads the
kinds that way. The sprites hint at some kinds, but they are not drawn over people: in the game, a person who
spots an enemy (kind 21) shows no icon [observed]; where the game shows them is open. Sprites: 1 and 21 123 (an
arrow over a figure), 3 131, 5 and 7 139/147 (helmets), 11 163 (a hand over a bowl),
13 171 (a book), 15 179 (a robed figure), 17 187 (logs), 19 195 (a house frame), 23, 37 and 41 211 (two swords),
31 243 "DISGUISE", 33 251 "PREACH", 35 115 (a helmet rim), 39 259 "PORTAL", 43 267 "IN". With the guess, a
person walking under an order never auto-attacks, a person doing most orders only checks its own cell, and the
second halves of the attack (22), disguise (32), preach (34) and kind 36 keep the full radius.

| Type | +0x18 (no order) | Melee square | +0x19 (some orders) |
|---|---|---|---|
| wild | 0 | never | 0 |
| brave | 5 | 5 × 5 cells (2 each side) | 1 (1 × 1: its own cell) |
| warrior | 5 | 5 × 5 cells | 1 |
| preacher | 3 | (conversion scan) | 1 |
| spy | 0 | never | 0 |
| firewarrior | 11 | (fire scan: 9.2-17.2 cells by height, see "Firewarrior fire") | 5 |
| shaman | 3 | (only during some orders) | 1 |
| angel of death | 7 | 7 × 7 cells (3 each side) | 7 |

- Not named constants: `constant.dat` cannot change them. The `<TYPE>_DT_RADIUS` constants (person table +0x22)
  are read only by drum-tower code (0x40B450, 0x40BD20), not by this scan.
- A preacher attacker only targets preachers and shamans (0x51EC80), angels of death are never targeted by the
  melee scan, and spies and wildmen never scan.

### Order kinds and player commands [exe]
`process_tribe_cmd` (0x43E8E0, "Commands") is **not** the move/attack order path (correcting the earlier
assumption in "Open questions"): its ~76 cases are cheats, debug toggles and engine/multiplayer options (reseed
the shared RNG, give mana, pull a person out of a building, toggle screen/speed settings, ring a tribe's
buildings with an "alarm" order); only that last case touches the order-group machinery at all. The real
"give one unit an order" entry points are eight small, separate routines at 0x43B180-0x43BB54, each hard-coding
one kind and building its target from its own arguments, then releasing the unit's old groups and assigning the
new one into its *secondary* slot (`+0x8B[+0xA6]`, not the primary `+0x9B`):

| Kind | Helper | Target built from | Handler |
|---|---|---|---|
| 8 (enter building) | 0x43B180 | building id | 0x434610 `go_into_building` |
| 3 (move) | 0x43B2A0 | a packed position | 0x4336C0 |
| 6 | 0x43B3C0 | unit/building id + own cell; gated by a per-type capability table (0x5A7E48) | 0x495520 |
| 0x13/19 (attack, generic) | 0x43B540 | a unit id | 0x51A2A0 "Starting a melee" |
| 0x1C/28 | 0x43B670 | a unit id | inline in `order_execute` |
| 0x11/17 | 0x43B790 | a cell rounded to a corner grid | 0x43A4D0 (shared with 30, 31) |
| 0xF/15 | 0x43B8E0 | a door/throw point (0x404420) | 0x439D30 |
| 0x16/22 (attack, "doing" half) | 0x43BA10 | a unit id + own cell | 0x435160 |

Kind 3 is the only one of these confirmed to be a plain "move to a point". Kinds 19 and 21 both reach the
attack-order handler but are not a go/do pair of each other: **kind 19** is this generic, externally-issued
attack order (its ~9 confirmed callers are all spell-effect code; the literal UI attack-click call site was not
located); **kind 21** is issued only by the auto-attack scan's own self-order, never by anything else. Kind 22
(above) is confirmed as 21's "doing" half, keeping the full auto-attack radius once an attack is under way. The
~25-call-site kind-3 helper (0x43B2A0) is by far the most used of the eight, mostly from spell-effect code - the
player's own ground-click and attack-click call sites were not located in this pass either; map them with a live
breakpoint on 0x43B2A0/0x43B540 if needed.

### Formations [exe]
A group move does **not** spread its units across different points: every unit in the same command group reads
the identical target out of the shared record (above). The only spacing mechanism is a **formation**, a class-10
controller (type 1/6, `process_formation_unit` 0x501000) that a unit joins or creates for itself
(`add_unit_to_formation`, 0x5018C0) whenever **both**: a global flag (0x895DA6 bit 0x4, read-only everywhere
found - not traced where it is set, plausibly a per-level/option byte) is set, and the unit's own `+0x76` bit 0x8
is set, which `order_execute`'s shared tail copies (0x432DF0) from bit 0x10 of byte+0xA of the *current* order
kind's 22-byte flags record (0x5A7DC0, indexed by the **raw** kind value, not kind−3 as the code-dispatch table
is).
```text
"wants formation" (0x5A7DC0 byte+0xA bit 0x10) is SET for kinds: 3, 4, 7, 9, 20, 22, 25-30, 41, 42
                                                  CLEAR for kinds: 8, 19, 21 (enter building, both attack kinds)
```
So **move** (kind 3) can trigger formation spacing; **attack** (19/21) never does - confirming our own attack
dispatch (giving every selected attacker the identical target, no spreading) already matches the original, while
our move dispatch (grid-based spot search, "Impact on our specs") does not model formations at all.

`add_unit_to_formation`'s preconditions: not already in one (`+0x76` bit 0x20), a 24-turn retry cooldown
(`+0xAB`) at 0, `+0x5F` nonzero, `+0xD` bit 0x8 clear. It looks for a compatible existing controller
(`find_formation_for_unit`, 0x501AB0, search criteria not read) or creates one, seeded from the unit's own order
target. `process_formation_unit` then holds up to 12 members at a fixed `(dx, dz) × 16`-unit slot offset around
the controller's own position, dropping a member that strays ≥ 568 units off its slot with a bad heading and an
unreachable path (0x450450), jittering the controller's own heading periodically. Whether several units given
one move order are placed into the SAME new formation together, or join one at a time as each clears its own
cooldown, is not traced (live check: give several units one move order where the flag is on, and watch whether
`+0x76` bit 0x20 flips for all of them on the same turn).

### Command groups (the shared order record) [exe]
Selecting several units and giving one order does not copy that order into each unit: they share one **command
group**, a 10-byte record in a fixed array of 800 at `0x938830 + group_id × 10` (confirms and details the
one-line guess already in "Reference": "new command group / give a group to a unit / add an order to a group").
```text
group record (0x938830 + id × 10):
  +0x0 kind       the order kind (3-33, the "going"/"doing" pairs above)
  +0x1 flags      bits OR'd in by whoever creates the order; 0x20 seen on self-issued auto-attack/scan orders
                    (0x51FF60 skips the auto-attack radius lookup above when it is set); 0x40 seen gating
                    "Preacher conversion" below; the rest not traced
  +0x2 ref_count  word, how many units currently point at this group (0 = free slot)
  +0x6 target     dword, a target unit id (word) or a packed cell position; meaning depends on kind

alloc  0x436C20(): scans forward from a rotating cursor (0x96AA78) for ref_count == 0, wrapping past the array
                    end back to id 1 (id 0 is skipped); returns the free id
assign 0x436D00(unit, group_id, slot): ref_count += 1 (and a global active-group counter when it was 0);
                    slot < 0  → releases the unit's current primary group (0x4364D0(unit, -1), not traced) and
                                 sets unit+0x9B = group_id (its one primary group)
                    slot >= 0 → sets unit+0x8B[slot] = group_id (a secondary group; purpose of more than one
                                 not traced)
set    0x438730(group_id, kind, target_or_pos_ptr, extra_flags): no-op if the group already has this exact kind
                    and target; else writes kind/flags/target above, then snaps the target to a cell or a
                    building's door point (per-kind flags dword at 0x5A7DC0 + kind × 22 + 0xA, via 0x4044B0);
                    one of those flags (bit 0x80000) can later overwrite the group's own kind to 0xA once its
                    target resolves to a unit with +0x9D bit 0x80 set (not traced further)
run    order_execute (0x432590, called from unit_processing_class_1_person only while a person is in state 0xA
                    "carrying out an order"): resolves the person's active group (+0x9B if set, else
                    +0x8B[+0xA6]), reads its kind byte, and jumps a 31-entry table (0x432D18, kinds 3-33) to the
                    matching handler - e.g. kinds 19 and 21 both reach the attack-order handler below, kind 33
                    reaches the preach-order handler ("Preacher conversion"); two further redirects (group
                    flag +0x1 bit 0x1, or the unit's own +0x13 bit 0x10) were seen but not traced
```
The auto-attack scan above does not reuse any existing group: it allocates a fresh one just for the attacker
(`0x436C20` then `0x438730`) every time it starts a new order. A secondary (`+0x8B`) group slot is confirmed to
be where the eight "give one unit an order" helpers ("Order kinds and player commands") assign their orders
(always the secondary slot, never the primary `+0x9B`); why a unit would ever need more than one secondary slot
at once is still open. Open: the rest of the group flag byte (only 0x20 and 0x40 are known), and the two unread
`order_execute` redirects.

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
7 target kicked away, at once: velocity 100 + rand(100) units per turn at (heading to the attacker) + 120°..240°
    (+0x2AB + rand(0x2AA) of 2048: away from the kicker, ±60°); no damage; anim 16 (one tilted frame; a shaman
    stays idle, 53) for the first 2 turns of the flight; stopped (+0xE bit 0x8 clear) → sub 4
8 attacker runs again (as 5, a new 40-turn timer); reached and the target in sub 4 → the fight starts:
    a fight 10/8 at the target's place, both in state 25, placed by 0x519D10 (below)
global flag 0x89D17C bit 0x40 (an option, not traced): no kick, reaching the target starts the fight
mode 1 (pulled out of a building): both sub 3; the target is thrown out of the door (0x4044B0, 0x404420):
    speed 100, ×12/16 per turn while ≥ 40 (floor 30), from the door outwards; after 6 turns or on landing
    → target sub 4, attacker sub 5 (then as mode 0)
mode 2: both sub 1: wait up to 100 turns (8.3 s) for 0x439030, then sub 2: walk to the building door, up to
    100 turns; both in sub 2 → sub 3 as mode 1                                   (meaning unverified)
```
So the opening kick exists: a knockback without damage, once per pair, before the first exchange. Nobody is
gathered: a pre-fight is always one attacker and one target.

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
- **Witnessing and joining a fight**: there is no free-standing "anyone within some fixed reach joins"
  rule - all six call sites of `join` are inside the attack/pre-fight/fight code itself. A person only notices a
  fight on its own **auto-attack scan rhythm** (the melee scan's "a fight controller to join" match category,
  flag 0x8 on the matched unit's state - re-derived this session, unchanged: states 1, 13, 17-19, 21, 30, 57, 58),
  which only runs while that person has no order, or an order whose kind is not in the no-scan bucket
  ("Auto-attack"); once a person is itself mid-attack (kinds 19/21, both no-scan) it stops scanning and only
  joins if its *own* target happens to already be the one fighting. What flag 0x8 itself means is still open:
  neither state 25 (fight) nor 29 (pre-fight) carries it, and several of its state numbers are reused with
  unrelated meaning by other unit classes, so it may be a loosely-reused generic table rather than a
  purpose-built "in combat" flag.
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

### Preacher conversion ("soul convert") [exe]
The preacher does not convert in the melee scan: `unit_processing_class_1_person`'s state dispatch (table at
0x4D3D18, keyed by person state `+0x2C`; cross-checked against the known fight = state 25 and pre-fight = state 29)
sends state 3 to `0x4D5CF0`, state 23 to `0x4D83B0` and state 24 to `0x4D8E40`. Roles below are inferred from field
use (population cap on the tribe at `+0x89` beyond; "no breeding" tribe flag; the melee scan's own exclusion of
state 23 targets) and not yet watched live.
```text
preacher's own scan (0x51E7B0), replacing the melee scan on its auto-attack turn (same rhythm/radius as the melee
  scan; "Auto-attack"): scans its square twice (0x51F030, not fully read) for an enemy person and, separately, a
  candidate building
  person found (preacher or shaman only, 0x51EC80) → ordinary attack order (kind 0x15): preachers and shamans
    fight each other in melee like anyone else, no conversion on a person match
  building found → an order at the preacher's own position (kind 0x20/32 pushed); kind 32's handler is confirmed
    to be the same one as kind 31's ("DISGUISE", 0x43A4D0: a 5-substate approach-and-RNG-idle machine with no
    attacker-type branch in it), so the building-scan branch does not lead to conversion by itself - see
    "Command groups" and the open questions below

state 3 (0x4D5CF0), the preachee the instant it is struck: at its own position and tribe,
  spawns a 7/0x4D effect (a soul-like visual, not confirmed) and a class 10 type 0xC controller ("Soul Convert 2")

state 23 (0x4D83B0), the preachee while a conversion is in progress; `+0x89` the preacher, `+0x70` the timer:
  abort (idle) if the preacher is gone, or not carrying out an order (state != 0xA) and not itself preaching
    (state != 0x21/33)
  every turn: timer -= 1
  timer < 0, OR the preachee's own tribe has the "no breeding" flag (tribe +0x941 bit 6: set for, at least,
    wildmen) which forces a roll every turn regardless of the timer:
    rearm the timer: PREACHEE_CONV_FREQ (exe 500 turns), halved to 250 unless global flag byte 0x89C668 bit 2 is
      set (meaning/scope of that byte not traced: many unrelated bits of it are read all over the exe), jittered
      by the pop3 PRNG by roughly ± (freq / 16) turns
    roll: success if rng() % PREACHEE_CONV_CHANCE (exe 5) == 1, or flag 0x89C668 bit 2 is set, or the preachee's
      tribe has the bit-6 "no breeding" flag (both make every roll succeed)
    roll failed → rearm the timer again at the full PREACHEE_CONV_FREQ (not halved this time) and continue
    roll succeeded:
      the preacher's tribe is at its population cap (tribe +0x89DAE5 array >= 200, "Village growth") and the
        preachee's tribe does not have the bit-6 flag → nothing happens this turn, try again later
      else → queue a trailing visual (particle list 0x892443, `[0x892443]` += 0x14 per entry) at the preachee's
        position; spawn a person of the preachee's own type in the preacher's tribe, at the preachee's position,
        with a random short-range offset and a movement-profile-scaled random speed; copy the preacher's facing
        bits; play a sound (0x48A050(preacher, 5, 0)); spawn a class 7 type 0x3A ("soul") effect at the
        preacher's position and link it to the new person (`+0x72` = the new person's id, presumably so the
        effect's own per-turn routine flies the sprite from the preacher to the new body - not traced)
      destroy the preachee (0x4D4B50) either way once a roll has succeeded this check

state 24 (0x4D8E40): the same shape as state 23 (target via `+0x89`, a check gated the same way), but increments
  a second counter at the target `+0x2D` (capped at 0x13/19) instead of rolling a chance - not fully read; likely
  the building-occupant variant paired with the class 10 type 0xC controller, the counterpart of state 23's
  type 4 (unverified: no creation site for type 4, only for type 0xC at 0x4D5E78, was found)
```
- Numbers: with the file default (12 turns/s), `PREACHEE_CONV_FREQ` 500 is 41.7 s between checks (20.8 s with
  flag 0x89C668 bit 2 set), each with a 1 in `PREACHEE_CONV_CHANCE` (5) chance: about 5 checks (3.5 min at the
  full interval, worst case) on average per conversion once a preacher has engaged a target, instantly for a
  wildman or any other member of a "no breeding" tribe.
- A converted person keeps its original type (a converted warrior stays a warrior, in the preacher's tribe), not
  the preacher's own type.
- Open questions: what makes a preachee enter state 3 in the first place is still not pinned down, but narrowed
  this session, correcting several of this section's earlier guesses:
  - kind 32 (0x20, the preacher's building-scan order) is now confirmed to be disguise's own second half, not a
    separate conversion path (see above) - ruling out the building scan as a direct trigger.
  - the player-issued preach order (kind 33, the preach-order handler 0x43C7A0) walks the preacher up to its
    target and, on arrival, plays a sound (0x9F) on the target and sets the target's own state to **5**, not 3,
    while chaining its command group's kind 0x98 → 0x9B → 0x99 over 40 ticks; state 5's own per-turn handler
    (0x4D6970) reads as a generic "idle near a reference unit" behaviour (it only picks an RNG animation variant,
    never writes the person's own state), so it looks unrelated to conversion rather than a step towards state 3.
  - the melee target-eligibility check (0x51C3C0, called from the attack-order handler 0x51A2A0) has a gate that
    fits exactly this gap: a preacher may only engage a non-preacher person target if the attack order's own
    command-group flag byte `+0x1` has bit 0x40 set - otherwise a preacher can only ever fight other preachers or
    shamans (0x51CE50, "Starting a melee"). This flag is the best remaining candidate for what turns an ordinary
    attack into a conversion attempt, but which code sets it was not found: of `order_set`'s (0x438730) call
    sites, only one passes 0x40, and it belongs to an unrelated per-tribe routine creating order kind 0x1C (28).
  - the "lookup table at 0x938830" from earlier drafts of this section is not a lookup table: it is the shared
    command-group record array ("Command groups"); whatever state 23/24 read there (values 0x11/0x1F/0x20, i.e.
    kinds 17/31/32) is a group's own kind byte, not re-checked against this session's findings.
  - still untraced: 0x5A707E (a per-type byte read on a model-variant branch) and the exact meaning of flag byte
    0x89C668 (many unrelated systems read other bits of the same byte, so it is unlikely to be preach-specific).
- A live check would settle this precisely: watch a preacher's and its target's command group (`+0x9B`, resolving
  into `0x938830`) and the order's own `+0x1` flag byte alongside state/sub-state/`+0x70` timer and tribe every
  turn (as `fightwatch.py` does for firewarriors), while ordering a preacher to preach an enemy brave, until the
  brave disappears and a new one appears in the preacher's tribe.

### Attacking a building [exe]
An attack order on an enemy building (`+0x89` the building; sub-states in `+0xA8`, 0x51A2A0):
```text
at the door, 16 turns: hacking anim (action 6: anim 11, the axe chop; a shaman stays idle) and the building shakes
  then occupied (0x51E300: people inside, or enemies on a plan's cells) → one occupant is pulled out
    (0x407490) and a pre-fight mode 1 starts ("Starting a melee"), again while people are inside
  empty → hacking turns:
hacking (0x51B37D): anim 11, sound 1, for 8 + rand(16) turns; every turn:
    shake(building, 6) (0x407810), building +0x9C bit 0x10 (attacked)
    damage(building, attacker) (0x409140): unless the attacker is a ghost,
      building +0x9E += person table +0x16 >> 2: brave, preacher, spy 7; warrior 12; shaman 10; firewarrior 16;
      angel 31; the attacker's tribe kept in +0xAF; the player warned once (0x89CE60, map position 0x89C6E5)
  then → walk to a random point 360 units from the building's cell centre (0x51B42E), and hack again
```
The building's side, every turn (`unit_processing_class_2_bldg` 0x403280, 0x4092A0):
```text
shaking (+0x9C bit 0x2, timer +0xA7 = 6, refreshed by every blow; not while burning): draw offsets +0x6C, +0x6E
  each ±2, flipping with class_counter bits 1 (x, z out of phase): a small jitter of the whole model, gone 6
  turns after the last blow
+0x9C bit 0x40: the accumulator also gains 24 + rand(128) per turn (what sets the bit: not traced)
every 4 turns: accumulator −1
accumulator >= threshold (building type +0x2E: 2000; wall 16 000, prison 22 000; library 0: never):
  accumulator −= threshold;  one structural hit: −100 wood (0x4BA2C0), with:
    a size ≥ 3 building with people inside: everyone thrown out
    a damage smoke (0x40B320) at one of the model's smoke points, lasting DME_RESTORE_TIME (6000) + rand(256)
      turns (8.3 min)
    a partial explosion (0x407860: internal 10/7 "object face" units, the model's faces flying off, and smoke 7/3)
    repair delay: the building's plan +0x94 = SW_BLDG_DAMAGE_DELAY (exe 2000, file 1200 turns: 100 s), counted
      down by the plan (0x4B8BB0, state 2): no repair until it reaches 0; at 0, 0x40AFD0 (probably the smoke
      removal)
    the plan's +0xA0 = the attacker's tribe
  no wood left: the building is destroyed (stats[tribe][7] += 1, sound 0x34)
```
- So damage goes through an accumulator, not straight to wood: one brave hacking (7 per turn, −1 per 4 turns
  decay) takes about 300 turns (25 s) per 100 wood, plus its walks between spots; a small hut (300 wood) about
  75 s, a warrior about 14 s per 100 wood (estimates, not measured). Several attackers add up. Firewarrior shots
  (100 file) and blasts (`BLAST_DAMAGE_BLDG` 40) feed the same accumulator.
- "Unusable for a while": a building destroyed this way leaves ashes on its footprint, which cannot be built on
  for about 8.7 min ("Building ashes").

### Building ashes ("Damaged Ground") [exe]
The ashes are a mark on the landscape, not a sprite, a model or a unit: the destroyed building's footprint cells
get the map-cell flag 0x4000000 and their blend byte (cell +0xA) forced to 127, the landscape texture then draws
its darkest "cliff" colour row there. Mouse-over text: "Damaged Ground: Cannot be built on." (lang00 string 0x348).
```text
map cells: 0x8A03E4, 16 bytes, 128 per row; +0 flags (0x2 units, 0x10000 reincarnation site, 0x4000000 damaged
  ground, 0x8000000 lava), +4 height, +0xA blend byte (normally clamp((max neighbour height − min) / 8, 1, 127),
  0 at height 0, from level_land_processing_2 0x44DF40, which skips flagged cells), +0xD precomputed brightness
  byte for the landscape texture (see "Terrain land colour"), +0xE high nibble downhill direction (0 flat, 1-8
  compass, low nibble a precomputed ambient-light term, also "Terrain land colour") - same 0x44DF40, see
  "Volcano lava flow"

a building losing its last wood (wood +0x96 <= 0 on its shape +0x82):
  attack accumulator 0x4092A0 (branch 0x409498), fire 0x408AB0 (0x408C38), whirlwind 0x4980A0 → 0x40B130
    make_ashes(shape)  0x40B230
    delete the shape and the building, stats[tribe][7] += 1
make_ashes 0x40B230:
  own tribe, not type 0x13: 0x499D90(0x4000, 0x253)   (probably the "building lost" alert, not checked)
  every footprint cell (0x4B9E20, building-type table 0x59DF3C, mask bit 0x2): +0xA = 127, flags |= 0x4000000
  damage smoke 7/0x4C (0x40B320) lasting DME_RESTORE_TIME + rand(256)
  scorch_add(kind 1, position, building type)   0x4509D0
scorch list 0x969E9F: 100 slots × 8 bytes (age, kind 1 footprint / 2 rectangle, phase, size, position);
  full: the oldest slot is cleared at once and reused
scorch_tick 0x450A70 (from 0x4ECA09), on turns where turn & 15 == 0, every slot: age += 16
  phase 1: age >= DME_RESTORE_TIME (exe and file 6000) → phase 2
  phase 2 (0x450B30): every flagged cell with +0xA >= 0x5C: +0xA −= 2, at <= 0x5C the flag is cleared;
    a pass changing nothing → phase 3, the area's blend recomputed from its slopes (0x44DDF0)
  phase 3: slot freed
placement 0x44EE50: a cell with flags & 0x4010000 cannot be built on
```
- Durations: 6000 turns (500 s, ±15 from the 16-turn check) of full ashes, then a 288-turn (24 s) fade, 127 to 91
  in 18 steps of 2; the building ban lasts about 6290 turns (8 min 44 s) per cell.
- Drawn (`set_texture_landscape_main` 0x4BE330, texels 0x4BEC09-0x4BECB1): for each rendered quad (the 4 cells at
  (x,z), (x+2,z), (x+2,z+2), (x,z+2)), each corner samples that ONE cell's own +0xA directly (no averaging across
  neighbours at this step) - +0xA / 4 picks one of the 64 rows of the 128-wide table 0x5D45CC (8 KB, named
  `set_cliff_palette`, probably `data/cliff0-N.dat`, guessed from its size); a non-zero +0xA also adds 0x4B to
  that corner's own altitude colour index (height + 0x4B, max 0x3FE). Both values are then interpolated smoothly
  across the quad like any other vertex colour (the code computes per-corner deltas scaled by 0x1000 for the
  fill) - **this, not any lava-specific effect, is why adjoining hot cells visually read as a soft round blob
  per cell that fuses into a connected glow where two hot cells share an edge**: a single hot cell peaks at its
  own corners and fades toward cold neighbours, while two adjacent hot cells keep their shared edge bright, with
  no dip between them. The SAME mechanism colours every slope/cliff/ash tile; lava is just the fastest-moving,
  highest-contrast case of it. Ashes are row 31, the darkest a normal slope reaches; lava goes up to 0xFF (rows
  32-63, presumably glowing) and cools back to 127. **No animation exists in this draw path or the palette
  table itself** (confirmed: `set_cliff_palette`/0x4BDB50 has only 2 call sites, both one-shot level-asset
  loaders - `load_al0`, `load_bigf0_cliff0_disp0` - never called again per frame or per turn, and nothing in
  `set_texture_landscape_main` reads the turn counter, a PRNG or any oscillator) - **the "brighter, then fading"
  look is the blend byte's own real value doing exactly what "Volcano lava flow" and the ash table above already
  describe** (rising to 0xFF as a cell matures and spreads, falling 8/turn over its 16-turn wind-down, then 2/turn
  as the resulting permanent ash cools from 0xFF back to 127), reflected straight into this same per-cell colour
  lookup every frame - not a separate pulsing/flicker effect layered on top. The row colours themselves were not
  checked. Only other visual: the long damage smoke.
- No elevation-vs-theme override sits anywhere above this mechanism: both lookups (bigfade row, cliff row) are
  keyed by height/blend into whichever table the level's own theme loaded once, never by a global height band
  independent of theme (see terrain-textures.md "bigfade"/"cliff" and TODO.md "Terrain" for the reimplementation
  gap this explains - a hardcoded universal "snow at high elevation" band with no counterpart here).
- Ashes or not:

| Destroyed by | Path | Ashes |
|---|---|---|
| attackers (melee, shots, blasts) emptying the wood | 0x4092A0 → 0x40B230 | yes |
| fire (lightning 0x408CB0, firestorm, lava → state 4) | 0x408AB0: fire timer +0xA7 127, occupants out at 0x77, one −100 wood hit at 0x4F | yes, if that hit empties the wood; else back to state 1 |
| whirlwind pulling the wood out | 0x4980A0 → 0x40B130 → 0x40B230 | yes |
| earthquake shake (turns 24, 56, 88, 20/128 chance per building) | 0x50D9A0 → effect 7/0xE (init 0x50FE60) → 0x4980A0(b, 1): −100 wood dropped as a pile 5/0xB, then a partial explosion | yes, if the wood runs out |
| foundation check (terrain no longer fits): the earthquake's sinking, swamp, flatten, land changes | 0x408080 → state 3 sub 1 | no: explodes |
| volcano | 0x50E5FB → state 3 sub 1 | no (its own lava cells are damaged ground) |
| foundation sub 2 | 0x4082A0 | no |

- State 3 (0x406F40, from the class-2 dispatcher 0x403280): a 2-turn countdown (+0x67); at 1 everyone inside is
  thrown out (0x407490); at 0, sub 1 a partial explosion (0x407860) and a blast 7/1, sub 2 a unit 10/0x13; then the
  building is removed (0x403860, landscape 0x403A00) without calling 0x40B230.
- Other damaged ground, same flag and list: lava squares (0x512B94, kind-2 rectangle at 0x513096), the earthquake
  effect 7/0x1A (0x50D9A0, rectangle at 0x50DFB7), one cell at 0xC4 (0x5134C0); the volcano cone sets the flag
  without a list entry (how it clears is not traced); 0x515AC0 clears a cell outright (caller not traced).

### Firewarrior fire [exe]
A firewarrior attacks from a distance with its own attack-order sub-states (person `+0xA8`, in 0x51A2A0): it
faces the target, throws, then waits for its reload.
```text
range (0x51F990), a square per axis around the firewarrior:
  cells = 11 (person table +0x18 of type 6) × ALT_BAND_<b>_SUPER_INCR / 256,  b = clamp(height / 128, 0, 7)
  + 4 in a drum tower;  cells |= 1;  reach = cells × 256 + 56 units
  then a line of fire, sampled every 384 units (from 0x51FAA2, not read in detail)
  the caller at 0x4E938B passes a flag adding 2 cells after the odd step (0x51FA10); option 0x89D17C bit 0x2:
  19 cells whatever the height
  file (80 % at band 0 .. 150 % at band 7), reach per axis: bands 0-1 9 cells (2360 units), band 2 11 (2872),
  bands 3-4 13 (3384), bands 5-6 15 (3896), band 7 17 (4408); a drum tower adds 4 before the odd step
  (exe: 100 % everywhere, 11 cells)
throw (+0xA8 = 0x2C): anim 7 (the firewarrior throwing fire, 5 frames, 1 turn each), speed 0 (half its speed
  when aboard a vehicle), and at once fire(self, target) (0x51FBF0):
    two shots 8/6, one per hand: started 96 units to the left and to the right of the thrower, at its height
      + its own height (+0x1C) + 16; launch sound 0xA1
    only the second (right) one hits; the left one only flies (its arrival does nothing)
    a ghost firewarrior's shots are ghosts too (+0x10 bit 0x800): they do nothing
    reload (+0xB2) = SW_FIRE_RATE (exe 36, file 25), SW_FIRE_RATE_TOWER (36) in a drum tower;
      bloodlusted: >> BLOODLUST_SW_BLAST_X (exe 2, file 3: 25 → 3 turns)
reload (0x4D42A4, every turn for type 6): −1 per turn; its last step only on a turn where class_counter & 3 == 0,
  so 25-28 turns (2.1-2.3 s) between throws with the file values
```
The shot 8/6 (init 0x4BBCF0, per turn 0x4BBF30, in the main loop's own list):
```text
speed 512 units per turn (1 cell), homing: aims at the target's position + its height + 16 every turn (a building:
  its door point at ground height); arrived when within one turn's move (3D), then a last 15/16 step
every turn: a smoke puff 7/3 at its position (a trail); own people standing still in the 3 × 3 cells around it get
  +0x10 bit 0x400000 (cleared each turn by their own update; probably a duck or flinch, unverified)
the hitting shot carries a ground light (`sunlight_update_unit_landscape` 0x4010B0(shot, 2, 4, 0): one of 50
  dynamic lights that brighten the landscape under it)
arrival of the hitting shot: snapped onto the target, sound 0xB2, then hit (0x514410):
  every enemy (not of an allied tribe, 0x416D70) in the impact cell:
    person: 100 (× BLOODLUST_DAMAGE_X when the shot is bloodlusted), killed → credited to the thrower
    vehicle: 100;  tree (scenery flag 4): set burning (+0x65 = 1, +0x61 = its cell)
  the target, if it was in that cell:
    person: + SW_BLAST_DAMAGE (exe 400, file 100; SW_BLAST_DAMAGE_TOWER 400 from a drum tower)
              × SW_BLAST_DAMAGE_<type> % (exe 100 % all, angel 10 %; file: warrior 5 %)
            a shaman inside a building or vehicle: max_life / 2 instead (0x5AA5E8 = 128 / 256)
            a firewarrior target fires back (0x520610), an angel of death reacts (0x4DE570)
    building (its own cell): structural damage SW_BLAST_DAMAGE (0x409200) and set burning (+0x65, +0x61)
  every person hit is thrown: dead, or a type without person-table +0x31 bit 0x2 (brave, spy, firewarrior,
    shaman): 64 units per turn away from the thrower + 110 up (blown, "Person physics"); a living warrior or
    preacher only hops (vertical speed 36); a building or vehicle holding the person is flagged (+0xA0)
```
So with the file values a firewarrior's hit costs a brave 200 (100 in the cell + 100 targeted), a warrior 104 (5 % stored as 12 / 256, so 4), and
anyone else standing in the target's cell 100. The live capture of "Combat" shows those 200-life steps.

Drawn:
- The shot: hfx 1120-1123 (`hfx0-0.dat`, alpha range: a 16 × 16 white-yellow fireball, 4 frames), draw class 29
  (table 0x5A6AF8 row 29); the volcano's small fireball 8/7 uses the same sprite (0x4BC537), the big 8/8 hfx 1248.
  1124-1127 (the same ball shrinking: 14, 11, 9, 5 pixels) follow it, probably its fade (not traced).
- The trail: a smoke puff 7/3 every turn (init 0x50BF60): hfx 314-321 (a 15 × 16 puff dissipating over 8 frames),
  rising 20 units per turn, 4 turns of life, its randomness from a second generator (0x89BC72, not the game's).
- A light on the ground following the hitting shot; no explosion sprite at the impact (none spawned by the hit),
  only sound 0xB2 and the victims thrown.
- The thrower: anim 7 (5 frames, gear drawn in).

### Measured (a level against tribe 1, `fightwatch.py`, about 500 turns) [measured]
- Timings as traced: the opening blow 6 turns, the target flung when the attacker has 2 left; moves 2, 3, 4 last
  6, 7, 7 turns, a hit 7 (4 then the knockback for move 2); a firewarrior's move or hit 14. A pair exchanges
  about every 8 turns (0.67 s). With the attacker already in reach, a pre-fight lasts about 8 turns before the fight.
- The ring radius is 180 (two fighters 179 apart). Fights were always 1 against 1-3, never 2 against 2.
- Moves 2 and 3 cost both fighters health, move 4 only the target, as traced.
- The opening blow's flight: 190-370 units on its first turn, then slowing: 0.7 to 2.5 cells in all (the long ones
  kept sliding for up to 10 turns, probably downhill: "Person physics"). The fight's knockback (sub 7): about 50.
- Damage fits `FIGHT_DAMAGE × life / max_life` only with each person's own maximum (`+0x6C`), which is not always
  `LIFE_<type>`: these braves fitted about 800 (tribe 0) and 970 (tribe 1), and a warrior's blows (270-320 against
  braves) fit about 1450-1620 instead of 1800. Where the maximum comes from is open.
- Healing, outside and inside fights, every 8 turns (turns that are multiples of 8): brave and preacher +4,
  warrior and firewarrior +6, as traced ("Health and healing").
- A second capture (4 tribes, about 1700 turns) adds:
  - a shaman target is flung by the pre-fight's opening blow like anyone (about 3 cells, sliding): only the fight's
    knockback spares her, as traced; in the fight she only used moves 4 (5 turns) and the 7-turn hit, and healed
    +32 every 8 turns;
  - the same id was read in 2-3 slots of one fight (#1620 in fight #1134, turns 904-941; #1298, #1292), `n`
    counting them, and that person's move timer then dropped 2 per turn (turns 911-913: 6, 4, 2) as if handled
    once per slot. Doubtful (not seen in the game, the watcher reads memory while the game runs): unverified, to
    check with a raw dump of the fight's `+0x70` slots and the person's `+0x9D`;
  - pre-fight mode 2 seen: both people walking side by side (about 82 units per turn) for up to 100 turns, the
    target stopping at its building's door, then thrown away from it (about 2.8 cells over 10 turns, the speed
    rising to about 185 per turn before slowing, faster than the traced ejection; slope not checked) while the
    attacker runs up (sub 5);
  - people leave a fight or a pre-fight into state 10 (an order), state 2 (drowning), state 14 (both cases were
    preachers, so probably preaching; not traced), kept in the fight's list for 15+ turns, and state 44 (struck by lightning).
- Edge cases seen: the attacker in one pre-fight became the target of another, and the first one stood
  stalled (attacker in sub 4) until a fight took it away; a fighter walked off by an order was pulled into a new
  pre-fight and its fight ended; a person kept running in a pre-fight for 11 turns at negative life (hurt by
  something else) before dying.
- A third capture (`fightwatch.py --fire --no-fights`, turns 2548-2901, 4 tribes) of firewarriors:
  - damage as traced: a brave or preacher target loses 200 per throw, a warrior target 104 and the warriors standing
    in its cell 100 each (up to 6 hit by one throw); throws stack: 2 firewarriors on one brave 400, 3 of them 600
    (the shots arriving the same turn);
  - the bolts fly 3-8 turns, their aim point updated every turn while the target walks (homing);
  - one firewarrior on its own threw every 27-28 turns; between throws it went to state 19 (not traced) for about
    20 turns, then back to its attack order (state 10, sub 10 or 11);
  - throws happened at up to 9.4 cells (per axis) from height 0 and 13 cells from height band 3, within the reach
    above (the target walking away after the check);
  - one firewarrior at height 0 threw 5 times at the same preacher, standing still at height 186 about 5.8 cells
    away: each pair ended within one move of it, at the same spot every time, and it never lost life: probably blocked by a building
    (seen in the game, not traced);
  - the reload byte (`+0xB2`) sometimes read 252-255 on firewarriors that had read 0 a turn or two before, then
    counted down one per turn as usual: five of them stood in their attack order (sub 10, `+0xA8` = reload) next
    to their target for 20+ turns without throwing. The countdown (0x4D42A4) stops at 0 and cannot wrap, and no
    writer of `+0xB2` found sets 255 for a firewarrior (the others are the spy's and other states' flags): open;
  - healing as traced: +6 per 8 turns for firewarriors, on turns that are multiples of 8;
  - wildmen (type 1, tribe 255) going from 32 life to 0 about every 40 turns: they walk into water on their own
    and drown, unrelated to fighting (out of scope here).

Strength per type (exe default / `constant.dat`); it falls with the attacker's health, never under 32:

| Type | `LIFE_` | `FIGHT_DAMAGE_` | Full-health blows to kill a full brave (file, estimate) |
|---|---|---|---|
| brave | 1400 / 1000 | 60 / 60 | about 21 (60 falling to 32 as the attacker weakens) |
| warrior | 1800 / 1800 | 180 / 360 | 3 |
| preacher | 1400 / 1100 | 60 / 60 | about 21 |
| spy | 1400 / 600 | 60 / 60 | about 21 |
| firewarrior | 1200 / 700 | 180 / 60 | about 21, at half the rhythm |
| shaman | 900 / 2000 | 60 / 60 | about 21 |

- The file spells the shaman's constant `FIGHT_DAMAGE_SHAMAN`, the exe table `FIGHT_DAMAGE_SHAMEN`; if
  `read_levels_const` matches names exactly, the file's line is ignored (both are 60: no effect).

## Health and healing [exe, measured]
`unit_processing_class_1_person` (0x4D3B2C) calls the life step 0x4D43A0 every turn for every person, unless its
state has flag 0x1 in the state table 0x5A6F79 (stride 5): states 2 (drowning, set by this step), 3 (dying), 22,
24, 27, 28, 37, 40, 42 and 44 (struck by lightning). The fight (25) and pre-fight (29) states do not have it: people
heal and die normally while fighting.
```text
in the air (+0xC bit 0x80000, blown or flung):  nothing: no healing, and no death until it lands
cheat flag 0x89D17C bit 0x80:                   life = max_life (+0x6C)
life <= 0:                                      state 3 (dying), previous state kept in +0x7D
heals (person table +0x30 bit 0x8) and turn & 7 == 0:
    life < max_life: life += heal (person table +0x1A, a byte; no clamp to the maximum)
    else:            last attacker (+0xB0) = none
low health (+0x14 bit 0x1000, the sparkles) = life <= max_life / 4
+0xC bit 0x8 (set on fire) and not already state 31: state 31 (burning)
```

| Type | Heals | Per 8 turns | Per second | Full from empty (file `LIFE_`) |
|---|---|---|---|---|
| wild | no | | | |
| brave | yes | 4 | 6 | 1000: 167 s |
| warrior | yes | 6 | 9 | 1800: 200 s |
| preacher | yes | 4 | 6 | 1100: 183 s |
| spy | yes | 4 | 6 | 600: 100 s |
| firewarrior | yes | 6 | 9 | 700: 78 s |
| shaman | yes | 32 | 48 | 2000: 42 s |
| angel of death | no | | | |

- The heal amounts are not named constants (`constant.dat` cannot change them). The live capture of "Combat" shows
  the same +4 (brave, preacher) and +6 (warrior, firewarrior) on the turns that are multiples of 8, inside fights.
- Being thrown delays death: a person at life ≤ 0 keeps moving until it lands (the capture's runner at −144 for 11
  turns was flung by a firewarrior's bolt).

### Drowning [exe]
The life step's very first call, every turn, is a drowning predicate (`0x4EEFF0`), which defaults to "yes,
drowning" and only a handful of specific conditions escape it:
```text
not drowning if: flags +0xC bit 0x2; a per-instance exemption table (0x5A7BA8, bit 0x2 - checked, no person
    type is ever exempt via it); airborne (+0xC bit 0x80000); standing ABOVE the real ground/water height
    (calc_point_height 0x44E940 vs +0x41, e.g. a boat deck); is_land_at (0x44F980) true - solid support; inside
    something (+0x9F != 0)
otherwise, re-derive the FINE per-pixel coastal shape (the same table 0x5AA328 used elsewhere for a cell's
    coast shape, but testing only byte+0 bits 0x3C - no "is this really a shore shape" gate) at the unit's
    EXACT current position:
  shape 0 (pure land) or 1 (pure sea)  → drowning
  any boundary shape (2-15)            → drowning unless flags +0x10 bit 0x1000000 is set
```
This runs **every turn, with no timer**: a unit's exact sub-cell position decides drowning fresh each turn, at a
finer grain than the per-cell coastal-shape class used to pick a destination elsewhere (e.g. "Wildmen"'s shore
search). A "shore-shaped" cell is, by construction, part land and part sea at this fine grain - standing
anywhere in one is not inherently safe, it depends on exactly where in the cell.

### Death and the spirit rise [exe]
Death (state 3, `0x4D5CF0`, entered when life ≤ 0) does the shaman-specific mana-loss ("Shaman death") inline,
pushes the corpse's own pointer onto a small global one-slot-per-death LIFO (`0x892443`), spawns a 7/0x4D
death-puff effect, and allocates a **new class-10 object** - the "spirit" - which is not a state on the dying
unit itself but a separate controller that only references the corpse for cosmetic lookups.

The spirit's own setup (`0x502910`) pops the corpse off the LIFO and picks one of (at least) two different rise
variants for class-10 state 12 (dispatch `0x5029D0`, by its own sub-state `+0x2D`):
```text
sub-state = 3 ("climbing") if the corpse's type is WILD (1) - unconditionally, regardless of cause of death
sub-state = 3 ("climbing") if any OTHER type, but only when 0x4EEFF0 (the drowning predicate above) is
    currently true for the corpse - i.e. it died while standing in a drowning-eligible spot
sub-state = 0 ("calm") for any other type, otherwise
```
```text
sub-state 0, "calm" (0x5029EF): 4-turn timer; a per-type object id (table 0x5A6EA6) → VSTART 38 for every
    follower type (tint only differs by draw class), 85 for the shaman. No altitude change: a stationary rise.
sub-state 3, "climbing" (0x502BA3): 32-turn timer; a per-type object id (table 0x5A6EB8) → VSTART 31 for a
    wildman (the SAME clip already documented as the "flung in the air" blast-knockback tumble animation), 40
    for a follower, 45 for the shaman. Every turn: altitude += 40 world units (no gravity, no deceleration - a
    real, sustained, roughly 2.5-cell vertical launch over the full 32 turns); when the copied type is
    specifically wild, ALSO spins facing by 91/2048 every turn - a continuous spin found nowhere else.
```
**So a wildman's death always looks like a straight, spinning launch into the sky, reusing the tumble/flung
sprite - never the calm rising-ghost pose everyone else normally gets.** Anyone else only gets that same
launch (without the spin) if they happen to die while standing in a drowning-eligible spot; otherwise they get
the brief, stationary, per-type-tinted "ghost" variant. This directly answers "why does a wildman's death look
different" and is a different claim from the existing art-reference note that spirit rising is "a see-through
copy rising, drawn in post" for everyone alike (true-ish for the calm variant, not for the climbing one, which
is a real sprite object with real upward velocity).

Open: sub-states 1, 2, 4, 5 and 6 of the same dispatch (0x502A5C/0x502AEA/0x502CDB/0x502D7E/0x502E7A) were not
read; what finally removes the spirit object (presumably one of those); whether `DROWN_DAMAGE` is really 60 per
turn (not independently re-confirmed this session); a live check to directly observe timing:
`livemem.py` watch a dying unit's new class-10 object's `+0x2C`/`+0x2D`/`+0x41` (altitude)/`+0x74` (copied type)
for ~40 turns, comparing a wildman dying on dry land (expect climb+spin immediately) against a follower dying on
dry land (expect the brief calm variant) and a follower dying in shallow water (expect climb, no spin).

## Vehicles [exe]
Class 4: types 1, 2 boat, 3, 4 balloon (airship). Init `init_unit_class_4` 0x463370 (state entry, jump table
0x46372C), per turn `unit_processing_class_4_vehicle` 0x463780 (jump table 0x463B78 by state `+0x2C`), then the
life step 0x463CB0 every turn in every state. Vehicle table 0x5A7938, stride 0x17 (types 1-2 and 3-4 identical):

| Field | Boat | Balloon | Meaning |
|---|---|---|---|
| +0x0 (u32) | 8000 (file 5000) | 8000 (file 5000) | `VEHICLE_LIFE_BOAT` / `_BALLOON`: life `+0x98` at creation and refill |
| +0x4 | 143 | 144 | object index of the model (`+0x33`, draw class 2) |
| +0x6 | 838 | 839 | not read by the vehicle code (unverified: a text id) |
| +0x8 | 5 | 2 | seats: passenger ids at `+0x7A`, count `+0x9E` |
| +0xA, +0xB | 1, 3 | 2, 4 | the type's parked and moving states |
| +0xD, +0xF | 0, 0 | 560, 200 | hover heights (see "Terrain changes"; parked balloon: ground + 200) |
| +0x13 | 600 (file 400) | 1000 (file 300) | `WOOD_VEHICLE_BOAT1` / `_AIRSHIP_1`, the hut's cost |
| +0x15 | 0 | 1 | flag 1: flies (selects the balloon branch everywhere) |

### Drawing
- The model is a 3D object, not a sprite: `0x4EE700(&vehicle+0x33, 2, table+0x4)` sets the object index 143
  (boat) or 144 (balloon) and draw class 2 (`0x5A6AF8` row 2: renderer 3, the object path `0x4708D0`, the same
  as buildings and trees). The object record is `[0x895EC1] + 54 × index` (the `objs` record of objects.md).
- 143 and 144 are the boat and the airship in the layout of object banks 2-7 (objects.md: bank 0's 181 and
  182). In bank 0 they are warrior training huts, so the game's loaded object table follows the banks 2-7 layout
  (which file the game loads for a bank-0 level is still open, objects.md).
- The model is turned by the heading `+0x26` and two tilt angles, `+0x6C` (pitch, `0x47F930`) and `+0x6E`
  (roll, `0x47FA30`), 2048 per turn: the sinking, beaching and spinning below are these angles, not other models.
- Heading at creation (0x463BA0, called by the type inits 0x463220-0x46333D): on a coast cell, away from the shore
  (cell shape direction + 4, × 256).
- A parked balloon (state 2) floats at ground + 200 (+0xF); a moving one samples the ground ahead (0x4E8F00).
  A moving balloon bobs: height ± (`+0x2E` & 15 folded to 0..8) × 128 / 256 every turn, carrying its passengers.
- Sounds: moving loop 0x4F (boat) / 0x4E (balloon), started with flag 0x40 (0x48A050) and stopped when it parks
  (0x48A770); destruction 0x4D (boat) / 0x4C (balloon), played on the blast.

### States
| State | Kind | Per turn |
|---|---|---|
| 1 | boat parked | movement pass, starts moving (state 3) when its driver (0x466760) moves |
| 2 | balloon parked | same, state 4 |
| 3 | boat moving | 0x463F20 |
| 4 | balloon moving | 0x4641D0 |
| 5 | boat destroyed | sinks or explodes (0x4638E6) |
| 6 | balloon destroyed | rises, then explodes (0x4639C8) |
| 7 | empty, drifting to a resting place | 0x464480 |
| 8 | boat stranded on land | keels over (init 0x464710, per turn 0x4648A0) |
| 9 | caught by a whirlwind: spins, then explodes | 0x4649B0 (whirlwind [player]; the setter is not traced) |

### Life, damage and destruction
`+0x98` is both the hull's health and an abandon timer:
```text
life step (0x463CB0), every turn, every state:
  life -= 1
  life <= 0 (and not +0xC bit 0x100000):
      state 8 → +0x92 |= 0x400000
      boat: state 5;  balloon: state 6
  else if speed (+0x5F) != 0 or anyone aboard (+0x9E):
      life = VEHICLE_LIFE (file 5000)              ; full again every turn

damage_vehicle(vehicle, tribe, amount)  0x466F00:
  skipped when global 0x895DA7 bit 4 is set or the vehicle is of that tribe
  life -= amount
  callers: simple blast 7/1 (0x50B8C8): BLAST_DAMAGE_PERSON (exe 50) per turn of the blast, any vehicle in
           its radius; firewarrior bolt (0x514583): 100 to every enemy vehicle in the impact cell
```
- So damage only sinks an empty, stopped vehicle: a manned or moving one is refilled to the full 5000 every turn,
  and would need 5000 damage between two of its turns (100 blast turns, 50 bolts). No spell writes the vehicle
  states directly (only 0x463CB0 sets 5 and 6); whirlwind and blast reach vehicles through the simple blast.
- An abandoned vehicle (empty and still) is destroyed after 5000 turns: 417 s, about 7 minutes (exe 8000: 667 s).
- Destroyed boat (state 5). Entry (0x463532): everyone aboard is put out at its position (0x4659D0, 0x432520,
  0x402E70, person `+0xC` |= 0x10). Then, by the 8 × 8 land mask of its cell (0x44F980):
  - on water: sinks, height −8 and pitch +8 per turn (pitch up to 398, 70°), deleted (0x4EF180) at height
    −192: 24 turns (2 s) from the sea surface, no explosion;
  - on land: at once a partial explosion (0x407860: the model's faces fly off, smoke), a simple blast 7/1
    (0x50B6F0 with 3, 2, 5, 0x62, 0x8C, 1, 0), sound 0x4D, and the boat is deleted.
- Destroyed balloon (state 6): passengers put out the same way (they fall), then it rises 80 per turn and, at
  height 1024 or more, explodes like a boat on land (sound 0x4C). From 300 that is 10 turns (0.8 s).
- State 9 (a whirlwind's victim, as seen in play): speed 0, passengers put out, a countdown `+0x9A` of 10 (boat) or
  2 (balloon) turns; a boat spins (roll +204, pitch +153 per turn) and its countdown ends early when the ground
  reaches its height; at 0 the same explosion.

### Boarding and driving [exe, player]
```text
board(vehicle, person)  0x4657D0:
  no free seat among the type's seats (table +0x8: boat 5, balloon 2)  →  refused (a hard limit)
  the person plays its boarding animation (table 0x5A6DCE by type, 0x5A6D98 when +0x78 is set), its +0x9F =
    the vehicle; in a balloon its +0x10 bit 0x2000000 (aloft) is set
  seat = the person, +0x9E += 1, +0x9A = 7 (the vehicle waits 7 turns before moving off)
  vehicle tribe (+0x2F) and +0xA1 = the person's tribe (a spy: 0x4DE740, probably its disguise)
  a person of the same group (+0x63) as the vehicle's leader (0x4668B0) within 440 units joins its group
```
- Ownership follows whoever boards: an empty enemy or neutral vehicle becomes yours when one of your people gets
  in [player]; the exe rewrites the tribe on every boarding.
- Anyone can drive: there is no driver type; the vehicle moves when its leading passenger (0x466760) is given a
  move [player].
- Getting on and off [player]: the person walks under the vehicle or within 1-2 cells of it, then jumps on
  (boat) or the balloon lands on it; getting off is the reverse. A passenger stays selected, and a move order
  given to it moves the vehicle instead. The vehicle's tooltip has the unload button; clicking the ground
  several times also unloads (minor).

### Parking and the shore
Map cells (0x8A03E4, 16 bytes): the low nibble of `+0xC` is the cell's coast shape, read through the table
0x5AA328 (14 bytes per shape: +0 class bit, +1 direction 0-7 or none, +6..+13 an 8 × 8 land mask, one byte per
row):

| Shape | Class | Land in the cell |
|---|---|---|
| 0 | 0x01 | all land |
| 1 | 0x02 | none (open sea) |
| 2-5 | 0x04 | a strip along one side (straight coast) |
| 6-9 | 0x08 | a small corner (water cell by a headland) |
| 10-13 | 0x10 | two sides (water cell in a bay corner) |
| 14-15 | 0x20 | two opposite corners |

```text
moving (states 3, 4), each turn: movement pass, then when the driver has stopped, the vehicle's speed <= 1 and
it stands on a cell centre (x and z & 0x1FF == 0x100):
  boat may park (0x4650D0) when its cell has no flag 0x204, holds no other vehicle, and is
      open sea (shape 1), or a coast cell (shapes 2-15) with cell flag 0x1000000 that is not a bay corner (10-13)
  balloon may park when its cell has no flag 0x204, holds no other vehicle, and, when empty, is not open sea
      (shape 1) and has no cell flag 0x2
  parks: speed 0, the moving sound stops, state 1 / 2;  else 0x466FC0(vehicle, 12) and it goes on
```
- Cell flag 0x1000000 ("boat landing", partly traced): set by `level_land_processing_2` (0x44E2CF) when four of
  the cell's neighbouring points are at height 0, cleared when one of its 8 neighbours has flag 0x200 (a
  building stands there, its id at cell +0x8), and cleared on a boat hut's cells by `init_boat_hut` (0x40B7F0).
  Flag 0x4 of the 0x204 test is not identified.
- An empty vehicle not at a resting place (0x465650: boat on a coast cell, shapes 2-15, without cell flags
  0x100004; balloon anywhere but open sea without 0x100204) goes to state 7 at the next 64-turn check: it picks a
  destination (boat 0x464AE0, balloon 0x4651B0) and moves there at speed 8 for up to 128 turns (or, without one,
  towards a random point 4096 away for 64 turns), parking within 80 units of it at a resting place. Abandoned
  boats drift back to a shore.

### Stranded boat (state 8)
```text
check (0x463CB0), boats only, every 16 turns (+0x2E & 15 == 0) or when "ground moved" (+0xC & 0x2004):
  its cell is all land (shape 0), not +0xC bit 0x100000, not already state 8  →  state 8
entry (0x464710): speed 0; everyone aboard is put off on the ground, each at its cell's centre (or by the
  building there, 0x4044B0); life = 160 + rand(128) turns; roll (+0x6E) = 1 or −1 at random (its side)
per turn (0x4648A0): kept on the ground; while the cell is still all land the roll grows 8 per turn up to 512
  (90°, on its side, 64 turns = 5.3 s); when the cell is water again: back to state 1 (parked)
the life step then counts the 160-287 turns down (13-24 s): state 5 on land, the boat explodes
```
- This is what Raise Land, Land Bridge, Earthquake and the like do to a boat whose cell becomes land; boats on
  coast cells (shapes 2-15) are not stranded.

## Impact on our specs
- huts-and-training.md rule 7 (`MANA_F[kind]` × `HOUSED`/`WORKING`/`TRAINING` %, × `HUT_LEVEL`) is not what the
  income routine does: it uses the busy/idle constants above. The `MANA_F_<PERSON>` and activity % values are read
  somewhere else, not found yet. Brave 15 and specialists 4 happen to equal the busy and idle defaults.
- spells.md "Charges": the charge count is read from `SP_1_OFF_MAX`, so `SpellKind::max_charges()` should read it
  rather than derive it from the cost.
- TODO "Mana", "split between spells and training": training buildings take their part of each income first (up to
  half, `cost / 32` per building and pass), then the spells; training never takes from the bank. A cancelled
  training gives its store back at 100 per turn.
- huts-and-training.md rule 5 ("transformed into the hut's kind, same unit", hut meaning the training building):
  the original deletes the occupant and allocates a new person (new id, full life), carrying only queued orders and
  the ghost flag.
- huts-and-training.md rules 2-3 (unlimited queue, one at a time) [player] vs the exe ("The queue at the door"):
  the queue is unlimited, but the head goes in whenever fewer than 5 are inside and 16 turns have passed since
  the last admission (12 after someone came out), so up to 5 train together for `n × cost` and come out
  together; ours holds one (`BuildingKind::capacity`). The queue hugs the building at 4 spots per cell, moves up
  2 turns per rank, and people already of the output type let the others pass (every 16 turns). Ours has no
  queue yet.
- huts-and-training.md rule 4 (cost / duration per update, paused without mana): the original has no duration;
  the time is the cost filled at `cost / 32` per income (16 turns), 256-512 turns (21-43 s) at best. In our
  ticks (1 tick = 1 turn): a share every 16 ticks.
- huts-and-training.md "Original values": `CONV_TEMPLE`.. `CONV_SUPER` 4000 are not exe constant names (the exe
  has `CONV_TIME_*`, 32, no reader found), so they are not the training time.
- Training cost: `TRAIN_MANA[kind] × trained × BAND%[specialists of that kind]`, `HUMAN_` for controller type 2,
  `CP_` otherwise.
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
- terrain-textures.md "bigfade"/"disp"/"Our bake" ("Terrain land colour"): the original's row is `height + 75
  (+75 again unless perfectly flat), clamp 1022`, no `POP3_ROW_SCALE`-like flat multiplier; `disp` noise is
  signed and scaled by a fixed altitude-dependent curve (320..1024), not centred/offset by a theme constant; the
  brightness column is a precomputed per-cell byte fed by the level's own sunlight block (`ShadeStart`/
  `ShadeRange`/`Inclination`), not a baked Lambert light of the rendered heightmap slope as `terrain_texture.rs`
  currently computes it.
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
- `slots.rs` (`dispatch`/`free_spots_near`), the group-move spot spreading: the original has no such thing for
  the order itself (every unit in a group gets the identical target). Any spacing is a separate **formation**
  system (class-10 controllers, "Formations") wired to the move order (kind 3) but never the attack order
  (kinds 19/21), gated by a still-untraced global flag (0x895DA6 bit 0x4); our grid-based spot search has no
  equivalent concept and isn't gated by anything. The group **attack** dispatch (`units/selection.rs`, identical
  target for every selected attacker, no spreading) already matches the original as traced.
- `melee.rs` (`join_melees`) and `spotting.rs` (`sight_now`): both already match the traced mechanics reasonably
  well - joining is gated on a unit already ordered to attack a target that's now fighting, within the
  documented 224-unit reach, and `Action::Walking | Action::Attacking => Some(0)` matches the original's
  no-auto-attack-scan bucket for exactly the move (kind 3) and attack (19/21) kinds asked about. The original has
  a third, reduced-but-nonzero scan bucket (radius 1 cell, several other order kinds) that our two-bucket model
  (0 or full `sight_at_work`) doesn't reproduce for those other actions (`Queuing`, `Building`, `Worshipping`,
  ...) - out of scope here, worth a TODO if those are revisited.
- `command.rs`'s `Command::OrderUnit`/`QueueOrder` give each selected unit its own independent order copy; the
  original's shared 10-byte group record is mostly an implementation detail EXCEPT that several units can share
  live, mutable per-order state in the original (the group record can be rewritten under them) in a way our
  per-unit copies cannot express - moot unless a future spec item needs "all N units retarget at once mid-walk".
- `unit.rs`'s `Action::Idle`: a single, unified rest state with no autonomous behaviour at all, matching what was
  found for ordinary followers in the original (state 10, "Idle behaviour": does nothing when no command group
  is assigned - no wandering, no autonomous wood-gathering found). What's missing is the confirmed, rhythm-gated
  idle-fidget-animation picker (state 8 → 5, per-type RNG-weighted, ~16-turn cadence) - confirmed only for
  wildmen in the original, not (yet) for followers, so this is lower priority than it first appears; `art.rs` /
  `sheets.rs` have exactly one static idle pose with no variant system to connect it to either way.
- Wildmen (no AI in our code today; "Wildmen"): the original drives them through a confirmed 1 → 8 → (5 or 6) →
  16 → 8 loop - first search nearby (18 cells) for a tree or plant and idle beside it if found (state 5); only
  if none is near, walk to a nearby shore cell instead (states 6/16, explicitly rejecting inland and open-sea
  cells) and idle there with ambient sound. Both destinations are deliberate, keyed off the exe's own
  scenery-type and coastal-shape tables - not a random point. No hunger/thirst/age stat exists even so; nothing
  is actually consumed at either stop, it only decides where the idle animation plays.
  `attack.rs`'s `wildmen_never_fight_and_nobody_attacks_its_own` test makes wildmen entirely unattackable; the
  original only confirms the *automatic* proximity scan skips them (tribe 0xFF is never treated as an enemy) -
  whether an explicit player attack order also respects that filter is still open, so our blanket rule may be
  stricter than the original. No periodic wildman spawner exists in the original (a one-shot level-placement
  routine is the only creator) - confirms there should be none in ours either.
- `unit.rs`'s `Action::Drowning`/`DROWN_DAMAGE` ("Drowning"): the mechanism now matches (a generic, type-agnostic,
  per-turn re-check with no timer), but the original re-tests the unit's EXACT fine-grained position every turn
  (not a coarser "is this tile water" test), which is specifically why idling at a chosen "shore" cell (any
  destination search that only checks the coarse per-cell shape, as ours does too) can drown a unit sitting
  still - worth keeping in mind for any future autonomous-wandering or boat-docking logic that targets "shore"
  cells by the coarse shape alone.
- `unit.rs`'s `Action::Spirit`/`SPIRIT_TICKS` (explicitly commented "a guess"; "Death and the spirit rise"):
  confirmed wrong as a single constant/pose. The original has at least two structurally different variants - a
  brief (4-turn) stationary "calm" rise for an ordinary death, and a 32-turn, continuously-ascending (40 world
  units/turn, no gravity) "climbing" rise for anyone who died in a drowning-eligible spot, reusing the
  already-documented "flung in the air" tumble sprite rather than a translucent copy of the death pose. A
  wildman ALWAYS gets the climbing variant, with an extra continuous spin (91/2048 facing per turn) found
  nowhere else - the visual the user described as "flying into the sky." None of this (the duration split, the
  real upward velocity, or the wildman-specific spin) exists in our single `Action::Spirit` today. Also:
  `unit-art.md`'s "a see-through copy rising, drawn in post (alpha sprite)" note describes the calm variant
  reasonably but not the climbing one, which is a distinct sprite object with real velocity.
- TODO "Vehicles" and assets.md "Vehicles" (none in our code): the original's boat and balloon are 3D objects
  143 / 144 of the banks 2-7 layout (bank 0: `catalog::BOAT`, `AIRSHIP`), turned by a pitch and a
  roll for sinking and keeling over, no sprites. Life `VEHICLE_LIFE_*` = 5000 turns (one tick each), refilled
  every turn while moving or manned; damage 50 per blast turn, 100 per firewarrior bolt (enemies only). Destroyed:
  a boat sinks 8 units per turn on water (gone in 2 s) or explodes on land (blast 7/1); a balloon rises 80 per
  turn and explodes at height 1024. A boat whose cell becomes all land: passengers off, keels over 90° in 64
  turns, explodes after 160-287 turns ("Vehicles").
- units.md `Mobility::Sail` ("open sea only, boats") and `path::nearest_reachable` (boats unloading at a shore):
  the original parks a boat on open sea or on a coast cell with the landing flag (four neighbouring points at
  height 0, no building next to it, not a bay corner); only an all-land cell strands it.
- `lava.rs`/`volcano.rs` ("Volcano lava flow"): `Lava::turn` deals an instant kill every turn to anyone on a lava
  cell; the original only ever sets the fire flag (state 31) and lets the existing fire/evacuation mechanic run
  its course - the module's own doc comment already says this correctly while the code does the opposite.
  `LAVA_FLOW_TICKS` (half of `LAVA_LIFETIME`, then holding) should instead be: growth for the *entire* 16 s
  (with an early-stop accumulator), a further 1.3 s fade, then conversion to permanent building-ash scorch, not
  an indefinite hold. `RIM_RADIUS_CELLS`/`SLOPE_RADIUS_CELLS` (the crater cone's own shape) don't correspond to
  the lava's real starting/measured radius and shouldn't be reused for it. The growth shape itself (`disc()`, a
  uniform expanding circle) should instead follow a per-cell precomputed downhill-direction field, spreading one
  cell at a time outward from the crater along the real slope, not uniformly. The original's field is frozen at
  the last height edit (recomputed by the general terrain cascade, not by lava, and not every turn - "Volcano
  lava flow"); `game_core::lava::downhill` instead reads the steepest-of-8-neighbours live off the current
  heightmap every turn, so a cell's chosen direction can drift as nearby cells rise or fall while ours would
  hold it fixed - worth checking against the channel-stability symptom (thin radial channels, bare ground
  between them) if it recurs, though the trace doesn't show it changing the broad shape (channels one cell wide,
  not a flood fill, either way). `Lava::effect()` (Earthquake's static fissure) matches "lava square" structurally
  but has the same instant-kill issue and no decay-then-self-destruct-then-ash behaviour.

## Open questions
- Where `MANA_F_<PERSON>`, `MANA_F_HOUSED/WORKING/TRAINING` and `MANA_F_HUT_LEVEL_1..3` are applied. Only the max
  of the three activity percentages is stored, and its readers are not traced.
- What "busy" means (unit+0xE bit 7, unit+0xA7), and the preacher's extra check.
- The controller types of tribe+0xC1F (2 is human; 1 gets full mana).
- Spell order and share inside the distribution loop.
- Auto-attack: where the kinds' sprites are drawn (the record+6 offset could not be reproduced this session -
  unverified, not disproven), the firewarrior's own scan (0x51E9A0); the preacher's own scan (0x51E7B0) is now
  traced ("Command groups", "Preacher conversion") but what `0x51F030`'s two out-params/mode flag mean is not.
  The player's own UI → order call sites were not located ("Order kinds and player commands": the kind-giving
  helpers at 0x43B180-0x43BB54 are confirmed, but which exact code calls them from a mouse click is not); what
  flag 0x8 on a matched unit's state actually represents is still open (it may be a loosely-reused generic state
  table, not a purpose-built "in combat" flag - "Fight"); what sets global flag 0x895DA6 bit 0x4 (gates
  formations - "Formations") and what `find_formation_for_unit`/`0x501C00` match several units heading to the
  same place on; whether ordinary followers (not wildmen) ever reach the idle-fidget state 5 on their own
  ("Idle behaviour"), what order kind (if any) leads a person into the gather-wood state autonomously, and
  `0x405B80`'s own kind-4 "doing it" handler (0x433800, not yet read, would settle what its autonomous
  assignment actually produces).
- Building ashes: the cliff rows' colours, the volcano cone's clearing, foundation sub 2's condition and its unit
  10/0x13.
- Terrain land colour (2026-10-20, resolved): `sunX`/`sunZ`/`sunBase` do NOT derive from `ShadeStart`/
  `ShadeRange`/`Inclination` in normal play - `sunlight_init_default` (0x401040) sets all three to a flat
  hardcoded 0x93 (147), live-confirmed identical on a level with on-disk `Inclination` 0x40 and one with 0x20.
  The day/night-style tick that would vary them (0x401350) and the alternate fixed-value path (0x401920) are
  both gated behind flag `0x895DA5 & 0x8`, live-confirmed off (`Inclination` unchanged after 77+ turns on both
  test levels). `game_core::shading::SunParams::default` now ships the flat 147/147/147 confirmed here, replacing
  the `ShadeRange`/`ShadeStart`-scaled guess it used to have. Still open: the `+0xE` low nibble's own derivation
  (the ambient term `cell_light` adds, `(low_nibble + 8) * 16`) - kept as a constant 0 placeholder; and whether
  flag `0x895DA5 & 0x8` is ever set in some game mode (multiplayer, a specific level, a graphics setting) where
  the day/night tick or the fixed-Inclination-64 path would actually run. Re-checked 2026-10-20 (prompted by a
  report that our render looks too bright overall): re-read both candidate write sites byte for byte
  (`level_land_processing_2` 0x44DF40 and the bulk level-load light pass 0x4BDD40) - both only ever READ this
  nibble (`and dl, 0xf` before folding it into the ambient term), never write it, and the level `.dat` file itself
  carries no separate per-cell field that could seed it (level-format.md: only per-vertex heights plus
  `LandBlocks`/`LandOrients`, byte-identical across all 41 levels, and `NoAccessSquares`). Nothing found this
  session writes it anywhere - raises confidence the placeholder 0 is actually right, not just unverified, though
  still not a positive proof (a write via some bulk memcpy/initialisation path not yet located would still be
  possible). The "too bright" report's cause turned out to be on our own rendering side, not this
  field - see TODO.md "Terrain" and terrain-textures.md "Our bake" (a palette texture colour-space bug, confirmed
  and fixed).
- Attacking a building: what sets building +0x9C bit 0x40, and the partial explosion's 10/7 units.
- Firewarrior fire: the line-of-fire test, whether a building stops the bolts (5 throws at a still preacher did nothing), what sets the reload
  byte to 252-255 (third capture), what the +0x10 bit 0x400000 set on own people under a passing shot does,
  whether hfx 1124-1127 is the shot's fade, and the approach sub-states (10, 11) before the throw.
- Combat: where a person's maximum life (`+0x6C`) comes from when it is not `LIFE_<type>`, whether a taunt (48-50) plays to its end (0x4D4DA0
  resets a non-idle person to idle the next turn; the per-person `+0x7E` it sets may hold it),
  what option flag 0x89D17C bit 0x40 is (no kick, no knockback),
  the meaning of pre-fight mode 2, the ground test of a fight's place (0x51E4B0), the attacker's sub-state 8 when
  its target is already in a pre-fight, and the firewarriors' ranged attack (attack-order sub-states 10, 11).
- Training buildings: whether 5 trainees really go in together in play (the exe says so; live: queue 5 braves
  into a warrior training building and watch +0xA6), the queue spots past one lap, the meaning of
  `+0x9D` bit 0x4 and `+0x7E` set at the end, `+0x14` bit 0x1000 (set when a training building receives 32 or
  less), the reconversion building (type 9), and how long a training really takes (live: watch +0x96/+0x98 of a
  warrior training building per turn).
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
  building's second destroy reason (sub-state 2).
- Status markers: whether `frame_counter` 0x897981 advances per rendered frame or per turn, the anchor of the
  screen position the height offsets are taken from, what the colour tables (0x87D000 + n × 0x1000) do to an alpha
  sprite, the key behind commands 100/101 (health bars), and what +0x3C (a colour table index −15..15, 0x468B50)
  tints a person for. (Tribe +0x93D bit 0x8 is not "sees invisible people" - that guess is answered below, see
  "Tribe record" and "Spells".)

- Vehicles: where the whirlwind sets state 9 (not in its vehicle branch 0x50FA1F, which only makes a blast); the
  boarding range check and the unloading path (0x4659D0, 0x466190, 0x465C50); the 4 neighbouring points of the boat landing flag 0x1000000; cell flag 0x4;
  `+0xC` bit 0x100000 (no stranding, no destruction); the movement passes and the state-7 destination
  searches (0x464AE0, 0x4651B0); 0x466FC0; table +0x6 (838 / 839); whether a parked balloon's passengers can be
  hit; what a passenger put out over water does (drowns?). Live check of the 5000-turn abandon life.
- Volcano lava flow: the unnamed type-0x40 effect's (state 51) own role - spawned deterministically by the
  controller throughout cast-turns 120-240 (`ebp` 40-160, "Volcano rocks") as well as probabilistically by the
  lava spread, but what it actually does once alive is still untraced.
- Wildmen: state 0x22 (what "noticing" a nearby scenery-type-9 object or vehicle-type-3 object leads to) and the
  identity of those two types; the sub-state(8..19) → animation-id mapping in state 16; whether an explicit
  player attack order on a wildman is honoured in the original (only the automatic proximity scan is confirmed
  to skip them); whether wildman creation can ever retrigger mid-level rather than only at load.

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
| 0x41AE60 | sum of stored training building mana |
| 0x406218 | adds a training building's stored mana to a type 1 tribe |
| 0x405B80 | "training_building_turn": cost, conversion (delete occupants, alloc new persons) |
| 0x41B0C0 | "train_cost(building, &cost, type, n)": cost by band, returns `stored >= cost` |
| 0x407150 | "enter_building(person, building)": slot, training flag, cost |
| 0x434610 | "go_into_building" (person state 0xA/0xE): walk, queue (sub 1-3), admission, entry (sub 4-9) |
| 0x409580 | rebuild a building's queue: unlink who left, first freed rank to `+0xAC`, `+0x9C` bit 0x2000 |
| 0x409710 | queue spot of a rank: wall-following steps of 0x80 from the door point |
| 0x409B10 / 0x409BD0 / 0x409C50 | append to the queue / the one ahead of a person / the person at a rank (-1: last) |
| 0x4044B0 | a building's door point (pos, shape origin, door offset) |
| 0x40BBE0 | recomputes a training building's cost |
| 0x4EF180 | "delete_unit" |
| 0x504BC0 | `draw_ui_panel`: building panel, training bar at 0x504FDD (values) and 0x505604 (drawing) |
| 0x436C20 / 0x436D00 / 0x4364D0 / 0x438730 | alloc / assign (primary or `+0x8B` slot) / release / set-kind-and-target for a command group - "Command groups" |
| 0x432590 | `order_execute`: resolves a person's active command group and dispatches its kind - "Command groups" |
| 0x43A4D0 | order kinds 31/32 (disguise and the preacher's building visit): shared approach-and-RNG-idle machine |
| 0x43C7A0 | order kind 33 (preach): approaches the target, ends by setting its state to 5 - "Preacher conversion" |
| 0x51A2A0 | a person's attack order: reach, then join a fight or start a pre-fight |
| 0x51C3C0 | attack target eligibility by attacker type; the preacher/non-preacher gate - "Preacher conversion" |
| 0x51CE50 | preacher/shaman-only nearest-person scan, called from 0x51C3C0 for a preacher attacker |
| 0x4D6970 | person state 5 per-turn handler: idle near a reference unit (`+0x72`), not preach-specific |
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
| 0x4D4690 | auto-attack check (rhythm, busy test 0x4D44E0, per-type scan) |
| 0x51FF60 | auto-attack radius in cells (by order kind, type, height for firewarriors) |
| 0x51E5E0, 0x51EAB0 | melee auto-attack scan: square of cells, first enemy found |
| 0x51E7B0 | preacher's own auto-attack scan (person or building; replaces the melee scan) |
| 0x51F030 | scan helper used by 0x51E7B0 (two out-params; mode flag; not fully read) |
| 0x4D3D18 | person per-turn state dispatch table (`unit_processing_class_1_person`, keyed by `+0x2C − 1`) |
| 0x4D5CF0 | person state 3: a preachee struck - spawns a 7/0x4D effect and a 10/0xC controller at its own place |
| 0x4D83B0 | person state 23: a preachee's conversion roll ("Preacher conversion") |
| 0x4D8E40 | person state 24: the same shape as state 23, increments the target's `+0x2D` instead (not fully read) |
| 0x4D4B50 | destroy a unit (called on the preachee once its conversion roll succeeds) |
| 0x500E20, 0x500EC0 | class 10 controller init and per-turn dispatch (by state `+0x2C`; generic housekeeping) |
| 0x409140, 0x409200 | building damage: add to its accumulator `+0x9E` (per attacker turn, or an amount) |
| 0x4092A0 | building accumulator: decay, structural hits of 100 wood, destruction |
| 0x407810 | shake a building (timer `+0xA7`, offsets `+0x6C`/`+0x6E`) |
| 0x4B8BB0 | damaged building's plan per turn (repair delay `+0x94`) |
| 0x51F990 | firewarrior range and line of fire |
| 0x51FBF0 | "firewarrior_fire(thrower, target, point)": two shots 8/6, reload |
| 0x4BBCF0, 0x4BBF30 | shot 8/6 init and per turn (homing, trail, arrival) |
| 0x514410 | firewarrior shot hit: the impact cell, the target, buildings, throws |
| 0x4D42A4 | firewarrior reload countdown |
| 0x4010B0 | `sunlight_update_unit_landscape`: a dynamic ground light following a unit (50 at most) |
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
| 0x4BB290 | fireball impact: creates the shot's own caller-configured effect (class/type/queued params from the shot's own +0x7C-0x86), see "Fireball impact" |
| 0x511800 | Firestorm's own impact effect (type 0x1C) init: land/water branch, ground-fire or splash, a blast, building ignite |
| 0x4A8C60 | sets a scenery's own burn-length countdown from a band table (exact fields/turns not decoded) |
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
| 0x511640 | firestorm per turn (state 23, Ghidra's `process_firestorm` is correct): gated spawn, see "Firestorm fireballs" |
| 0x50FF30 | erosion per turn (state 24, Ghidra's `process_erosion` is correct, despite living in the shot code area): jitter + one pass via 0x4983A0, see "Erosion passes" |
| 0x4983A0 | one hydraulic-erosion pass over a window: per cell, steepest of its 8 neighbours (diagonals weighted 0xB505/0x10000); write-back magnitude not traced |
| 0x44DDF0 | recompute the landscape around a cell after a height change |
| 0x44F2F0 | notify units and buildings around a cell: mode 1 "ground moved", mode 2 "set on fire" |
| 0x408080 | building foundation check: re-level the footprint or collapse |
| 0x40B230 | make the ashes: footprint cells flagged damaged ground, damage smoke, scorch list entry |
| 0x4509D0, 0x450A70, 0x450B30 | scorch list: add, tick every 16 turns, fade / clear an area |
| 0x406F40 | building state 3: thrown out, explode, removed (no ashes) (Ghidra: `alloc_spell_blast_or_sinking`) |
| 0x50D9A0 | earthquake effect 7/0x1A per turn (Ghidra: `process_explode_building`), spawned only by the earthquake spell (spell table effect 26) |
| 0x50FE60 | effect 7/0xE init: shake one building (−100 wood via 0x4980A0, partial explosion) |
| 0x4E7FB0, 0x4E8F00 | boat and balloon movement passes (balloon hover height) |
| 0x463370, 0x463780 | vehicle class init (by state) and per turn (by state) |
| 0x463CB0 | "vehicle_life_step": −1 per turn, refill while moving or manned, destroy at 0, strand on land |
| 0x466F00 | "damage_vehicle"(vehicle, tribe, amount) |
| 0x463BA0 | vehicle creation: heading off the coast, model (0x4EE700 draw class 2), life |
| 0x4650D0, 0x465650 | boat may park here; empty vehicle at a resting place |
| 0x4657D0 | "board_vehicle"(vehicle, person): seat, animation, the vehicle takes the person's tribe |
| 0x464710, 0x4648A0 | stranded boat: entry (put everyone off, 160-287 turns of life), per turn (keel over) |
| 0x44F980 | "is_land_at"(pos): the cell's coast shape and its 8 × 8 land mask (table 0x5AA328) |
| 0x4EE700 | set a unit's drawable (`+0x33` index, `+0x3A` draw class from table 0x5A6AF8, 11 bytes per class) |
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
| 0x4D3386 | person state 1: counts down +0x70, then next-state by type from table 0x5A7064 byte 0 ("Idle behaviour") |
| 0x5A7064 | 9×50-byte per-type table: byte 0 state-1's next state, bytes 3+ state-5's fidget-odds (0x5A7067) |
| 0x4D60D0 | person state 8 (wildmen only): picks a "reference unit", may enter state 5 (fidget) or 6 ("Idle behaviour") |
| 0x4D48E0 | wildman "find a nearby tree/plant" search (18 cells, scenery-type flags table 0x5A79C4) - state 5 |
| 0x4D4A00 | wildman shore-destination search (12 cells, coastal-shape table 0x5AA328, rejects land/open sea) |
| 0x4D342A, 0x4D6E40 | wildman states 6 (walk to the shore destination) and 16 (idle/ambient animation there) ("Wildmen") |
| 0x4D66E0 | wildman state 6's "notice a nearby object/vehicle/enemy" check, binds into untraced state 0x22 |
| 0x4D4040 | `unit_set_object`: generic attached-prop setter, ~80 callers exe-wide; not an animation-id table |
| 0x514330 | `effect_place_wild_man`: one-shot level-placement scatter of up to 5 wildmen - the only wildman creator |
| 0x43B180-0x43BA10 | eight "give one unit an order" helpers, one kind each ("Order kinds and player commands") |
| 0x432DF0 | copies order-kind flags byte+0xA bit 0x10 ("wants formation") into person+0x76 bit 0x8 |
| 0x5018C0, 0x501AB0, 0x501000 | join/create a formation, find a compatible one, per-turn formation controller |
| 0x443B40 | ring a tribe's buildings of one type with a kind-0x1E "alarm" order (process_tribe_cmd's one order-related case) |
| 0x4BC500, 0x4BC660 | volcano fireball (shot 8/7 small, 8/8 big) init and per-turn (state 7): shared flight, size-specific trail/landing effects ("Volcano fireballs") |
| 0x4BE330 | `set_texture_landscape_main`: per-quad blend/altitude-colour and bigfade row/brightness sampling and interpolation ("Building ashes", "Terrain land colour") |
| 0x4BDB50 | `set_cliff_palette`: builds the 64-row cliff/lava colour table at level load, never again (no animation) |
| 0x4BD760 | `alloc_mem_1` (part of): builds `DISP_SENSITIVITY` (table 0x5D45B8, 1152 signed i16, `clamp(3*index-64, 320, 1024)`) at level load ("Terrain land colour") |
| 0x4BE230 | `compute_cell_light`: map-cell `+0xD` brightness byte from the cell's own height gradient dotted against the sun vector (0x937AA8/AA/AC) plus an ambient term from `+0xE`'s low nibble, clamp 0..255; called from `level_land_processing_2` per dirty cell ("Terrain land colour") |
| 0x4BE0B0 | bulk/loop-restructured copy of `compute_cell_light`'s own formula (re-disassembled 2026-10-20: bit-for-bit the same dx/dz/÷350/ambient/clamp math, same sun-vector reads, same `+0xD` write), for a multi-cell sweep rather than a single call - not a different formula or neighbour convention |
| 0x4BDE94 | tail of a third, whole-map bulk variant of the same `+0xD` computation, run at level load (start address not reverified this session) |
| 0x401040 | `sunlight_init_default`: sets `ShadeStart`/`ShadeRange`/`Inclination` (0x937AAE/AF/B0) to the exe defaults 28/15/32, and `sunX`/`sunZ`/`sunBase` (0x937AA8/AA/AC) to the literal immediate 0x93 (147) - a flat constant, NOT derived from `ShadeStart`/`ShadeRange`/`Inclination`; live-confirmed on two levels with different on-disk `Inclination` |
| 0x401090 | near-duplicate of part of `sunlight_init_default`: resets `sunX`/`sunZ`/`sunBase` to the same flat 147 without touching `ShadeStart`/`ShadeRange`/`Inclination`, then falls into `sunlight_init_2` |
| 0x401790 | `sunlight_init_2`: reads the already-set `sunX`/`sunZ`/`sunBase` (does not set them) to build two unrelated 1024-entry angle-keyed shade tables (0x89BC8E, 0x89C08D) via the sin/cos tables (0x5DDDE8/0x5DE5E8), not re-examined this session |
| 0x401350 | per-turn sun-angle tick, gated behind flag `0x895DA5 & 0x8` (live-confirmed OFF in normal play, 2026-10-20): when set and every 8th turn, increments `Inclination` and recomputes `sunX`/`sunBase` from the real sin/cos tables - the exe's actual day/night-style animation, distinct from the level's static sunlight block |
| 0x401920 | unconditionally forces `Inclination` to 0x40 (64, overwriting the level's value) and `sunX`/`sunBase` (0x937AA8/AC) from two FIXED scalar memory cells (0x5DE9E8/0x5DE1E8) - not an array indexed by `Inclination` (no write to either address found in the exe's code, i.e. build-time data, not level-derived); gated behind the same `0x895DA5 & 0x8` flag as 0x401350 |
| 0x512A00, 0x512A60 | lava flow (effect 34/state 33) init and per-turn: downhill spread, then fade, then ash ("Volcano lava flow") |
| 0x5134C0, 0x513570 | lava square (state 38) init and per-turn: a static, decaying hot cell |
| 0x44DDF0 | `land_level_processing_1`: queues a cell after a height change, flushes to 0x44DF40 at 0x400 queued |
| 0x44DF40 | `level_land_processing_2`: per cell, recomputes blend (+0xA), the downhill-direction nibble (+0xE) and, via `compute_cell_light` 0x4BE230, the light byte (+0xD) |
| 0x44F2F0 | notify_area(mode bitmask, cell, radius, tribe): bit 0 ground-moved, bit 1 set-on-fire (shaman exempt) |

Timing globals: turn counter 0x89D188 (u32), `turns_per_sec` 0x89D161 (u8, copied from the option at 0x98F712),
`next_turn_time` 0x5CD92C, `step_ms` 0x5CD930 (83 in the exe image), `extra_turns` 0x895DAC (i8). Under Wine the
exe is mapped at its image base 0x400000, so these addresses can be read as they are.

### Unit record [ghidra]
- `unit_struct`, 0xB3 bytes. Pool of 2000 (`unit_array` 0x8E0428), two free lists split at index 640
  (`alloc_unit`, `move_unit_to_free_list`).
- Fight links: controller id +0x9D (pre-fight or fight), state saved on entering one +0x7D, fight opponent +0x72.
- Fields: `unit_class` +0x2A, `unit_type` +0x2B, `state` +0x2C, `state_2` +0x2D, `tribe_index` +0x2F, `unit_obj`
  (model or sprite) +0x33, `pos` +0x3D, last attacker's tribe +0xB0.
- Training building fields: occupants +0x86 (6 × u16 unit ids, the type's +0x20 used), training cost +0x96 (u16),
  stored mana +0x98 (u16), training on +0x9C bit 0x80, next training building in the distribution list +0xA0.
- Queue fields (buildings with type flag 0x800): queue head id +0xA2, inside count +0xA6, admission gate +0xAB
  (turns), first freed rank +0xAC, entering count +0xAD and its reset timer +0xAE, queue changed +0x9C bit
  0x2000, entering jam +0x14 bit 0x40. Person: next in queue +0x85, ripple countdown +0xA9, rank +0xAA, listed
  +0x14 bit 0x20, target building +0x89.
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
  33, lava square 38, convert wild 60, shield 68, armageddon 71, bloodlust 72, teleport 73 (33/38 now confirmed,
  not just matched by guess - "Volcano lava flow"); state 51 (effect type 0x40) exists and is spawned by both
  the volcano controller and lava flow's own spread, but is otherwise unnamed - open.
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
| person types | 0x5A7060 | 0x32 | `HUMAN_TRAIN_MANA_` +0x24, `CP_TRAIN_MANA_` +0x28, `MANA_F_` +0xE, `LIFE_` +0x10, `WOOD_` +0x14, `FIGHT_DAMAGE_` +0x1B, chop time (u8, not a named constant) +0x1D, `CONV_` +0x1F, tower detection radius +0x22, `SW_BLAST_DAMAGE_` +0x2C |
| building types | 0x5A7228 | 0x4C | slots +0x20 (u8), output person +0x31 (u8), `CONV_TIME_` +0x2C, `WOOD_` +0x1A, next size +0x34 (u8), growth target +0x36 (2400, not a named constant), mana factor +0x38, `MAX_POP_VALUE__HUT_` +0x3A, `HUT_SPROG_TIME` +0x3C, flags +0x48 (bit 0: trains; bit 10: breeds; bit 6: holds wood), foundation re-level rate +0x44 (2) and tolerance +0x46 (200) |
| scenery types | 0x5A79B0 | 0x18 | object +0x0A (12: 30), `TREE_WOOD_VALUE` +0x4, `TREE_WOOD_GROW` +0x6, `TREE_DORMANT_TIME` +0x8, flags +0x14 |
| movement profiles | 0x5A7B90 | 0x1A | speed +0x4 (`<TYPE>_SPEED`: brave profile 2, warrior 14, preacher 15, spy 16, firewarrior 17, shaman 18), velocity caps +0x8 / +0xA and +0xC / +0xE (horizontal / vertical: 192 / 512 for persons), `WALK_ALT_DIFF2` +0x10 (copied for flagged profiles), gravity +0x12 (32), ground friction +0x14 (28) / +0x16 (72), flags +0x18 |
| spells | 0x5A80D0 | 0x3E | kind +0x00 (1 normal, 2 hidden), cost (u32) +0x4, id +0x8, flags +0x1A (bit 0 castable) and +0x1B (0x40 homing), `SP_W_RANGE_` +0x1E, mode-0x20 range +0x22, shot without shaman +0x26, shot from shaman +0x27, effects +0x28..+0x2C, `SP_1_OFF_MAX_` +0x2D, mode-0x20 charges (`LSME_1_OFF_MAX_`) +0x2E, deflectable +0x2F, sound +0x30, enemy-cast alert +0x35/+0x36, `OPT_S` +0x3A |

- `LAND_BRIDGE_MAX_CHANGE`: record size 4, flags 0, value at 0x5AA518, exe default 256 (the file's is 256 too,
  spells.md "Effect constants").
- The shipped `constant.dat` wins over the exe defaults, e.g. `SPELL_BLAST` 18 000 in the exe and 10 000 in the
  file, `LIFE_BRAVE` 1400 and 1000.
- Not in the table: 0x5AA44C (default 3) is masked with the turn number like `MANA_UPDATE_FREQ` should be, so it is
  probably that constant (unverified). The `MANA_IDLE_*`/`MANA_BUSY_*` values are exe-only too.
