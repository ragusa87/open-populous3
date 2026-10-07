# TODO

What is left to implement. Tick or remove an item in the commit that does it, add new ones as they
come up. Details live in the linked specs; done work is summarised in [docs/roadmap.md](docs/roadmap.md).

## Menu and sandboxes ([ui-and-editor.md](docs/specs/ui-and-editor.md))
- [ ] Sandbox Spells: test ground to cast every spell freely (no mana, no charges).
- [ ] Sandbox Buildings: placing and constructing buildings there (today it shows one of each).
- [ ] More sandboxes as features come (combat, vehicles, praying...).
- [ ] Sandbox Walk: the info line still shows "level 1/n" for sandbox maps.

## Units ([units.md](docs/specs/units.md))
- [ ] Vehicles: feet/hull on the ground and never cut by nearby slopes, like units (`units::toward_eye`).
- [ ] Vehicles ignore the walking slope speed (flying ones ignore the ground, the sea is always at height 0).
- [ ] Walk animation rate could follow the slope speed (slower steps uphill).
- [ ] Replanning: every walker replans on any terrain write; only replan when the change touches its route (`DirtyRect`) once there are many units. Group moves could share a flow field.
- [ ] Stranded units (not the shaman): arms-up animation for braves and others (the action exists in the simulation, shown as Idle).
- [ ] Boarding vehicles: walk to `path::nearest_reachable` next to the boat/balloon, then board when it is within reach. Boats path with `Mobility::Sail`, balloons with `Mobility::Fly`.
- [ ] Open-source units: models closer to the original look (feathers and staff for the shaman, tribal outfits); a prayer clip (SitDown's end stands in) and a swim/drown clip (RecieveHit sunk stands in).
- [ ] Load open-source unit sheets (`assets/units/<kind>/<pose>.png`, see [unit-art.md](docs/specs/unit-art.md)): 64x64 cells, feet at (32, 58), 5 directions mirrored to 8, magenta key ramp swapped per tribe; fall back to the generated figure per missing sheet.
- [ ] Original unit animations: confirm the spy outfit (`0x20/3`) and identify `0x20/2`; check the drown choices (52, preacher 42); use the attack, throw-fire, convert and worship anims once those actions exist.
- [ ] Their own behaviour: braves build/gather, warriors fight, preachers convert, spies disguise, firewarriors throw fire (today every kind only walks, prays, drowns and dies).
- [ ] Dead units other than the shaman lie where they fell forever: remove them after a while (views are indexed by unit position: give them stable ids first).
- [ ] Wildmen (neutral "gaia" braves, spawned from the levels, standing still today): wander around, drink, eat fruit from trees; the Convert spell turns them into the caster's braves. Their original anims beyond walk/stand/sit/flung (2 gesture, 4 crouching) are unused.
- [ ] When the shaman reincarnates at her site, the wildmen within range of it are converted into braves of her tribe for free, each with a conversion animation. Done at level start without animation (`ReincarnationSite::welcomes`); check the range in the game (the levels only use the ring of 8 cells) and whether it happens on every reincarnation.
- [ ] Animate the wildmen: idle loops and gestures (original anims 2 gesture, 4 crouching) and wandering around their spot, instead of standing still.
- [ ] Low health: a star/crown spinning over the head of units low on health (original art to find); health bars only show on selected units.
- [ ] Selection of vehicles and buildings (without the people inside), and of units inside them once they exist (`selection::selectable`).
- [ ] Walking trails: footprints / worn paths left on the ground where units walk (fading over time).
- [ ] Flying: units thrown by Whirlwind / Blast / explosions follow a ballistic or carried path (flying 61, tumbling 73 anims), land with damage, drown if they land in the sea.
- [ ] Vehicles (boats, airships from their huts): board, carry units over water/land, unload.
- [ ] The shaman's health is lowered while she is in a vehicle and goes back to normal when she leaves it.
- [ ] The shaman can cast only some spells from a vehicle (list to define: fire-type probably yes, Land Bridge no).
- [ ] Drowning animation: the original drowning pose is not identified (tumbling is used).
- [ ] Check the standing-still anims 81-84 and the tribe layer flag `0x10` mapping.

## Praying (worship)
- [ ] Praying only happens at a totem or a pyramid of knowledge; today `Order::Pray` works anywhere: restrict it.
- [ ] The player assigns units to pray there; each totem/pyramid has a gauge per tribe (tribes do not share progress).
- [ ] The gauge fills while that tribe's units pray; when full, the action triggers (discover a spell, a building, a totem effect...).
- [ ] Some totems need 1..x praying units for a decent fill time (more units = faster); some can only be used by the shaman.
- [ ] Nobody praying: the gauge drains, quite quickly.
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
- [ ] Spell cursors: map each spell to its gold icon in `POINT0-0.DAT` (38-66) in `virtual_cursor::spell_sprite`.
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
- [ ] Sunlight block (ShadeStart 28, ShadeRange 15, Inclination 32/64): find whether it drives the terrain lighting. `LandBlocks` / `LandOrients` are identical stale data: ignore them.

## Level scripting (triggers)
- [ ] Trigger semantics (layout and types known, level-format.md): what each type waits for (proximity, timed, player death, shaman proximity, library, shaman + angel of death), how `TriggerCount`, `NumOccurences`, `PrayTime` and `InactiveTime` play, and what activating a target does (discovery granted, effect fired, hidden thing revealed).
- [ ] Praying totems, stone heads (scenery 9), vault of knowledge: which trigger/discovery each uses, once or repeatedly.
- [ ] Implement the triggers and discoveries as deterministic simulation (through `Command`/state, no floats); AI scripts can fire them too (`TRIGGER_THING`).

## Buildings ([buildings.md](docs/specs/buildings.md))
- [ ] Open-source building models (e.g. CC0 Quaternius Medieval Village / Fantasy kits) instead of the labelled boxes when the original files are not used; generated maps and sandboxes have buildings only in Sandbox > Buildings.
- [ ] Identify the original objects of reconversion, wall, gate, guard post (stand-in boxes even with the original files); check temple = prayer hut object.
- [ ] Villager hut style (3 styles in the objects, style 0 always drawn): find what picks it.
- [ ] Check the facing of the other kinds against the game (boat huts are settled: jetty local +z, door -z, from the 11 in the levels), and the cells a footprint takes (used by the walking mask).
- [ ] General model 9 "building add-on" (one near each of 91 medium/large huts): find what it is (hut extension?) and draw it.
- [ ] Sea level: only height 0 is sea in the simulation, but the original draws very low ground (height 1, e.g. the inlet by level 19's boat hut) like water; find the original's threshold.
- [ ] Buildings do not heal: damaged ones need repairs by braves, using wood.
- [ ] Construction, see [buildings.md#construction-planned](docs/specs/buildings.md): only hut (size 1), drum tower, training huts, boat/airship huts; never the reincarnation site, prison, vault, totems.
- [ ] Build tab: icons; build books per tribe in the simulation (today `GameMap::build_book` is shared), discoveries unlocking "?".
- [ ] Blueprint: left click places it (`Command::PlaceBuilding`, `placement::can_place`); tune `STEEP_SPREAD`; check the door side of the other kinds against the original objects; construction sites block like buildings once they exist.
- [ ] At least one assigned brave is needed, up to the kind's maximum; more braves build faster (work shared).
- [ ] Buildings and sites block walking (blocked-cell mask in `path` next to the terrain); not for flyers, not the reincarnation site; braves assigned to a site (or dismantling) may walk on its footprint. Replan walkers when the mask changes, push units off a new footprint.
- [ ] `Command::PlaceBuilding` (with the selected braves) and `Command::Assign`; extra braves beyond the maximum walk to the site and idle unassigned.
- [ ] Orders: a new order to an assigned brave unassigns it (drops the wood it carries); braves working on the footprint cannot be selected on the map, only from the tooltip's brave icons (one at a time).
- [ ] Shift + click on a blueprint (no wood used yet) cancels it (`Command::Cancel`), its braves idle; once wood is used it can only be dismantled.
- [ ] Building stage (Site / Built / Dismantling) with wood needed / delivered / used; tooltip with braves assigned/max (one icon each) and wood, Dismantle toggle once wood is used (under construction or built).
- [ ] Braves' build cycle: gather and watch, flatten the footprint by jumping point by point, fetch wood (ground piece or cut a tree) to a pile by the door, build one piece at a time.
- [ ] Wood dispatch rule: a brave goes for wood only while delivered + claimed (fetched/carried by others) < needed, so at most needed - delivered braves are out; the others wait and build. Refine later.
- [ ] Site visuals: blueprint, then a wooden frame of the building's shape growing with progress, then the full building.
- [ ] Dismantling: braves remove one piece at a time, dropped as a wood piece (circle on the ground) near the door; the building disappears when empty and frees its ground.
- [ ] Wood pieces lying on the ground (`GameMap::wood`, drawn): picked up by any construction; hovering one could show "Wood". Maybe draw the original's shadow under them (`hfx0-0.dat` 22, unconfirmed).
- [ ] Destruction of buildings on uneven/flooded ground (spells, erosion).
- [ ] Damage from moving ground: check in the game what sets the damage (level 10's atlantis island comes back with its buildings missing 1 to 3 pieces of wood, no common percentage), and whether repairs need fetched wood ([buildings.md#damage-and-repair](docs/specs/buildings.md)).
- [ ] Effects 89 atlantis set / 90 atlantis invoke (level 10): sink the island at start, raise it back when the stone head trigger fires; 83 boat hut repair.
- [ ] Towers (drum tower): hold one unit, which gets a longer range from there: firewarriors throw farther, the shaman casts spells farther, a preacher converts enemies around the tower. No other tower effect.
- [ ] Training huts: warrior, firewarrior, preacher, spy training turns a brave into that unit (time + mana).
- [ ] Houses: hold a brave population, generate mana from the braves inside, spawn new braves over time.
- [ ] Houses grow (small -> medium -> large: `villager_hut` sizes), holding more braves. Growing has several criteria; one of them is a piece of wood brought by a brave (other criteria to find out).

## Wood ([trees.md](docs/specs/trees.md))
- [ ] Original scenery models 7 plant 1, 8 plant 2, 9 stone head (98 in the levels): not drawn yet. Trees have no size in the thing record: full size is right.
- [ ] A brave cuts one piece of wood at a time (`Tree::cut`): the tree shrinks by one, the brave carries the piece to a construction site or house.
- [ ] A tree does not grow back while a building stands on it; buildings can only be placed over size-0 (invisible) trees.
- [ ] Trees as obstacles for walking (around full trees?), to check against the original.

## Terrain ([terrain.md](docs/specs/terrain.md), [terrain-textures.md](docs/specs/terrain-textures.md))
- [ ] Curvature in a vertex shader sampling an R16 height texture (static grid).
- [ ] Dirty-rect re-bake of the texture/normals instead of full rebuilds.
- [ ] Check the height -> colour row scale against the real game, and how the original maps its sky (backdrop scrolling with the camera?).
- [ ] Cliffs: `cliff0-X.dat` turns land colours to rock by level (what picks the level: slope? damage?); `fade0-X.dat` palette light table (object/sprite shading, fog of war). Load both in `pop3_format::theme`.

## UI and editor ([ui-and-editor.md](docs/specs/ui-and-editor.md))
- [ ] Spell and building icons, tooltips; Stats tab (still "Coming soon").
- [ ] Editor: brushes under the mouse (`grounded::pick_ground` exists), brush radius UI, object placement.
- [ ] Editor: save back to the original `.dat`/`.hdr`/`.ver` (rules in level-format.md "Writing levels": things packed from slot 0, 1-based trigger links, buildings on corners).
- [ ] Editor: smooth brush and the raise/lower levelling step of the ALACN editor (ui-and-editor.md).

## Computer players (AI)
- [ ] AI for the computer-controlled tribes: their shaman and followers act on their own through `Command`s (deterministic, like a player's input): gather wood, build and grow the village, train units, pray, cast spells, attack and defend.
- [ ] `pop3_format::ai_script`: parse `cpscrNNN.dat` (12 552 B only) into fields and a statement tree per [ai-scripts.md](docs/specs/ai-scripts.md), with a decompiled listing in an example; `cpatr` name and masks.
- [ ] Script interpreter in `game-core`: user variables, internal variables read from the simulation, `EVERY` on the game turn (stored period - 1), DO commands mapped to AI states and `Command`s; start with the states, `ATTACK` and `SET_SPELL_ENTRY`. Levels 2100, 2110, 2131 name missing scripts: fall back to no script.
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
- [ ] CC0 low-poly packs (Kenney / Quaternius) for the `--no-original` mode.
