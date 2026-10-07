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
- [ ] When the shaman reincarnates at her site, the wildmen within range of it are converted into braves of her tribe for free, each with a conversion animation.
- [ ] Animate the wildmen: idle loops and gestures (original anims 2 gesture, 4 crouching) and wandering around their spot, instead of standing still.
- [ ] Low health: a star/crown spinning over the head of units low on health (original art to find); health bars only show on selected units.
- [ ] Selection of vehicles and buildings (without the people inside), and of units inside them once they exist (`selection::selectable`).
- [ ] Walking trails: footprints / worn paths left on the ground where units walk (fading over time).
- [ ] Flying: units thrown by Whirlwind / Blast / explosions follow a ballistic or carried path (flying 61, tumbling 73 anims), land with damage, drown if they land in the sea.
- [ ] Vehicles (boats, airships from their huts): board, carry units over water/land, unload.
- [ ] The shaman's health is lowered while she is in a vehicle and goes back to normal when she leaves it.
- [ ] The shaman can cast only some spells from a vehicle (list to define: fire-type probably yes, Land Bridge no).
- [ ] Drowning animation: the original drowning pose is not identified (tumbling is used).
- [ ] Unit shadows (shadow elements `0x4` are skipped; draw them flat on the ground).
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
- [ ] Load the list of enabled spells from the level (`.hdr`: available / discoverable / charges per tribe) into each tribe's `SpellBook`, instead of `demo_book`.

## Mana
- [ ] Mana per tribe in the simulation (integer, deterministic), shown in the HUD.
- [ ] Generated by braves in houses and by praying followers, instead of the fixed demo trickle.
- [ ] Spent on spell charges (the spell book already recharges from a mana amount per tick) and on training units in the training huts.
- [ ] Split between the two: how much of the tribe's mana goes to recharging spells vs training (original-like priority or a player setting).
- [ ] Swamp, Earthquake (`map::Lcg`), Volcano, Firestorm, Angel of Death, Armageddon...
- [ ] Spells through `Command::Cast` from the UI (the editor brushes apply directly).

## Things and level data ([level-format.md](docs/specs/level-format.md), [objects.md](docs/specs/objects.md))
- [ ] Decode the 55-byte thing record fully; spawn trees, buildings, totems from the level.
- [ ] Decode the `.hdr` beyond name/theme (spells/buildings availability, tribe count, sky).
- [ ] Verify the x/z axis order of level data.

## Level scripting (triggers)
- [ ] Analyse the original scripting mechanism: trigger things in the `.dat`, what links them to other things, their conditions (units in range, worship done, time...) and what they fire.
- [ ] Praying totems, stone heads, vault of knowledge, totem poles: which object triggers what (discover a spell, one-shot spell cast, unlock a building, raise land, reveal hidden things...), and whether it fires once or repeatedly.
- [ ] Check what the `cpscr*`/`cpatr*` files hold (computer player scripts/attributes) and how they relate to the level triggers.
- [ ] Document the findings in a spec and implement the triggers as deterministic simulation (through `Command`/state, no floats).

## Buildings ([buildings.md](docs/specs/buildings.md))
- [ ] Open-source building models (e.g. CC0 Quaternius Medieval Village / Fantasy kits) instead of the labelled boxes when the original files are not used; generated maps and sandboxes have buildings only in Sandbox > Buildings.
- [ ] Identify the original objects of reconversion, wall, gate, guard post (stand-in boxes even with the original files); check temple = prayer hut object.
- [ ] Villager hut style (3 styles in the objects, style 0 always drawn): find what picks it.
- [ ] Check the facing against the game (level 19's boat hut, facing 4, points its jetty into a low inlet: looks right), and the cells a footprint takes (units should not stand or walk through buildings).
- [ ] Sea level: only height 0 is sea in the simulation, but the original draws very low ground (height 1, e.g. the inlet by level 19's boat hut) like water; find the original's threshold.
- [ ] Buildings do not heal: damaged ones need repairs by braves, using wood.
- [ ] Construction, see [buildings.md#construction-planned](docs/specs/buildings.md): only hut (size 1), drum tower, training huts, boat/airship huts; never the reincarnation site, prison, vault, totems.
- [ ] Build tab: buildable kinds per tribe with `Availability` (Hidden / "?" / Available), from the level `.hdr` and triggers.
- [ ] Blueprint: white footprint draped on the ground following the cursor, door arrow, Space turns it; red where sea, too steep, another building/site or a tree with wood; red blocks placement.
- [ ] `Command::PlaceBuilding` (with the selected braves) and `Command::Assign`; extra braves beyond the maximum walk to the site and idle unassigned.
- [ ] Building stage (Site / Built / Dismantling) with wood needed / delivered / used; tooltip with braves assigned/max and wood, Dismantle toggle on built ones.
- [ ] Braves' build cycle: gather and watch, flatten the footprint by jumping point by point, fetch wood (ground piece or cut a tree) to a pile by the door, build one piece at a time.
- [ ] Wood dispatch rule: start with every assigned brave fetching while delivered + carried < needed; refine later.
- [ ] Site visuals: blueprint, then a wooden frame of the building's shape growing with progress, then the full building.
- [ ] Dismantling: braves remove one piece at a time, dropped as a wood piece (circle on the ground) near the door; the building disappears when empty and frees its ground.
- [ ] Wood pieces lying on the ground (`GameMap::wood`): picked up by any construction, drawn as small circles.
- [ ] Building costs: placeholder wood / max braves table in the spec; match the original values.
- [ ] Destruction of buildings on uneven/flooded ground (spells, erosion).
- [ ] Towers (drum tower): hold one unit, which gets a longer range from there: firewarriors throw farther, the shaman casts spells farther, a preacher converts enemies around the tower. No other tower effect.
- [ ] Training huts: warrior, firewarrior, preacher, spy training turns a brave into that unit (time + mana).
- [ ] Houses: hold a brave population, generate mana from the braves inside, spawn new braves over time.
- [ ] Houses grow (small -> medium -> large: `villager_hut` sizes), holding more braves. Growing has several criteria; one of them is a piece of wood brought by a brave (other criteria to find out).

## Wood ([trees.md](docs/specs/trees.md))
- [ ] Trees from original levels: decode their size/growth from the thing record if stored (all full size today); find which landscape themes use the tree objects 60-71 instead of 13-18; scenery models 7-9 (plants? stone heads?).
- [ ] A brave cuts one piece of wood at a time (`Tree::cut`): the tree shrinks by one, the brave carries the piece to a construction site or house.
- [ ] A tree does not grow back while a building stands on it; buildings can only be placed over size-0 (invisible) trees.
- [ ] Trees as obstacles for walking (around full trees?), to check against the original.

## Terrain ([terrain.md](docs/specs/terrain.md), [terrain-textures.md](docs/specs/terrain-textures.md))
- [ ] Curvature in a vertex shader sampling an R16 height texture (static grid).
- [ ] Dirty-rect re-bake of the texture/normals instead of full rebuilds.
- [ ] Theme sky; check the height -> colour row scale against the real game.
- [ ] Elevation rendering: low land is drawn like water. On original level 5 the reincarnation site looks like it stands in the sea, yet it is land units walk on. Fix the colour/water look of low ground so it matches what the simulation treats as sea (see also the sea-level item under Buildings).

## UI and editor ([ui-and-editor.md](docs/specs/ui-and-editor.md))
- [ ] Spell icons, tooltips; Build and Stats tabs (still "Coming soon").
- [ ] Editor: brushes under the mouse (`grounded::pick_ground` exists), brush radius UI, object placement.
- [ ] Editor: save back to the original `.dat` format.

## Computer players (AI)
- [ ] AI for the computer-controlled tribes: their shaman and followers act on their own through `Command`s (deterministic, like a player's input): gather wood, build and grow the village, train units, pray, cast spells, attack and defend.

## Multiplayer ([multiplayer.md](docs/specs/multiplayer.md))
- [ ] Turn scheduler with 2-3 turns input delay; local input goes through it instead of `GameMap::apply` directly.
- [ ] Host/join UI, lobby, reconnect.
- [ ] Desync checksum of heightmap + units.

## Art ([assets.md](docs/specs/assets.md))
- [ ] CC0 low-poly packs (Kenney / Quaternius) for the `--no-original` mode.
