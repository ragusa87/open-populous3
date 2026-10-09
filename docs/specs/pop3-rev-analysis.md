# Original engine internals (`pop3-rev` analysis)

What the reverse engineering of the original executable says about game logic, spells, combat, mana and wood,
laid out for implementation: the rules first, then what they change for us, then the addresses and layouts as
reference.

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
   decreased by what it receives, clamped at 0. When a charge is filled, the loop runs again. The order and share
   between spells are not traced.
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

## Spells, effects and combat [ghidra]
- A cast allocates a class 11 spell unit (`alloc_spell_unit`). It spawns class 7 effects, each with its own handler
  (`process_volcano`, `process_swamp`, `process_whirlwind`, `process_lightning_bolt`, ...), which may spawn class 8
  shots. Volcano: effect 0x0F plus fireball shots 8/7 and 8/8.
- Internal controller units (class 10):

| Type | Role |
|---|---|
| 10/8, 10/9 | fight, pre-fight |
| 10/1, 10/6 | formations (`process_formation_unit`, `add_unit_to_formation`) |
| 10/0xA | guard control |
| 10/4, 10/0xC | soul convert |

  A melee is probably one fight controller that its participants link to. How damage is resolved is not traced.
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

Timing globals: turn counter 0x89D188 (u32), `turns_per_sec` 0x89D161 (u8, copied from the option at 0x98F712),
`next_turn_time` 0x5CD92C, `step_ms` 0x5CD930 (83 in the exe image), `extra_turns` 0x895DAC (i8). Under Wine the
exe is mapped at its image base 0x400000, so these addresses can be read as they are.

### Unit record [ghidra]
- `unit_struct`, 0xB3 bytes. Pool of 2000 (`unit_array` 0x8E0428), two free lists split at index 640
  (`alloc_unit`, `move_unit_to_free_list`).
- Fields: `unit_class` +0x2A, `unit_type` +0x2B, `state` +0x2C, `state_2` +0x2D, `tribe_index` +0x2F, `unit_obj`
  (model or sprite) +0x33, `pos` +0x3D, building stored value +0x98, last attacker's tribe +0xB0.
- Wood fields: a person's state timer +0x70 and carried wood +0x78, a tree's wood left +0x84, a shape's wood +0x96,
  a building's wood +0xA4. The gather handler switches on a byte read as +0xA8, but that field stays 0 while
  chopping (measured), so the offset is unconfirmed.
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
| +0xA05, +0xA07 | reservoir `delay` and `rate` |
| +0xC1F | controller type: 2 human, 1 full mana, others unknown |
| +0xC22 | tribe number |

Other globals: the per-tribe level header `struct_56B` (0x38 bytes per tribe at 0x96070A) holds the spell
availability bitmask and charge counts (`set_struct_56B_array_spell_val`); the reservoir short-delay flag is bit 3
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
| building types | 0x5A7228 | 0x4C | `WOOD_` +0x1A, mana factor +0x38 |
| scenery types | 0x5A79B0 | 0x18 | `TREE_WOOD_VALUE` +0x4, `TREE_WOOD_GROW` +0x6, `TREE_DORMANT_TIME` +0x8, flags +0x14 |
| spells | 0x5A80D0 | 0x3E | cost (u32) +0x4, `SP_W_RANGE_` +0x1E, `SP_1_OFF_MAX_` +0x2D, second charge count +0x2E (level flag 0x20) |

- The shipped `constant.dat` wins over the exe defaults, e.g. `SPELL_BLAST` 18 000 in the exe and 10 000 in the
  file, `LIFE_BRAVE` 1400 and 1000.
- Not in the table: 0x5AA44C (default 3) is masked with the turn number like `MANA_UPDATE_FREQ` should be, so it is
  probably that constant (unverified). The `MANA_IDLE_*`/`MANA_BUSY_*` values are exe-only too.
