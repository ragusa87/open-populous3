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
- **The HUD mana bar is not `bank / MAX_MANA`** [exe]: with nothing charging it is full by construction; with a
  spell charging it shows last turn's income against an "advisor" pacing estimate for that spell, so a short bar
  means income fell behind the estimate, not that the bank is low. See "The mana bar (HUD)".
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
  reroll's own entry trigger is now found: `preacher_strike_scan` (0x43ABF0), called from the shared handler for
  order kinds 17/31/32 - **not preacher-exclusive** (kind 31 "disguise" is issued to brave/warrior/spy, never a
  preacher), and its own per-type target table restricts valid targets to enemy preachers/firewarriors only - this
  does NOT cover the "ordinary follower" conversion seen in play, which is a different spell entirely (Hypnotism,
  "Per spell" 7); see "Preacher conversion" for the resolved history of this distinction.
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
- **The camera's pitch is fixed only while the player drives it - an AI-script flyby animates pitch
  independently of yaw** [exe, measured, partial]: level 1's opening flyby was measured dipping pitch to about
  55° mid-flight before returning, while `flyby.rs` currently never varies pitch. The camera's rotation matrix
  itself turned out to live inside the active player's own tribe record, not a separate struct. See "Camera
  flyby state (tribe_struct)" - most of its candidate fields (position, distance, raw pitch, interpolation
  progress) are still unconfirmed, this investigation is ongoing.

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
  building" or "working" (unverified). **Note for whoever chases this next**: busy generates MORE mana than idle
  (15 vs 4 for a brave) - the opposite of "stay idle to generate mana" - so if "busy" does turn out to mean
  "currently running an order/state other than plain idle" (state 10 with no group at all, "Idle behaviour"), the
  economic incentive this creates is "get idle units into SOME task quickly", not "let them sit idle for income";
  not confirmed this session which field/state the income routine actually reads for this flag.
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
  bank is not traced - one partial answer found: when a pass finds no training building that needed funding that
  turn (0x41AB63), the leftover is not banked there either, just discarded with `incoming`, alongside firing the
  one-shot advisor message "The mana bar (HUD)" describes.

### Shaman death
- `base = bank`; for a human tribe (controller type 2), `base = needed + stored building mana`.
- The tribe loses `base * SHAMEN_DEAD_MANA_%_LOST`.
- If `SHAMEN_DEAD_MANA_%_GAIN` is non-zero and the shaman's last attacker is another tribe, that tribe gets
  `base * %_GAIN` through `give_mana_delayed`, so after a 1000-turn delay.
- File: 25 % and 25 %. Exe: 25 % and 0 %.

### Start
- `bank = START_MANA` (file 30 000, exe 42 000).
- `CONVERT_MANA` (exe 6000) and `SOUL_GRAB` (exe 30 000) exist, but no reader is found yet.

### The mana bar (HUD)
The spell tab's one HUD mana bar (`draw_mana_bar` 0x49E8B0), between the population strip and the spell list,
always for the local player's own tribe. It is not `bank` against `MAX_MANA`: it shows how well last turn's
income kept pace with an "advisor" estimate of what the currently-charging spell needs.
```text
advisor_eta = tribe.advisor_eta                   # +0x961; 0 when no learned/unblocked/not-full spell is picked
if advisor_eta <= 0:
    value, scale = 256, 256                       # => always full
else:
    scale = advisor_eta * 456 / 256                # advisor_eta x ~1.78
    value = min(last_income, scale)
fraction = value / scale
```
- `advisor_eta` (0x41A850, inside the distribution turn, "Distribution") is recomputed every income turn: pick a
  candidate spell the same way the distribution loop itself does (0x41AD70), read its `SPELL_<spell>_OPT_S`
  constant ("Charges and mana"), scale it by a global byte (0x89D161, not identified) over one more than the
  constant this file elsewhere guesses is `MANA_UPDATE_FREQ` (0x5AA44C, "Reference"), then by the spell's own cost
  over the picker's returned share. Written only for the local player's tribe - the only reader found anywhere of
  `SPELL_<X>_OPT_S`.
- So with nothing charging, the bar is always full by construction (`fraction = 1`), not because `bank` happens to
  be full. With a spell charging, a full bar means last turn's income kept pace with the advisor's own estimate
  for that spell; a short bar means it fell behind - correlates with the tribe's busy/idle mix that turn ("Income"),
  since fewer busy (or more idle) followers lower `last_income` directly.
- Fill colour: left of the fill point, the tribe's own UI-accent colour (0x89C6F5, the same 5-byte per-tribe
  palette record used elsewhere, "Status markers over people"); right of it, a second, tribe-independent colour
  from a small fixed table - a plain fill/track bar, not two tracked quantities.
- A separate draw path paints the whole bar one flat colour instead of the fill above, gated by a per-tribe flag
  (0x89C66D bit 0, set/cleared once per tribe inside the same distribution turn) and a second byte (0x89798B)
  being 0 - two bytes from the training bar's own documented blink flag (0x897989, "The panel's training bar"),
  so plausibly the same blink-timer family rather than an unrelated second blink; not confirmed to be literally
  the same flag. Suppressed entirely in the sandbox's "landscape shaping" mode.
- A separate, confirmed mechanism in the same distribution turn (0x41AB63): when a pass finds no training
  building that needed funding that turn, it fires a one-shot advisor message/sound (the same helper other tips
  use, 0x499D90), debounced so it only fires once. A genuine "mana had nowhere to go this turn" detector in the
  original, but debounced rather than a per-frame condition - not confirmed to be the same signal as the flat-
  colour flash above.

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

### The vault (`TriggerType` 4, "library") [exe, measured]
Traced this session (previously "not traced" above) and checked live with `pyramid_fight_watch.py` against level 1's
pyramid of knowledge - this is the answer to TODO.md's "check `PrayTime`'s unit and the drain speed in the game"
for vaults specifically.

- The type-4 branch of the trigger handler (`0x4fb813`) does its own preamble - record the driving tribe at
  `trigger+0xA0` from a global per-tribe loop counter, then validate a single referenced object (`trigger+0x92`) is
  still a live, intact building in the right state - **but `trigger+0x92` is the vault's own BUILDING id, constant
  for the trigger's whole life, not a tracked "current occupant" person id** (an earlier in-session reading of this
  preamble guessed the latter; live data disproved it: the object at `+0x92` was unit #628, person-type byte 18 -
  `docs/unit_types`'s building type 18, "vault" - sitting still with `hp0`, `tribe255`, the whole session, including
  before the shaman ever started walking there). Who is actually praying is tracked by the gauge engine below, not
  by this field.
- Past that preamble, a vault falls into the **same shared gauge engine as totems** (the gauge table, `+0x72`'s
  10-slot occupant array, the `+0x96`/`+0x9A` counter/cap and `0x4fbd20`'s add/remove calls) - this is literally the
  totem formula above with `C` (`TriggerCount`) at its degenerate value of 1 (one shaman slot), which collapses
  `C²/(C−n+1)²` to exactly 1 (full speed) whenever she's praying there and 0 otherwise - no vault-specific math.
- **Live-confirmed, cleanly**: `trigger+0x96` (gauge) rose by exactly 1 every 4 turns, 32 times in a row with zero
  gaps or skips, until it hit `trigger+0x9A` (cap) = 32 and stopped. 32 turns × 4 turns/step = 128 turns = 10.67 s
  at 12 turns/s - matches "`PrayTime / 3` seconds" above exactly (`PrayTime` = 32 for this vault, cap = `PrayTime`
  directly since `C² = 1`). **`PrayTime`'s unit is confirmed the same 4-turn tick as the totem formula, not
  something vault-specific** - answers the open-populous question directly.
- Before the gauge itself starts moving, there is real travel time: the order (self-issued? by the player - not
  traced which) appeared on the shaman (`+0xA7` order-kind-cache = **33**, a correction/addition to "Order kinds"
  above where 33 was only guessed as preacher's own "PREACH" banner - live, a shaman walking to and entering a
  vault of knowledge carries the same kind-cache value, so kind 33 looks like a shared "stand and perform a
  stationary ritual at a target building" primitive, not preach-specific; not traced further) 36 turns (3 s) before
  the gauge took its first step, consistent with "fills ... while she prays at the door" (worship.md) rather than
  from the moment the order was given.
- Not measured this session: the drain rate with nobody praying (no drain happened in this trial - she prayed
  straight through to full) and whether it matches the totem's "same time as filling at full speed" claim for a
  vault too.

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
  advisor's mana bookkeeping - this is the HUD mana bar's own fill value, see "The mana bar (HUD)".
- Hidden spells (record +0x00 = 2: armageddon, bloodlust, teleport) are not offered in normal play. Burn is not
  castable (record +0x1A bit 0 clear).
- A timed "landscape shaping" mode (global 0x89D17C bit 0x20, `LSME_DURATION_SECS`, file 120 s) uses record
  +0x2E as the charge cap (5 for erosion, land bridge and flatten, 0 = not offered) and +0x22 as the range
  (7168). What starts it in normal play is not traced.

### Components
| Component | Effect | What it does |
|---|---|---|
| Simple blast | 7/0x01, 0x4E (spell blast), 0x38 | 3 turns; radius 2, 4, 5 cells. Each turn every non-allied person within the radius takes `BLAST_DAMAGE_PERSON` (exe 50, not in the file; live-confirmed exactly, 5 separate turns across 3 persons, including while already airborne), so up to 150; pushed each turn within 2.5 cells, cumulatively (140 outward, +98 up, × (1280 − d) / 1280, scaled by the slope: "Blast push and flight"). Buildings take `BLAST_DAMAGE_BLDG` (exe 40) once, vehicles 50 per turn. Allies: no damage, at most `BLAST_FRIEND_AFFECT` (5) pushed, on the last turn. Not pushed: own shaman, angel of death; immune: shielded units. Ghosts in the centre cell are removed. |
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
Each turn of a simple blast (0x50B740), for every person it may push in its current radius (disassembled
this session, `0x50b9a6`-`0x50ba05`; `edi` is the blast instance, `esi` the candidate person):
```text
d     = horizontal distance to the centre (2D, wrapping, square root; 0x4503F0), clamped to 0..R
R     = the blast instance's OWN radius field (+0x6C, i32) - NOT a fixed 1280; read fresh every turn
f     = (R − d) / R                               # 1 at the centre, 0 from 2.5 cells on: no push, damage only
angle = direction from the centre to the unit     # straight outward, no randomness
len   = push_k × f, then the slope rule           # push_k = blast instance +0x74 (i16, "140"); slope rule 0x4E93F0
h     = ground(unit) − ground(unit + 32 units along the push)
len  += len × clamp(|h|, 1, 16) / 16 if h ≥ 0 else −that; at least 3
vel.x += len × sin(angle); vel.z += len × cos(angle)        # added to the velocity, not replacing it
vel.y += vert_k × f                               # vert_k = blast instance +0x78 (i16, "98"); no slope rule here
flags  : airborne (+0xC |= 0x2000 | 0x80000)
```
`push_k`/`vert_k`/`R` are per-instance config, not immediates in this shared code - set by whichever spell
creates the blast (Blast, Firestorm's fireball, Volcano's), so "140"/"98"/"1280" are this spell's own
defaults, not universal constants of the function itself. The slope rule's `0x4E93F0` call only ever touches
`vel.x`/`vel.z` (it's passed `len` and writes through a pointer into those two fields) - the vertical term is
a plain `add`, confirming the slope rule is horizontal-only.
- The slope rule gives ×1.06 on flat ground, up to ×2 when the ground falls away along the push, and almost
  nothing (3) against a rising one.
- The pushes add up: a person still within 2.5 cells on the 2nd and 3rd turn is pushed again, less as it got
  farther. Not pushed: the caster's shaman, angels of death, shielded units, units inside something (+0x9F);
  allies only on the last turn, at most `BLAST_FRIEND_AFFECT` (5).

**The fire trail, traced this session**: right before the push above, a per-instance blast flag (+0x7A bit
0x1 - not confirmed which blast callers set it, plausibly tied to Blast's own "burn cell" component, since the
same shared blast code also serves callers that clearly shouldn't ignite anyone, e.g. a destroyed boat's land
explosion) puts a pushed person not already there into a new, purely cosmetic **state 26** ("scorched by a
blast") and sets a trailing-effect counter (+0xA4) to **24**. That counter is read by a single shared function
(`trail_effect_tick`, 0x4D9200) called for every person every turn its own +0xA4 is nonzero, independent of
state: it always spawns a short-lived grey smoke puff (the same "7/3" effect documented as the firewarrior
bolt's own trail puff) at the person's position, and - only while the counter is still above 8 - ALSO spawns
the SAME trail effect a flying fireball shot leaves (0x50C410): **hfx 1120, confirmed by eye elsewhere in this
doc as "a 16x16 white-yellow fireball"** - not a dim ember, a trail of small bright fireball-coloured sprites.
With state 26 starting the counter at 24, that means the first several turns of being airborne get both the
grey smoke puff and the bright fireball-sprite trail together - **the bright trail is what actually reads as
"fire"**, tapering to smoke-only as the counter drops to 8 or below, then nothing once it reaches 0. **This is
purely visual**: state 26 deals no damage of its own (the damage is the already-documented
`BLAST_DAMAGE_PERSON` ticks) and is a different mechanism from actually being on fire (state 31, "Person
physics" below) - a scorched-by-blast person is not thereby burning in the damage sense, even though the
smoke-puff part of this same shared counter/function is also what an ACTUALLY burning person uses (state 31
resets the same +0xA4 to 4 each tick - always ≤ 8, so ordinary burning never gets the bright trail, only
intermittent smoke).

Then the person physics moves it every turn ("Person physics", blown mode).

Worked examples (hand estimates, flat ground, not measured):
- Hit once at the centre: 149 outward and 98 up; it rises about 200, lands after about 8 turns at about −126
  (dust, no damage), having travelled about 2.2 cells in the air and 0.8 sliding: about 3 cells.
- At the centre on all 3 turns: the vertical kicks add up (98, then about 153, then about 196); it rises about
  950 and lands at about −220: 700 fall damage on top of the 150 blast damage, 850 of a brave's 1000. Horizontal
  speed reaches the 192 cap.

Live-measured (two sessions, 11 pushed persons total, pop3-rev/docs/cheat.md "Live memory: measuring a
Blast"): the downstream physics this section hands off to ("Person physics") matched exactly - gravity
-32/turn and air drag -2/turn on velocity while airborne, zero exceptions across ~150 logged turns, and the
192 horizontal velocity cap, seen both untouched (192 itself, on two axes at once) and one drag-tick later
(190) - cross-checked against the movement-profile table on disk, not just live memory. Direct damage while
already blown is confirmed too: exactly **-50** life on five separate turns across three persons (even while
still rising, not just once grounded) matches `BLAST_DAMAGE_PERSON` exactly, and exactly **-700** life on
three separate landings (persons 1666, 1694, 1320) matches `FALL_OUT_OF_WW_DAMAGE` exactly, applied the turn
after `height` first reaches the ground rather than the same turn.

Third session, logging every person every turn unconditionally (not just already-blown ones, 192 persons,
14 real casts): finally caught the push itself pre-to-post, and found a processing-order detail this section
didn't have - **a pushed person's position and velocity stay completely frozen for the blast's own 3 active
turns**, even sitting exactly at the impact point throughout; the push (and the blown flags) only becomes
visible the turn *after* the blast effect itself disappears. Externally, the 3-turn effect commits as a
single step, not three incremental ones. Direction is confirmed clean: across ~15 detections, the push points
from the centre to the person to within 0-10°, matching "straight outward, no randomness" exactly.

Fourth session, a controlled flat-ground trial (15 units, 3 casts, every turn logged, no gaps): on flat
ground, horizontal push vs. predicted `140 × f` lands at **0.97-1.04x** on the cleanest single-push
detections - essentially an exact match (the small excess matches the slope rule's own flat-ground floor,
×1.0625). But vertical push at the centre (`f = 1`) measured **exactly 132, not 98 - reproduced identically
across this session and the third one**, different persons, different casts both times. Away from `f = 1`
the vertical readings get noisy (confounded by at least one unit's own unrelated vel.y wobble, unconnected
to any blast). The ~2x horizontal outliers seen both sessions are not a second push: since the slope rule
is horizontal-only (see the formula above), a bigger slope multiplier explains a ~2x horizontal ratio
sitting next to an unchanged ~1x vertical one, which a doubled push could not. "98"/"140" are independently
confirmed as real, used values: "Vehicles"/"Life, damage and destruction" already cites a destroyed boat's own
land-explosion call as `0x50B6F0 with 3, 2, 5, 0x62, 0x8C, 1, 0` - `0x62` = 98, `0x8C` = 140 decimal, passed
as literal init params to the very same shared function. So 98/140 are correct for at least that call site;
the Blast spell's own cast measuring 132 instead is consistent with the disassembly (every caller sets its
own `push_k`/`vert_k`), not a doc error - still worth reading `vert_k` straight off a live `kind=blast` row
(now logged by `measure_blast.py`) to confirm which spell's instance uses which value.

**What happens to a person landing in water, confirmed**: a blown person whose landing point is water
doesn't just land - height stays at the water surface (`0`, locally) for one extra turn, then its blown flag
(+0xC bit `0x80000`) clears and a *sinking* sub-state takes over: flags gain bits `0x2`/`0x10000`/`0x100000`/
`0x40000000`, and height now drops by a fixed **-16/turn regardless of the stored velocity** (which freezes,
unused) for exactly **16 turns**, reaching **-256**, at which point life is forced to exactly **0** (a clean
kill, not a subtraction) and the flags lose bit `0x2`. Confirmed identically 3 times (persons 1827, 1831,
1272 across two sessions) - same -16/turn rate, same 16-turn count, same -256 kill depth, every time. This is
independent of the ordinary hard-landing fall damage (`FALL_OUT_OF_WW_DAMAGE` 700, conditional on vel.y ≤
-200 the turn height first reaches 0): person 1827 took both (-700 first, then drowned 17 turns later),
while 1831 and 1272 touched down too gently for the fall damage (vel.y -188, -188) but still drowned on
schedule - drowning here looks unconditional on impact speed, only on the landing point being water. This
mirrors a destroyed boat's own sinking ("Vehicles", height -8/turn, deleted at -192) - the same kind of
fixed-rate sink-then-remove mechanic, different per-thing-type constants.

Still not measured precisely: the very first `BLAST_DAMAGE_PERSON` tick that turns a standing person into a
blown one (invisible to a blown-only filter; the person's velocity jump these sessions caught is the
*committed* push already past that point) - see the open item under "Open
questions" ("Blast") for what a corrected trial needs to catch instead.

### Per spell
| # | Spell (file cost, range, charges) | Mechanism |
|---|---|---|
| 1 | Burn (exe 12 000; not castable) | Standard shot, burn cell, smoke, explosion. No damage to people. |
| 2 | Blast (10 000, 3072, 4) | Fireball shot, then simple blast, burn cell, smoke, explosion. No thrown state: people are pushed (see "Blast push and flight"), and only a hard landing hurts them. |
| 3 | Lightning (80 000, 6144, 4) | Kills outright up to `LIGHTNING_NUM_KILLS` + 1 (file 7) people in the single target cell, any tribe except the caster's shaman, angels of death and shielded units: own followers and enemy shamans die too (0x511F70, walking the cell's unit list). People inside a building are not in that list (leaving a hut puts a person back into it, 0x4D80E0 → `insert_unit_into_land_tile`), so they are not killed (inferred): the building catches fire and throws them out, panicking, 8 turns later. Plus the lightning component. |
| 4 | Whirlwind (90 000, 4096, 3) | 200 + 24 turns (18.7 s), 120 units per turn. Wanders around the target for 100 turns, then drifts away. Lifts every person in its own cell, any tribe, for 12-19 turns, then throws them: the landing costs 700. Each turn a building in its cell has a 25 % chance to lose 100 wood (a hut survives 3, a temple 8); trees lose 100 wood (50 %); library and prison immune; vehicles get a simple blast. Fully traced this session - mechanism, colour, and exactly what can be lifted: "Whirlwind mechanics" below. |
| 5 | Insect plague (40 000, 6144, 4) | 60 insects for 200 turns (16.7 s), at most 6 cells from the target. Every 8 turns, enemies in its 3 × 3 cells take `SWARM_PERSON_DAMAGE` (100) and panic; shamans and angels immune; ghosts destroyed. Up to 3 times it raids an enemy building: occupants thrown out, damaged and panicked. |
| 6 | Invisibility (50 000, 4096, 4) | The `INVIS_NUM_PEOPLE` (file 6) nearest own followers (not the shaman), for `INVISIBLE_COUNT_X8` × 8 turns (120 s). Enemy auto-targeting skips them; some actions end it early. |
| 7 | Hypnotism (85 000, 4096, 3) | The `HYPNO_NUM_PEOPLE` (file 6) nearest persons of other tribes (not wildmen, shamans, angels) are recreated in the caster's tribe for `HYPNO_COUNT_X8` × 8 turns (file 440, 36.7 s), then return. If their tribe is gone, they stay. Ghosts are destroyed. Exe code found and traced this session (previously derived from file/constant analysis only): `process_hypnotism` (0x50EA60) fires the scan 5 ticks after cast via the shared nearest-N selector (0x515E30) with predicate 0x50EAB0 (the exclusion list) and action 0x50EB00 (the actual kill-and-recreate-under-new-tribe primitive, preserving type/life/max-life/carried wood); the "then return"/"tribe is gone" half is 0x4D92B0 (a per-person-per-tick hook, countdown `+0xAC`, cancelled via a per-tribe wipeout table at 0x89DB11 keyed by the stored original tribe `+0xAD`). This is also, independently, the mechanism behind 7 live-captured GONE/NEW conversion pairs that an earlier session mistook for "Preacher conversion" - see that section's own resolved contradiction. |
| 8 | Firestorm (400 000, 4096, 2) | `FIRESTORM_DURATION` (file 220 turns, 18.3 s); one fireball every 4 turns (about 55) falling on a random point of a 6 × 6-cell square around the target. Each, on a land cell: a 5-cell simple blast that panics plus a ground-fire scenery object (same as Lightning's); on water: an unnamed splash effect instead, no fire; either way, a building there is set alight (0x408CB0, same as Lightning). No terrain height or blend change ("Fireball impact"). |
| 9 | Ghost army (18 000, 4096, 4) | Copies the best own unit type near the target (shaman > firewarrior > preacher > spy > warrior > brave): 6 brave ghosts, 3 specialists or 1 shaman; at most 60 per tribe. No expiry found. They attack only enemy ghosts and shamans, their shots do nothing; melee untested. Blast and insects destroy them. |
| 10 | Erosion (210 000, 4096, 2) | 63 hydraulic-erosion passes, one per turn (5.25 s), on an 8 × 8-cell window jittered around the target: ground washed downhill. No direct effect on units. |
| 11 | Swamp (100 000, 4096, 3) | No terrain change. Lasts 32 000 turns (44 min). Every 4 turns it kills outright every person on its 3 × 3 cells, any tribe, until `SWAMP_NUM_PEOPLE` (exe 10) are dead. At most 30 per tribe (the oldest goes). Removed when most of its cells are unsuitable. |
| 12 | Land bridge (70 000, 5120, 4) | From the caster to the target, 4 cells wide (−1, 0, +1, +2 across the line), heights on a straight slope between both ends (at least 90): each turn `h += clamp(T − h, ±LAND_BRIDGE_MAX_CHANGE) / (64 − turn)` (`LAND_BRIDGE_MAX_CHANGE` 256, 64 being `LAND_BRIDGE_DURATION`), so every cell reaches its height after 62 turns (5.2 s) when the gap is small: the cap makes large gaps fall short (gap 256: 251 reached, 500: 488, 800: 759, 1000: 900); sea cells raised too; a smoke puff on every bridge cell every turn. Buildings left on ground uneven by more than 200 collapse ("Terrain changes: what reacts"). |
| 13 | Angel of death (510 000, 3072, 1) | A meteor, then an angel (person type 8, `LIFE_AOD` 10 000) hunts the nearest enemy on the whole map, shamans included, pulling people out of buildings; carries and throws each victim to death, about one per 28 turns (2.3 s). Ends after `AOD_KILL_COUNT` (40) kills or `AOD_DURATION` (2500 turns, 208 s). Two angels fight by health. Its hunting AI is person **state 28** (`0x4DB980`, found this session while chasing unrelated preacher questions - the per-type default-fallback table `0x5A7064` maps type 8, and only type 8, to state 28): on entry, if it has no live target yet, allocates a class-6 type-8 helper object with model id `0x45` (plausibly a cosmetic marker/trail, not confirmed) and plays a sound once; scans for a nearby ENEMY angel first (matching "two angels fight by health"), else falls back to a per-tribe reference scan sharing the SAME attacker/candidate type-pairing eligibility rule ordinary combat uses (`0x51C4C0`) - duplicated here, not combat-specific code; not fully traced beyond that (a large multi-substate approach/engage machine, `+0x2D` substate, `+0x4F`/`+0x51` target position pair, `+0xA8` a second sub-state layer resembling "Attacking a building"'s own field). |
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
effect 7/0x0A instances, one spawned per turn of flight (clamped 0-4, unconditional) - traced this session
(0x50C410): **hfx 1120, draw class 29, the EXACT SAME sprite the fireball's own glowing ball uses** (confirmed
by eye elsewhere in this doc as "a 16x16 white-yellow fireball") - not a separate dim ember sprite, a trail of
small bright fireball-coloured sprites left behind every turn; not the plain grey smoke puff (7/3) the
firewarrior's bolt leaves; and a self-attached effect 7/0x4F
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

### Whirlwind mechanics [exe]
`process_whirlwind` (0x50F490, effect state 21), traced in full this session. Two timers: +0x72 counts down the
active "scanning and damaging" phase; once it hits 0 the effect switches to a quieter "drifting" phase instead of
ending outright (ending itself is a separate, not-re-traced branch reached later). Every call, regardless of
phase, walks and recolours a linked chain of its own effect objects (+0x74, a "next" pointer) - this chain is the
funnel's own visual.

**The water-colour effect, mechanism found**: for each link in the chain, the cell underneath is read (the
ordinary map-cell blend byte, +0xA of 0x8A03E4 - "Terrain land colour"'s own table) and a colour/intensity byte
(+0x3C) is picked: **open sea gets its own dedicated colour class**, distinct from three blend-range classes used
over land/shore. Critically, each link's NEW colour is computed only after first copying the PREVIOUS link's
colour into it - so a transition cascades one link at a time up the chain rather than recolouring every segment
on the same tick. That one-tick-per-link lag is exactly the "changes progressively, not instantly" look you
described, and it runs both ways (reverts the same way coming back over land).

**What gets lifted, and what doesn't** - only the active ("scanning") phase does this, by looking at whatever
single thing occupies the funnel's own current cell:
- **A person** (any tribe, not an angel, not shielded, not already airborne): pulled out of a building first if
  it's inside one (the same eviction helper combat uses), then set to **person state 24** - genuinely lifted,
  up to 19 ticks (matching "12-19 turns" exactly), then thrown: a 192-unit push at a random angle plus a flat
  230 vertical velocity, which is what the ordinary fall physics then turns into `FALL_OUT_OF_WW_DAMAGE` (700)
  on a hard landing - the "WW" in that already-known constant name is "whirlwind", now confirmed. State 24 looks
  generic ("held by/orbiting a reference thing", 0x4D8E40) and the previous draft guessed it was shared with
  preacher conversion - re-checked this session with an exhaustive exe-wide search for every literal "set state
  to 24": **Whirlwind is the only thing that ever puts a person there**; preacher conversion's own state-23
  handler never writes it, and no per-type default-state table entry resolves to it either (0x4D8E40's row).
- **A building**: never lifted. Same 25%/turn wood-pull already documented, through the same code path attacks
  and fire use; once a pull removes no wood (it's already at its floor), the building is destroyed outright with
  the same mesh-shatters-into-debris sequence the "uneven ground" destroy reason uses ("Terrain changes: what
  reacts") - worn down and eventually shattered, not flung into the air.
- **A tree**: never lifted either. Same 50%/turn wood loss already documented; while it still has at least 100
  wood left, this ALSO spawns a small flying effect object at its position (carrying the tornado's own id) -
  **this is the closest thing to "lifted cut wood" in the whole mechanic**: a loose debris/chip puffed visibly
  off the tree, not the tree itself going airborne.
- **Loose scenery of a couple of specific types** (plausibly wood already lying on the ground, not independently
  confirmed): deleted outright - sucked away with no flying visual at all, unlike a tree's own chip effect.
- **A vehicle**: never lifted - hit with an ordinary simple blast (same radius/push numbers as Blast, Lightning,
  and building destruction elsewhere).

So to directly answer "can only units and cut wood be lifted": **persons are the only thing that is actually
lifted** (state 24, carried, thrown). Loose/cut wood is never carried along bodily - a tree sheds a small flying
debris effect while being worn down, and ground-lying wood piles are simply deleted, which might be what reads
as "the wood gets lifted away" in play. Buildings, trees and vehicles are all affected in place (wood loss,
destruction, or a blast) without ever leaving the ground.

### Erosion passes [exe]
Per-turn (state 24, per-turn 0x50FF30 - Ghidra's own name `process_erosion` is correct, despite sitting in the
same code area as the shot handlers; init at 0x509DC8, the class-7 type 0x17 case of `init_unit_type_7`'s
dispatch table, pushes state 24 then sets the duration counter +0x6C to a bare 64, not a named constant):
```text
every turn: duration -= 1; duration == 0 -> end (teardown, no pass this tick)
  else: two PRNG draws, each "& 7, - 4": a jitter of -4..+3 half-cells on each axis around the own position
        call 0x4983A0(jittered point, 4) - one hydraulic-erosion pass
```
Decrement happens before the end check, so the effect runs 64 ticks (5.33 s) but only **63 actual passes**
(the 64th tick just ends it) - 5.25 s of visible erosion, matching the per-spell table above.

0x4983A0 scans an 8 x 8-cell window centred on the jittered point (the `4` argument is doubled internally to
the window's full side length) and, per cell, compares its height against each of its 8 neighbours - the 4
diagonals weighted by 0xB505/0x10000 (~=0.707, a genuine distance correction), the 4 orthogonal ones
unweighted - keeping the steepest downhill one and the count of neighbouring cells that flow into it. That
per-cell flow count is normalised against the single largest flow count found anywhere in the window this
pass (divide 1.0 by the max, in 16.16 fixed point), then combined with a per-cell factor and randomised
multiplicative terms (constants 0xC350 = 50000 and 0xFA0 = 4000, plus a `(prng & 0x7FFF) + 0xC000` term, i.e.
roughly a 0.75-1.0x jitter) to get the actual height change applied to that cell, all carried internally in
16.16 fixed point. **The exact closed-form combination of those terms (and so the literal height-per-pass
number) is still open** - this session traced as far as identifying the pieces (next step: finish reading
0x498741-0x498883's fixed-point chain, or measure live - cast Erosion on a sloped area in a test level and
sample several cells' heights every turn with `livemem.py`, which would settle it faster than finishing the
static trace).

Two things are now confirmed that were open before: the write-back **floors each cell at height 0** (`cmp
[cell],0 / jge / else set 0` immediately before the store, 0x4988EA-0x4988EF) - ground never goes negative,
it erodes down to sea level and stops, confirming "it can go down to 0 (water)" literally; and the result is
converted back from 16.16 fixed point (`sar ebx, 0x10`) and written as a plain integer into the live
heightmap, then followed by the same two calls every terrain-changing effect ends a cell change with -
`0x44DDF0(cell, r, 1)` (landscape recompute) and `0x44F2F0(1, cell, r, -1)` ("ground moved" flag, "Terrain
changes: what reacts") - so persons/buildings/vehicles on eroded ground react exactly as they do to any other
terrain change, nothing erosion-specific.

This is a real steepest-descent slope pick, a different algorithm from land bridge/flatten's "converge toward
a target height": erosion looks at local slope only, confirming "ground washed downhill" literally rather
than as a figure of speech.

Impact on our specs: ours applies one instant, flat, radius-4, -120 height drop (spells.md); the original
instead runs 63 of these small, locally-slope-following passes over 64 ticks (5.25 s), each jittered within a
few cells of the target and touching only an 8 x 8-cell window, following the real local slope (diagonal
neighbours weighted ~0.707) rather than a fixed radius and depth, with each cell individually floored at 0 -
a visibly gradual, small-footprint erosion that approaches sea level and stops, never a sudden crater and
never undershoots into negative height. The exact per-pass height delta is still open (see above).

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

### Insect plague ("Swarm"): the cloud itself is invisible, drawn only by its own spawned bee-dots [exe]
Traced this session (2026-10-20) to answer "how are the bees rendered", complementing the already-documented
mechanics in "Per spell" row 5 (60 insects, 200 turns, 6-cell leash). The spell's single effect byte (spell
record, model 5, `base = 0x5A80D0 + model*0x3E`, the per-type effect byte at `+0x28`) is class-7 type 0x15; its
init thunk (table 0x50A59C, index `type−1`) is a one-liner forwarding to a dedicated function, `0x510020`
(the cloud's own setup) — not the generic drawable path every other traced class-7 type above uses:
```text
init 0x510020:
  height = ground(cast point) + 200          # hovers 200 units above the ground, not on it
  speed  = 80                                # SPEED, already cross-confirmed ("Walking speed")
  duration (+0x6C) = 200                     # 16.7 s - matches "Per spell"'s 200 turns exactly
  insects-left (+0x72) = 60                  # matches "Per spell"'s "60 insects" exactly - this is that counter
  home position (+0x75) = the cast point     # its own anchor, read back every turn (below)
  heading (+0x57) = random 0..2047; re-aim timer (+0x70) = random 0..31 turns
  5 target slots (+0x79..+0x89) cleared; a "flying/no-collision"-style flag (+0xC bit 0x80) set
```
**It never calls `0x4EE700`, the usual drawable setter, anywhere in its init or its per-turn code** (checked the
full body of both) - so it keeps whatever the shared class-7 init (`0x50BCD0`, run before every type's own init)
already set: `0x4EE700(&unit+0x33, index=0, class=0)`, the same "class-7 default" the Swamp write-up above
names and confirms draws nothing. The swarm's own wandering point is therefore invisible in the world, same
convention as Swamp's own body - not an oversight unique to this session's trace, a shared pattern for effects
whose role is pure simulation, not a drawn shape.

Per turn (state 0x16/22, not 23 - an easy off-by-one trap in this table, see the Reference row below -
`0x510120`, Ghidra's own name `process_insect_plague` is correct):
```text
every turn: play ambient sound 0xA4 (unconditional unless a flag is set) - the only direct audible trace
sub-state 0 (the first 4-ish turns after cast): just a delay, then falls into the main loop below
main loop, every turn:
  d2 = squared distance from home (+0x75) to current position
  if d2 > 0x900000 (~6 cells, matching "at most 6 cells from the target"):
      heading = angle(current -> home) (0x586074) + a small random jitter (±63)   # steers back toward the cast point
  else, once every re-aim timer expiry (random 0..31 turns): heading = a fresh random angle
  move one step at speed 80 along the current heading (the standard move helper, 0x4E6A70)
  every 64 turns (duration's own low 6 bits == 0): look for a nearby enemy to "raid" (not re-traced this
    session - the kill/panic/building-raid mechanics are already in "Per spell" row 5)
  while insects-left (+0x72) > 0: spawn one class-7 type-0x1B "bee-dot" (below) at a random offset up to about
    ±320 world units (roughly half a cell) from its own current position, on every tick this runs (0x510D40)
```
The visible "bees" are these separately-spawned type-0x1B instances, not the cloud itself - up to 60 of them
over the spell's life, matching "Per spell"'s own count. Type 0x1B's init (table index 0x1A, `0x509E19`) DOES
call the normal drawable setter: `0x4EE700(&unit+0x33, index=0, class=0x1A)` (table `0x5A6AF8` row 26, renderer
byte 0x0D - not one of the already-named renderer bytes 0/1/3 elsewhere in this doc, so its exact visual
(a single hfx sprite id? a special multi-point renderer?) is not identified this session), hovering 50 units
above the ground. Its own per-turn (`0x510CB0`) idles for its first 4 sub-states, then cycles a ~19-step
sub-state loop (wrapping at 23): while it holds a "reference unit" (+0x74, the same per-unit target-tracking
field style as the wildmen's own "idle near a reference unit", "Wildmen") it repositions itself each step
relative to THAT unit's current position (`0x49BCF0`) rather than its own spawn point - i.e. once the cloud has
picked a victim, its bee-dots visibly orbit the victim, not the cloud's own wandering point. Not confirmed this
session: whether the spawner (`0x510D40`) copies the cloud's own current target into each new bee-dot's `+0x74`
at spawn (plausible from the shared field convention, the actual write wasn't single-stepped), and the bee-dot's
own expiry condition (its own duration field wasn't read this session).

**A few bee-dots ARE tribe-coloured, confirmed (2026-10-20, following up a direct question):** right after
`0x510D40` allocates each new bee-dot, one more LCG draw (`& 0xF`) gives a 1-in-16 chance (about 6%); on that
roll it reads the new bee-dot's own tribe (`+0x2F`, inherited from the caster at `alloc_unit` time) into a
per-tribe byte table at `0x5A89CA` (stride 5) and stores the result in the bee-dot's own `+0x8A`; the other
15/16 of spawns instead store one fixed global byte (`0x89C6F5`, not tribe-dependent) there. `0x5A89CA` is the
SAME underlying table "Status markers over people" already names as `0x5A89CB` for the shield rings' own
`tribe_colour[tribe]` (that code reads the record's byte+1; the swarm reads byte+0 - one shared 5-byte-per-tribe
struct, two different fields, not two tables) - **and byte+0, read directly against the shipped `pal0-0.dat`
this session, is each tribe's own flag-coloured palette index**: tribe 0 `0xDA` → RGB (35, 51, 111) dark blue,
tribe 1 `0xF2` → (95, 7, 7) dark red, tribe 2 `0xEC` → (163, 119, 19) mustard/gold, tribe 3 `0xE2` → (23, 103, 63)
dark green - an exact match to the four player colours. `draw_polygons` (`0x46AE3D`) reads a unit's own `+0x8A`
back, multiplies by 4 into a small cached RGB table (`0x87EFF8`, runtime-built, not static exe data - likely a
fast palette-index→RGB cache shared with other flat-colour draws, not bee-specific) and passes the result to a
dedicated flat-coloured-point draw call (`0x4B7F90`), distinct from the ordinary textured hfx billboard path -
strong circumstantial fit (it is reached reading the SAME `+0x3A`/table-`0x5A6AF8`-row fields the bee-dot's own
init sets, right in the same code neighbourhood) but the exact dispatch value that selects this specific branch
over `draw_polygons`'s many other small per-kind cases was not pinned down this session. Net picture: roughly one
spawned bee-dot in sixteen flashes the caster's own tribe colour as a plain coloured point, the rest share one
fixed neutral tint - matching "probably a few are coloured" exactly, not an all-or-nothing effect.
- Impact on our specs: TODO.md "Spells" - Swarm isn't implemented client-side yet; when it is, the original's
  own look is a diffuse, slow-wandering scatter of ~60 small point-sprites hovering above the ground (clustering
  around a current victim once one is picked), not one big cloud sprite or a ground-level mist like Swamp's own
  (deliberately absent) rendering - with roughly 1 in 16 of those points recoloured to the caster's own tribe
  colour (the same four flag colours `0x5A89CA` gives, see above), the rest a fixed neutral tint.

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
| Building (0x408080) | Runs this check every 4 turns, or every turn for a "water-sited" type (boat huts 13/14 and any other type sharing the flag). The base becomes the average height of its footprint's fine-grid cells, except a water-sited type, which instead just keeps its base clamped to a minimum of 1 (never recomputes an average). **This is the slow "building flattens the floor back" case**: every footprint cell within 200 of the base (building record +0x46, every type) is pulled back towards it, 2 units/turn (0x44FDE0, rate +0x44), each pull re-notifying "ground moved" for that one cell; no damage, no destruction - a water-sited type's own water footprint cells are pulled towards sea level (0) instead of the base. Separately, **two destroy reasons** (checked before/instead of the pull-back, never together with it for the same cell): **(1) uneven ground, sub-state 1** - the first footprint cell found MORE than 200 away from its target (i.e. outside the range the pull-back above would have handled) destroys the building on the spot instead of being pulled back, and stops the rest of that call's cell checks/pulls - "too steep/uneven", never checked for a water-sited type (no single target height to diverge from). **(2) flooding, sub-state 2**, ordinary (non-water-sited) types only - before the per-cell pass even starts, if more than 5/8 (62.5%) of the footprint's fine-cells are at or below sea level (height <= 0), the building is destroyed right away; this is the "second destroy reason" the TODO previously listed as untraced/guessed-flooding - now confirmed flooding, not steepness, and confirmed exclusive to ordinary buildings (a water-sited type instead keeps sitting there, base clamped to a minimum of 1, immune to this check). The two reasons are also visibly different: sub-state 1 shatters the building's own mesh into flying debris then triggers a "simple blast" explosion on it (0x406F40, Ghidra's own name for it is `alloc_spell_blast_or_sinking`); sub-state 2 instead hands it to the same pending-object slot person-death's spirit controller uses, spawning a dedicated "building sinks and rocks" animation ("Building sinking (flooded destruction)" below). Either reason sets state 3 and fires the destroy/explosion pair (0x4ED6F0/0x4ED640) unless already suppressed (+0xe bit 0x10); no ashes are made by this path either way (0x40B230 is attack/fire-destruction only). |
| Boat (vehicle types 1, 2) | One forced movement pass (0x4E7FB0); no damage or destroy call in it. A boat whose cell becomes all land is stranded at the next 16-turn check (state 8): passengers put off, it keels over, explodes 160-287 turns later ("Vehicles"). |
| Balloon (vehicle types 3, 4) | Never explodes from it. Each turn (0x4E8F00) it samples the ground ahead, takes the highest point plus a hover height of its type (another one over buildings: vehicle table 0x5A7938, stride 0x17, +0xD / +0xF) and climbs or sinks towards it at its profile's vertical rate: a bridge under it lifts it. |

### Building sinking (flooded destruction) [exe]
What the flooding destroy reason above (sub-state 2) actually spawns: a class-10, type-0x13 stand-in object (`alloc_spell_blast_or_sinking`'s own sink branch, 0x406F40 -> creation handler 0x500D1B -> per-turn animation 0x503550), copying the dying building's position, facing and a large block of its own instance fields so the stand-in keeps looking and sitting exactly like it did. It lives for a fixed 80 turns (6.7 s) and runs two things at once over most of that time, before a short final flourish:

- **Sinks**: every 8 turns, a ramping counter (0 up to 20, growing by 2 each step) is subtracted from the stand-in's own base height - i.e. it drops lower and lower, accelerating from a slow start up to 20 units/8 turns (2.5 units/turn) at full ramp.
- **Rocks**: every turn, a damped oscillator (amplitude up to 56 units, reflecting - flipping sign - whenever it crosses zero) is added straight into the stand-in's own facing/yaw angle, wrapped to a full circle. This is a genuine orientation change, not a position wobble: **the building's own angle oscillates and decays, like a structure physically tipping back and forth as it founders** - confirming the original does tilt the building while it drowns, not just sink it straight down. The oscillator's target lean is picked from an 8-direction compass histogram of nearby land cells (closest shore wins), recomputed each turn until the rocking settles (both axes of a secondary shake displacement under ~56 units), at which point a short spiral/settle flourish plays (the object nudges itself along a small interpolated path, with something spawned at each point along it - plausibly a splash/bubble, not independently confirmed) before the 80-turn timer runs out and it's removed, same as the blast reason, with no ashes either way.

Not traced further this session: the exact oscillator math (a handful of small trig/angle helpers, 0x586074/0x4E9BA0/0x4E9B70/0x4E6C80 - their role is clear enough from how the result is used above, just not their internal formulas) and the object spawned during the final spiral (0x4EE580, plausibly a splash/bubble particle).

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
| M | 0x74A354 | pure rotation (column norms 16 384) | yaw follows the player; **pitch fixed at about 20.4°** (cos 15 357/16 384, sin 5709/16 384) **while the player drives the camera** - varies during AI-script flybys, see "Camera flyby state" below |

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
- M decoded in full (column order `M00 M01 M02 / M10 M11 M12 / M20 M21 M22`, slots at +0x00, +0x04, ... +0x20):
  `M00=cosY, M02=sinY, M10=sinP·sinY, M11=cosP, M12=-sinP·cosY, M20=-cosP·sinY, M21=sinP, M22=cosP·cosY` - so
  `yaw = atan2(M02, M00)` and `pitch = asin(M21/16384)` read straight off the matrix, live, every frame.

## Camera flyby state (tribe_struct) [exe, partial]
Where the matrix actually comes from, and how it behaves during an AI-script flyby (ai-scripts.md "Worked
example: level 1's opening flyby") rather than normal play. Confirmed:
- `clear_land_draw_state` copies the matrix from a pointer at 0x74A350, rebuilt each frame as
  `tribe_struct_base(local player's own tribe)`, i.e. `0x89D1C8 + tribe*0xC65` ("Reference" "Tribe record") with
  the tribe index read from 0x89C6F0 - the matrix is the ACTIVE PLAYER's own tribe record's first 36 bytes, not
  a separate camera struct. Fits a hot-seat design (each tribe keeps its own last camera orientation); means any
  further camera-path fields live inside tribe_struct too, not elsewhere.
- Pitch is not a fixed per-game constant - it's only fixed while the player drives the camera (the 20.4° figure
  above). Measured live through level 1's opening flyby: pitch swings from that idle 20.4° down to about 55°
  (measured turn ~130-140 after the flyby starts) and back, then holds flat for a long stretch (about 7s) while
  yaw keeps changing continuously underneath it - yaw and pitch are driven independently during a flyby, not
  locked together.
- `FLYBY_START`'s own turn is anchored to about turn 77-80 after level start (±15, not exact), from three
  independent lines of evidence: the first `FLYBY_SET_EVENT_ANGLE 0 5 35` keyframe's predicted completion turn
  (`start+duration` after `FLYBY_START`) lands almost exactly on an observed yaw plateau; the reincarnation
  centre's own intro animation (turns 0 to about 70) ends right before; and the `POS`/`ZOOM` channels' own hold
  windows (see next point) land in the same range once fitted.

Candidates, NOT yet confirmed - tribe_struct fields right after the matrix (`+0x24` on; the struct's own known
fields resume at `+0x881`, "Reference" "Tribe record"), found by diffing the whole range live during a flyby:

| Offset | Shape observed | Candidate | Confidence |
|---|---|---|---|
| `+0x24` | packed dword, two 16-bit halves move independently | camera focus x (low half), z (high half) | direction of x vs z confirmed by a joint fit against the script's `POS` keyframes' x/z; exact scale/offset not |
| `+0x28` hi 16 bits | wraps like a cyclic angle; flat exactly when pitch is flat even while yaw keeps moving | raw pitch angle | shape strongly correlates with pitch; raw-to-degrees transform not solved (tried simple ratios, inconsistent) |
| `+0x2c` | smoothly monotonic, always a multiple of 0x10000 in this capture | camera distance/zoom | correlates with the script's `ZOOM` channel's hold windows; exact scale/offset not fitted |
| `+0x30` | ramps between 0 and exactly 0x1000000 (2³⁴ / 2¹⁰ = 2²⁴), direction flips at each transition | per-segment interpolation progress (8.24 fixed, 0.0-1.0) | plausible shape, not cross-checked against a known progress value |
| `+0x34` | grows with an almost-constant second difference (quadratic) during transitions | easing/acceleration counter | least certain; could be unrelated |

A joint least-squares fit (grid search on `FLYBY_START`'s turn, affine regression per channel) of `+0x2c` against
the `ZOOM` channel and `+0x24`'s two halves against the `POS` channel's x/z gives R² of 0.1-0.6 depending on the
channel and candidate start turn - clearly the right general shape, not a solved formula; linear interpolation
fit marginally better than smoothstep easing at every candidate turn tried, but not decisively. ai-scripts.md
"Flybys" notes `FLYBY_SET_EVENT_ANGLE` goes "the shortest way" - one measurement during this investigation saw
yaw take what looked like the long way round, but since yaw is a confound of this `ANGLE` channel and the `POS`
channel's own continuous re-aiming (not yet separated), that is not a solid refutation of "shortest way" either.

**D3D axis caveat**: D3D's coordinate convention is left-handed, differing from this engine's own on at least the
Z axis - any of the above values, once their scale/offset is pinned down, need an explicit axis/handedness
conversion before they inform `flyby.rs` or any other spec. One concrete instance already found: the matrix's
pitch sign is flipped relative to how this section used to state the pitch constants (now corrected above).

Not yet done, see TODO.md "Camera flyby state": pin `FLYBY_START`'s turn exactly (find the AI script's own
per-tribe flyby-active flag, the `$57`-equivalent variable the script sets right before `FLYBY_START`, instead of
curve-fitting camera motion), solve `+0x28`'s raw-to-pitch transform, decode the `POS` channel's own coordinate
units, and apply the axis caveat once the above is solid enough to change `flyby.rs`.

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
row_jitter_rows = (sensitivity * disp_sample) >> 10              # disp_sample: SIGNED byte from disp0-X.dat (movsx,
                                                                 #   never treated as unsigned or re-centred by a
                                                                 #   theme constant - a theme's own narrow byte
                                                                 #   range, e.g. file 1's 45..127 vs file 5's
                                                                 #   130..193, already IS the per-theme bias once
                                                                 #   read as signed, no separate constant needed).
                                                                 #   A signed ROW-COUNT jitter, not a small raw
                                                                 #   value - see the correction note below.
altitude_int = (interpolated altitude_index fixed-point value) >> 16   # plain floor - the SAME integer
                                                                        #   DISP_SENSITIVITY was indexed by above;
                                                                        #   no fraction retained past this point
row = altitude_int + row_jitter_rows                              # the TRUE effective row: an integer altitude
                                                                   #   index, shifted by a potentially large
                                                                   #   disp-driven jitter on steep/high terrain
brightness  = clamp( ((disp_sample_at_next_tile_row - disp_sample) >> 2) + (interpolated_light_fixed_point >> 16),
                     0, 255 )                                   # `light` (the +0xD field above) interpolated the
                                                                 #   same way, nudged by disp's OWN row-to-row
                                                                 #   difference - a second, smaller use of disp
index = row*256 + brightness                                     # one flat offset: a large `brightness` can
                                                                   #   legitimately roll over into the next row,
                                                                   #   this is not guarded against
colour = bigf0-X.dat[index]                                      # 0..111 land colour, same byte then fed into
                                                                   # cliff[(blend/4)*128 + colour] (Building ashes)
```
**Correction 2026-10-21 (spec-session follow-up, instruction-level re-read of `row_jitter`/`brightness`'s own
shifts, pop3-rev docs/functions.tsv 0x4C0EC0/0x4BF860/0x4C0380 - the same depth of care that found the masking
below): `row_jitter` carries the SAME undocumented masking-before-shift trick `row_fraction`/`altitude_int` does,
previously missed.** The exe builds it as `imul ecx,ebp` (= `sensitivity*disp_sample`, call it `P`), then
`and ecx,0xFFFFFC03; sar ecx,0x2` - verified algebraically and numerically (signed 32-bit, both positive and
negative `P`) that this ALWAYS equals `(P>>10)<<8`, not `P>>10` as a prose-only reading of the shift amount
suggested: the AND clears bits 2:9 of `P` (the same "clear a bit-window, then shift" idiom `altitude_int`'s own
masking uses, just a narrower window ahead of a smaller shift) before the `sar 2`, so the result always lands
pre-multiplied by 256 - in **row** units, exactly like `altitude_int`, not in small raw-value units comparable to
`brightness`. The pseudocode above is written with that already divided back out (`row_jitter_rows`, the quotient
before the implicit x256) folded into `row` alongside `altitude_int`, then multiplied by 256 once, explicitly, to
avoid the ambiguity the original "`row_jitter + row_fraction*256 + brightness`" phrasing had (both `row_jitter`
and the old `row_fraction` are, as the exe actually computes them, ALREADY multiples of 256 - summing one of them
again `*256` would have doubled that scaling). Net effect: disp noise genuinely jitters the ROW by up to roughly
`sensitivity*disp_sample/1024` - on high terrain where `DISP_SENSITIVITY` reaches its 1024 ceiling and a theme's
`disp` byte is near its extreme, this can be dozens of rows, not a small sub-row perturbation. `brightness`'s own
two shifts (the disp-row-difference `>>2`, `light_fixed>>16`) have NO preceding AND anywhere in any of the three
functions - confirmed plain, undoctored shifts in all three; `brightness`'s formula was already accurate.
**Confirmed 2026-10-21 (instruction-level re-read, pop3-rev docs/functions.tsv 0x4C0EC0/0x4BF860/0x4C0380): this
`bigf0-X.dat` read happens EXACTLY ONCE per pixel, at this one flat `index` - there is no second read at an
adjacent row and no weighted blend of two rows anywhere in any of the three pixel-fill functions.** The row a
pixel lands on is a hard floor of the interpolated altitude (the same truncated integer used for the
`DISP_SENSITIVITY` lookup above), not a value blended with its neighbour - open-populous's own row/row+1 RGB
blend (`terrain-textures.md` "Our bake") has no basis in the traced exe, and the "fraction" in `row_fraction`
above is explicitly discarded by the exe's own masking before it ever reaches this index, which is also why
feeding that same fraction into the column axis directly (the earlier reverted attempt, terrain-textures.md "Our
bake") produced a wrong, dense zigzag: it kept live, per-pixel-changing sub-row entropy that the real game
throws away at this exact step. See terrain-textures.md "Our bake" and TODO.md "Terrain" for the reimplementation
fix this resolves.
- No per-theme or per-level constant exists anywhere in this path beyond the loaded `bigf0-X.dat`/`disp0-X.dat`
  files and the sunlight-derived `sunX`/`sunZ`/`sunBase`: `POP3_ROW_SCALE`'s single global multiplier has no
  counterpart - the original's disp sensitivity is a fixed, altitude-dependent CURVE (320..1024, the table
  above), not a flat per-theme or per-level scale, and it is multiplicative on disp, not on height.
- `light`'s divide-by-350 and the `(low_nibble + 8) * 16` ambient term are now live-verified on a flat cell (`dx
  = dz = 0`): measured `light = 128` exactly, matching `clamp(147/350 + 128, 0, 255)` with the `147/350` term
  flooring to 0 - consistent with the formula and the flat 147/147/147 sun vector above. A live spot-check on
  sloped cells did not reproduce the formula from a naive `base + (z*128+x)*16` cell-address read; `compute_cell_
  light` (0x4BE230) builds its map-cell address with `x`/`z` masked by `0xFE`/`0xFE00` before scaling
  (pop3-rev/docs/functions.tsv), which a plain row-major calc does not reproduce bit-for-bit.
  **First pass 2026-10-20 claimed this was inert and the neighbour distance matched - that was WRONG, caught
  and corrected the same session.** The error: treating `compute_cell_light`'s own raw `x`/`z` argument as if
  it were the same unit as open-populous's real 0..127 file-cell coordinate, when it provably is not.
  **Corrected, cross-checked against `load_level2`'s height-table populate loop** (0x484C9A-0x484CB8,
  pop3-rev/docs/functions.tsv): that loop runs exactly 0x4000 (16384 = 128*128) iterations, advances the
  `0x8A03E4` table pointer by a flat `+0x10` (16 bytes) on **every** iteration, and reads the level file's
  height values sequentially in the file's own `(z*128+x)` order - i.e. the table's real, ground-truth layout
  is `base + 16*(z_real*128 + x_real)`, one record per real file cell, `x_real`/`z_real` each 0..127, no
  doubling, no skipping. For `compute_cell_light`'s own address formula (`base + (x&0xFE)*8 + (z&0xFE)*1024`)
  to land on that same record, its raw inputs must satisfy `x_raw = 2*x_real`, `z_raw = 2*z_real` - its x/z
  are provably DOUBLE the real file-cell coordinate, not equal to it. So its internal `+2` neighbour step
  (`x_raw -> x_raw+2`) lands on real cell `x_real+1` - the immediately adjacent real cell, a ONE-cell step.
  open-populous's `height(x, z)` takes real 0..127 file-cell coordinates directly (confirmed separately,
  byte-for-byte, against `levl2001.dat`'s own `(z*128+x)*2`-byte layout), so `height(x, z) - height(x+2, z)`
  measures across TWO real cells where the original measures across ONE - the overshoot the original
  hypothesis suspected, now proven from the load-loop's address formula rather than a block-granularity guess.
  Re-read `0x4BE0B0` the same session regardless: identical single formula, identical `+2` raw-unit step inside
  a nested x/z loop over a rectangular range (a bulk sweep, not a second diagonal pass whose result needs
  combining with `0x4BE230`'s) - that part of the first pass still holds. Net: `cell_light`'s neighbour should
  read `height(x+1, z)` / `height(x, z+1)` in open-populous's own (undoubled) coordinate convention, not `+2` -
  see TODO.md "Terrain".
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
  separately - its own water surface, traced this session below.

## Water rendering (watdisp.dat) [exe]
A separate, global asset from the per-theme terrain tables above: `data\watdisp.dat`, 65536 bytes (256x256,
same size class as `disp0-X.dat`'s own noise table), but with **no theme suffix** - one single file shared by
every level, confirmed by its only 4 call sites all being level lifecycle functions (`load_level`,
`load_level_conditional`, and two others) rather than the per-theme asset loaders `al0`/`bigf0`/`cliff0`/`disp0`
use. Loaded (and the water buffers rebuilt) on every level load, not once at engine start.
```text
load: malloc 0x10000 bytes, read the whole file into it, keep the pointer at a fixed global (0x5D45BC) -
  a small state machine (one Ghidra-named function with 3 entry points, plus a 4th separate "load_watdisp_2")
  also allocates two much larger derived caches on the same trigger (0x80000 and 0x28000 bytes, exactly
  65536 x 8-byte and 16384 x 10-byte entries) and fills them with an invalid sentinel - what they are actually
  FOR was not traced this session (not read anywhere in the water-drawing code below); candidates are a
  per-cell wave/foam lookup or a dirty-rect cache, given the first one's entry count matches watdisp.dat's own
  byte count 1:1
draw (main_landscape_mesh_generation, 0x46CB90's per-vertex body): a vertex whose cell is pure open sea (the
  same coastal-shape test "Parking and the shore"/"is_land_at" uses - land and shore vertices take the real
  heightmap value instead, no watdisp involved) reads watdisp.dat TWICE:
    index  = cell_coord x8 -+ (turn % 256) x 257, wrapped to 0..65535      # cell_coord packs (x, z) into one word
    sample1 = watdisp[index with "-"]; sample2 = watdisp[index with "+"]  # scrolling in opposite diagonals
    height_offset = (sample1 + sample2) / 8                                # the ripple
    light        = clamp((sample1 + sample2) / 16 + 16, 1, 32)             # the shimmer
```
No fluid simulation: the "wave" look is two reads of one static 256x256 noise texture, scrolling diagonally in
opposite directions as the turn counter advances (wrapping every 256 turns, ~21 s), summed into a small height
perturbation and a brightness value - cheap, fully data-driven, and the reason the pattern both ripples and
visibly "flows" without the engine ever simulating water. `turn % 256 x 257` shifts the sampled texel by (1, 1)
per turn in the packed coordinate space, i.e. the two layers drift along the texture's own diagonal at a
constant rate, one each way - not random per-frame jitter.

**What consumes `height_offset`/`light` downstream, confirmed by [peterderivaz/populous](
https://github.com/peterderivaz/populous)** (a third-party reverse-engineering writeup, not traced independently
this session - README "References"): the sea is drawn through the same static per-cell lighting algorithm the
land uses, EXCEPT both its vertex altitudes and its lighting are different - the vertices' altitude is adjusted
by `height_offset` (not the real heightmap value, confirming "pure open sea... takes... no watdisp involved" above
is about LAND/shore vertices only, not that sea skips watdisp - it reads watdisp INSTEAD of the heightmap), and
the lighting there is computed from those adjusted altitudes rather than the land's own (distance/slope-derived)
scheme - i.e. `light` is a genuine per-vertex dynamic lighting term reacting to the live ripple height, not a
static value. The writeup also explains WHY two counter-scrolling samples, not one: a single scrolling sample
would show "a fixed wave pattern moving in a fixed direction" (a visible prevailing wind); summing two samples
scrolling in opposite directions cancels that net directional drift while still producing "lots of local swirls
and seemingly non-uniform motion" - the sea should read as animated and swirling, never as a frozen static
surface, but never as a simple flow in one direction either.

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
| Blown, landing | vertical speed ≤ −100: dust (7/3) and a sound; ≤ −200: `FALL_OUT_OF_WW_DAMAGE` (700, ignoring the shield), not on terrain with flag 2 - live-confirmed exactly (-700 life, the turn after `height` reaches ground), 3 separate landings, pop3-rev/docs/cheat.md |
| Blown, on the ground | a slope vector (gradient over ±76 units, 0x4EBC20) is added: the unit slides downhill; friction takes 28 per turn off each horizontal axis (profile +0x14), 72 on terrain with flags 0x3E (+0x16); the unit faces where it slides |
| Ordinary fall | ground dropping under a person (earthquake, erosion, flatten lowering) makes it fall with gravity and drag; the fall damage above is only in the blown mode, so an ordinary fall seems harmless (unverified: the grounded path also calls a landing routine, 0x4E9950, not read) |

- People do not need the terrain's "ground moved" flag: this step follows the ground every turn ("Terrain
  changes: what reacts").
- Example: an earthquake's drop of up to 600 under a person: it falls for about 6 turns (0.5 s) and lands at
  about −190, below the fall-damage threshold even in the blown mode.
- **`ground(pos)` identified and traced this session, answering TODO.md's open "landing on a cliff or a building"
  question: `0x44E940`.** It is a pure TERRAIN-MESH height query - bilinear interpolation of a cell's 4 corner
  heights (read straight from the per-cell struct at `0x8A03E4`, picking the diagonal split from the cell's own
  flag bit, falling back to an edge-interpolation table `0x8A03E8` when the position straddles a cell boundary) -
  **with no class, building, or unit check anywhere in it.** So: a cliff is handled correctly for free (it's just
  steep terrain, the same interpolation everywhere), but **a building gets no special treatment at all** - a
  falling/thrown person's touchdown height is always the raw ground height of the cell, regardless of whether a
  building sits on it. There is no "stands up on the roof" behaviour to trace; if a blown person's flight carries
  it over a building's footprint, it lands at ground level there exactly as it would on open ground (visually
  passing through/landing inside the building's base, not on top of it) - this resolves that TODO item as "no
  special handling exists", not as "not yet found".
- The landing-effects block (dust/sound at vel.y ≤ −100, `FALL_OUT_OF_WW_DAMAGE` at ≤ −200) is gated by the
  landing cell's own terrain-shape-class byte (table `0x5AA328`, bit `0x2`) skipping it entirely on that one shape
  class (not identified further this session) - on top of the already-documented "none on water cells". The scream
  on touchdown (not to be confused with the take-off scream above) is per-type: a shaman plays sound group `0x1B`,
  a wildman stays silent, everyone else plays group `9` - only when life is still above 0 after any fall damage.
  The fall-damage amount itself is read from a named tunable, `0x5AA534` (`FALL_OUT_OF_WW_DAMAGE`, matching the
  live-measured 700 exactly), applied via `0x4DA080` with no attacker (−1).

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

**Corrected a later session - this was only checking state 10's own per-turn body, not the fallback `order_advance`
(0x4366B0) itself runs once a unit's own order queue is empty**: that fallback calls two functions, both now
traced (previously just "neither traced" placeholders in "Command groups"):
```text
0x43D0E0, fn(unit) -> bool: reads the unit's own remembered TARGET CELL (`+0x83`, setter not found this session);
  if none, no-op. Else resolves whichever unit currently occupies that cell (via the map-cell's own unit-list head
  and the global id table 0x890390) and requires it be the SAME TRIBE as this unit (any class, not person-only);
  on a match, auto-issues either:
    kind 8 "enter building" (target the occupant's own `+0x24` id) IF that occupant is itself in person/building
      state 2 AND the cell's own flags byte+1 bit 0x2 is set (a "this is a structure cell" marker, the same bit
      family the preach-order handler's own door-point lookup reads)
    kind 6 "go near X" (target the occupant's own position) otherwise
  gated by a per-kind eligibility table (0x5A7DC4/0x5A7DCD, two more fields of the already-documented 22-byte
  kind-flags record at 0x5A7DC0); either way the remembered cell (`+0x83`) is cleared once this runs, hit or miss.
  **This reads exactly as "if you remember a spot with one of your own tribe's buildings/units on it and have
  nothing else to do, go rejoin it - enter it if it's a ready building, otherwise just walk over and stand near
  it"** - a strong match for the in-game behaviour of an idle follower walking itself into a nearby hut. Not a
  general "search for the nearest house" though: it only ever acts on this ONE remembered cell, never searches.

  **`+0x83`'s writer(s), traced a later session, chasing the user's own follow-up question**: it turns out to be a
  heavily overloaded word, reused for at least four unrelated purposes across the exe (per-state/per-function
  scratch is clearly a common idiom here) - only one of which is the "remembered cell" `0x43D0E0` reads:
  ```text
  group_assign (0x436D00), the ONLY confirmed writer of the "remembered cell" meaning: on a SECONDARY-slot
    assignment, unconditionally ZEROES `+0x83`; on a PRIMARY-slot assignment (`slot=-1`, confirmed exclusive to
    the 5 self-issued autonomous scanners - "Command groups"), UNLESS a flag (`unit+0x17` bit 0x2, setter not
    found) is set, STAMPS `+0x83` with the unit's own CURRENT CELL at that exact moment (packed from `+0x3D`/
    `+0x3F`, the same `>>8 & 0xFE` idiom used for cell coordinates everywhere else in this codebase). So the
    field is not "my assigned home" at all - it's **"where I was standing the moment my last autonomous order
    began"**, and `0x43D0E0` then reads as "once that autonomous task is done and you're otherwise idle, drift
    back toward wherever you started it from" - which, if a unit happened to be standing right by (or about to
    re-enter) its home hut when an auto-attack/auto-scan function grabbed it, looks exactly like "idle unit walks
    itself back into the nearest hut".
  unit_processing_class_1_person (~0x4D3719-0x4D3775), gated to AI-CONTROLLED TRIBES ONLY (reads the person's own
    tribe's controller-type byte, `tribe_struct+0xC1F == 1`, the same byte "Preacher conversion" already uses for
    its own AI-only gates) - runs every turn regardless of the person's own state: INCREMENTS `+0x83` as a plain
    counter, and once it exceeds `(unit+0x24 & 0xFF) + 0x400` (roughly 1024-1279 turns, 85-107 s) resets it to 0
    and calls a new function, `0x4D9B50` (exempts shamans, type 7, outright). **This is a second, independent,
    more direct autonomous work-assignment mechanism**: it walks a GLOBAL linked list of class-9 objects (not yet
    identified by name - plausibly worship/work-site markers, one of the few remaining uncatalogued object
    classes), filters to the unit's own tribe, and for the first one whose own building-type-indexed capacity
    field (a new field in the canonical building-type table, `+0x14`, halved twice before use) exceeds its current
    occupant count (`+0x9A`, the SAME occupancy field "Training buildings" already documents) - **self-issues the
    unit a kind-6 "go near X" order targeting that work site** (via `0x43B3C0`, the kind-6 "give an order" helper
    itself traced earlier this session). This is a MUCH more direct match for "an idle unit periodically looks for
    a workplace with room and walks itself there" than the remembered-cell theory above, but it is exclusively an
    AI-tribe mechanism - for a human-controlled tribe, this whole block never runs, so `+0x83` stays available for
    `group_assign`'s own "remembered cell" meaning instead. **So there are two distinct, non-overlapping causes for
    "idle unit autonomously heads to a building"**, selected by which tribe's controller type the unit belongs to:
    `0x43D0E0`/`group_assign` for human-controlled tribes (passive, only returns to a spot it was already at),
    `0x4D9B50` for AI-controlled ones (active, periodically searches for free capacity and relocates there) - worth
    confirming live which one the user is actually observing (their own tribe vs. an AI one).
  Angel of Death's own state-28 handler (`0x4DB980`, already documented) separately writes literal constants
    (0x1F4/500, 0x258/600) into `+0x83` as its own countdown scratch - unrelated to either mechanism above, just
    the same offset reused by a totally different state.
  The gather-wood state handler (`0x495D70`, "Chopping") takes `&unit+0x83` as a transient 2-byte scratch buffer
    for an internal helper call, never persisting a meaningful value there across turns - also unrelated.
  A handful of other sites (`0x4DAB60`, `0x4DDB30`, `0x4E5FD0`, `0x4E66E0`) read/write `+0x83` too and were not
    chased this session; `find_formation_for_unit`'s own read of a SIGNED BYTE at `+0x83` is a different struct
    entirely (a formation CONTROLLER, class 10, not a person) and is unrelated by construction.
  ```
  So: no, this is not "go fetch wood to grow a building" in the sense of a dedicated wood-gathering AI - the
  gather-wood state itself (`0x495D70`) still has no autonomous caller found anywhere. It's either "drift back to
  wherever you were last doing something" (human tribes) or "periodically relocate to any work site with spare
  capacity" (AI tribes) - neither one specifically about wood, though both would visibly look like "idle unit
  walks into a nearby hut".

  **A candidate explanation for the
  user-observed "idle units tend to form small circles together"**: this fallback's kind-6 branch is the ordinary
  move-family order used all over the exe, and "Formations" above documents several move-ish kinds joining a
  shared formation controller (`add_unit_to_formation`) via a per-kind "wants formation" table bit - if kind 6 is
  one of them, several idle units all autonomously walking toward the same reference point via this fallback would
  naturally cluster at fixed slot offsets around a shared controller, which would look exactly like a loose circle.
  **Not confirmed**: an attempted direct read of kind 6's own "wants formation" bit this session did not reproduce
  the already-documented SET/CLEAR pattern for the kinds that session DID verify (3/4/7/9/20/22 SET, 8/19/21
  CLEAR) even for those same kinds, so the read was against either the wrong address/field or a different
  table-access instruction than the one `0x432DF0` (the actual bit-copy site, itself only partially re-read this
  session) really uses - left as an open hypothesis, not a finding, until that's resolved properly.
0x43D2F0, fn(unit) -> bool: a SEPARATE fallback, reads a different remembered REFERENCE OBJECT (`+0xA1`); if it
  resolves to a live unit of CLASS 4 (**vehicle**, not building - "docs/unit_types" class list) of an
  eligible per-type-table-gated type, same or compatible tribe, with room (0x465750) and not already
  boarded/claimed (0x5178D0) and within reach (0x4503F0 <= 0x3000 units), auto-issues kind 0x16 (the "attack,
  doing half" kind, same handler, 0x435160, already catalogued for ordinary combat) targeting it. Despite sharing
  a handler with combat, the actual precondition chain here (class-4/vehicle, room, reachability) reads as
  "return to / board your remembered vehicle", not combat - worth a second look at 0x435160 itself to see whether
  it is generic "approach and interact with a referenced unit" rather than attack-specific, since this call site
  doesn't fit "attack" at all.
```

**The idle-dispatch cycle, confirmed (user-requested summary) [exe]**: when a unit's own group (primary or
secondary) finishes and `order_advance` finds every secondary slot empty, it does NOT just leave the unit idle -
it runs the save/dispatch/return cycle traced above:
```text
save:     whenever a PRIMARY (autonomous) order is newly assigned, group_assign stamps the unit's CURRENT cell
            into +0x83 (its "last known spot") - unless unit+0x17 bit 0x2 suppresses it
dispatch: the autonomous order runs (see the action list below)
return:   once nothing else is queued, order_advance's own fallbacks re-read +0x83 (or +0xA1 for a vehicle) and
            issue a fresh secondary-slot-0 order back to that remembered spot - entering it if it resolves to a
            same-tribe BUILDING in state 2, otherwise just walking over and standing beside whatever is there
clear:    the remembered field is zeroed the moment this fallback runs, whether or not anything was found
```
This is confirmed code, not a guess - but it's exclusively the HUMAN-tribe path (`0x43D0E0`/`0x43D2F0`); AI tribes
instead get the more deliberate, periodic `0x4D9B50` work-site search (also landing in secondary slot 0, also
clearing/reusing the same `+0x83` field) - see above.

**Known actions that can occupy PRIMARY and so trigger this save/return cycle (not exhaustive - todo: keep
extending as more are traced)**:
| Issuer | Kind issued | What it does |
|---|---|---|
| `preacher_auto_scan` (0x51E7B0), preacher only | 0x15 (21) attack, or 0x20 (32) building-visit | "Preacher conversion" |
| per-type auto-attack scanner inside `0x4F0460` | 0x15 (21) attack | ordinary auto-attack, not preacher-specific |
| firewarrior ranged auto-attack, near `0x51E9A0` | 0x15 (21) attack | ranged variant of the above |
| `0x40BD20`, trigger not pinned down this session | 0x13 (19) attack | self-issue condition not traced |
| `0x520480`, trigger not pinned down this session | not captured this session | gated on `unit+0xA7 == 0` ("no cached order kind yet") |
Followed, once idle, by the return-cycle fallbacks themselves (landing in SECONDARY, not primary, but still
autonomously dispatched): `0x43D0E0` → kind 8 (enter building) or kind 6 (go near X); `0x43D2F0` → kind 0x16
(return to/board a vehicle); `0x4D9B50` (AI tribes only) → kind 6 via `0x43B3C0` (go to a work site with room).
Only one other non-player, non-combat source of an autonomous command-group assignment was found: a training
building's own per-turn code (0x405B80, "Training buildings") gives a handful of its occupants a kind-3 "go to a
position" order from a per-building-type zone table - read so far as occupant/queue positioning (a training or
worship spot), not a wood-fetch assignment; its own "doing it" handler (kind 4, 0x433800) is not yet read, so
this is inferred, not proven. No caller of the gather-wood state (0x495D70, "Chopping") was found outside the
player's own `Order::FetchWood`/`CutTree`-equivalent command path.

**On the idle pose itself (e.g. a warrior's push-ups) [exe]**: `order_execute`'s OTHER fallback - reached only
when a unit has no group at all, not even a stale one, the generic "settle and go idle" path - calls `0x4E32A0`,
traced this session: it returns a fixed id `0x29` (41) for one specific condition (the unit's tribe matches a
global byte at `0x89C6F0`, under a couple of other global flag bits - plausibly "the human player's own tribe,
under some display/option setting", not confirmed), and otherwise returns a **per-type default idle pose id from
a newly-identified field in the canonical person-type table, `+5`** (`0x5A7065` = base `0x5A7060` + 5) - a sibling
of the already-documented `+4` (next-state) and `+7` (fidget-odds) fields in that same 50-byte record. So yes, the
per-type idle pose (push-ups for a warrior, etc.) is table-driven exactly as it looks in play; what the special
`0x29` case actually displays, and what the two global flag bytes gating it mean, were not chased further.

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
| 10/8, 10/9 | fight, pre-fight ("Starting a melee", "Pre-fight (per turn 0x518630...)") |
| 10/1, 10/6 | formations (`process_formation_unit`, `add_unit_to_formation`) |
| 10/0xA | order kind 11's own "melee ring" companion object (`0x519F10`, person sub-state `+0xa9` 0-3: fresh/walking to slot/holding slot/performing a move) - confirmed this is a THIRD combat-adjacent controller, distinct from fight (10/8) and pre-fight (10/9); supersedes the earlier "guard control" guess, not yet reconciled with it |
| guard_control_units (0x890360) | **Started this session, chasing "guard between markers"**: a genuinely Ghidra-named global list (`backup/backup.xml`, `SYMBOL ADDRESS="00890360" NAME="guard_control_units"`), walked by the main turn loop right after `fight_units`/`pre_fight_units` (same cluster, ~0x4EC7F0-0x4ED340) via a per-CLASS dispatch (byte table `0x4ECA80`, jump table `0x4ECA6C`): class 2 (building) → the ordinary building per-turn function (`unit_processing_class_2_bldg`/0x403280, already documented); classes 3/4/5/6/9 → no-op (skipped); class 7 (effect) → `0x478EE0` (unread); class 8 (shot) → `0x4BBF30` (unread); class 10 (controller) → `unit_processing_class_10_internal_2`/0x5028B0 (Ghidra's own name). **That class-10 handler dispatches by controller TYPE 6 vs. not-6** - i.e. it reads as acting on FORMATION controllers specifically (type 6 is the already-documented formation type, "Formations"), calling `0x51EAB0` (type 6) or `0x51F470` (otherwise), both unread - NOT a dedicated "guard post" controller type of its own. So `guard_control_units` looks like a mixed bag of objects needing a periodic "still being guarded/watched" re-check (buildings, effects, shots, and formation controllers) rather than a single dedicated guard-coordinator object. **Resolved, a later session: this list is NOT the movement/patrol mechanism at all.** The actual "patrol/guard between markers" movement is `0x4BA9B0` (see its own `functions.tsv` row and "Command groups" above) - the confirmed "patrol points, up to 8" implementation (shortcuts.md), which fills multiple secondary command-group slots with kind-6/kind-0xB waypoints that `order_advance`'s existing round-robin logic then cycles through. `guard_control_units`/`0x51EAB0`/`0x51F470` is a SEPARATE, parallel concern - periodically checking for a nearby enemy to react to while guarding/patrolling - not yet traced to completion (still open: `0x51EAB0`/`0x51F470`'s own bodies past the enemy-scan preamble, what inserts an object into this list, and the AI script's own `GUARD_AT_MARKER`/`SET_MARKER_ENTRY` opcodes' exact call path - plausibly they call `0x4BA9B0` directly or an equivalent, not yet confirmed). **The patrol-point mechanism itself is now fully traced and written up address-free in `docs/specs/units.md` ("Patrol orders") - see `0x4BA9B0`'s own `functions.tsv` row for the full exe-level trace this was built from.** |
| 10/4, 10/0xC | spirit-rise controller (type 0xC created by the universal death handler, `0x4D5CF0`/person state 3, for ANY death, not preacher-specific - corrects this row's previous "from the preachee's own state 3... Preacher conversion" framing, see that section); type 4's own creation site not found |
| 10/0x13 | building-sinking stand-in, created by the foundation check's flooding destroy path ("Building sinking (flooded destruction)") |

- **Preacher conversion (person state 23) creates no class-10 controller at all** - unlike every row above, it is
  pure person-state bookkeeping on the candidate and striker themselves, see "Preacher conversion".

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
  are read only by drum-tower code (0x40B450, 0x40BD20 - see "Watchtower sentry scan and alarm" below), not by
  this scan.
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
| 0x1C/28 | 0x43B670 | a unit id | 0x51FCE0 - CORRECTS the previous draft's "inline in order_execute": kind 28's own dispatch-table trampoline is a real out-of-line call, not traced further this session |
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

**A whole separate subsystem was traced this session and then RULED OUT as a candidate for the click
dispatcher - a per-tribe "command queue":**
```text
0x8979D3  per-tribe queue header, stride 0x5E2 (1506) bytes: +0x0 write-cursor word, +0x4 "slot
            busy" word, +0x6 onward a 100-entry x 15-byte ring buffer
0x897997  per-tribe STAGING record, stride 0xF (15) bytes - the actual pending-command content,
            copied verbatim into the ring buffer when flushed
tribe+0xC1F   per-tribe controller-type byte (1 computer/AI, 2 human - see "Preacher conversion" for a
                third, independent confirmation of this reading)
0x89D18C  per-tribe "active THIS TURN" slot, stride 0xF (15) bytes - what process_tribe_cmd (above)
            actually reads at its own +0xC

inc_tribes_commands (0x43E320): flushes the LOCAL PLAYER's staging record into their own ring
  buffer (skip if already busy)
copy_tribe_commands_from_buffer (0x43E3C0): loops every tribe (stride 0xC65, bound by the active-
  tribe-count global 0x96EAC0 - re-read this session, confirmed a real loop, not a single fixed
  tribe as an earlier pass's wording implied), flushing whichever ones have tribe+0xC1F == 1
  (computer/AI - not a separate "ready" flag, see the Reference table's +0xC1F row), gated by an
  extra AI-pacing modulo check on a global (0x8928FD) - this IS the AI's own flush path, now
  confirmed by the controller-type value itself rather than just the "AI/network" framing guessed
  from context
mld_function_3 (0x43E480), the main loop's per-turn-boundary gate: pops the ring buffer's front
  entry into the active slot each turn - this is what process_tribes_cmd (0x43E890, iterates
  exactly 4 tribe slots) then dispatches via process_tribe_cmd
```
Checked EVERY writer that could feed this pipeline on an ordinary turn (9 call sites of
`inc_tribes_commands`): every one is level-load/session-start code (`load_level_3`/`_4`/`_upper`,
one "game just started" branch of `draw_main`), never an ongoing per-frame handler. **Correction
this session**: `inc_tribes_commands` (0x43E320) itself was re-read in full and writes no byte
anywhere near `tribe+0xC1F` - it only touches the queue header's busy word and write-cursor and the
staging record; the earlier "the single writer of tribe+0xC1F" phrasing was wrong (or referred to
something found near one of its 9 call sites rather than inside the function), and `tribe+0xC1F`'s
actual writer is still not located - its value is set per-tribe at scenario load from the level
file's own player-type data, not written by this queue machinery. **This queue is tribe-wide, coarse-grained event plumbing (session lifecycle,
and very plausibly the AI/network command channel) - it is not how per-unit move/attack orders are
issued.** Ordinary orders go straight through `group_alloc`/`order_set` as already documented above;
the player's actual click→order translation remains unlocated, now with a large neighbourhood of the
exe ruled out as its location. Next place to look: mouse/cursor-polling code, not yet approached
from this angle.

**A related, unresolved thread**: what makes a unit unselectable while being preached at (a real
in-game rule: the preacher stays selectable, the person being preached at does not) was also chased
this session and not settled. The one "can select" gate found, `can_select(unit, mode)` (0x4E3430,
tested only by `unit+0x10` bits 0x80/0x800), is called from ~18 sites, all inside `process_tribe_cmd`'s
own range (so itself not the general selection path either). Of the 8 places in the whole exe that
set those two bits, one is inside the kind-33 preach handler (0x43C7A0) - but it sets the bit on the
PREACHER, not the target, while the preacher is in its approach/face substates - which contradicts the
observed rule rather than explaining it. None of the 8 sites fall inside any fight/pre-fight function
or inside state-5/state-23's own per-turn handlers either. Left open (kept to static analysis); a live
check (watch `+0x10`/`+0x7A` on both participants across a real sermon) would settle it quickly.

**All 31 kinds**, decoded this session from `order_execute`'s own dispatch table (0x432D18 - most entries are a
trampoline that calls out to a separate handler; "inline" means the trampoline IS the whole handler):

| Kind | Handler | What it does |
|---|---|---|
| 3 move | 0x4336C0 | arrival check against the group's own target (above) |
| 4 | 0x433800 | the training building's own "doing it" half of its own kind-3 zone-positioning order (0x405B80's note); not independently traced |
| 5 | inline | unconditionally "done" - a no-op placeholder |
| 6 "go near X" | 0x495520 | reusable approach utility, allocates its own fresh kind-3 order |
| 7 | 0x4340A0 | not traced |
| 8 enter building | 0x434610 `go_into_building` | |
| 9 | inline | arrival check against a SECOND position pair (+0x4F/+0x51, distinct from the group's own target), gated by a turn-parity bit; optionally calls 0x4D58C0 on arrival - "reached an alternate/work position", pairs with kind 10 as its "doing" half |
| 10 | 0x497A30 | not traced (paired with 9 above) |
| 11 "join/hold a melee ring" | 0x519F10 | traced this session - see "Melee ring order (kind 11)" below |
| 12 | none | jumps straight to the shared tail - a true no-op kind |
| 13 | 0x439A00 | not traced |
| 14 | inline | unconditionally "done" |
| 15 | 0x439D30 | door/throw point |
| 16 | inline | reads a byte from the group's own target, passes it to 0x4DE760; if the unit's own +0x9F resolves to another live unit, calls 0x4DE740 and writes its result into THAT unit's own tribe field (+0x2F) - a strong, not independently confirmed lead for Convert/Hypnotism's actual "switch tribe" application |
| 17 | 0x43A4D0 | cell-rounded approach, shared with 31/32 below; issuer traced a later session - a preacher-specific branch of `0x4CE2C0`, an AI-only per-tribe tasking sweep (`maybe_ai_func_1`), see "Preacher conversion"'s open questions |
| 18 | 0x433A10 | not traced |
| 19 attack (generic) | 0x51A2A0 | "Starting a melee" |
| 20 | inline | unconditionally "done" |
| 21 attack (auto-scan) | 0x51A2A0 | same handler as 19 |
| 22 attack "doing" half | 0x435160 | |
| 23 | inline-ish | unconditionally calls 0x435550(unit, group-target, extra=1) then always "done" - single-shot, fire-and-forget; address falls near the attack-order code |
| 24 | inline | unconditionally "done" |
| 25 | inline | re-runs kind 3's own arrival check first; on arrival, walks a linked chain of class/type 10/0x10 controllers clearing one's own +0x70 field on a match - "arrive, then detach from whatever controller (formation/vehicle/worship group) is tracking you" |
| 26 | inline | unconditionally "done" |
| 27 | 0x43BCC0 | not traced |
| 28 | 0x51FCE0 | see the table above (0x1C) |
| 29 | inline (0x432961) | **Fully traced a later session, chasing "guard between markers"**: target is a UNIT ID (not a position) - on first entry, computes a destination from that reference unit's CURRENT position ONCE (snapping to its building's door point if it's standing on one), then just waits; every other tick, checks arrival within 56 units and, on success, stops the unit and plays a per-type "arrived" pose (`0x4D4EE0`). Done once the REFERENCE unit's own `+0x7A` becomes nonzero (meaning not traced). Reads as "go to wherever unit X currently is, settle, until X signals something" - a general escort/regroup primitive, NOT a continuous follow (the destination is never re-read after the first tick) and not itself a multi-waypoint patrol. Its own issuer could not be found (checked all 38 literal `order_set` kinds, the multi-unit spell utility, and the one other dynamic-kind call site - none produce kind 29) - still open. See "Guard controller" below for why this doesn't settle guard/patrol either. |
| 30 "alarm" | 0x43DAA0 | given to a tribe's buildings by 0x443B40; completing one has a worship-adjacent side effect in `group_release` (above) |
| 31/32 disguise & building-visit | 0x43A4D0 | shared with 17 above |
| 33 preach | 0x43C7A0 | "Preacher conversion" |

A free slot's kind byte is 0 (kinds 0-2 never occur - below the dispatch table's range).

### Formations [exe]
A group move does **not** spread its units across different points: every unit in the same command group reads
the identical target out of the shared record (above). The only spacing mechanism is a **formation**, a class-10
controller (type 1/6, `process_formation_unit` 0x501000) that a unit joins or creates for itself
(`add_unit_to_formation`, 0x5018C0) whenever **both**: a global flag (0x895DA6 bit 0x4, read-only everywhere
found - not traced where it is set, plausibly a per-level/option byte) is **CLEAR** (corrects the previous
draft, which had this backwards - verified against the raw opcode this session: the code jumps PAST the join
call when the bit is tested nonzero, i.e. it reads as a "formations off" switch, normally clear), and the unit's
own `+0x76` bit 0x8 is set, which `order_execute`'s shared tail copies (0x432DF0) from bit 0x10 of byte+0xA of
the *current* order
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
This session fully re-traced the add/remove machinery and the kind-dispatch table - the picture below replaces
and corrects the previous draft in several places (noted inline).

**The queue model, consolidated across several sessions' findings (user-requested summary) [exe]**: each unit
holds exactly **2 kinds of slot**, not a single queue:
```text
PRIMARY   unit+0x9B, ONE slot. Reserved exclusively for self-issued "always-on" autonomous scanning (confirmed
            exhaustively: of 41 group_assign call sites exe-wide, only 5 ever pass slot=-1, and all 5 are
            per-type auto-attack/auto-scan functions - preacher_auto_scan and its 4 siblings). order_execute
            ALWAYS runs primary over secondary when both are populated - primary outranks, it does not erase
            whatever is parked in secondary.
SECONDARY unit+0x8B[0..7], EIGHT independent slots plus a rotating cursor unit+0xA6 (not a FIFO queue of
            sequential tasks from one source - several unrelated systems CAN each hold one standing order
            without clobbering each other). order_execute only ever runs the slot the cursor points at;
            order_advance (0x4366B0), once that slot's order finishes, scans forward from the cursor for the
            next populated, non-stale slot.
```
**Correction, a later session (user asked directly: is slot N always the same mechanism, or does each mechanism
pick its own slot?) - neither, mostly - but one confirmed exception exists.** Surveyed every writer found
exe-wide: ALL 8 numbered player/spell helpers, both idle-dispatch fallbacks (`0x43D0E0`/`0x43D2F0`), the AI
work-scan (`0x4D9B50`), and the per-tribe sweep (`0x41CC20`) hardcode **literal slot 0** (after resetting the
cursor to 0 first) - none of them search for a free slot, and none of them is "reserved" to a slot other than 0.
Non-zero literal slots used across SEVERAL DIFFERENT UNITS (`0x405585`-`0x4055A5`'s 0/1/2, the training building's
own `0x406310` loop) aren't one unit's own second task either - they're one shared group handed to several
different units, each at its own index.

**The one genuine exception, found a later session by chasing shortcuts.md's "Ctrl+Alt + left click (with
followers): patrol points, up to 8" row - `0x4BA9B0`, called from `process_tribe_cmd` (0x440857)**: a per-tribe
counter (`tribe+0xC5C`, capped at 8 - the literal "up to 8") tracks how many patrol points have been set so far.
On the FIRST point of a new sequence (counter == 0), every currently-selected unit (`+0x7A` bit 0x80) has its
ENTIRE order state wiped (`0x436CA0`, the same full wipe the 8 numbered helpers inline). Every call then allocates
ONE shared group (kind 6 "go near X", or kind 0xB/11 "melee ring" reused for a unit-targeted point, depending on
whether the click hit a unit or open ground) and assigns it into **secondary slot `tribe+0xC5C` specifically -
without wiping the other slots first**, for every selected unit; the counter increments afterward. So repeated
Ctrl+Alt+clicks genuinely populate DISTINCT secondary slots (0, 1, 2, …) on the same units, and `order_advance`'s
own already-documented round-robin scan (starting right after the cursor, wrapping at 8) is what then cycles the
unit through all of them in sequence, wrapping back to the start once it passes slot 7 - **this is the real
"guard/patrol between markers" implementation**, not `guard_control_units` (whose own class-10 dispatch turned out
to be formation-controller enemy-scanning, not movement - see the Guard controller note elsewhere in this
section). Not yet pinned down: the exact unit-vs-ground boolean semantics, what `tribe+0x931` (a precondition this
function also checks) gates, or when/how `tribe+0xC5C` resets for a later, unrelated direct order to the same
units. So: outside this one confirmed mechanism, essentially everything else funnels through secondary slot 0; the
other 7 slots are NOT generally unused headroom as an earlier draft of this note concluded - they are exactly how
patrol points work, just not used by anything else found so far.
**There is no "replace primary and clear secondary" helper** - no confirmed code path ever writes a fresh,
player-facing order into primary. Every "give this unit an order" path found anywhere in the exe - all 8 numbered
player/spell helpers (0x43B180-0x43BB54), the multi-unit spell-target utility, the per-tribe sweep, AND
`order_advance`'s own idle-dispatch fallbacks below - targets SECONDARY instead, via one of two patterns:
```text
"replace everything" (all 8 numbered helpers, the multi-unit spell utility): reset cursor to 0, release ALL 8
  secondary slots AND primary if set, THEN install the fresh group into secondary slot 0. From the player's
  perspective this looks exactly like "my new order replaces whatever I was doing" - it does, but structurally
  it's "wipe then occupy slot 0", not a write to a literal primary field. A primary-slot autonomous order, if one
  is active, survives this wipe untouched and keeps outranking the freshly-wiped secondary order (see "Preacher
  conversion" for the concrete case).
"insert without wiping" (order_advance's own idle-dispatch fallbacks, 0x43D0E0/0x43D2F0, and the AI-only work
  scan 0x4D9B50 via 0x43B3C0): reset cursor to 0, write slot 0 directly, no release pass over the other 7 slots
  first. In practice this only matters if some OTHER system already parked something in a secondary slot other
  than 0 - slot 0 itself still gets overwritten either way.
```
So: think of it as **one always-on "combat reflex" layer (primary) sitting on top of one "current task" slot
(secondary slot 0 - the only secondary slot any confirmed single-unit mechanism actually writes to) with 7 more
slots that exist as unused structural headroom**, not a primary/secondary pair where either side can "chain or
queue", and not 8 actively-scheduled roles either. Shift-clicking several destinations is not modelled by this
array at all - a direct order is always a full replacement, never an append (already confirmed, "What 'adding an
order' actually does" below).
```text
group record (0x938830 + id × 10):
  +0x0 kind       the order kind (3-33)
  +0x1 flags      bits OR'd in by whoever creates the order; 0x20 seen on self-issued auto-attack/scan orders
                    (0x51FF60 skips the auto-attack radius lookup above when it is set); 0x40 gates a kind-0x1C
                    attack-eligibility check (0x51C3C0/0x51FCE0) - traced this session to a dead end for
                    "Preacher conversion" (its only setter, 0x41CC20, is an unrelated per-tribe PERSON sweep -
                    corrected, separate session: the list it walks, tribe+0x881, is named `person_units` in the
                    Ghidra export's own `tribe_struct` type, not buildings; see functions.tsv's own 0x41CC20 row -
                    that never issues kind 33); bit 0x1 marks a slot stale/not a real pending order (consumed
                    by order_advance below, which garbage-collects it instead of running it); the rest not traced
  +0x2 ref_count  word, how many units currently point at this group (0 = free slot)
  +0x4 helper     word, NOT padding (corrects the previous draft, which didn't give it a role): a disposable
                    "helper unit" id, deleted (delete_unit, 0x4EF180) the moment ref_count reaches 0 - zeroed by
                    every "give an order" helper on allocation; which order kinds actually populate it is open
  +0x6 target     dword, a target unit id (word) or a packed cell position; meaning depends on kind

alloc    0x436C20(): scans forward from a rotating cursor (0x96AA78) for ref_count == 0, wrapping past the array
                    end back to id 1 (id 0 is skipped); returns the free id. The eight "give an order" helpers
                    (below) each inline this same scan themselves rather than calling it.
assign   0x436D00(unit, group_id, slot): ref_count += 1 (and a global active-group counter when it was 0);
                    slot < 0  → releases the unit's current primary group (0x4364D0(unit, -1)) and
                                 sets unit+0x9B = group_id (its one, exclusive primary group - always wins over
                                 any secondary slot)
                    slot >= 0 → sets unit+0x8B[slot] = group_id (one of 8 secondary slots, confirmed count this
                                 session - see below for what the slot array is actually for)
release  0x4364D0(unit, slot), traced this session (was a stub before): resolves the group the same way as
                    assign; if there is one, tells the UI about it when the unit is player-visible (0x4DA1D0) -
                    resolves the unit's own "target" word (+0x89) to a live unit and calls 0x51FF40 on it, a
                    completion reaction/sound/deselect, not traced further; two per-KIND side effects before the
                    generic cleanup - kind 7 sets the unit's own +0x77 bit 0x80 (meaning not traced), kind 30
                    ("alarm") decrements a per-tribe counter and, on it reaching 0, rerolls a building's own
                    +0x5F field (a worship-adjacent side effect of an alarm order completing, not traced
                    further); then always: ref_count -= 1, and if that reaches 0, frees the +0x4 helper unit (see
                    above) and decrements the global active-group counter (0x96AA7A); finally zeroes the unit's
                    own +0x9B or +0x8B[slot], clears +0xA7 (a cached copy of the kind just released, read by
                    order_advance below) and calls 0x501BE0 (plausibly "leave formation", not traced)
set      0x438730 `order_set`(group_id, kind, target_or_pos_ptr, extra_flags): no-op if the group already has
                    this exact kind and target; else writes kind/flags/target above, then snaps the target to a
                    cell or a building's door point (per-kind flags dword at 0x5A7DC0 + kind × 22 + 0xA, via
                    0x4044B0); one of those flags (bit 0x80000) can later overwrite the group's own kind to 0xA
                    once its target resolves to a unit with +0x9D bit 0x80 set (not traced further)
run      order_execute (0x432590, called from unit_processing_class_1_person only while a person is in state
                    0xA "carrying out an order"): resolves the person's active group (+0x9B if set, else
                    +0x8B[+0xA6]), reads its kind byte, and jumps a 31-entry table (0x432D18, kinds 3-33) to the
                    matching handler - full kind list in "Order kinds and player commands" below. CORRECTS the
                    previous draft's "two further redirects (group flag +0x1 bit 0x1, or the unit's own +0x13
                    bit 0x10) were seen but not traced": those addresses are not per-kind branches at all - every
                    one of the 31 handlers returns a "done" boolean that flows into this SAME shared tail, which:
                    (1) joins/creates a formation (`add_unit_to_formation`, below) when eligible; (2) if the
                    handler reported done, releases the group from whichever slot held it (0x4364D0, above)
                    unless the kind's own flags record says not to (bit 0x80 at 0x5A7DC0+kind×22+0xB - a few
                    self-managing kinds skip auto-release); (3) calls `order_advance` (below) to pick what runs
                    next; only if THAT finds nothing does the generic "settle and go idle" code run (snap
                    position/facing, maybe stop the current animation, then 0x4E32A0 picks default idle
                    behaviour)
advance  0x4366B0 `order_advance`, traced this session (not in the previous draft at all): called right after a
                    finished order's group is released, decides what the unit runs next. For an ordinary kind
                    (flags bit 0x80 clear): scans the 8 secondary slots starting right after the current cursor
                    (+0xA6), wrapping, for the first one holding a group whose own flags byte bit 0x1 is clear -
                    garbage-collecting (releasing) any stale/flagged one it passes along the way; if none found,
                    falls back to two built-in "is there autonomous work?" checks (0x43D2F0, then 0x43D0E0,
                    neither traced) which can point the cursor at slot 0 instead. For a self-managing kind (flags
                    bit 0x80 set): a different, narrower rule - look for ANOTHER secondary slot whose group kind
                    also has that bit set and matches the kind just finished, sweeping every other populated slot
                    (including the primary) clean along the way if nothing matches. Either way, a slot found is
                    written to +0xA6 and re-derives the formation-eligibility flag (0x432DF0) for the newly
                    active kind.
```
**What "adding an order" actually does** (traced this session from two of the eight "give an order" helpers,
0x43B180 and 0x43B2A0, byte-identical preambles): allocate/reuse a free group, write the order into it
(`order_set`), then **release every slot the unit currently holds - all 8 secondary slots AND the primary, if
set** - before installing the fresh group into secondary slot 0 (the cursor +0xA6 is reset to 0 first). A
directly-given order is not appended to a queue: it wipes everything the unit was doing and becomes the one and
only thing it's doing, confirmed identically on both helpers checked.

**Correction, a later session (re-read while chasing "Preacher conversion"'s kind-33 pre-emption question)**: "the
auto-attack scan does the same" above is wrong - re-read `preacher_auto_scan` (0x51E7B0) and its direct siblings
(the ordinary per-type auto-attack issuer inside 0x4F0460, the firewarrior ranged-auto-attack issuer near
0x51E9A0, two further "if genuinely idle, self-issue something" functions at 0x40BD20 and 0x520480) end to end:
none of them run the 8-iteration wipe loop at all. Each calls `order_set` then `group_assign(unit, group, -1)`
directly, with nothing in between - and `group_assign`'s own `slot<0` branch only ever releases whatever is
CURRENTLY in the PRIMARY slot (`0x4364D0(unit, -1)`), never touching the 8 secondary slots. **A full, exe-wide
survey of every `group_assign` (0x436D00) call site this session (41 total) found a clean, total split**: every
self-issued autonomous-behaviour scanner (the five functions just listed) passes `slot=-1` (primary) and ONLY
those do; every other order-issuing path found anywhere in the exe - all 8 numbered "give an order" helpers
(0x43B180-0x43BB54, re-verified for all 8 this session, not just the 2 originally spot-checked), the multi-unit
spell-target assigners, the per-tribe sweep (0x41CC20), and every other one-off issuer - targets a secondary slot,
overwhelmingly slot 0 after the wipe. So primary (`+0x9B`) is reserved exclusively for self-issued autonomous
behaviour, and it pre-empts by outranking, not by wiping: `order_execute`'s own resolve step (`0x4325A7`-`0x4325DF`,
re-verified this session) checks primary first and runs ONLY that group when it's nonzero, so an autonomous order
claiming primary leaves whatever is sitting in a secondary slot untouched but completely inert until primary frees
up again - see "Preacher conversion" for the concrete case (a self-issued kind-32 building-visit order silently
out-prioritizing an in-progress player-issued kind-33 preach order without destroying it).

So the 8-secondary-slot array plus its rotating cursor is not a sequential, player-authored task queue in the
way a "shift-click to chain three moves" UI might suggest - it is **several independent systems each able to
hold one standing order on the same unit at once without clobbering each other's slot** (a spell effect, a
building's own autonomous assignment such as 0x405B80's occupant positioning, and whatever else calls
`order_set`/`assign` directly rather than through one of the eight wipe-everything helpers - the auto-attack
scanner is NOT one of these: it always runs in primary, outside this round-robin entirely), with the unit
round-robining between whichever secondary slots are still populated only once its *currently active* one
finishes (`order_advance`), falling back to a couple of built-in autonomous-behaviour checks and finally true
idle only when every slot is empty. Open: exactly which non-player systems populate a secondary slot directly
(bypassing the wipe). The two "autonomous work" checks are now traced - see "Idle behaviour": 0x43D0E0 returns a
unit to a remembered cell's own occupant (enter a building in state 2, else walk over and stand near it); 0x43D2F0
returns it to a remembered class-4 (vehicle) reference instead.

### Starting a melee (the attack-order handler, 0x51A2A0)
```text
attacker within reach of the target (0x439850(person, 224)):
  target already in a fight (+0x9D is a 10/8)  → join it (below)
  target already in a pre-fight (10/9)         → attacker sub-state 8 (not traced further)
  else                                         → new pre-fight(attacker, target, mode)
mode 0: both in the open
mode 1: the attacker is in an enemy building and pulls one occupant out (0x407490)
mode 2: the target is at its building (target +0x72 is the building on its cell) - a slower lead-in to the
    SAME pull-out-and-throw mode 1 reaches directly, see "Pre-fight" below (which caller uses which mode: open)
```
Both people take state 29 (their previous state saved in `+0x7D`); the pre-fight keeps the attacker (`+0x6A`),
the target (`+0x6C`), their tribes (`+0x6E`, `+0x6F`) and, for modes 1-2, the building (`+0x68`). A pre-fight or
a fight involving the player's tribe sets 0x89D167 = 2 (probably the fight music or alert, unverified), and
0x431EA0 is called on every new controller (an alert to both tribes, confirmed this session, body not read).

**Creation, traced this session (`start_pre_fight`, 0x51E150, and `start_fight`, 0x51DE60):** both are class
10 (`alloc_unit`), tribe 255 (no owner) - pre-fight type 9, fight type 8 - and both are created, and every
precondition/position checked, off the **target**, never the attacker: `start_pre_fight` bails if the target
already has a controller (its own `+0x9D`) and spawns at the *target's* position; `start_fight` (called from
pre-fight's own sub-state 8, after pre_fight_turn clears both participants' `+0x9D` back to 0) does the same -
bails if the target still has one, spawns at the *target's* position, and the target becomes the fight's
initial **centre person** (`+0x6C`) and ring slot 0 (`+0x70`), with the attacker as slot 1 (`+0x72`) - not the
other way around, despite "attacker" being named first in both function signatures. Both participants get the
new controller's own id written into their `+0x9D` right away (the field every other check in this system reads
to mean "my current combat controller"). Fight additionally starts `+0x68` (its member count) at exactly 2,
rolls a random ring angle (0-359°) into `+0x26`, and immediately runs `0x519D10` once (the "move to suitable
ground" check, confirming it really does run "on creation" as the Fight section below already said) before the
alert call. A person can be checked/reused by neither of these two until it already has an opponent *resolved*
to nothing (`+0x9D == 0`) - there is no separate "is anyone else already about to start a pre-fight with this
target" lock beyond that field itself.

**Joining an existing fight** (`fight_pick_slot`/`fight_join`, 0x51DCC0/0x51DDC0, traced this session - reached
from 0x51A2A0 when a unit's own target already resolves to a type-8 fight): fails outright if the joiner's
tribe is neither of the fight's own two, or if a per-unit-type table (0x5A7090 bit 0x40) marks that type as
unable to melee at all. Otherwise prefers an **empty** ring slot; failing that (the joiner's own tribe already
holds all 3 of its allowed slots - **the ring is capped at 3 per side even though the array holds 6**), it can
**bump an existing same-tribe occupant** whose own unit type ranks lower on a per-type priority table (0x5A7077)
than the joiner's - the bumped unit is ejected (0x4A3920) rather than the count (`+0x68`) changing; the count
only grows on a genuine new addition. Either way the joiner takes state 25, its id goes into the chosen slot,
and the fight's own id goes into its `+0x9D`.

### Pre-fight (per turn 0x518630; person sub-state `+0x2D`, timer `+0x70`)
```text
ends (both freed, +0x9D = 0) when a person is gone, both are of one tribe, a person leaves state 29,
or the attacker's approach timer runs out
mode 0: attacker sub 5, target sub 4
4 target stands: stops, faces the attacker (a held look-at angle, `+0x57`, separate from the person's own
    movement facing `+0x26` - it keeps looking at the attacker even if something else turns its body), idle (6);
    while idle, 1 chance in 8 per turn of a taunt (48, 49 or 50 at random); waits as long as the pre-fight lasts
5 attacker runs: walks (anim 5, 9 carrying wood; walking speed + rand(speed / 4)), 40-turn timer (3.3 s)
    reached when |dx| < 315 and |dz| < 315 (0.6 cell) and the target is in sub 4 → attacker sub 6
6 attacker kicks: speed 0, anim 15 (shaman 57), faces the target; 6 turns (shaman 4)
    2 turns before the end (turn 4 of 6): target sub 7; at the end: sound 13, attacker sub 8
7 target kicked away, at once: velocity 100 + rand(100) units per turn at (heading to the attacker) + 120°..240°
    (+0x2AB + rand(0x2AA) of 2048: away from the kicker, ±60°); no damage; anim 16 (one tilted frame; a shaman
    stays idle, 53) for the first 2 turns of the flight; stopped (+0xE bit 0x8 clear) → sub 4
8 attacker runs again (as 5, a new 40-turn timer); reached and the target in sub 4 → the fight starts:
    a fight 10/8 at the target's place, both in state 25, placed by 0x519D10 (below)
global flag 0x89D17C bit 0x40: no kick, reaching the target starts the fight - traced this session to its
    source, a level/script option (set from a token read at 0x48E3D5: token 0x3FF sets it, 0x3FE clears it,
    nowhere else writes it), not a debug/cheat switch; the same flag also suppresses the ordinary knockback
    variant of Melee's own Reeling (see "Fight" below), so it reads as a single "no pushing in combat" option
mode 1 (pulled out of a building): both sub 3; the target is thrown out of the door (0x4044B0, 0x404420):
    speed 100, ×12/16 per turn while ≥ 40 (floor 30), from the door outwards; after 6 turns or on landing
    → target sub 4, attacker sub 5 (then as mode 0)
mode 2: both sub 1: wait up to 100 turns (8.3 s) for 0x439030, then sub 2: walk to the building door, up to
    100 turns; both in sub 2 → sub 3 as mode 1                                   (meaning unverified)
```
So the opening kick exists: a knockback without damage, once per pair, before the first exchange. Nobody is
gathered: a pre-fight is always one attacker and one target.

### Fight (per turn 0x518FB0)
Controller fields: count `+0x68` (≤ 6, exactly 2 at creation), the two tribes `+0x69` (the original target's)
/ `+0x6A` (the original attacker's), winner tribe `+0x6B`, the centre person `+0x6C` (the target at creation,
traced this session - see "Starting a melee" above), the 6 people `+0x70` (u16 ids, slot 0 = the target, slot 1
= the attacker at creation), ring angle `+0x26`. A person's opponent is in its `+0x72`.
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
- **Melee ring order (kind 11, 0x519F10)**: a per-fighter order kind, traced this session, that is a close
  numeric match to the per-person state-25 machine just above - same shape (walk to a slot, then hold it, then
  a scripted move), and the EXACT SAME 12-unit arrival threshold for "reached my slot" - but tracked through a
  different pair of fields (the order's own sub-state byte +0xA9, not the person's state/sub-state +0x2C/+0x2D)
  and a different companion object: sub-state 0 (fresh) calls the SAME attack-order handler (0x51A2A0) kinds
  19/21 use, then (success) moves to 1; sub-state 1 walks to the unit's own +0x4F/+0x51 position (within 12
  units -> 2); sub-state 2 holds that position, facing around a shared companion object (allocated once per
  order group, class 10 **type 0xA** - not the documented pre-fight (10/9) or fight (10/8), a third,
  not-yet-cross-referenced controller type) and drifting back to sub-state 1 if it strays too far from a
  radius stored on that companion object (+0x72); sub-state 3 plays a type-specific move animation for an
  RNG-rolled duration (from the same per-type odds table, 0x59DF44, the disguise/building-visit order 0x43A4D0
  also rolls against) then reverts to 2. **Resolved this session**: pre-fight (10/9) and fight (10/8) are
  created by two dedicated functions, `start_pre_fight` (0x51E150) and `start_fight` (0x51DE60) - neither lives
  inside 0x51A2A0 itself, which is why the earlier search of that function's own body came up empty (see
  "Starting a melee" above for both). Kind 11's type-0xA companion is confirmed a genuinely separate, third
  object, not a mis-identification of either - what it's actually for (a melee ring recruiter staged before the
  real fight/pre-fight, something else entirely, e.g. a Guard Post sentry or CONTEXT.md's "Challenge" stage
  under a different internal name) is still open.
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

### Resuming a fight cut short by a push [exe]
TODO.md asked whether an attacker whose target is blasted out of reach chases it via a chained/queued order, a
dedicated pursuit state, or stored "last seen" data. Traced this session: **none of those** - it's simpler, and
falls out of two things already covered above working together.

1. **Nothing about starting or joining a fight ever releases a participant's own command group.** `start_fight`
   (0x51DE60), `start_pre_fight` (0x51E150) and `fight_join` (0x51DDC0) only ever change the person's *state*
   (to 25 or 29) and write the controller's id into `+0x9D` - none of them call `group_release` (0x4364D0). An
   attacker's original attack order (kind 19/21, "Order kinds and player commands") is still sitting in whichever
   slot it was given. It simply goes dormant: `order_execute` only runs for a person in state 0xA, and fighting
   means state 25/29 instead, so the order is paused, not cancelled, for as long as the fight lasts.
2. **Leaving the fight falls back to the person's own per-type default state - which, for an ordinary fighter,
   is state 0xA.** Every turn, state 25's and state 29's entire per-turn body (both reached through the generic
   per-state dispatch table, 0x4D3D18) is one call to 0x518560: if the person's own `+0x9D` still resolves to a
   live controller, nothing happens (stay). If not - dropped by the fight's own roster check (0x519A70, below)
   or by pre-fight ending - it returns the person's per-type fallback from the same table (0x5A7064) state 1's
   handler already uses for "what do I do with nothing assigned": types 2-7 (brave..shaman) fall back to
   **state 0xA**, i.e. straight back into `order_execute`.

Put together: a blast knocks the target out of the ring; next turn, the fight's own roster check (0x519A70,
called first thing by `0x518FB0`) notices it can no longer path back to the ring's own position (`0x450450`,
the same reachability helper formations use to drop a strayed member - this, not a raw distance constant read
anywhere in the function, is the traced identity of the "a fighter farther than some leash leaves" rule) and
drops it: its slot and its own `+0x9D` are both zeroed right there. On that unit's own next turn, `0x518560`
notices `+0x9D` is empty and returns it to state 0xA. `order_execute` resumes - with the SAME, never-touched
attack order, same target id - and the attacker just walks back towards wherever that target actually is now,
because that is exactly what an ordinary, uninterrupted attack order does. **It is the original order quietly
surviving the whole fight, not a new one.** A target with no order of its own (the common case) just goes idle
instead, since it had nothing queued to resume.

Not traced this session (ordinary attack-order behaviour, not fight-specific): how long/far the resumed order
keeps chasing before giving up on its own, if at all - that is whatever 0x51A2A0's own ordinary termination
rules already do for any attack order that can't reach its target, not a special case added for this.

### A blasted person fighting back [exe, measured]
TODO.md asked whether getting hit by an offensive spell (Blast specifically) makes an otherwise-idle person fight
back against the caster, the way a struck firewarrior does, or via some other "remember who hit me" mechanism.
Watched live this session with `pyramid_fight_watch.py` (follows a person's state/order fields for turns after any
life-loss or newly-set BLOWN flag): **there is no such mechanism, and the question's own guess was right** - it is
nothing but the already-documented generic Auto-attack scan (`0x4D4690`, above) running into a person who happens
to have landed near an enemy.

Trial: a tribe-1 brave (person state 1, flags `+0xC` bit 0x80000 = BLOWN) was hit by two separate Blast casts.
- First cast: 2 damage ticks (-50 each), tumbling (person **state 19**, substate cycling through its flung/landing
  sub-poses - not previously catalogued, see `person_states.tsv`), lands with no enemy nearby. It then sits idle
  for ~90 turns (7.5 s) doing nothing - no order, no target, exactly "a target with no order of its own just goes
  idle instead" from "Resuming a fight cut short by a push" above.
- Second cast on the same brave, ~90 turns later: 3 more damage ticks, tumbles again, and this time lands a few
  cells from an enemy shaman. Four turns after landing, the brave's **primary** order slot (`unit+0x9B`) gets
  populated with a fresh group, **kind 21** (`0x15`, "attack around that cell" - confirmed self-issued: the group's
  `flags` word is `0x30`, matching the doc's own "0x20 seen on self-issued auto-attack/scan orders"), its `target`
  field a packed **cell position** (`0x404f8fe`), not a unit id - matches "Auto-attack"'s own wording exactly: the
  scan targets a cell, not a remembered attacker. One turn later the brave's resolved target (`+0x89`) becomes the
  shaman's own id, state goes to 10 (`order_execute`), and it walks straight to her; by turn 20 it is in a fight
  (state 25) and dies in it ~170 turns later (state 3, "death"), having done comparatively little damage back
  (the shaman's `LIFE_SHAMAN` dwarfs a brave's, confirming "Shaman has by far the highest HP" live).

So: **a blasted person's only path back into a fight is the same passive, rhythm-gated scan everyone always runs**
(the first cast's landing spot simply had nobody in range, the second one did) - not a spell-specific reaction, not
a stored "last attacker", and not targeted at the caster in particular: whichever enemy the scan's square happens
to contain first. This resolves TODO.md's open question for Blast; the same should hold for any spell that moves
or stuns a person without giving it an order (Lightning, Swarm, Whirlwind), since the mechanism is generic to being
idle-with-no-order, not Blast-specific.

### Tumbling and ground recovery (person state 19) [exe, measured]
Traced this session, following up on "A blasted person fighting back" above: state 19's dispatch-table entry
(`0x4D36F1`, trampoline) leads to its real body, **`0x4D73E0`** (not previously catalogued; Ghidra's own function
range for this address, ending at `0x4D80B9`, is wrong - a different, unrelated function, the death/spirit-rise
record push also used by state 3, actually starts at `0x4D7FD0`, well inside that claimed range).

- **No velocity, height or gravity field (`+0x49`/`+0x4B`/`+0x4D`/`+0x41`) is read anywhere in this function.**
  Confirms the actual ballistic flight (gravity, drag, friction, velocity caps - "Person physics", "Blast push and
  flight") runs entirely in the separate, universal per-unit physics step (`0x4E6D1D`/`0x4E6D6D`, already cited by
  `measure_blast.py`), independent of the per-state dispatch. **State 19 itself is the GROUND recovery choreography
  that runs once a blown/tumbling person has landed and physics has gone quiet - not the fall itself.**
- Its own sub-state (`+0x2D`) is a two-layer machine: an initial 0/1/2 arrival phase (checks the person's position
  against a reference point - probably where it's meant to end up standing - within a 48-unit, then a 216-unit,
  threshold on each axis) feeds into a second 8-way jump table for sub-states 3-10, matching exactly the substate
  values seen live in "A blasted person fighting back" (4, 5, 6, 7, 10 observed cycling for ~90 turns while the
  unit sat still).
- That 8-way machine picks a clear nearby resting cell (an 8-direction offset table, `0x895ED9`, indexed by a
  facing nibble, with a default at `0x895EF1`, added to a reference position and moved to via `0x4E9DD0` -
  the same low-level "place a unit" primitive used elsewhere) and waits short randomized timers (`+0x70`, 4-31
  turns, rolled off the shared LCG at `0x89D178` - the same generator used for ambient idle rolls elsewhere).
- **Correction (the live trial did not actually exercise this - caught after presenting it as settled): `+0x78` is
  NOT a trailing-effect counter - the Reference table above already documents it as a person's plain **carried
  wood** amount ("Wood fields"), an earlier draft of this note wrongly equated it with state 26's own counter
  (that one is `+0xA4`, a different field entirely). Read correctly, this handler's two checks on `+0x78`
  (threshold `< 1`) are simply "is this person currently carrying wood" - picking the matching Idle/Walk vs.
  holding-wood/Walk-with-wood pose pair (see the animation note below) so a wood-carrying brave still looks like
  it's carrying wood while it relocates. Separately, in the branch reached once the first wait timer expires with
  wood still carried (same `+0x78`), it spawns a small class-5 type-0xB debris effect once per **exactly 100**
  wood decremented from `+0x78` (the standard one-load unit, "Wood rules") until it reaches 0, playing a sound each
  time - read as **the person spilling/dropping its carried wood in a small debris puff per load** as part of
  settling, not a generic "effect burst" (the same `+0x78`-countdown-by-100-while-spawning-type-0xB-effects pattern
  also appears, unexplained at the time, in "Preacher conversion"'s sub-state 4 - almost certainly the same
  wood-drop mechanic there too, now that `+0x78`'s real meaning is pinned down). **Not actually observed live**:
  the one live trial (the brave above) never logged `+0x78`/wood, so whether this specific brave was carrying wood
  - and so whether the holding-wood pose or the wood-drop puffs ever fired for it - is unknown; a carrying brave
  specifically would be needed to confirm this branch live.
- **Animations, resolved this session from the exe's own static tables (not from the live trial - see correction
  above)**: this phase has no dedicated "lying down" sprite of its own. It calls the same generic per-person
  animation picker everything else uses (`0x4D3FF0(person, action)`, `[action × 9 + type]` at `0x5A6D50`, "Fight"
  above) with actions **1/5** (the already-documented Walk / Walk-with-wood pair, picked by the `+0x78` wood check)
  while moving to the picked spot, and **0/4** while settling - resolved directly from `0x5A6D50`/`0x5A6858`
  (cross-checked against the 7 already-confirmed action values in "Fight"'s own table, all matching exactly):
  action 0 is plain **Idle** (VSTART 6/53, already named), action 4 is **VSTART 10**, already named too ("standing
  still, holding wood", brave-specific) - both picked precisely when the person is carrying wood, not arbitrarily.
  So the visible read during this whole recovery window is most likely an ordinary standing/idle (or, if carrying
  wood, holding-wood) person that simply isn't taking orders yet, with a few dropped-wood debris puffs along the
  way if it had any - not a visibly prolonged "struggling on the ground" animation, correcting this write-up's own
  first-pass framing below and in units.md/TODO.md.
- **While grounded, every 32 turns it rerolls the shared LCG twice more** and, with roughly 13/32 odds, kicks back
  into sub-state 6, or with roughly 3/64 odds into sub-state 10 (both also setting flags `+0xC` bit `0x40000000`,
  the same bit cheat.md's live drowning trace already found set on entering the water-sink sequence - this looks
  like a general "lying/struggling" pose-variant bit reused by this handler, not a water-exclusive signal, but that
  is not confirmed independently here) - **this is why the live log showed substates wandering non-monotonically
  (4→5→10→5→6→7…) rather than counting through a fixed sequence**: it is a randomized "still down, shifting around"
  idle loop, not a linear recovery animation.
- A later sub-state (7, "getting up") rerolls a new facing and a fresh wait timer before looping back into the
  resting sub-states again, so a person can cycle through several more rounds of this before actually standing.
- **Exit**: two return epilogues at the function's end return a next-state value exactly like state 1's own
  per-type-default lookup (`table 0x5A7064`, `type*50 + offset 0`, same formula/table already documented for
  `0x4D3386`) - confirming state 19 eventually hands off to the person's ordinary per-type state (10 for
  brave..shaman) once recovery finishes, matching units.md's "back to Idle once on the ground and still". A third
  epilogue returns a fixed state `0x27` (39) for the shaman's own global debug-mode override, the same pattern
  already documented for `0x518560` and others.
- **Gap against our implementation** (units.md "Tumbling"): ours goes straight from flung/off-the-ground back to
  Idle the instant it lands and stops moving. The original instead stays locked (no orders) for a visible stretch
  of time (tens of turns, longer with bad luck on the 1-in-several rerolls) while it picks a clear spot and settles
  - visually an ordinary Idle/Walk/holding-wood person, per the animation note above, just unresponsive - before
  resuming its ordinary per-type behaviour. Worth adding as a short, randomized "locked, settling" grace period
  rather than an instant recovery; no new animation asset is needed for it. See TODO.md.
- **Traced this session, following a question about whether this is specifically about avoiding landing on a
  building**: `0x4D5420` (the "is this candidate spot OK" check, called from the sub-state-3 case above) does
  **not** check for buildings at all - it walks the candidate cell's own per-cell linked list (`0x8A03E4`) filtering
  strictly for CLASS-1 (person) entries, checking only (a) the per-type terrain-climbability limit
  (`WALK_ALT_DIFF2`, movement-profile table `0x5A7B90+0x10`, the same field "blocked (stuck on terrain)"/state 33
  already uses) against the candidate cell's height, and (b) whether another living person is already claiming the
  exact same resting spot (same stored target word, same facing group, not itself). A building (class 2) sitting
  on that cell is invisible to this particular check - it only ever matches/skips over class-1 entries.
- The OTHER untraced function from the initial arrival dispatch, `0x4D5120`, is more involved and does surface one
  new, unconfirmed lead worth flagging: it reads a per-CELL linked-list HEADER flag (`[cell_list_head+0x1]` bit
  `0x2`, not a per-object field) and, when set, re-places something via `0x4044B0` (the same cell-linked-list
  placement primitive "Preacher conversion"'s success path uses) before continuing - plausibly "this cell is
  occupied by something solid" (a building footprint is the obvious candidate, but scenery or a different unit
  class would fit equally well), not confirmed either way this session.
- **Resolved, same session, by tracing the physics/landing step itself ("Person physics" below): landing on a
  building gets no special handling at all.** `ground(pos)` (`0x44E940`) is a pure terrain-mesh height
  interpolation with no building/class check anywhere in it - a cliff is handled correctly as steep terrain (same
  function), but a building sitting on the cell is simply invisible to it. So a thrown person lands at raw ground
  height there regardless, same as open ground; this state's own spot-picking logic (confirmed above, person-only)
  was never going to be the place building-awareness lived, and now neither is the landing step - there just isn't
  any. Not
  traced further this session: the exact semantics of the two closeness thresholds (48/216 units) and what the
  "reference point" they check against actually is (a pre-fall position? a nearby clear cell picked at the moment
  of landing? not confirmed), the precise facing/sprite calls (`0x4EA460`), the new cell-header flag `0x2` lead
  above, and whether `0x895ED9`'s table is shared with any other "pick a free adjacent cell" code elsewhere in the
  exe.

### Preacher conversion ("soul convert") [exe, measured]
**Correction this session, replacing the previous model below almost entirely**: the earlier draft assumed a
struck preachee passed through person state 3 ("struck") before reaching state 23 (the conversion roll), and that
state 24 was conversion's own "building occupant" variant. Both are now disproven by direct, independent reads:
state 3 is `0x4D5CF0`, which is reached only from the universal life-step (`0x4D43A0`, life ≤ 0) and is ordinary
**death** (the spirit-rise controller every death gets, not preach-specific - see "Combat"'s own 10/0xC row, which
needs the same correction); state 24 is `0x4D8E40`, confirmed this session (exhaustive search for every literal
"set state to 24" in the exe) to be **Whirlwind's own lift state and nothing else** ("Whirlwind mechanics"). State
23 is entered directly, with no state-3 step in between, by a dedicated scan function - traced this session.
```text
preacher_strike_scan (0x43ABF0), fn(striker, radius_byte, out_angle_ptr) -> bool found, called from
  order_building_approach (0x43A4D0) on an RNG-roll success (same substate-4 roll "Auto-attack" already documents,
  odds table 0x59DF44 by type) - and order_building_approach is NOT preacher-specific: it is the shared handler
  for order kinds 17, 31 AND 32 alike, with no attacker-type branch anywhere in it (re-verified this session)
  kind 32 "preacher building-visit": issued only by preacher_auto_scan (0x51E7B0), which DOES check
    `self.type == 4` explicitly before considering it - this one IS preacher-exclusive
  kind 31 "disguise": issued by 0x4D6F90, gated by a per-type table (0x5A7091 bit 0x4) read directly from the exe
    this session - set for brave/warrior/spy, CLEAR for preacher/firewarrior/shaman/angel/wildman. **[game, user-
    confirmed]: disguise is a spy-only command (select the spy, right-click a tribe's HUD icon; a short
    vulnerable transition, then the unit counts as that tribe - no auto-attack on it - until another spy spots it
    or the player explicitly orders an attack on it, which un-disguises it); it never strikes or converts anyone.**
    `order_building_approach`'s substates 1-4 (walk to a spot, wait, RNG roll) read as that vulnerable transition,
    and the roll success setting a "disguise" flag (`+0xB2` bit 0) plus an ~840-turn/70 s duration matches "takes a
    short time" and the disguised period itself. The trailing call to preacher_strike_scan on that same success is
    real at the code level but is a NO-OP in the overwhelming common case (its own target table, below, only ever
    matches an enemy preacher or firewarrior standing right there) - incidental plumbing reuse, not a second
    gameplay effect of disguising. The exact trigger for the PLAYER'S disguise command specifically (as opposed to
    0x4D6F90's own per-type gate, which may also be reached by a different, non-player trigger for brave/warrior)
    was not pinned down this session - a related function, 0x4DEFF0, reaches the same kinds 17/31 but gated to
    `self.type == 4` (preacher), so it is a different thing, not the disguise command either.
  kind 17: issued by 0x43B790, itself called from an untraced per-tribe/per-candidate dispatcher (0x4CE2C0), AND
    (as kind 17's own default, see 0x4DEFF0) by a preacher-gated function reached from `init_unit_class_1`
    (0x4D2740, a per-new-state hook, dispatch key not fully traced) - neither issuer's own trigger condition was
    pinned down this session
  → so, at the code level, a disguising brave/warrior/spy (kind 31) and a visiting preacher (kind 32) reach the
    SAME strike scan - but see the disguise note just above for why this doesn't mean disguise strikes anyone in
    practice
  scans an expanding square of cells around the striker (same per-cell "other object" list "Terrain land
    colour"'s shadow code also walks) for a class-1 (person) candidate passing every one of:
    a per-type table (0x5A7090 byte+0, stride 50/type, bit 0x20) - read directly from the exe this session:
      **set ONLY for type 4 (preacher) and type 6 (firewarrior)**, clear for brave/warrior/spy/shaman/angel/
      wildman - see the open question below, this contradicts observed play
    not the striker's own tribe; a per-state table (0x5A6F79, stride 5/state, bit 0x20); a tribe-relations
      bitmask (table 0x9608B6, by striker tribe then candidate tribe) unless either tribe is 0xFF or they match;
      candidate +0x16 bit 0x8 clear; candidate +0x9F == 0 (not already claimed by another striker); a mutual
      shield check (0x4DE7B0); candidate +0xE bit 0x10 clear (not already being removed)
  first candidate to pass: **struck immediately** - set candidate state to 23 (0x4D83B0), seed candidate +0x89
    with the striker's own id (the "who's affecting me" reference read back below); if the candidate's own tribe
    is 0 (the human player) also raises a camera-alert marker (global flag 0x89DB05 bit 0x80000000, position word
    0x98CECC); notifies both tribes (0x431EA0, mode 2 - mode 1 is a fight starting, "Combat")
  no candidate struck, kind 32 specifically, and the wait timer (+0x70) exceeds 32 turns: calls 0x43AEC0 (a
    sibling scan) which instead REVERTS any OTHER nearby person already in state 23 back to its default state -
    a stuck-roll cleanup, plausibly for when the original striker died or walked off mid-roll

state 23 (0x4D83B0), the candidate while a roll is in progress; `+0x89` the striker, `+0x70` the timer:
  abort (idle) if the striker is gone, or not carrying out an order (state != 0xA/10) and not in state 0x21/33
    (**correction, separate session, now fully confirmed against live play**: this is NOT "itself preaching" as
    previously guessed - it is a unit whose walk got physically BLOCKED by terrain its own type can't climb
    (user-observed: a unit visibly halts arms-up on a slope, then starts losing life after a while - matches
    exactly). The real trigger: the generic movement-step helper (0x4EA920, used for ordinary walking everywhere,
    not preach-specific) calls a blocked-step check (0x4EA970, almost certainly the per-type `WALK_ALT_DIFF2`
    terrain-height-difference test) and sets a flag (`unit+0x13` bit 0x10) when a step can't go through;
    `order_execute`'s own shared tail reads that exact flag and transitions the unit straight into state 33 instead
    of continuing its order. State 33's own per-turn body then runs a tribe-scaled grace timer before periodic life
    loss kicks in (56 every 8 ticks), while trying up to 8 waypoints to find a way out - see person_states.tsv's own
    state-33 row for the full trace. The precondition most likely just means "carrying out an order (10), including
    this transient blocked-by-terrain state (33) it can momentarily fall into mid-approach" - the striker doesn't
    need to be doing anything preach-specific to pass this check, just not idle or busy elsewhere); **then
    independently requires the striker's own active
    command-group kind to be
    0x11/0x1F/0x20 (17/31/32)** - confirmed this session to be the exact same three kinds preacher_strike_scan's
    own caller handles, with no person-type check anywhere in this function either. **Re-read in full this
    session** (chasing `0x43AEC0`, see below): the striker's "active command-group kind" is resolved as the
    primary group reference at striker `+0x9B` if nonzero, else a secondary-group array at striker `+0x8B`
    indexed by a byte at striker `+0xA6` (the same `+0x8B` slot the command-group assign helper, `0x436D00`,
    documents as its own "primary or `+0x8B` slot" choice). Also requires striker `+0x5F == 0` - this is not an
    unidentified field, it is the striker's own WALK SPEED (already in "Unit record": "walk speed `+0x5F`") -
    i.e. **the striker must be standing completely still** for a roll to even be attempted, matching the
    in-game picture of a preacher stopping in place to preach rather than converting on the move.
  every turn: timer -= 1
  timer < 0, OR the candidate's own tribe has the "no breeding" flag (tribe +0x941 bit 6: set for, at least,
    wildmen) which forces a roll every turn regardless of the timer:
    rearm the timer: PREACHEE_CONV_FREQ (exe 500 turns), halved to 250 unless global flag byte 0x89C668 bit 2 is
      set (meaning/scope of that byte not traced: many unrelated bits of it are read all over the exe), jittered
      by the pop3 PRNG by roughly ± (freq / 16) turns
    roll: success if rng() % PREACHEE_CONV_CHANCE (exe 5) == 1, or flag 0x89C668 bit 2 is set, or the candidate's
      tribe has the bit-6 "no breeding" flag (both make every roll succeed)
    roll failed → rearm the timer again at the full PREACHEE_CONV_FREQ (not halved this time), and continue running
      the sermon's own "being preached to" animation sequence below (0x4D880A) - **this never escalates to combat**,
      see "The sermon's candidate sub-state (0-6)" just below
    roll succeeded:
      the striker's tribe is at its population cap (tribe +0x89DAE5 array >= 200, "Village growth") and the
        candidate's tribe does not have the bit-6 flag → nothing happens this turn, try again later
      else → queue a trailing visual (particle list 0x892443, `[0x892443]` += 0x14 per entry) at the candidate's
        position; spawn a person of the candidate's own type in the striker's tribe, at the candidate's position,
        with a random short-range offset and a movement-profile-scaled random speed; copy the striker's facing
        bits; play a sound (0x48A050(striker, 5, 0)); spawn a class 7 type 0x3A ("soul") effect at the striker's
        position and link it to the new person (`+0x72` = the new person's id, presumably so the effect's own
        per-turn routine flies the sprite from the striker to the new body - not traced)
      destroy the candidate (0x4D4B50) either way once a roll has succeeded this check
```
#### The sermon's candidate sub-state (0-6) [exe]
Every tick that does not end this check in a successful roll (whether the timer hasn't expired yet this tick, or
it has and the roll itself failed), `0x4D83B0` falls through to `0x4D880A`, which re-arms the timer (above) then
runs a SEPARATE 0-6 sub-state byte on the candidate (own field, independent of person state 23 itself) that
choreographs the visible "being preached to" sequence - confirmed this session to never include any order-set,
attack call or fight-related state write anywhere in it:
```text
0: walk to a spot near the striker (direction from the wrapped striker-candidate delta; on failure, falls back to
   the candidate's own per-type default animation and stays at 0) → 1
1: rhythm gate, runs only on even ticks (`+0x2E` bit 0, the ordinary `class_counter` field, NOT a disturbance
   flag - see the correction below); on a tick it runs: wait for arrival within 112 units on both axes, then
   snap into position facing the striker and spawn a placement visual, or reset to 0 to retry the approach calc
2: compute and lock a facing angle toward the striker (`+0x57`); hold still → 3 (same tick)
3: rhythm gate, runs only once every 16 ticks (`+0x2E & 0xF == 0`), AND a 50% RNG coin flip within that tick;
   on pass, start a per-type "listening" animation and arm a countdown → 4
4: wait out the countdown; once it expires, start a second per-type animation and spawn a looping burst of
   small visual-effect objects at the candidate's own position (one every few ticks, via the SAME alloc_unit
   the conversion success path itself uses) → 5
5: steady "just keep listening" loop (literally the function's own do-nothing tail) - unblocked only by the
   independent roll above eventually succeeding (conversion), or state 23 being left some other way
6: rhythm gate, runs only on even ticks (same `+0x2E` bit 0 as sub-state 1); once arrival/settle conditions are
   met, finishes cell-placement bookkeeping and reverts the candidate to its per-type DEFAULT behaviour state
   (table 0x5A7064, the same per-type fallback table every other "done" state in this codebase uses) - this is
   also exactly what happens at every other exit from `0x4D83B0` (striker gone, busy fighting, or no longer
   running an order kind 17/31/32 approach): a plain revert to ordinary behaviour, never a forced attack order
```
**Correction, confirmed live this session (`preachwatch5.py`, `DIAG8.TXT`, ~760 turns of logged activity, 70+
sermons watched end to end)**: candidate `+0x2E` bits 0-3, which an earlier read of this session called an
"interrupted" flag that aborts the sub-state machine for a tick when set, is NOT a disturbance flag at all - it
is the ordinary per-unit `class_counter` field already documented elsewhere in this doc ("Unit record", `+0x2E`,
"+1 per turn for every unit whose state is not 0"). Live proof: watching it on every tracked candidate, every
"cleared" reading is exactly 0 and every "set" reading is exactly 1, the gap from clear to the next set is
ALWAYS exactly 1 turn, and the gap between consecutive sets is 16 turns in 309 of 330 measured cases (the rest
are small multiples of 16, from gaps while a candidate wasn't being tracked) - textbook evidence of a counter
that increments by 1 every turn and wraps mod 16, not a flag that toggles in response to an event. So
`test [candidate+0x2E],0x1`/`0xF` in sub-states 1/3/6 are plain **rhythm gates** (run this step only every 2, or
every 16, ticks), the exact same `class_counter & mask == 0` idiom "Auto-attack" and others already document -
there is no disturbance-detection mechanism inside this sub-state machine at all.

**Settles a long-open question: a failed conversion roll never falls back to melee combat against the striker**
[game, the user's own account re-tested against this trace: "if the preaching doesn't pass it's then close range"
is NOT what the code does]. Nowhere in this sub-state machine, nor in any of `0x4D83B0`'s other exits, is there a
call into the order system or a write of a combat-related state; every exit is either a no-op/retry, a further
choreography step, or a revert to the candidate's own ordinary default behaviour. This matches two independently
traced facts elsewhere in this doc: `0x51D0B0` (firewarrior/angel ranged scan) and `0x4DB980` (Angel of Death's
hunting AI) both explicitly SKIP any candidate currently in state 23 from their own targeting - state 23 reads as
structurally excluded from THEIR combat consideration, not a gateway into it.

**However, live data shows state 23 is NOT a blanket combat immunity**: of 71 sermons that ended during the
`DIAG8.TXT` capture, 8 ended in a direct transition to person **state 29 (pre-fight)** and 1 to **state 3
(death)** - the candidate was pulled straight out of state 23 into ordinary melee or killed outright, with no
state-10 "default" step in between. The remaining 46 reverted to the ordinary default (state 10) and 8 vanished
entirely (conversion success, see "Numbers confirmed live" below). **This resolves the open question below about
`0x51C4C0`** (the generic attacker-eligibility path used by anyone except a preacher or firewarrior): unlike
`0x51D0B0`, it does NOT exclude state-23 candidates - an ordinary brave/warrior/spy/shaman attacker can engage a
mid-sermon candidate directly, dropping it out of state 23 into pre-fight. This is very likely the real mechanism
behind the user's original account ("if the preaching doesn't pass it's then close range combat"): not the
*sermon's own failed roll* escalating to melee (confirmed false above), but a **third party attacking the
candidate while it's mid-sermon**, unrelated to whether any conversion roll on it is currently succeeding or
failing.

**Numbers confirmed live**: all 8 conversions (candidate vanished, not reverted) happened at exactly `timer == -1`
(i.e. the very tick the resist timer crosses below 0 and a roll is attempted), independent of the candidate's own
sub-state - 7 of 8 from the steady sub-state 5 ("just keep listening") and 1 from sub-state 3 (a candidate stuck
retrying its listening-animation roll for ~100 turns) - confirming the success roll is entirely independent of
how far the 0-6 choreography has progressed, exactly as the static read above describes.

**`0x43AEC0` re-read in full this session, confirms the original description, does NOT just reset sub-state**:
it scans an expanding square of cells around the striker (same `0x8A03E4+0x6` cell-list convention as elsewhere)
for a DIFFERENT class-1 person whose own tribe differs from the striker's, type is not 4/7 (preacher/shaman), and
whose state is exactly 23 - i.e. exactly "some other stuck conversion-roll candidate nearby", matching the
existing description. On a match (after a guard call to `0x4E9B40`, not traced, and an "not already being
removed" `+0xE` bit-0x10 check): saves the candidate's current state into `+0x7D` (the SAME field "a fight/
pre-fight controller saves the state you had on entering" documents elsewhere, reused here too), then runs the
exact generic `set_state`-style sequence (`0x4ED6F0` remove-from-list, write the per-type default from table
0x5A7064 into `+0x2C`, `0x4ED640` re-add-to-list) - this is a full eviction from person state 23 to the
candidate's own ordinary default behaviour, byte for byte the same pattern `0x401B10` uses elsewhere. It never
touches `+0x2D` (the sub-state byte) directly at all.

This means the live-observed "sub-state resets back to 0 while still counted as state 23" (27 cases, mostly
several candidates under the same still-live striker) are NOT `0x43AEC0` resetting a sub-state in place - they
are most likely a full evict-by-`0x43AEC0`-then-immediately-re-struck-by-the-same-preacher's-own-scan cycle that
completes within a single turn: `preachwatch5.py` only samples once per turn boundary, so a candidate leaving
state 23 and re-entering it (sub-state 0) within that same turn is indistinguishable in the log from a plain
sub-state reset. This fits the data better than a hidden in-place reset: the SAME striker id is present before
and after in nearly every case, consistent with one preacher managing several claimed listeners and periodically
cycling a stuck one back out (via `0x43AEC0`) and a fresh one back in (via its own `preacher_strike_scan` pass
moments later), both inside one 83 ms turn.

- Numbers: with the file default (12 turns/s), `PREACHEE_CONV_FREQ` 500 is 41.7 s between checks (20.8 s with
  flag 0x89C668 bit 2 set), each with a 1 in `PREACHEE_CONV_CHANCE` (5) chance: about 5 checks (3.5 min at the
  full interval, worst case) on average per conversion once a roll is in progress, instantly for a wildman or any
  other member of a "no breeding" tribe. These numbers are unaffected by this session's correction.
- A converted person keeps its original type (a converted warrior stays a warrior, in the striker's tribe), not
  the striker's own type.
- **Former "open contradiction", now RESOLVED [exe, measured]**: an earlier session read the per-type target
  table above (0x5A7090 bit 0x20) as "only enemy preachers and firewarriors can ever be struck into state 23",
  contradicting `combat.md`'s own `may_preach` rule (ordinary followers, never preachers/shamans, from the user's
  confirmed account of real play). A dedicated live watcher (`preachwatch4.py`, tracking GONE/NEW unit ids per
  turn by type/tribe/position rather than assuming a same-id tribe flip) caught 7 clean conversions during real
  play: every one an ORDINARY follower (type 2 brave or type 3 warrior), exact position match, tribe flipped
  (enemy → the converting preacher's tribe), type preserved, e.g. `turn 16722: GONE #1749 tr3 type2 pos=0xdc710723
  <-> NEW #1074 tr0 type2 pos=0xdc710723`. The previous write-up here called this "settled" in favour of
  preacher_strike_scan/state 23 and declared the table 0x5A7090 bit 0x20 reading "the wrong one" - **that
  conclusion is itself wrong, found by a later session that went back to re-verify it.** Two independent checks
  this session both confirm the ORIGINAL static reading, not the live-data override: (1) the raw table bytes at
  0x5A7090 were read directly from the exe again (stride 50, byte 0, bit 0x20) - set ONLY for type 4 (preacher)
  and type 6 (firewarrior), clear for every other type including 2 (brave) and 3 (warrior); (2) an exhaustive
  exe-wide search for every literal `mov BYTE PTR [reg+0x2C],0x17` (the only way to write person state 23) found
  exactly ONE site in the whole exe - `0x43AD67`, inside preacher_strike_scan itself, reached only after that same
  type-4/6-only gate. **State 23 is therefore provably unreachable for a type-2/3 person - the 7 GONE/NEW
  brave/warrior pairs in `diag6.txt` cannot have gone through preacher_strike_scan or 0x4D83B0 at all**, whatever
  they actually are. The kind-33 player-issued preach order was re-checked as the other candidate and ruled out
  too (see its own bullet below - it never calls `alloc_unit` anywhere in its body). So **neither of the two known
  preacher mechanisms can produce what `diag6.txt` recorded**, not settled by either preacher pathway found so far.

  **Resolved in the same session, by chasing the "separate mechanism" hypothesis**: the real cause is **Hypnotism**
  (shaman spell 7, "Per spell" above), not preaching at all - a completely different mechanism that happens to
  produce an outwardly identical kill-and-replace signature. Traced end to end: `process_hypnotism` (0x50EA60,
  Ghidra-named, not catalogued before this session) scans a radius-3 area 5 ticks after cast via the shared
  "nearest-N selector" (`0x515E30`, already catalogued only as "invisibility, shield, bloodlust, hypnotism" with no
  detail); its eligibility predicate (`0x50EAB0`) accepts any class-1 person **except type 7/8/1 (shaman, angel,
  wildman)** - i.e. ordinary followers of ANY type, exactly where preacher_strike_scan's type-4/6-only table
  excludes them; its action callback (`0x50EB00`) is the actual "person now belongs to a different tribe"
  primitive - kill via the same spirit-rise handoff, allocate a new person of the SAME type under the new tribe at
  the same position, copy life/max-life/carried wood across. This is a byte-for-byte match for every clause of
  this document's own pre-existing "Per spell" 7 entry ("the `HYPNO_NUM_PEOPLE` nearest persons of other tribes
  (not wildmen, shamans, angels) are recreated in the caster's tribe... then return... if their tribe is gone, they
  stay... ghosts are destroyed") - that entry was previously derived from file/constant analysis only; this session
  found the exe code implementing it and had simply never cross-referenced it against this section's own open
  contradiction. The "return" and "tribe gone" halves are `0x4D92B0` (already catalogued, but only as a 1-line
  guess before this session - see its own row): it re-fires the same kill-and-replace back to the person's
  original tribe once a per-new-record countdown (`+0xAC`, seeded from constant `0x5AA4F4`) expires, unless a
  per-tribe wipeout table (`0x89DB11`) shows the original tribe is gone, in which case the change is cancelled and
  stays permanent. **Bonus resolution**: `0x50EB00`'s own "ghosts are destroyed" branch tests `self+0x10` bit
  0x800 - the ALREADY-DOCUMENTED Ghost status (Unit record reference, "ghost +0x10 bit 0x800") - which is the exact
  same bit as the long-mysterious `candidate+0x11`/`striker+0x11` bit 0x8 this whole investigation kept running
  into without identifying (preacher_strike_scan bailing on a ghost striker; `0x4D4B50` spawning a special death
  effect for a dying ghost; `0x4D83B0`'s `0x4D850C` branch destroying a ghost candidate with no replacement) - all
  four are the same one rule ("a ghost has no real body to hand off"), not four separate mysteries.

  So: `combat.md`'s `may_preach` rule and the exe's type-4/6-only preacher candidate table were never actually in
  conflict - they describe two different spells (Hypnotism vs. the preacher's own conversion roll) that both
  happen to use the identical kill-and-replace idiom. `diag6.txt`'s 7 pairs were a Hypnotism cast (or several),
  not a sermon. The kill-and-replace mechanism itself (kill old record, allocate a new one under the new tribe at
  the same position, not an in-place tribe-byte flip) is shared code/idiom between the two spells, confirmed now
  from both sides. **A genuine implementation gap found along the way**: our own `hypnotism.rs`
  (`GameMap::change_owner`) does the tribe swap in place and resets health to FULL and carrying to 0, where the
  exe explicitly preserves the captured person's current life, max life and carried wood across the capture -
  flagged in TODO.md. **Bit 0x20's role no longer needs re-deriving**: the earlier suspicion that it "plausibly
  gates something equally unrelated to preach-targeting" only existed because the live data seemed to contradict
  it; now that the live data is explained by a different spell entirely (Hypnotism, above), there is no more reason
  to doubt the straightforward reading - bit 0x20 is exactly what it looks like, preacher_strike_scan's own
  candidate-eligibility gate (enemy preacher/firewarrior only). Read as a deliberate design split rather than a
  puzzle: the preacher's own subversion mechanic (kind 17/31/32, `preacher_strike_scan`) specifically targets an
  enemy's specialist defenders (preacher, firewarrior), while Hypnotism (a shaman spell, separate mana cost and
  cast) handles mass ordinary-follower capture - two different "join my tribe" tools for two different jobs, not
  one mechanic with an unreliable target table. This table is still not single-purpose in general: `fight_pick_slot`
  (`0x51DCC0`, "Fight") independently reads the SAME table's bit 0x40 for an unrelated purpose ("can this person
  type join a melee ring at all") - a different bit, a different per-type eligibility rule entirely, on the same
  50-byte-stride record.
- **Closed this session**: does preacher_strike_scan's candidate selection ever favour "whoever is currently
  attacking the striker", as opposed to pure proximity? No - re-read the whole scan loop (the expanding-square
  cell walk plus every per-candidate gate) specifically for this; nothing in it reads a candidate's own order,
  order target, or any "who's attacking me" back-reference. Selection is proximity (nearest eligible cell found
  first, radius capped by the caller's own `radius_byte` argument) plus the static eligibility gates already
  listed above, full stop - independent of whether the candidate is currently fighting someone else or not.
- Other open questions:
  - **Kind 17's own issuer traced, a later session**: `0x4CE2C0` is a per-tribe-member sweep (walks `tribe+0x881`'s
    linked list, the same convention `0x41CC20`/`0x4CB400` use), called from `0x4CCEB0` called from `0x4CB400`
    called from `maybe_ai_func_1` (0x4623E0, Ghidra's own guessed name) - **this whole chain looks like the computer
    AI's own per-tribe unit-management logic, not a player-facing or universal mechanic**. Per member: braves/
    warriors (and other types via a further unread branch) that pass a per-state eligibility check (table
    `0x5A6F79` bit 0x8) get an ordinary attack order (kind 19/0x13, via `0x43B540`); a PREACHER specifically
    (dispatched separately, `0x4CE43D`) gets kind 17 instead, gated by a per-unit flag (`+0x10` bit 0x40) and a
    resolved target position (`0x4F5770`, not traced) - reads as "go investigate/approach this position", a
    separate behaviour from the attack branch, not "attack" and not confirmed as "defend the building" either.
    `0x4DF0E0` ("preacher's extra check") gates whether a preacher instead falls through toward that SAME attack
    branch later in the same function. Still open: where the position/reference parameters threaded through this
    whole chain actually originate (an enemy sighting? an attacker reference?).
    **Resolved, a later session, tracing `maybe_ai_func_1` itself**: `maybe_ai_func_1` (0x4623E0) and its
    "decide a new goal" sibling (0x4625E0) are both called only from `process_tribe_2`'s own per-tribe turn
    processing (0x4615F0), and `process_tribe_2`'s own caller, `process_tribe_1` (0x461510, re-read this
    session), explicitly gates which per-tribe function runs on `tribe+0xC1F == 1` (the controller-type byte,
    see the Reference table and its own "Order kinds" subsection - 1 is computer/AI, 2 human, set once per
    tribe at level load by `clear_tribe`/0x42B660): a tribe with `0xC1F == 1` gets `process_tribe_2` (which
    ticks `maybe_ai_func_1`'s whole task-slot scheduler, 10 slots/tribe, 29 possible task types including this
    kind-17 dispatcher at task id 20); any other value (in practice 2, human) instead gets `0x4F6C20` directly
    as its ONLY per-turn work from this dispatcher, never `process_tribe_2`/the AI tick. (`0x4F6C20` itself, a
    per-building housekeeping pass unrelated to AI task slots, is not human-exclusive - `process_tribe_2` also
    calls it, near its own end, for the AI-tribe case; only the AI task-slot machinery above is gated.) **So
    a human-controlled tribe's preacher can never receive an unprompted kind-17 approach through this path at
    all** - it structurally only runs for computer-controlled tribes. A human player's preacher is left with kind
    32 via `preacher_auto_scan` (self-issued auto-attack/building-visit) or kind 31 via the player's own disguise
    command, exactly as this section already concluded, now confirmed rather than inferred from the chain's
    "AI-only" look. See `pop3-rev/docs/functions.tsv`'s rows for `0x461662`/`0x4623E0`/`0x4625E0` for the full
    scheduler trace.
  - still untraced: 0x5A707E (a per-type byte read on a model-variant branch) and the exact meaning of flag byte
    0x89C668 (many unrelated systems read other bits of the same byte, so it is unlikely to be preach-specific).
  - **Checked this session: the Ghidra project export (`backup/backup.xml`) adds nothing new to any open question
    here.** None of `preacher_strike_scan` (0x43ABF0), its sibling (0x43AEC0), state 23's handler (0x4D83B0), the
    player-issued preach order (0x43C7A0) or state 5's handler (0x4D6970) have a real (non-auto-generated) name or
    comment anywhere in the export - all four are plain `FUN_<addr>`, and the address ranges they cover carry zero
    `<COMMENT>` entries (checked directly). The one relevant cross-check it does offer: its independently-built
    `unit_type_person` structure (50 bytes, never actually applied to a data address in the export, so it's a
    standalone type recovery, not a placement) assigns real field names lining up exactly with this doc's own
    already-derived offsets into the person-type table at 0x5A7060 - `next_state` at +0x4 (the already-documented
    "0x5A7064" per-type fallback-state table used throughout this section and "Combat"/Whirlwind) and `flags` at
    +0x30 (this section's own preacher/firewarrior target-eligibility byte, bit 0x20) - folded into the reference
    table above. No bit-level names, no help on `candidate+0x11` bit 0x8, the 0x89C668 byte, or the kind-33/state-5
    reconciliation below.
  - **`0x4D850C` now fully resolved** (`test candidate+0x11,0x8; jne 0x4D87E8`): on a roll SUCCESS there are two
    distinct "no new unit" outcomes, not one, both read in full this session. (a) this bit set on the candidate
    skips straight to destroying it via `0x4D4B50` with nothing spawned - confirmed real code, but
    `candidate+0x11` bit 0x8's own meaning is still not pinned down (tested as a bail/exemption flag in at least
    three unrelated functions exe-wide - including preacher_strike_scan bailing on the STRIKER having it set - but
    never found being set at runtime anywhere; every direct write to this byte found this session is a whole-byte
    `mov`, mostly zeroing it, so it looks like a per-type/per-scenario value baked in at allocation rather than
    something toggled in play). (b) separately, just before the spirit-rise push: a per-tribe population counter
    at `[tribe*4+0x89DAE5]` is compared against `0xC8` (200) - at or over that cap, with the force-success tribe
    flag clear, the roll succeeds but NEITHER the old record is destroyed NOR a new one spawned (presumably
    retried on a later tick) - this is the literal mechanism and exact 200-head number behind this section's own
    "capped by the preacher's tribe population" line above, not previously connected to it. Neither path has been
    directly observed live (all 7 real conversions captured so far had a clean GONE+NEW pair), but (b) is now a
    fully understood, confirmed-to-exist mechanic either way, not just a hypothesis.
  - the candidate sub-state 4 effect burst above (the "sermon's candidate sub-state" subsection) allocates
    `alloc_unit(class=5, type=0xB, tribe=0xFF, pos)` - class 5 is scenery, and `pop3-rev/docs/unit_types` lists
    scenery type 0xB as "wood pile". **Narrowed, not fully settled, in a later session**: `alloc_unit` (0x4ED8A0)
    itself has no per-class/per-type init dispatch at all (checked directly, reading it start to end) - it only
    allocates, zeroes, and stamps class/type/tribe/position, so there is no `init_unit_type_5` to check (that
    premise was wrong). The call site itself is more telling: it fires in a tight loop (while a counter at
    `candidate+0x78` stays positive, decrementing by 100 each time) producing a BURST of several of these objects
    in quick succession, each immediately re-positioned via `0x4EE580` (a generic "snap to the right cell" utility,
    not scenery-specific) and followed by `0x48A050(candidate, 0xB, 0)` (the same "play a canned sound/reaction
    kind" call used all over this codebase) - this is a rapid, linked, self-targeted particle/sound burst
    accompanying the "listening" animation, not a world-placed stockpile, and `tribe=0xFF` (no owner) is consistent
    with a transient effect rather than a real resource pile. Leaning toward "scenery type 0xB is overloaded" (a
    genuine wood pile at level load, the same type id reused for a short-lived visual when spawned this way) over
    "the catalog entry is wrong", but still not proven without either a sprite-selection check for `tribe==0xFF`
    scenery or a live screenshot of the actual burst.
  - the player-issued preach order (kind 33, the preach-order handler 0x43C7A0) walks the preacher up to its
    target and, on arrival, plays a sound (0x9F) on the target and sets the target's own state to 5, chaining its
    command group's kind 0x98 → 0x9B → 0x99 over 40 ticks; state 5's own per-turn handler (0x4D6970) reads as a
    generic "idle near a reference unit" behaviour and never writes the person's own state - this still looks
    unconnected to state 23/preacher_strike_scan, a third, separate preacher mechanism.
    **Re-read end to end this session, both this function and state 5's handler, instruction by instruction (all
    9 of 0x43C7A0's own sub-states plus its cleanup tail, and the whole of 0x4D6970) - confirms the disconnect is
    total, not a tracing gap**: neither function ever calls `alloc_unit`, writes person state 23, or issues order
    kind 17/31/32 anywhere, and state 5 itself is a short, self-expiring 40-tick "face the preacher, play an
    idle-fidget animation from the per-type odds table (0x5A7067, already catalogued under "0x5A7064's" own row)"
    state with no re-arm - once its own `+0x70` timer or the preacher reference runs out it unconditionally reverts
    to the candidate's per-type default (table 0x5A7064), regardless of anything else, exactly like every other
    "give up" exit in this document. Two more findings from the same pass: (1) `preach_order_handler`'s target
    validity check only requires `target+0x2A` (`unit_class`) to be nonzero, never checking for class 1 specifically,
    and its own sub-state 2→3 gate reads `target+0x96`/`target+0x9A` - the same occupant-count/capacity pair
    documented elsewhere as a BUILDING's own occupancy fields - so the kind-33 order's code path is shared,
    unconditionally, with a "preach at a building" target, not a person-only mechanic requiring a separate branch
    (the long-flagged "building variant" TODO item is this same function, not a separate one, though the exact
    gameplay meaning of that occupancy check as applied to a building target is still unconfirmed); (2) the moment a
    target is actually pushed into state 5 (both at sub-state 8's own write and in the cleanup tail's unconditional
    fallback), `target+0xA7` is also set to `0x28` (40) - the same generic "busy" field this document's own open
    questions list elsewhere ("What 'busy' means (unit+0xE bit 7, unit+0xA7)") has never pinned a concrete setter
    for; this is the first confirmed write site found for it.

    **The actual reconciliation, found this session by chasing the OTHER side - what calls `preacher_auto_scan`
    (0x51E7B0, kind 32's issuer) and when**: `preacher_auto_scan` is reached from the generic per-person
    auto-attack rhythm scan (`0x4D4690`→`0x4D4723` for a type-4 unit, already catalogued at 0x4D4690's own row,
    re-read this session for this specific question) which runs for EVERY person every eligible tick regardless of
    its current order - **it is not gated on being idle**. Inside the preacher branch, it skips calling
    `preacher_auto_scan` for exactly one case: the preacher's own active (primary, else secondary) command group has
    kind 0x11/0x1F/0x20 (17/31/32) AND a flag bit (`self+0x76` bit 0x40, setter not traced) is set; for every other
    active kind - including 33 - it calls `preacher_auto_scan` unconditionally. **So a preacher running the
    player's kind-33 preach order keeps getting `preacher_auto_scan` evaluated on it every eligible tick, exactly as
    if it had no order at all.** `preacher_auto_scan` itself consults a second, previously-uncatalogued check,
    `0x4DF140` (traced this session - see its own new row below) to pick between its two modes: that check returns
    true only when the preacher's active group is ALREADY kind 17/31/32, past its own first sub-state, and
    standing still (`+0x5F == 0`) - conditions a kind-33 order never satisfies (its own group kind is 33) - so a
    mid-sermon preacher always falls into `preacher_auto_scan`'s "mode B" (building-candidate only, per that
    function's own row). If `0x51F030`'s building scan finds an eligible candidate, `preacher_auto_scan` allocates a
    brand-new group and assigns it to the preacher's PRIMARY slot via `group_assign(preacher, group, slot=-1)`
    (confirmed from the raw push order at its own call site, matching `group_assign`'s already-documented
    `slot<0` → primary convention).

    **Refined this session, having now surveyed every `group_assign` (0x436D00) call site in the exe (41 total) to
    settle exactly what "pre-empt" means here**: `group_assign(unit, group, -1)` only releases whatever group is
    CURRENTLY sitting in the unit's own PRIMARY slot (`+0x9B`) - it never touches the secondary-slot array at all.
    The player's kind-33 preach order does not live in primary to begin with: **every "give this unit an order"
    path that isn't a self-issued autonomous scan - all 8 of the numbered player/spell order-issuing helpers at
    0x43B180-0x43BB54 (confirmed this session for all 8, not just the 2 previously spot-checked: every one reads
    its own `+0xA6` cursor, just reset to 0, and passes THAT as the slot argument), plus every other one-off
    order-issuing site found in this survey (spell effects, building logic, the per-tribe sweep at 0x41CC20) -
    targets a SECONDARY slot, overwhelmingly slot 0 after the universal "wipe everything first" idiom. The ONLY
    call sites in the whole exe that ever pass `slot=-1` (primary) are self-issued autonomous behaviour scanners:
    `preacher_auto_scan` and its direct siblings for other person types (the ordinary auto-attack issuer inside
    0x4F0460, the firewarrior ranged-auto-attack issuer inside 0x51E9A0/nearby, and two further "if genuinely idle,
    self-issue something" functions at 0x40BD20 and 0x520480) - a clean, exe-wide split between "autonomous
    behaviour always claims primary" and "every directed/externally-triggered order always claims a secondary
    slot". kind 33's own issuer was not located this session (none of the 41 `order_set`/0x438730 call sites pass
    a literal kind of 0x21; it must come from one of the small number of call sites that forward a kind value from
    a register/struct field instead of a literal, not chased further), but every other directed-order path in the
    exe uses a secondary slot, so there is no reason to expect kind 33 to be the one exception - it almost
    certainly lives in a secondary slot too, not primary.

    **So the correct picture is "outranked and parked", not "destroyed and replaced"**: a self-issued kind-32
    order claims PRIMARY without touching whatever secondary slot holds the kind-33 group, but `order_execute`'s
    own resolve step (re-verified this session, `0x4325A7`-`0x4325DF`) always checks primary first and runs ONLY
    that group when primary is nonzero - so the kind-33 group sits alive but completely inert in its secondary
    slot for as long as the autonomous order occupies primary. Per `order_advance`'s own already-documented
    behaviour (above), once that autonomous primary order finishes and releases, `order_advance` scans the
    secondary slots from the cursor and would find the still-live kind-33 group there, making it primary-eligible
    again (i.e. the sermon would resume on its own, not restart from a fresh player click) - PROVIDED nothing else
    wrote over that secondary slot in the meantime. One real wrinkle this doesn't resolve: `preach_order_handler`'s
    own sub-state counter lives at the UNIT's `+0x2D`, a field this document already established elsewhere is a
    generic, shared "current order's own scratch" byte, not a per-group field - and `order_building_approach`
    (kind 32's handler) uses that SAME byte for its own sub-state while it runs as primary. So even though the
    kind-33 GROUP record survives untouched, the PREACHER's own `+0x2D` progress marker for it does not - whatever
    sub-state the sermon had reached is clobbered by the interim kind-32 approach, and what `preach_order_handler`
    sees on resuming (sub-state 0, since nothing was found resetting `+0x2D` back to 0 between orders, so it is
    most likely whatever kind-32 left behind) was not pinned down this session. This resolves the standing question
    of how an explicit player preach command can ever result in a real conversion despite kind 33 itself never
    touching state 23: **it can't, directly - any conversion that follows a player's preach order is actually
    produced by that same preacher's own concurrent autonomous auto-scan, which keeps running throughout, claims
    execution priority over the manual order without destroying it, and is the only path that ever reaches
    `preacher_strike_scan`/state 23.** This is a fully traced, static mechanism; none of it (the pre-emption, the
    later resume, or the `+0x2D` collision) has been confirmed against live play this session, and `0x51F030`'s own
    building-candidate scan radius/eligibility remains unread, so how OFTEN this actually fires in a typical sermon
    is still open.
  - **`preach_order_handler` (0x43C7A0) never calls `alloc_unit` (0x4ED8A0)** - unlike fight/pre-fight (class-10
    controller types 8/9), the sermon/listening mechanism creates no controller object at all; it is pure
    person-state bookkeeping (the preacher's own substates, state 5 on the listener). Our own `combat.md`
    "Sermon" coordinator (`Sermon{preacher,listener,left}`) is plausibly not a faithful structural mirror of this -
    flagged for whoever next reconciles combat.md against this section.
  - **Ordinary attack-eligibility (0x51C4C0, used by brave/warrior/spy/shaman attacking anyone) vs. the
    firewarrior/angel ranged scan (0x51D0B0): RESOLVED live this session**, see the "However, live data shows
    state 23 is NOT a blanket combat immunity" note above (`DIAG8.TXT`) - `0x51C4C0` does NOT exclude state-23
    candidates (8 of 71 observed sermons ended with the candidate pulled straight into pre-fight, state 29);
    only `0x51D0B0` (firewarrior/angel) explicitly does. So "a preacher is attackable unless preaching, and only
    another preacher/shaman can close in to disturb it" does not hold as a general rule either - an ORDINARY
    attacker (not just preacher/shaman) can disturb a mid-sermon candidate directly. The per-candidate-STATE
    table lookup (0x5A6F79, byte offset +1, feeding `candidate+0x11` bit 0x4) that looked like a plausible
    "state changes whether you're attackable" mechanism is now a dead end for explaining this specifically, since
    the live data shows ordinary attacks reaching state-23 candidates regardless; its true role (table layout
    still not re-derived, cross-check against 0x4D3B21's byte+0 bit 0x1 use and 0x41CC20's byte+1 bit 0x20 use)
    is still open, just not for this question. **Correction**: candidate `+0x2E` bits 0-3 are NOT an
    "interrupted" flag at all - confirmed live to be the ordinary `class_counter` rhythm field, see the
    correction above; there is no disturbance-detection mechanism found anywhere inside the state-23 sub-state
    machine itself - the disturbance plainly comes from outside it, via ordinary combat targeting pulling the
    candidate into a different person state altogether (29 or 3), which the sub-state machine never gets a
    chance to react to since it stops running the moment state 23 ends. Still not looked at: whether listener(s)
    in the OTHER, seemingly-unconnected sermon mechanism (person state 5, set by the player-issued preach order
    `0x43C7A0`) have any equivalent re-engage behaviour.

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
This is the shore case (a person standing/walking somewhere water-eligible); a person **blown into open water**
by a blast goes through a different, dedicated mechanic instead - see "Blast push and flight": sink at a fixed
-16/turn for 16 turns, forced to exactly 0 life at -256. That mechanic sets flags +0xC bit `0x2` while
sinking - almost certainly exactly to exempt it from this per-turn predicate (bit `0x2` above means "not
drowning", i.e. "already being handled elsewhere") while its own dedicated sink-and-die sequence runs; the bit
clears again at the moment of death.

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

## Notifications (advisor messages & alerts) [exe]
**Reported baseline (player observation, not yet independently confirmed in full against the running game this
session)**: a notification area sits to the right of the side panels. New notifications fall in as a small
square icon and pile up vertically (older ones get dismissed, probably by category/type). Clicking one of two
kinds opens a modal dialog ("information" notices); clicking a "preaching" one (a unit has started listening to
a sermon) or an enemy-spell-cast one (volcano, Angel of Death, etc. - shown with the spell's own icon) moves the
camera to the event's location instead; enemy-spell notices also carry a small vertical bar tinted in the
casting tribe's own colour. A separate "tower alert" (a sound, when an occupied tower/building spots a nearby
enemy) was also described, distinct from the icon stack.

**The advisor-message system, traced this session (`fire_advisor_message_once`, 0x499D90, `fn(debounce_bit,
message_id)`)**: this is very likely the *"information"* bucket of the reported system above (plain tips the
player can dismiss, debounced to once-ever by default).
```text
bail if: landscape-shaping mode (0x89D17C bit 0x20) or 0x98F746 bit 0x10 or Pre-fight's own "no kick" option
  (0x89D17C bit 0x2) or 0x89C664 bits 0x2/0x4 are set, or `debounce_bit` is already set in the
  "already shown" mask (0x89B739)
decode `debounce_bit` (0x499F40): a literal bitmask -> (category 0/3..10, a bool, a "repeatable" bool, a word)
  only 8 distinct bitmasks are recognised (0x8000/0x10000 -> 3/4, 0x20000/0x40000 -> 5/6, 0x80000/0x100000 -> 7/8,
  0x200000 -> 9, 0x8000000 -> 10, 0x4000000 -> a 9th, special "always repeatable" category); any other bitmask
  decodes to category 0 (unrecognised)
category 0 (unrecognised bitmask) and the "repeat" bit (0x89C66D bit 0x8) not overriding it -> bail
else: mark `debounce_bit` as shown (0x89B739) unless the decode said "repeatable"
category 0 (unrecognised)   -> the generic TEXT-BANNER path (0x499E31): allocate/configure a message SLOT -
    a per-slot struct array at 0x683BB3 (stride ~0x48, holds flag bits and, per 0x683BAE, a countdown word set to
    `-duration`) - set its text (0x430E40), optionally an icon/sound (0x430F30), and (OPEN: not traced this
    session) its own on-screen stacking/fall-in animation and dismissal
category 1 (one of the 8 recognised ones) -> a DIFFERENT path (0x499F16): reads a layout constant (0x5CA850)
    and calls 0x44D7F0(category, 0, scaled_value) - a 10-case dispatch (0x44D7F0, read in full this session,
    see below), writing a SHARED notification record (0x68B6A1/0x68C6xx: a position, a dword, a "kind" byte, a
    formatted text string) that whatever draws the reported icon almost certainly reads - the drawing code
    itself (whoever reads this record each frame) was not located
```
`0x44D7F0`'s 10 categories, as far as traced (its own address is reached from `fire_advisor_message_once` for
categories 3-10, but categories 1-2 are reached ONLY from two OTHER callers, both tied to the level-SCRIPT
interpreter - 0x44A030, a tiny wrapper reading a literal 3-byte script data record, and an inlined call at
0x490D05-0x490D28 inside a function that visibly steps its own script instruction pointer `[+0x3104] += 2`):
```text
1 (0x44D861): walks a linked chain of objects (class 5 / type 9, "head" scenery - docs/unit_types) via each
    object's own "next" field until a head is found (else no message); text built from the head's own data -
    a worship/trigger-head event (ai-scripts.md "TRIGGER_THING"/"GET_HEAD_TRIGGER_COUNT") is the best guess,
    NOT confirmed as the reported "preach" alert
2 (0x44D8F6): resolves an object from a packed id; when the caller also flags it and the object has a real
    tribe, additionally pulls a 10-byte PER-TRIBE record (table 0x8A035C) and draws it - **a strong match for
    the reported enemy-spell notice's small vertical tribe-coloured indicator**
3, 5, 6, 7, 8 (one shared handler): bail unless the LOCAL PLAYER's tribe has a living shaman (table 0x89DA65),
    else show one of 4 literal "about your shaman" text ids - none of these 5 is reached by any of the 11 known
    advisor-message call sites, so they may be unused in the shipped levels, or reached from an unfound caller
4, 10 (one shared handler): plain text only, positioned at the map centre - the two known call sites are both
    "you can't go/build there" tips (see the table below), not an enemy-spell notice as first guessed this
    session - correcting that initial hypothesis
9: reads a position-like global and a small counter into the record, then plays sound 0xE1 - matches its one
    known call site (0x4D9580's local-player, recent-event notice, which also plays 0xE1/0xE2 itself first)
```
Reached this way, category 1/2's trigger is most likely a level-script `DO` command (plausibly one of
`CREATE_MSG_INFORMATION`/`CREATE_MSG_INFORMATION_ZOOM`/`CREATE_MSG_NARRATIVE`, ai-scripts.md "Level events"),
not an automatic runtime scan - though the engine could in principle also call `0x44D7F0` directly with
category 1/2 the same way it does for categories 3-10, which would make it a built-in alert too; which literal
opcode maps to which category wasn't confirmed (the script interpreter's own opcode table wasn't read).
11 call sites found this session, each with its own literal `(debounce_bit, message_id)` pair - most fall
through the generic/unrecognised path (plain text tips), only three decode to a recognised category:

| Call site | debounce_bit | message_id | category | Context (not traced beyond what's shown) |
|---|---|---|---|---|
| 0x41AB9C | 0x4000000 | 0x4A3 (1187) | special/repeatable | the already-documented "mana wasted" tip (spells.md) |
| 0x4D963B | 0x200000 | 0x25B (603) | 9 | a person per-turn handler (0x4D9580) that, only for the LOCAL PLAYER's own tribe and within 2 turns of some other counter (0x89BC22), plays sound 0xE1 (or 0xE2 for a shaman) right before this call - 0x44D7F0's own category-9 handler plays 0xE1 again and builds its text from a position-like global, consistent with a death/loss notice, not confirmed by name |
| 0x4C27BB | 0x8000000 | 0x261 (609) | 10 | a destination-cell validity check (0x4C24F0, shared with the plain-tip sibling at 0x4C278B/msg 0x255/cat 4 just above it) against the coastal-shape table (0x5AA328) - reads as "you can't go/build there", NOT an enemy-spell-cast notice as first guessed - **correction of this session's own initial hypothesis**; category 4 and 10 share the same 0x44D7F0 handler (plain text at the map centre) |
| 0x40B259 | 0x4000 | 0x253 (595) | 0 (plain tip) | gated on a unit/building type == 0x13 (19) |
| 0x41A905 | 0x800000 | 0x25D (605) | 0 (plain tip) | gated on the turn counter >= 0x5A0 (1440 turns, 120 s) |
| 0x43F3CF | 0x2000 | 0x252 (594) | 0 (plain tip) | gated on a tribe-index comparison |
| 0x499D2B | 0x10000 | 0x256 (598) | 0 (plain tip) | |
| 0x4AC1C8 | 0x200 | 0x24B (587) | 0 (plain tip) | |
| 0x4C278B | 0x8000 | 0x255 (597) | 0 (plain tip) | the "invalid destination" sibling of 0x4C27BB above |
| 0x4D5EDA | 0x1000 | 0x251 (593) | 0 (plain tip) | gated on global flag 0x89DB07 bit 0x1 |
| 0x4ECDCC | 0x1000000 | 0x24F (591) | 0 (plain tip) | gated on a counter >= 0xC8 (200) |

**`0x89D167`, a strong candidate for a general "you are in combat" alert** (separate from the message system
above): two different writers. `pre_fight_turn` (0x518630) and `fight_turn`'s own creation path set it
unconditionally to 2 every turn a fight/pre-fight involves the local player's own tribe (a continuous "currently
in a fight" signal). `0x51A2A0` ("Starting a melee") instead writes it edge-triggered - only when it is
currently 0 and a brand-new fight/pre-fight is about to involve the local player's own tribe does it get set
(to a different value than 2, not decoded this session) - a one-shot "a new fight involving you just started"
trigger, the shape an alarm/alert-sound trigger would need. One confirmed reader, `0x4ECA2D`: copies the byte
into `+0x8` of a struct pointed to by global `0x895DBB` (gated on that pointer being non-null) - plausibly a HUD
widget's own state field, not traced further.

### Watchtower sentry scan and alarm (0x40BD20) [exe]
Found this session, tracing from the already-known `<TYPE>_DT_RADIUS` constants ("Constant table") through their
only two readers (`0x40B450`, `0x40BD20`) - **the exe-side mechanism behind the reported "Tower alert"**.
```text
fn(building): only for building+0x2b==4 ("tower", docs/unit_types) AND occupied (+0xa6!=0) AND its first valid
  garrisoned occupant (+0x86, via 0x890390) is specifically TYPE 3 (warrior) - a tower manned only by a brave,
  priest etc. never arms this check
radius = DT_RADIUS[occupant type] (person-type table 0x5a7060/stride 0x32 +0x22; defaults 4/7/5/5/8/3 for
  brave/warrior/religious/spy/super_warrior/medicine_man) + a small building-height bonus (+0x41)
  (0x40B450 computes this same radius standalone - read by two unrelated range-display consumers,
  `add_tribe_unit_to_globe`/0x41F16C and `set_landscape_pos_fields_2`/0x502136 - neither is the scan itself)
scan the (radius+1)x(radius+1) cell square (shared cell-walker, same family as the person melee-scan) for ONE
  eligible enemy: different tribe, hostile relationship (both ways), alive, visible (LOS check, not hidden,
  not mid-conversion, not already preaching/attached), and the SAME attacker/candidate type-pairing rule used
  by the melee scan and the Angel of Death's hunting AI - for a plain warrior sentry this means it never
  detects an Angel of Death
found → a SECOND pass over the same cells flags (a bit on +0x14) every OWN unit and OWN building (of a type
  flagged "reacts" in its own type table) in range, then gives each of them (skipping any already busy with an
  excluded order) order KIND 19 ("attack (generic)", `attack_order_handler`/0x51A2A0) via one SHARED command
  group - i.e. a rally-to-defend call, not a direct sound/icon trigger
```
**No sound or notification-record write exists anywhere in this function.** Kind 19's own handler, `0x51A2A0`,
is exactly where `0x89D167` above gets edge-set - so the tower does not play its own siren: it forces a fight to
start, and that pre-existing generic "a fight involving you just started" alert is what actually fires, same as
any other new fight. This **confirms `0x89D167` as the tower alert's trigger after all** - not because it is
tower-specific, but because the tower's own response *is* "start a fight", which is the one thing `0x89D167`
already watches for.

**Correction of this session's own first pass**: `0x40BD20` is never reached from an unconditional "for every
tower, every turn" loop - its only call path is through a small per-[tribe][type] worklist walker (`0x40BCE0`),
which in turn has exactly two callers: `process_tribe_cmd` (0x44191C, raw command id 0x7C) and the level-script
interpreter (`interpret_script`, 0x48E2B7, opcode `CALL_TO_ARMS`/1198 per ai-scripts.md). The first draft of this
section called the `process_tribe_cmd` path "a debug/cheat case" and concluded the whole mechanism was
"script/command-triggered per level, not a native always-on engine check" - **that was wrong, caught when the
user pointed out the alarm also happens in multiplayer, where campaign-style level scripting doesn't apply.**
Tracing further: `process_tribe_cmd`'s command id 0x7C is drained every turn for whichever of the 4 tribe slots
has one queued (`process_tribes_cmd`/0x43E890 - runs in every game mode, not campaign-specific), and that command
is queued by an ordinary **player hotkey**: `process_cmd` (the UI command dispatcher, own command id 0xBF) calls
`set_tribe_command` (0x479CF0) with literal opcode 0x7C for the local player's own tribe - almost certainly
shortcuts.md's already-documented "**B** (double tower alarm): nearby followers help the tower", which this
session hadn't cross-checked before writing the first draft. So the real mechanism is a plain player-triggered
command available in any mode, not level-script content; `CALL_TO_ARMS` is a secondary, rarely-used (1 level
total, per ai-scripts.md's own usage count) way to invoke the identical engine primitive for scripted campaign
moments. Still open: that keypress's own two command args are both 0 (no building-type choice made at the
keypress) while `0x40BCE0`/`0x40BD20` read the type to scan from `tribe+0xc22` - where that field gets its "4"
(tower) from was not traced; and the "Space" half of the shortcut ("camera to the tower under attack") wasn't
re-examined this round, though it reads as a plausible consumer of the already-traced `0x89D167`/`0x895DBB+8`
state.

**Correction**: order kind 30 (0x1E), catalogued as "alarm" in `pop3-rev/docs/functions.tsv` from its Ghidra-
adjacent name, turns out on reading its handler (0x43DAA0) to be an ordinary "walk to this building" unit order
(an arrival check against a building, no sound or notification anywhere in it) - the "alarm" label looks like a
guess, not confirmed sentry/tower behaviour; left as-is in the catalog with a note, not renamed outright.

### Guard the shaman (G key) [exe, measured]
**Reported (player observation)**: pressing G with followers selected makes them regroup next to the shaman and
turn to face her; pressing G with nothing selected cancels it.

**First pass of this session's own tracing was wrong, caught by a live memory trace.** A static reading
identified `ui_cmd 0x6D -> tribe_cmd 0x6E` (living-shaman gate, writes a position/mode onto the shaman, sets her
own person state to 38) as a strong-looking candidate for G. A live watch of the running game (`watch_guard2/
3/4.txt`, following the shaman's own fields and every selected unit's command-group slots at 50 Hz) proved this
candidate never fires on an actual G press - a coincidental false lead, not G. State 38 is real, traced code
(left in `person_states.tsv` under a corrected name, trigger unidentified), just not this.

**The real mechanism, confirmed live both ways:**
```text
ui_cmd 0xC1 (process_cmd, Ghidra caseD_c1) -> tribe_cmd 0x82 (process_tribe_cmd, caseD_82, 0x441FA3)
  -> 0x443B40(tribe_idx, bool_arg)
```
`0x443B40` walks the tribe's own person list (not buildings - an earlier pass of this analysis, inherited from
before this session, wrongly called it "rings a tribe's buildings with an alarm order"):
```text
any non-shaman unit currently selected (+0x7A bit 0x80)?
  yes -> give each one order KIND 30 (0x1E) targeting the shaman (the value at her own +0x24 field, plausibly
    her own cached id), sharing one command group, into each follower's SECONDARY group slot (not primary);
    then clear that follower's own selected flag
  no  -> instead scan the whole person list for anyone already in state 10 with +0xA7==0x1E (a "currently
    escorting" marker) and CANCEL that order - "stop guarding"
```
Order kind 30 itself (`0x43DAA0`, also corrected this session - an earlier pass called it an inert one-shot
"walk to a building" order) is a genuine **continuous escort**: every tick it re-reads the reference unit's
*current* live position (not a frozen target), keeps the follower within ~824 units of it on both axes, and
never reports itself done while the reference stays alive. Live-confirmed exactly: a selected follower given
this order continuously circled the stationary shaman at a ~200-unit working radius, matching "regroup next to
the shaman" - the "turn to face her" half (facing/orientation) wasn't separately instrumented but is consistent
with ordinary walk-facing toward a moving destination. The player also confirmed live that G with no selection
cancels an active guard, matching the second branch above exactly.

Two open items, both low priority given the mechanism itself is now solid: what the shaman's own `+0x24` field
actually is (read as her target id - plausibly a self-reference, not independently confirmed), and whether the
unread `0x433490`/`0x4999D0`-sourced `bool_arg` changes anything in either branch.

**The shaman is NOT protected from attack while guarded - live-tested and disproven.** The player's own
hypothesis ("an attack on the shaman is absorbed by a guard instead of her") was checked by watching the
shaman's and every guard's own person state for entry into fight/pre-fight (25/29) on a live level with several
guards active (`guard1.txt`). She entered state 25 (an ordinary Fight, not merely a pre-fight) at the exact same
moment as two of her own guards, all three going straight to state 25 together while a fourth unit (not on a
guard order at the time) had gone through the normal pre-fight (29) moments earlier - i.e. she joined the SAME
fight ring as her guards, she was not excluded from it. This reads as the already-documented auto-attack/melee-
scan rule applying normally to her too: one nearby unit's scan starts a fight, and other eligible units nearby -
guards and the shaman alike - then match the scan's own "a fight controller to join" result (Reference,
"Auto-attack") and join the same ring (up to 6 participants) rather than her being specially redirected around.
So guards appearing to "take the hit" in practice is better read as "whoever the attacker's scan reaches first
starts the fight, then everyone nearby piles into the same ring" - not a deliberate protection mechanic.

## Impact on our specs
- `hud/mana.rs`'s strip (`mana_view`, `pool.bank` against `MAX_MANA`) does not match "The mana bar (HUD)": the
  original's bar is full whenever nothing is charging regardless of `bank`, and otherwise shows `last_income`
  against an advisor pacing estimate for the charging spell, not `bank`'s own share of `MAX_MANA`. It is also a
  plain single-colour fill with no fill/track two-tone and no flash.
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
  is flatter than the original. The original camera pitch is fixed at about 20°, 25 cells behind the focus,
  **but only while the player drives the camera** - during an AI-script flyby it animates independently of
  yaw (measured dipping to about 55° mid-flyby in level 1's opening flyby), see "Camera flyby state
  (tribe_struct)". `game-client/src/flyby.rs`'s `drive_camera` never touches `CameraRig.pitch` during a
  flyby (only `focus.x/y`, `yaw`, `distance`) - confirmed gap, not yet fixable precisely (the raw pitch
  field found live, tribe_struct `+0x28` hi 16 bits, doesn't have a solved scale yet).
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
- Spell order and share inside the distribution loop.
- Mana bar (HUD): what global byte `0x89D161` (scales `advisor_eta`, "The mana bar (HUD)") actually tracks -
  looked like a game-speed setting, not confirmed; what tribe index `0x759E9C` is compared against for the
  bar's flat-colour flash, and `0x89798B`'s own role versus the training bar's documented blink byte `0x897989`
  (plausibly the same blink-timer family, two bytes apart, not confirmed); whether that flash and the debounced
  "no training building needed funding" advisor message (0x41AB63) are the same "mana wasted" signal or two
  independent ones.
- Notifications: `0x44D7F0` is now read in full (10 categories - "Notifications" above); still open: whoever
  reads the shared notification record it writes (`0x68B6A1`/`0x68C6xx`) and actually draws the reported falling/
  stacking icon - not located; which level-script `DO` command(s) map to its categories 1 and 2, OR to the
  watchtower scan's own type argument (the script interpreter's own opcode dispatch table wasn't read, so none
  of these are pinned to a named PopScript command yet); whether category 1's "head"-chasing is really the
  reported "preach" alert (unconfirmed guess) or something else (worship/trigger-head is the stronger reading of
  the code itself); the generic text-banner slot array's own stacking/fall-in/dismissal visuals (0x683BB3); the
  enemy-spell-cast alert's own TRIGGER (category 2's tribe-coloured-bar drawing is traced, but nothing was found
  that calls it automatically when an enemy casts a spell - category 2 is only confirmed reached from the script
  interpreter so far). **Resolved this session**: the guard-post/tower "spots a nearby enemy" scan is `0x40BD20`
  ("Watchtower sentry scan and alarm") - it doesn't play a sound itself, it issues a rally/attack order whose
  handler is the same `0x51A2A0` that edge-triggers `0x89D167`, confirming that byte as the tower alert's actual
  trigger path. It is queued by the player's own **B** hotkey (shortcuts.md), not level-script content (an
  earlier draft of this section wrongly called it script/cheat-only - corrected in "Watchtower sentry scan and
  alarm" above after the player pointed out the alert also happens in multiplayer). Still open: exactly what UI
  code reads `0x89D167`/`0x895DBB+8` to draw the icon and play a sound; where `tribe+0xc22` (the scan's own
  building-TYPE argument) gets set to 4, since the B-key command's own args are both 0; and whether **Space**'s
  "camera to the tower under attack" half reads that same state.
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
  and fixed). **Correction, 2026-10-21: the "nothing writes it" conclusion above was wrong** - it only checked
  terrain-specific functions. A live read on `levl2001.dat` (the white-patch repro level) found this nibble
  nonzero (6) at the one cell under/near the hill's temple, zero everywhere else nearby, reproducible across a
  full game restart - so it is neither always 0 nor stale garbage. The real writers live in unit/object-movement
  code: `landscape_move_1` (0x403A00) and `landscape_set_unit_shadow` (0x403C10) both do
  `cell[+0xE] = (cell[+0xE] & 0xF0) | clamp(calc_unit_shade_value(cell), 0, 0xF)`, i.e. they replace only this
  low nibble, preserving the downhill-direction high nibble. `calc_unit_shade_value` (0x450D50) walks the things
  placed on/near that cell (via two separate thing-index fields in the cell record, +0x6 and +0x8, into the
  global thing-pointer table 0x890390) and sums a per-object-type/size shadow contribution (tables at 0x5A7271/
  0x5A725D/0x5A79C3) for every non-person thing found, clamped to 15. **So this nibble is not an ambient light
  constant at all - it's a dynamic 0-15 shadow intensity cast onto the ground by standing buildings/trees/
  scenery**, recomputed whenever an object is placed or moves, which is exactly why it's nonzero only under this
  hill's temple. `game_core::shading::CellShading`'s hardcoded `ambient_nibble = 0` is a real gap on any cell
  near a building, tree, or other scenery object, separate from (and unrelated to) the divisor/clamp question
  resolved just above - see `pop3-rev/docs/functions.tsv` rows `0x403a00`/`0x403c10`/`0x450d50`, and
  `pop3-rev/docs/plans/terrain-white-patch.md`. Not yet traced: what distinguishes the two thing-index roles
  (+0x6 vs +0x8) on the same cell, or the exact object-class/sub-kind values the weighting tables key on - left
  for a future session.
- Attacking a building: what sets building +0x9C bit 0x40, and the partial explosion's 10/7 units.
- Firewarrior fire: the line-of-fire test, whether a building stops the bolts (5 throws at a still preacher did nothing), what sets the reload
  byte to 252-255 (third capture), what the +0x10 bit 0x400000 set on own people under a passing shot does,
  whether hfx 1124-1127 is the shot's fade, and the approach sub-states (10, 11) before the throw.
- Combat: where a person's maximum life (`+0x6C`) comes from when it is not `LIFE_<type>`, whether a taunt (48-50) plays to its end (0x4D4DA0
  resets a non-idle person to idle the next turn; the per-person `+0x7E` it sets may hold it),
  why pre-fight mode 2 exists as a slower lead-in to the same pull-out-and-throw mode 1 reaches directly (which
  caller passes which mode was not checked), the ground test of a fight's place (0x51E4B0, its own body unread -
  only its usage, "called periodically, false means relocate", is confirmed), the attacker's sub-state 8 when
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
- Blast: whether this spell's own `push_k`/`vert_k`/`R` instance fields (+0x74/+0x78/+0x6c, "Blast push and
  flight") are really 140/98/1280 or something else - the live horizontal figure checks out (140 confirmed
  to ~1.04x on flat ground) but the vertical one measured 132, not 98, twice (98/140 ARE independently
  confirmed real values, just for a different call site - a destroyed boat's land explosion, "Vehicles"); and
  the very first `BLAST_DAMAGE_PERSON` tick is still unseen. What happens to a person landing in water is now
  answered (see "Blast push and flight": sinks at -16/turn for 16 turns, dies outright at -256, independent
  of the ordinary fall-damage check). Four live sessions (pop3-rev/docs/cheat.md "Live memory: measuring a
  Blast") confirmed "Person physics" exactly (gravity, drag, the 192 horizontal velocity cap, ground friction,
  `FALL_OUT_OF_WW_DAMAGE` 700, `BLAST_DAMAGE_PERSON` 50/turn) and the push's direction (radial, 0-10° off),
  timing (frozen through the blast's own 3 turns, committed in one jump right after), and that the slope rule
  is horizontal-only (confirmed by disassembly, `0x50b9a6`-`0x50ba05`). Next step is simplest now: read
  `+0x74`/`+0x78`/`+0x6c` straight off a live `kind=blast` row (`measure_blast.py` already logs them) rather
  than inferring them from a pushed person's velocity.
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
- Camera flyby state (tribe_struct, see that section): `FLYBY_START`'s exact turn (anchored to only ±15 by
  curve-fitting camera motion against level 1's opening flyby script; the AI script's own per-tribe flag it sets
  right before `FLYBY_START` would pin it exactly, not yet found); `+0x28`'s raw-to-pitch-degrees transform;
  `+0x24`/`+0x2c`/`+0x30`/`+0x34`'s exact scale/offset (direction of `+0x24`'s x/z halves confirmed, nothing
  else); whether `FLYBY_SET_EVENT_ANGLE`'s documented "shortest way" rule is really contradicted by one
  long-way measurement, or whether that's just the `POS` channel's own confound on yaw (not yet separated).

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
| 0x41AD70 | picks a learned/unblocked/not-full spell to fund (and its share); feeds both the distribution loop and the advisor estimate below |
| 0x415F70 | looks up a render colour by index from a system palette |
| 0x49E8B0 | "draw_mana_bar": the HUD mana bar; see "The mana bar (HUD)" |
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
| 0x432590 / 0x432D18 / 0x4366B0 | `order_execute` / its 31-entry kind table / `order_advance` (picks what a unit runs next once its current order finishes) - "Command groups" |
| 0x43A4D0 | `order_building_approach`: shared approach-and-RNG-idle machine for order kinds 17/31/32, calls preacher_strike_scan on a roll success - "Preacher conversion" |
| 0x43ABF0 | `preacher_strike_scan`: the real trigger into person state 23 (the conversion roll), NOT preacher-exclusive despite the name - "Preacher conversion" |
| 0x43AEC0 | sibling of 0x43ABF0: reverts a nearby stuck state-23 roll after a failed kind-32 visit times out |
| 0x4D6F90 | issues order kind 31 ("disguise" [game, user-confirmed: spy-only, no striking]); gated by a per-type table to brave/warrior/spy, never preacher; exact player-command trigger not pinned down - "Preacher conversion" |
| 0x4DEFF0 | issues order kind 17 (default) or 31, gated to `self.type == 4` (preacher) - a different thing from the disguise command above, reached from `init_unit_class_1` (0x4D2740) - "Preacher conversion" |
| 0x43B790 | "give this unit an order" helper for kind 17, called from an untraced dispatcher (0x4CE2C0) - "Preacher conversion" |
| 0x43C7A0 | order kind 33 (preach): approaches the target, ends by setting its state to 5 - "Preacher conversion" (a separate mechanism from the above, not reconciled) |
| 0x51A2A0 | a person's attack order: reach, then join a fight or start a pre-fight |
| 0x51C3C0 | attack target eligibility by attacker type; a preacher/non-preacher gate on order kind 0x1C specifically - a dead end for "Preacher conversion", see that section |
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
| 0x51E7B0 | `preacher_auto_scan`: person-or-building auto-attack scan, explicitly checks `self.type == 4` before the building branch - confirms order kind 32 is preacher-exclusive at issuance, "Preacher conversion" |
| 0x51F030 | scan helper used by 0x51E7B0 (two out-params; mode flag; not fully read) |
| 0x4D3D18 | person per-turn state dispatch table (`unit_processing_class_1_person`, keyed by `+0x2C − 1`) |
| 0x4D5CF0 | person state 3: universal DEATH, reached from the life-step (0x4D43A0, life <= 0), NOT preach-specific - spawns a 7/0x4D effect and a 10/0xC spirit controller at its own place, corrects this row's previous "a preachee struck" |
| 0x4D83B0 | person state 23: the conversion roll, entered directly by preacher_strike_scan (0x43ABF0), requires the striker's own order kind to be 17/31/32 - "Preacher conversion" |
| 0x4D8E40 | person state 24: Whirlwind's own lift state and nothing else (confirmed this session, exhaustive search), NOT linked to conversion - "Whirlwind mechanics" |
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
| 0x50A750 | effect per turn: switch on +0x2C (table 0x50AFD4); effect init by type: table 0x50A59C. Both tables are indexed `state−1`/`type−1` (`dec eax` before the bounds check and `jmp [eax*4+table]` in both dispatchers, confirmed by reading them directly, 2026-10-20) - an off-by-one trap when computing a row's address by hand, see "Insect plague" for a worked example |
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
| 0x509DC8 | erosion init (class-7 type 0x17 case of `init_unit_type_7`'s dispatch): state -> 24, duration +0x6C -> 64 |
| 0x50FF30 | erosion per turn (state 24, Ghidra's `process_erosion` is correct, despite living in the shot code area): jitter + one pass via 0x4983A0, 63 passes over 64 ticks, see "Erosion passes" |
| 0x510020 | insect plague (swarm) effect 7/0x15 init: height/speed/duration/insect-budget/home position/heading, never sets a drawable - "Insect plague" |
| 0x510120 | `process_insect_plague` (Ghidra's own name is correct), state 0x16 (22) per turn: sound, home-leash steering, bee-dot spawning - "Insect plague" |
| 0x510D40 | insect plague's bee-dot spawner (its own sub-state 0): `alloc_unit(7, 0x1B, ...)` at a jittered point near the cloud, gated by the insect-budget counter +0x72; also, 1-in-16, tints the new bee-dot with its tribe's own colour (+0x8A, from table 0x5A89CA) - "Insect plague" |
| 0x509E19 | bee-dot effect 7/0x1B init: the ONE drawable call in this whole chain, `0x4EE700(&unit+0x33, 0, 0x1A)` (table 0x5A6AF8 row 26, renderer byte 0x0D, not otherwise identified this session) - "Insect plague" |
| 0x510CB0 | bee-dot per turn: a ~19-step sub-state cycle, repositioning relative to a held reference unit (+0x74) once set - "Insect plague" |
| 0x5A89CA | per-tribe 5-byte struct (same table "Status markers over people" calls 0x5A89CB for the shield's tribe_colour, a different field of the same record): byte+0 is the tribe's own flag-colour palette index (blue/red/gold/green for tribes 0-3, confirmed against pal0-0.dat), read by the bee-dot tint roll - "Insect plague" |
| 0x46AE3D | `draw_polygons`: reads a unit's own +0x8A tint byte, looks it up (×4) in a runtime-built RGB cache (0x87EFF8), and draws a flat-coloured point via 0x4B7F90 - the bee-dot's likely real blit, not fully pinned to its exact dispatch case - "Insect plague" |
| 0x4983A0 | one hydraulic-erosion pass over an 8x8-cell window: per cell, steepest of its 8 neighbours (diagonals weighted 0xB505/0x10000), flow normalised against the window's own max then scaled by a per-cell factor and randomised terms (0xC350, 0xFA0, prng), floored at 0, written back to the live heightmap, then the usual 0x44DDF0/0x44F2F0 notify pair; exact magnitude formula not fully traced |
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
- `pos` (6 bytes, confirmed from the person physics step, 0x4E6D00/0x4e6d1d-0x4e6d7b): `x` (i16, low word of the
  dword at +0x3D), `z` (i16, high word of that same dword, i.e. +0x3F), `height`/altitude (i16, +0x41) - the same
  x/z-packed-plus-height convention as the tribe's reincarnation site (+0x911/+0x915). Confirmed live (a blown
  unit's `height` moves by exactly its `vel.y` each turn, `x`/`z` by `vel.x`/`vel.z`). Six bytes follow (+0x43 to
  +0x48, before velocity starts at +0x49) that live-tracked the same values as velocity in one trial - not a
  separately confirmed field, open.
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
  +0x14 bit 0x80000 (timer +0xB1), hypnotised +0x10 bit 0x4000 (timer +0xAC, original tribe +0xAD; the code behind
  this status - setter 0x50EB00, per-tick countdown/revert 0x4D92B0 - was found this session, see "Per spell" 7 and
  "Preacher conversion"'s resolved contradiction), ghost +0x10 bit 0x800 (this same bit also explains three
  previously-separate "mystery exemption flag" sightings elsewhere - preacher_strike_scan, 0x4D4B50, 0x4D83B0 - see
  "Preacher conversion"), low health +0x14 bit 0x1000; the timers count down every 8 turns. On fire: +0xC bit 0x8 leads to
  state 31. Life +0x6E, maximum life +0x6C. Drawing: translucent +0x36 bit 0x40, hidden +0x35 bit 0x10,
  selected +0x7A bit 0x80.
- Movement: profile index +0x30 (into 0x5A7B90), walk speed +0x5F, heading +0x5D, velocity +0x49 (x),
  +0x4B (vertical), +0x4D (z).
- Per-cell index `unit_land_array` (the original "mapwho"), filled by `insert_unit_into_land_tile`.
- Global lists: `wild_units`, `fight_units`, `pre_fight_units`, `guard_control_units`, `boat_units`,
  `airship_units`, `trigger_units`, `head_units`, `swamp_effect_units`.
- Each class has `init_unit_type_N`, `init_unit_class_N` and a per-turn `unit_processing_class_N_*`.
- Full catalogs of each class's own `+0x2C` per-turn state (number, name, status, handler, role) are kept
  separately, one file per class, not duplicated here: `pop3-rev/docs/person_states.tsv` (class 1, states like 3
  "death" or 23 "the conversion roll"), `pop3-rev/docs/effect_states.csv` (class 7, states like 21 "whirlwind" or
  38 "lava square"), `pop3-rev/docs/controller_states.csv` (class 10, states like 12 "post-death spirit" or 18
  "sinking-building animation"). **These are three SEPARATE numbering spaces** - the same number means something
  different in each class (e.g. person state 24 is Whirlwind's own lift state, while effect state 24 is erosion,
  and controller states only go up to 18) - always say which class a bare "state N" refers to.

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
| +0x00 .. +0x23 | camera rotation matrix (the active player's own tribe only - "Camera flyby state (tribe_struct)") |
| +0x24, +0x28, +0x2c, +0x30, +0x34 | candidate camera path fields (position, pitch, distance, interpolation progress) - see "Camera flyby state (tribe_struct)", none of their exact scale/offset confirmed |
| +0x881 .. +0x899 | heads of the person, building, shape, formation and beacon lists |
| +0x89D | shaman |
| +0x91D | number of people (reaching 200 calls 0x499D90, probably an error or limit report) |
| +0x93D | flags; bit 3: spells are free |
| +0x94D | `bank`, clamped to `MAX_MANA` |
| +0x951 | `reservoir` |
| +0x955 | `incoming` |
| +0x959 | `needed` |
| +0x95D | `last_income` |
| +0x961 | `advisor_eta`, player tribe only: see "The mana bar (HUD)" |
| +0x969 + 4·spell | `charge[spell]`, spells 1-21 |
| +0xA27 + 2·type | number of people per person type (rebuilt each turn) |
| +0xB7D + 2·type | number of huts per hut type (1-3), for the capacity |
| +0x921 | base population capacity, added to the huts' |
| +0x941 | flags; bit 6: no breeding |
| +0xA05, +0xA07 | reservoir `delay` and `rate` |
| +0x911, +0x915 | reincarnation site position (x, z) and height |
| +0xA0D + 2·slot | the site's 8 pillar unit ids |
| +0xC1F | controller type: 2 human, 1 computer/AI (confirmed, "Preacher conversion" - gates the whole per-tribe AI tick, not just mana), others unknown |
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
| person types | 0x5A7060 | 0x32 | `HUMAN_TRAIN_MANA_` +0x24, `CP_TRAIN_MANA_` +0x28, `MANA_F_` +0xE, `LIFE_` +0x10, `WOOD_` +0x14, `FIGHT_DAMAGE_` +0x1B, chop time (u8, not a named constant) +0x1D, `CONV_` +0x1F, tower detection radius +0x22, `SW_BLAST_DAMAGE_` +0x2C, per-type default/fallback next-state (the table elsewhere in this doc addressed as "0x5A7064") +0x4, flags +0x30 (bit 0x20: preacher/firewarrior-only conversion-target eligible, "Preacher conversion"'s `preacher_strike_scan`; bit 0x40: melee-ring eligible, `fight_pick_slot`/0x51DCC0, "Fight" - same byte, two unrelated readers), disguise-gate flags +0x31 (bit 0x4: brave/warrior/spy only, clear for preacher/firewarrior/shaman/angel/wildman - "Preacher conversion"), per-type auto-scan rhythm mask +0x2F (`class_counter` tested against this byte to gate the generic auto-attack-rhythm check, 0x4D4690 - "Preacher conversion"), default idle pose id +0x5 (read by `order_execute`'s "settle and go idle" fallback, 0x4E32A0 - "Idle behaviour") |
| building types | 0x5A7228 | 0x4C | slots +0x20 (u8), output person +0x31 (u8), `CONV_TIME_` +0x2C, `WOOD_` +0x1A, next size +0x34 (u8), growth target +0x36 (2400, not a named constant), mana factor +0x38, `MAX_POP_VALUE__HUT_` +0x3A, `HUT_SPROG_TIME` +0x3C, flags +0x48 (bit 0: trains; bit 10: breeds; bit 6: holds wood), foundation re-level rate +0x44 (2) and tolerance +0x46 (200), work-site capacity (u16, >>2 before use) +0x14 ("Idle behaviour" - `0x4D9B50`'s AI work-assignment scan) |
| scenery types | 0x5A79B0 | 0x18 | object +0x0A (12: 30), `TREE_WOOD_VALUE` +0x4, `TREE_WOOD_GROW` +0x6, `TREE_DORMANT_TIME` +0x8, flags +0x14 |
| movement profiles | 0x5A7B90 | 0x1A | speed +0x4 (`<TYPE>_SPEED`: brave profile 2, warrior 14, preacher 15, spy 16, firewarrior 17, shaman 18), velocity caps +0x8 / +0xA and +0xC / +0xE (horizontal / vertical: 192 / 512 for persons), `WALK_ALT_DIFF2` +0x10 (copied for flagged profiles), gravity +0x12 (32), ground friction +0x14 (28) / +0x16 (72), flags +0x18 |
| spells | 0x5A80D0 | 0x3E | kind +0x00 (1 normal, 2 hidden), cost (u32) +0x4, id +0x8, flags +0x1A (bit 0 castable) and +0x1B (0x40 homing), `SP_W_RANGE_` +0x1E, mode-0x20 range +0x22, shot without shaman +0x26, shot from shaman +0x27, effects +0x28..+0x2C, `SP_1_OFF_MAX_` +0x2D, mode-0x20 charges (`LSME_1_OFF_MAX_`) +0x2E, deflectable +0x2F, sound +0x30, enemy-cast alert +0x35/+0x36, `OPT_S` +0x3A |

- `LAND_BRIDGE_MAX_CHANGE`: record size 4, flags 0, value at 0x5AA518, exe default 256 (the file's is 256 too,
  spells.md "Effect constants").
- The shipped `constant.dat` wins over the exe defaults, e.g. `SPELL_BLAST` 18 000 in the exe and 10 000 in the
  file, `LIFE_BRAVE` 1400 and 1000.
- Not in the table: 0x5AA44C (default 3) is masked with the turn number like `MANA_UPDATE_FREQ` should be, so it is
  probably that constant (unverified). The `MANA_IDLE_*`/`MANA_BUSY_*` values are exe-only too.
