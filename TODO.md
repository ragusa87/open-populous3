# TODO

What is left to implement. Tick or remove an item in the commit that does it, add new ones as they
come up. Details live in the linked specs; done work is summarised in [docs/roadmap.md](docs/roadmap.md).

## Menu and sandboxes ([ui-and-editor.md](docs/specs/ui-and-editor.md))
- [ ] Sandbox Spells: test ground to cast every spell freely (no mana, no charges).
- [ ] Sandbox Buildings: placing and constructing buildings on its free ground (braves, wood and trees are there; every model and every construction stage are shown).
- [ ] More sandboxes as features come (combat, vehicles...).
- [ ] Sandbox Worship: totems are drawn but nothing can be prayed at yet; the generated stand-ins are identical pillars.
- [ ] Sandbox Walk: the info line still shows "level 1/n" for sandbox maps.

## Camp fire ([buildings.md](docs/specs/buildings.md) "Camp fire")
- [ ] Guard posts' torches (objects 190-193, blended tile 92): draw them with `flame.rs` once guard posts have an original object (the firewarrior and prayer huts' torches burn).
- [ ] Generated firewarrior hut: drop the static ember cone from its braziers (tools/generate_buildings.py) now that an animated flame stands on them.
- [ ] Check in the game: what a camp fire is for (people gathering? mana?), its burn time when left alone (`ABANDON_TICKS`, 60 s guessed), the ring size, the flatness rule, and when object 12 (other logs) is used.
- [ ] Hover/selection of a camp fire (tooltip with who is around it), a hover cursor when units are selected over one.

## Units ([units.md](docs/specs/units.md))
- [ ] Vehicles: feet/hull on the ground and never cut by nearby slopes, like units (`units::toward_eye`).
- [ ] Vehicles ignore the walking slope speed (flying ones ignore the ground, the sea is always at height 0).
- [ ] Walk animation rate could follow the slope speed (slower steps uphill).
- [ ] Replanning: every walker replans on any terrain write; only replan when the change touches its route (`DirtyRect`) once there are many units. Group moves could share a flow field.
- [ ] Worshipping the shaman: check in the game the range (3 cells guessed) and the states she must be in (idle or walking today).
- [ ] Stranded units: find the game's anim (frame 1 of anim 12, the flattening jump, stands in today).
- [ ] Boarding vehicles: walk to `path::nearest_reachable` next to the boat/balloon, then board when it is within reach, through `game_core::enter` (`Unit::inside` naming a vehicle: turn its building corner into a holder enum). Boats path with `Mobility::Sail`, balloons with `Mobility::Fly`.
- [ ] Open-source units: models closer to the original look (feathers and staff for the shaman, tribal outfits); a prayer clip (SitDown's end stands in) and a swim/drown clip (RecieveHit sunk stands in). CC0 poses to render as each action lands: [unit-art.md, Planned poses](docs/specs/unit-art.md#planned-poses-cc0-stand-ins).
- [ ] CC0 sheets for the wood poses (`chop`, `carry_walk`, `carry_idle`: axe and log props, unit-art.md "Planned poses"); the braves show idle / walk meanwhile.
- [ ] Load open-source unit sheets (`assets/units/<kind>/<pose>.png`, see [unit-art.md](docs/specs/unit-art.md)): 64x64 cells, feet at (32, 58), 5 directions mirrored to 8, magenta key ramp swapped per tribe; fall back to the generated figure per missing sheet.
- [ ] `just bake-units` is not bit-reproducible: two bakes differ by 1-2 edge pixels in a few frames (GPU rasterisation), so there is no `--check` like the building kit's; render on the CPU or compare with a tolerance.
- [ ] Original unit animations: who the pointed-helmet attacker of 20-24 is, and anim 13; use the fighting (14 hit, 15 punch, 25 kick, 50 dodge and punch), throw-fire (7), spy fire-setting (17), tornado tilt (16), pedalling (51), running from bees (52) anims once those actions exist; idle for too long: braves scratch (89), warriors do push-ups (90/91).
- [ ] Their own behaviour: braves build/gather, warriors fight, preachers convert, spies disguise, firewarriors throw fire (today every kind only walks, prays, drowns and dies).
- [ ] Spies: the player can disguise one in another tribe's colour; it then walks through that tribe unnoticed, except when hovered (spy indicator) or when it passes by a spy of the tribe it imitates. Its attack on a building: it sets a fire (anim 17), walks away, the building then takes damage; the spy stays unnoticed and can attack again later.
- [ ] Dead units other than the shaman lie where they fell forever: remove them after a while (views are indexed by unit position: give them stable ids first).
- [ ] Wildmen (neutral "gaia" braves, spawned from the levels, standing still today): wander around, drink (anim 4), eat fruit from trees (anim 2); the Convert spell turns them into the caster's braves. Their eating and drinking anims are unused.
- [ ] When the shaman reincarnates at her site, the wildmen within range of it are converted into braves of her tribe for free, each with a conversion animation. Done at level start without animation (`ReincarnationSite::welcomes`); check the range in the game (the levels only use the ring of 8 cells) and whether it happens on every reincarnation.
- [ ] Animate the wildmen: eating fruit (2) and drinking (4), wandering around their spot, instead of standing still.
- [ ] Low health: a star/crown spinning over the head of units low on health (original art to find); health bars only show on selected units.
- [ ] Selection of vehicles and buildings (without the people inside), and of units inside them once they exist (`selection::selectable`).
- [ ] Walking trails: footprints / worn paths left on the ground where units walk (fading over time).
- [ ] Flying: units thrown by Whirlwind / Blast / explosions follow a ballistic or carried path (flying 61, tumbling 73 anims), land with damage, drown if they land in the sea.
- [ ] Vehicles (boats, airships from their huts): board, carry units over water/land, unload.
- [ ] The shaman's health is lowered while she is in a vehicle and goes back to normal when she leaves it.
- [ ] The shaman can cast only some spells from a vehicle (list to define: fire-type probably yes, Land Bridge no).
- [ ] Drowning animation: the original drowning pose is not identified (tumbling is used).
- [ ] Use the shaman's wand gesture (81-84), the conversion sit (18), the spy's juggling idle (92), the carbonized brave (97) for lightning; check the tribe layer flag `0x10` mapping.

## Praying (worship) ([worship.md](docs/specs/worship.md))
- [ ] Stone totem: check in the game what it does after a completion that is not its last, the timings (turn 2 s, hold 1 s, sink 4 s) and the smoke; other turns to try are listed in worship.md.
- [ ] Totem gifts not handled yet (logged as `Reward::Unhandled`): effects (land bridge, lightning...), revealed scenery and triggers, vehicles, the Angel of Death of type 5 totems; mana once mana exists. Check what `NumOccurences` 0 means.
- [ ] Pyramids: done for the player (`game_core::worship`); rewards per tribe once books are per tribe (today `granted` updates the shared level books and the player's panels); check `PrayTime`'s unit and the drain speed in the game.
- [ ] Prisons: levels where the shaman starts imprisoned; she is locked inside (units.md "Locked shaman") until freed.
- [ ] Generated pyramid: fold its capstone like the original's top (a separate mesh in `tools/generate_buildings.py`).
- [ ] Pyramids: draw the reward as an icon at the top (cursor sprites 38-57 spells, 58-65 buildings), gone once the reward is granted.
- [ ] HUD: show the gauge over the totem/pyramid and who is praying.

## Combat
- [ ] Fight logic: units engage enemies in range, melee exchanges, damage per unit type, death.
- [ ] Shaman fighting: strike (57) and kick (69) animations.
- [ ] Warriors: melee specialists, about 3-4x a brave's combat strength.
- [ ] Preachers convert enemies.
- [ ] Firewarriors (pyro-warriors) throw fire at enemies, both on the ground and in vehicles.
- [ ] Health, hit feedback and death animations for every unit type.
- [ ] Healing needs no action: every unit heals over time (the shaman already does, except while casting or drowning; make it the rule for all units). Exception: flying monsters (e.g. Angel of Death) never heal.

## Spells ([spells.md](docs/specs/spells.md))
- [ ] Spell effects with their own animations/visuals (lightning, swarm, whirlwind...); casting (C) only uses a charge and makes the shaman jump.
- [ ] Aim the other spells on the terrain like Teleport (`hud::spells::ground_spell`), with cast range from the shaman.
- [ ] Spell cursors: confirm 40 = Armageddon and 43 = Ghost Army in the game (`virtual_cursor::spell_sprite`); check the gold arrow's frame order and speed against the game; the pause menu releases the mouse, so it shows the system cursor, not the gold arrow.
- [ ] Burn (1), bloodlust (20) and the original teleport (21): what they do and who casts them (never on the player panel).
- [ ] Level spells per tribe in the simulation (today `GameMap::spell_book` is the player's loadout, shared): keep each discovery's availability (permanent / this level / one shot) for when it is discovered; `SpellsAvailableLevel` / `SpellsNotCharging` and `SpellsAvailableOnce`.
- [ ] A "once" spell discovery should give a single shot (`Provided { shots: 1 }`) when found (e.g. Convert in levels 4 and 5).

## Mana
- [ ] Mana per tribe in the simulation (integer, deterministic), shown in the HUD.
- [ ] Generated by braves in houses and by praying followers, instead of the fixed demo trickle.
- [ ] Spent on spell charges (the spell book already recharges from a mana amount per tick) and on training units in the training huts.
- [ ] Split between the two: how much of the tribe's mana goes to recharging spells vs training (original-like priority or a player setting).
- [ ] Swamp, Earthquake (`map::Lcg`), Volcano, Firestorm, Angel of Death, Armageddon...
- [ ] Spells through `Command::Cast` from the UI (the editor brushes apply directly).

## Things and level data ([level-format.md](docs/specs/level-format.md), [objects.md](docs/specs/objects.md))
- [ ] Start camera from the header's start position (cell) and angle (2048ths) instead of looking at the shaman.
- [ ] Level flags: fog of war (0x01, levels 9 and 18), shaman omni (0x02, level 25), no guest spells (0x10), no reincarnation time (0x20).
- [ ] Default allies per tribe (`.hdr` 92): alliances in the simulation (no attacking allies), e.g. level 14's three tribes against blue.
- [ ] `NoAccessSquares` (non-zero only in levl2002 and levl2079): probably cells nobody walks on; check them on the map and feed them to the `path` blocked mask.
- [ ] Load the object bank the level header names (byte 97: 6 for levels 3, 5, 16, 22, 2120; 7 for 2110; 2 for 2127) instead of always bank 0 (`original_models`): either load bank N with the index table of objects.md, or stay on bank 0 and pick the trees by bank (60-71). Bank 6 levels show the wrong trees today.
- [ ] Other original objects (trees, camp fire logs, reincarnation site stones) are still drawn from both sides: cull their back faces like buildings (objects.md "facs"), checking each for holes.
- [ ] Level names instead of the header's "Level N": `pop3_format::language` reads the texts ([language.md](docs/specs/language.md)); find which text number names each level (open there), find the install's `language/` dir like `data/`, pick the language (a setting, English by default).
- [ ] Our own texts in Fluent (`assets/lang/<lang>.ftl`, named keys), a key -> original text number table, lookup from `langNN.dat` with the original files and from our `.ftl` otherwise ([language.md](docs/specs/language.md) "Our texts and the fallback"); when the game first shows translatable text.
- [ ] Sunlight block (ShadeStart 28, ShadeRange 15, Inclination 32/64): find whether it drives the terrain lighting. `LandBlocks` / `LandOrients` are identical stale data: ignore them.

## Level scripting (triggers)
- [ ] Trigger semantics (layout and types known, level-format.md): what each type waits for (proximity, timed, player death, shaman proximity, library, shaman + angel of death), how `TriggerCount`, `NumOccurences`, `PrayTime` and `InactiveTime` play, and what activating a target does (discovery granted, effect fired, hidden thing revealed).
- [ ] Praying totems, stone heads (scenery 9), vault of knowledge: which trigger/discovery each uses, once or repeatedly.
- [ ] Level 5: a countdown (about 15 min) starts at some point; the winged death totem must be prayed at before it reaches 0. In the level: trigger #98, type 5 (shaman only, brings an Angel of Death), on cell 83, 65 (the start cell, marker 11), `PrayTime` 10, `CreatePlayerOwned` 1, targets effect 91 (same cell) and effect 88 (cell 106, 116). The timer is not in the level things: probably red's script cpscr058 (`SET_TIMER_GOING`, `HAS_TIMER_REACHED_ZERO`; the spec says level 5's script ends the level as won). Decompile it (`ai_script` above) to learn when the timer starts, what reaching 0 does and how the totem stops it; then implement its reward (not done yet, unlike the other totems).
- [ ] Implement the triggers and discoveries as deterministic simulation (through `Command`/state, no floats); AI scripts can fire them too (`TRIGGER_THING`).

## Buildings ([buildings.md](docs/specs/buildings.md))
- [x] Generated building kit (AI-written script) instead of labelled boxes for every named kind in generated mode: 16 reproducible GLBs, tribe accents, timber construction frames; see [kit workflow](assets/3d/buildings/README.md). Unknown IDs remain labelled boxes; see Sandbox > Buildings.
- [x] Generated procedural building textures: clay, timber grain, straw, stone and cloth; shared mipmapped atlas linked from every GLB, UVs preserved during construction.
- [ ] Generated buildings: coherent part-by-part assembly instead of height-sorted triangles, damage/fire/rubble, working-production animation, hut style variants and building add-ons.
- [ ] Check temple = prayer hut object. Reconversion, wall, gate, guard post have no object and no use in the levels: the generated models stand in even with the original files (objects.md).
- [ ] Villager hut style (3 styles in the objects, style 0 always drawn): find what picks it.
- [ ] Check the facing of the other kinds against the game (boat huts are settled: jetty local +z, door -z, from the 11 in the levels), and the cells a footprint takes (used by the walking mask).
- [ ] General model 9 "building add-on" (one near each of 91 medium/large huts): find what it is (hut extension?) and draw it.
- [ ] Sea level: only height 0 is sea in the simulation, but the original draws very low ground (height 1, e.g. the inlet by level 19's boat hut) like water; find the original's threshold.
- [ ] Buildings do not heal: damaged ones need repairs by braves, using wood.
- [ ] Construction, see [buildings.md#construction-planned](docs/specs/buildings.md): only hut (size 1), drum tower, training huts, boat/airship huts; never the reincarnation site, prison, vault, totems.
- [ ] Build tab: icons; build books per tribe in the simulation (today `GameMap::build_book` is shared), discoveries unlocking "?".
- [ ] Blueprint: tune `STEEP_SPREAD`; check the door side of the other kinds against the original objects.
- [ ] Construction pace: check in the game how long a jump and building one piece take (`JUMP_TICKS`, `JUMP_STEP`, `BUILD_TICKS`, guessed) and whether braves gather and watch first (a plan on level ground is flat at once today).
- [ ] Walls: replan only walkers whose route crosses a changed wall (every walker replans today); a boat hut's jetty side stays open to boats, to check with vehicles.
- [ ] Units inside a building are hidden but still selectable with a box; check what the original allows. Units walking out of several buildings: the door may be blocked by another wall or the sea.
- [ ] CC0 sheets: a jump pose for flattening (`Pose::Jump` falls back on arms up) and a hammer pose (`Pose::Hammer` falls back on idle).
- [ ] Find the original hammering anim (the axe swing, anim 11, stands in for `Pose::Hammer`).
- [ ] Huts: auto-housing of idle braves, breeding, mana from people inside, Eject (huts-and-training.md rules).
- [ ] Wood claimed across sites: two sites' braves can head for the same tree piece; braves of another task may take wood a site counts on. Refine the dispatch rule (nearest brave per piece, gatherers vs builders).
- [ ] Site visuals: views are redone whenever any building changes; update them per building (stable ids) instead; compare the part order (from the ground up) and the structure with the original; chimney position per hut model; busy look for the other kinds; the pile by the door could be drawn as a stack rather than loose pieces.
- [ ] Dismantling: braves remove one piece at a time, dropped as a wood piece (circle on the ground) near the door; the building disappears when empty and frees its ground.
- [ ] Wood pieces lying on the ground (`GameMap::wood`, drawn): picked up by any construction; hovering one could show "Wood". Maybe draw the original's shadow under them (`hfx0-0.dat` 22, unconfirmed).
- [ ] Destruction of buildings on uneven/flooded ground (spells, erosion).
- [ ] Damage from moving ground: check in the game what sets the damage (level 10's atlantis island comes back with its buildings missing 1 to 3 pieces of wood, no common percentage), and whether repairs need fetched wood ([buildings.md#damage-and-repair](docs/specs/buildings.md)).
- [ ] Effects 89 atlantis set / 90 atlantis invoke (level 10): sink the island at start, raise it back when the stone head trigger fires; 83 boat hut repair.
- [ ] Towers (drum tower): hold one unit (going in is done, `game_core::enter`), which gets a longer range from there: firewarriors throw farther, the shaman casts spells farther, a preacher converts enemies around the tower. No other tower effect.
- [ ] Training huts, see [huts-and-training.md](docs/specs/huts-and-training.md): queue around the door (folded snake), one follower inside at a time, transformed into the hut's kind, mana along the training, cancel by ordering it out.
- [ ] Training: check in the game how mana is taken, the training time (`CONV_*` = 4000?) and the cost by specialist count (`TRAIN_MANA_BAND`).
- [ ] Houses, see [huts-and-training.md](docs/specs/huts-and-training.md): green (birth) and red (grow) bars in the tooltip, a star animation at birth; capacity 3/4/5, supply 3/5/7 (cap min(supply, 200)), breeding by occupants and population band, mana from the people inside.
- [ ] Houses grow (small -> medium -> large: `villager_hut` sizes): red bar to 100 %, then if the population has room braves fetch wood and the hut becomes a construction site of the next size, still usable (huts-and-training.md rule 6). Growing takes 3 pieces (1 consumed, 2 added to the hut). Settle the red bar speed, the wood per size (3/5/7?) and who works on it before implementing.
- [ ] `game_core::balance`: `Turns` (original game turns) converted to ticks, `Balance::ours()` defaults, later `Balance::from_constants`; every rule reads it from `GameMap` (huts-and-training.md "Balance and game turns").

## Wood ([trees.md](docs/specs/trees.md))
- [ ] Original scenery models 7 plant 1, 8 plant 2: not drawn yet. Scenery 9 (the totems, 98 in the levels) is drawn as the stone head (object 82), a guess: find its real model. Trees have no size in the thing record: full size is right.
- [ ] Wood for repair and hut growth, like construction (`game_core::work`: assigned braves fetch, carry to the door, build). Cutting, carrying, fetching and construction are done (units.md "Wood", buildings.md "Construction").
- [ ] Check in the game how long a brave chops one piece (`CHOP_TICKS`, 6 s guessed) and whether a brave ordered on a tree with no wood to spare goes to another one.
- [ ] A tree does not grow back while a building stands on it; buildings can only be placed over size-0 (invisible) trees.
- [ ] Trees as obstacles for walking (around full trees?), to check against the original.

## Terrain ([terrain.md](docs/specs/terrain.md), [terrain-textures.md](docs/specs/terrain-textures.md))
- [ ] Curvature in a vertex shader sampling an R16 height texture (static grid).
- [ ] Dirty-rect re-bake of the texture/normals instead of full rebuilds.
- [ ] Check the height -> colour row scale against the real game, and how the original maps its sky (backdrop scrolling with the camera?).
- [ ] Cliffs: `cliff0-X.dat` turns land colours to rock by level (what picks the level: slope? damage?); `fade0-X.dat` palette light table (object/sprite shading, fog of war). Load both in `pop3_format::theme`.

## UI and editor ([ui-and-editor.md](docs/specs/ui-and-editor.md))
- [ ] Spell and building icons, tooltips.
- [ ] Stats tab (still "Coming soon"): the unit matrix of ui-and-editor.md "Stats tab": Selected, Idle, Housed, Working, In boat, In balloon by kind, with totals; clicks add units to the selection (Shift: all). Counting and picking done in `game_core::headcount`.
- [ ] Population strip and the Stats tab's huts box (ui-and-editor.md "Population"), from `GameMap::housing`.
- [ ] Vehicles fill the In boat / In balloon rows (`headcount::State::InBoat`, `InBalloon`, always 0 for now).
- [ ] Spies: hovering one shows a "spy indicator" (in the original a cursor with a punch, to check) instead of its disguise.

## Tooltips ([tooltips.md](docs/specs/tooltips.md))
- [ ] Rendered tooltip: done for buildings, trees and totems (`tooltip.rs`: name, slot rows, anchored above, sticky, clicks, right click at once); the toggle to add.
- [ ] People rows filled: totems' counted prayers (once praying exists), vehicles, the trainee (once training exists). Buildings, pyramids and totems' places are done, a click adds the unit to the selection.
- [ ] Wood rows: the pieces still needed on a growing house (once growth exists). Plans, sites, built buildings and trees are done.
- [ ] Bars fed by the simulation: totem prayer progress (once praying exists), house growth and birth, training (with their blocked state). The widget, its blinking and the pyramid's prayer bar are done.
- [ ] Dismantle toggle on the player's buildings that take wood.
- [ ] Vehicles: an unload button in the toggle's place, everyone aboard gets off; disabled with no walkable ground to land on (balloon over water or steep ground, boat away from the shore).
- [ ] Editor: brushes under the mouse (`grounded::pick_ground` exists), brush radius UI, object placement.
- [ ] Editor: save back to the original `.dat`/`.hdr`/`.ver` (rules in level-format.md "Writing levels": things packed from slot 0, 1-based trigger links, buildings on corners).
- [ ] Editor: smooth brush and the raise/lower levelling step of the ALACN editor (ui-and-editor.md).

## Computer players (AI)
- [ ] AI for the computer-controlled tribes: their shaman and followers act on their own through `Command`s (deterministic, like a player's input): gather wood, build and grow the village, train units, pray, cast spells, attack and defend.
- [ ] `pop3_format::ai_script`: parse `cpscrNNN.dat` (12 552 B only) into fields and a statement tree per [ai-scripts.md](docs/specs/ai-scripts.md) (grammar, token, parameter and internal variable tables are there), with a decompiled listing; `cpatr` name and masks. Then `just level-info` prints the scripts of the level's tribes decompiled next to its things, triggers and markers, so a level's events can be read in one place.
- [ ] `pop3_format::constants`: decode `levels/constant.dat` (XOR key, 2-byte marker, `P3CONST_` lines, duplicate `CONV_SPY` kept in order) per [constants.md](docs/specs/constants.md); then use the real balance values (spell costs, charges and cast range scaled by altitude band, see spells.md; unit life, speed and damage, wood, hut breeding) when an install is present, with our own defaults in generated mode.
- [ ] Script interpreter in `game-core`: user variables, internal variables read from the simulation (`headcount`: units by state and kind, `housing`), `EVERY n m` firing when `(turn + m) % n == 0` (stored minus 1), DO commands mapped to AI states and `Command`s; start with the states, `ATTACK` and `SET_SPELL_ENTRY`. Levels 2100, 2110, 2131 name missing scripts: fall back to no script.
- [ ] Script timing: measure the original turn rate (wiki: about 8 turns/s) against our 10 ticks/s, and whether `EVERY` adds a per-tribe phase, before porting script timings (see [ai-scripts.md](docs/specs/ai-scripts.md) "What this means here").
- [ ] Script side effects outside the AI: messages, flybys, `GIVE_ONE_SHOT`, `GIVE_MANA_TO_PLAYER`, `TRIGGER_LEVEL_WON/LOST`, user input lock (campaign scripts).

## Multiplayer ([multiplayer.md](docs/specs/multiplayer.md))
- [ ] Turn scheduler with 2-3 turns input delay; local input goes through it instead of `GameMap::apply` directly.
- [ ] Host/join UI, lobby, reconnect.
- [ ] Desync checksum of heightmap + units.

## Original sprites and blending ([sprites.md](docs/specs/sprites.md))
- [ ] Spell effects from the alpha sprites (`hfx0-0.dat`, decoded by `blend::AlphaTable`, blended on the GPU): map each effect to its frames (sprites.md).
- [ ] Ghost table `ghost0-X.dat` (66% mix) for see-through units (Invisibility).
- [ ] `sprites` parser: test the empty (0 x 0) entries and the `0x7F` trailer seen in the real banks.

## Sound ([sound.md](docs/specs/sound.md))
- [ ] `pop3_format::sound`: SDT banks (count, offsets, 40-byte headers), signed 16-bit PCM mono/stereo, the 4 looping sounds, `0x80` placeholders skipped; music `PopDrones22.SDT` as MPEG Layer II.
- [ ] Play SFX in the client (Bevy audio): spells, chopping, building, follower voices, ambiences; drums and music. Free sounds for `--no-original`.

## Art ([assets.md](docs/specs/assets.md))
- [x] Audit the original-data-free path and missing assets across buildings, units, scenery, VFX, UI and audio ([inventory](docs/specs/assets.md)).
- [ ] Next open art: building/spell icons, ritual scenery/reincarnation stones, vehicles and worker action clips (inventory P1). CC0 trees/unit sheets and the generated building kit already ship.
