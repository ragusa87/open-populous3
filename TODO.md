# TODO

What is left to implement. Tick or remove an item in the commit that does it, add new ones as they
come up. Details live in the linked specs; done work is summarised in [docs/roadmap.md](docs/roadmap.md).

## Performance (60 fps)
Measured with `just profile` (units sandbox, dev build, headless, tracing on): about 73 ms a frame. The simulation
(`run_ticks`) takes 0.014 ms a frame and all our systems together about 1 ms: the time goes to Bevy's own CPU work
around rendering (render app about 43 ms, extract and asset preparation about 10 ms, transform propagation in
PostUpdate about 5 ms), driven by how we hand it the world:
- [ ] Unit sprites: one image, material and quad mesh per animation frame (`units::upload_frame`), so each unit is its
  own draw call and bind group, nothing batches. One atlas texture and material per kind and tribe (frames as UV
  rects in a sprite material, instanced); the same for markers, dust and wood. Biggest win expected.
- [ ] Health bars: `animate_views` takes every fill's material mutably every frame (`mats.get_mut`), so all of them are
  prepared again each frame; write only when the colour changes, or share a material per colour step.
- [ ] Some image is prepared again every frame (`prepare_assets<GpuImage>` about 3 ms, its extract 1.6 ms): find which
  (the UI text atlas of readouts changing every frame? the cursor? the offscreen target?).
- [ ] Mesh allocator about 3 ms a frame: meshes rewritten every frame (range ring `put` while shown, blueprint marks)
  and the 129 x 129 terrain mesh rebuilt on every camera move (`world::rebuild_terrain`): rewrite only on change;
  the planet curve could move to the vertex shader over a static grid.
- [ ] Transforms written every frame for everything grounded (`grounded::place_grounded`) and every unit part
  (`units::pull_to_eye`), so Bevy propagates all trees, buildings and unit parts each frame: write with
  `set_if_neq`, or only when the camera or the thing moved.
- [ ] Unit views carry 7 marker slots and a health bar each, hidden most of the time: spawn markers when shown.
- [ ] Whole-world respawn on terrain changes: when the simulation levels ground (land bridge, flatten, braves
  flattening a site, every few ticks) `run_ticks` marks `CurrentMap` changed, and trees (re-reading their original
  meshes), sites, buildings, unit views, shadows and totems all despawn and rebuild. Those views should only
  re-ground on `world::TerrainPatches`, and rebuild on a real change of their list.
- [ ] Startup: the unit sprites take about 6.5 s (Scale2x twice on every frame of every kind and tribe), now a boot job
  off the main thread behind the loading screen (`units::UnitSpritesJob`); still to shorten: one job per kind to use
  every core, recolour tribes in the shader or a palette swap, or cache the result. Other loaders could become boot
  jobs (markers, dust, wood, tooltip icons, cursor: each under 10 ms today), and the map's own work at a game start
  (terrain bake, tree meshes) too.
- [ ] Measure the windowed release build too (`cargo run --release`): the numbers above are the dev build under
  tracing. The simulation only matters with many units: every walker replans A* on any terrain write
  ("Replanning" in Units).

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
- [ ] Health: read the lives from `constant.dat` once parsed (`KindStats::life` holds the file's values); find the wildman's life; check in the game the drowning and stranded rates (`DROWN_DAMAGE`, `STRANDED_DAMAGE`, scaled x20 from our old 100 HP guesses).
- [ ] Vehicles: feet/hull on the ground and never cut by nearby slopes, like units (`units::toward_eye`).
- [ ] Vehicles ignore the walking slope speed (flying ones ignore the ground, the sea is always at height 0).
- [ ] Walk animation rate could follow the slope speed (slower steps uphill).
- [ ] Replanning: every walker replans on any terrain write; only replan when the change touches its route (`DirtyRect`) once there are many units. Group moves could share a flow field.
- [ ] Worshipping the shaman: check in the game the range (3 cells guessed) and the states she must be in (idle or walking today).
- [ ] Stranded units: find the game's anim (frame 1 of anim 12, the flattening jump, stands in today).
- [ ] Boarding vehicles: walk to `path::nearest_reachable` next to the boat/balloon, then board when it is within reach, through `game_core::enter` (`Unit::inside` naming a vehicle: turn its building corner into a holder enum). Boats path with `Mobility::Sail`, balloons with `Mobility::Fly`.
- [ ] Open-source units: models closer to the original look (feathers and staff for the shaman, tribal outfits); a prayer clip (SitDown's end stands in) and a swim/drown clip (RecieveHit sunk stands in). CC0 poses to render as each action lands: [unit-art.md, Planned poses](docs/specs/unit-art.md#planned-poses-cc0-stand-ins).
- [ ] CC0 sheets for the wood poses (`chop`, `carry_walk`, `carry_idle`: axe and log props, unit-art.md "Planned poses"); the braves show idle / walk meanwhile.
- [ ] CC0 sheets for tumbling (`flung`: in the air, arms spread; `tumble`: falling down a slope); the fall loops meanwhile.
- [ ] Wildmen: find their death anim (47, falling down a slope, stands in).
- [ ] Load open-source unit sheets (`assets/units/<kind>/<pose>.png`, see [unit-art.md](docs/specs/unit-art.md)): 64x64 cells, feet at (32, 58), 5 directions mirrored to 8, magenta key ramp swapped per tribe; fall back to the generated figure per missing sheet.
- [ ] `just bake-units` is not bit-reproducible: two bakes differ by 1-2 edge pixels in a few frames (GPU rasterisation), so there is no `--check` like the building kit's; render on the CPU or compare with a tolerance.
- [ ] Original unit animations: who the pointed-helmet attacker of 20-24 is, and anim 13; use the fighting anims (pop3-rev-analysis.md "Combat": 15 move 2 and the opening blow, 13 move 3, 25 move 4, 14 hit, 16 struck before the flight; taunts 48/49/50 while waiting), throw-fire (7), spy fire-setting (17), tornado tilt (16), pedalling (51), running from bees (52) anims once those actions exist; idle for too long: braves scratch (89), warriors do push-ups (90/91).
- [ ] Their own behaviour: braves build/gather, warriors fight, preachers convert, spies disguise, firewarriors throw fire (today every kind only walks, prays, drowns and dies).
- [ ] Spies: the player can disguise one in another tribe's colour; it then walks through that tribe unnoticed, except when hovered (spy indicator) or when it passes by a spy of the tribe it imitates. Its attack on a building: it sets a fire (anim 17), walks away, the building then takes damage; the spy stays unnoticed and can attack again later.
- [ ] Death: check in the game how long the spirit rises (`DYING_TICKS` 1.5 s and `SPIRIT_TICKS` 3 s, guessed), whether the shaman's body shows a spirit too, and find the wildmen's death and spirit anims. Unit views are indexed by unit position and rebuilt whenever a unit leaves the map: key them by unit id once there are many units.
- [ ] Wildmen (neutral "gaia" braves, spawned from the levels, standing still today): wander around, drink (anim 4), eat fruit from trees (anim 2); the Convert spell turns them into the caster's braves. Their eating and drinking anims are unused.
- [ ] When the shaman reincarnates at her site, the wildmen within range of it are converted into braves of her tribe for free, each with a conversion animation. Done at level start without animation (`ReincarnationSite::welcomes`); check the range in the game (the levels only use the ring of 8 cells) and whether it happens on every reincarnation.
- [ ] Animate the wildmen: eating fruit (2) and drinking (4), wandering around their spot, instead of standing still.
- [ ] Selection of vehicles and buildings (without the people inside), and of units inside them once they exist (`selection::selectable`).
- [ ] Walking trails: footprints / worn paths left on the ground where units walk (fading over time).
- [ ] Statuses (`Unit::statuses`, set by no spell yet): the timers of invisibility, hypnosis and ghosts (shield and bloodlust have theirs, `status::Timed`), the hypnotized unit's original tribe, other options to define; set by the spells (Invisibility, Shield, Bloodlust, Hypnotise, Ghost Army) and read by combat, selection and drawing. Drawing: the markers are done (`units/markers.rs`); left: the low-health looping sound 0xCF, own invisible and ghost people translucent (invisible flickering faster near the end) and invisible enemies hidden (sprite, shadow, hover, selection), the original's colour tables (our shield tint is a guess), the optional health bar over own damaged people (commands 100/101).
- [ ] Physics (`game_core::physics::step`: velocity, gravity 32, touchdown; landing in the sea or on walkable ground done, `Unit::touch_down`), to the original's rules (pop3-rev-analysis.md "Blast push and flight"): velocity caps (192 per horizontal axis, 512 vertical), air drag 2 per turn per horizontal axis, the unit spinning 26° per turn in the air, a scream on take-off; landing: dust and a sound at a vertical speed <= -100, `FALL_OUT_OF_WW_DAMAGE` 700 at <= -200 (`Touchdown::speed`), none on water cells; on the ground: sliding down the slope (gradient over ±76 units) with friction 28 per turn per axis (72 on special terrain), facing where it slides (`Tumble` pose; a grounded tumbling unit does not move yet). Speeds are per original turn: convert to our ticks (`game_core::balance`). Ordinary falls (ground dropping under a standing or walking person: earthquake, erosion, flatten) fall with the same gravity and drag, without fall damage; rising ground lifts people. Still open: landing on a cliff or a building (stands up meanwhile), what the original does to a unit landing in water (ours dies at once), boats landing on the sea once vehicles exist, bumping into buildings and cliffs, a task dropped or kept when flung.
- [ ] Flying: units thrown by Whirlwind / Blast / explosions follow a ballistic or carried path (flying 61, tumbling 73 anims), land with damage, drown if they land in the sea.
- [ ] Vehicles (boats, airships from their huts): board, carry units over water/land, unload. A unit in a balloon stands at its height (`GameMap::altitude`), so a shaman casts farther from one. Boat and balloon huts make their vehicle like a training unit: the same mana store and dispatch (`game_core::production`) and tooltip bar, with more than one person inside, the vehicle spawned when the store reaches the cost.
- [ ] The shaman's health is lowered while she is in a vehicle and goes back to normal when she leaves it.
- [ ] Walking into range from a balloon: `GameMap::walk_to_cast` already plans with the caster's `Unit::mobility()` (`Fly` reaches any cell), so once she can board one, `mobility()` must say so (or the order go to the balloon carrying her), and the balloon fly to the closest point in range.
- [ ] The shaman can cast only some spells from a vehicle (list to define: fire-type probably yes, Land Bridge no).
- [ ] Drowning animation: the original drowning pose is not identified (tumbling is used).
- [ ] Use the shaman's wand gesture (81-84), the conversion sit (18), the spy's juggling idle (92), the carbonized brave (97) for lightning; check the tribe layer flag `0x10` mapping.

- [ ] The shaman's reincarnation is not instant: ours waits a fixed `RESPAWN_TICKS` (3 s). Find in the original how long she waits and what it depends on (her tribe's mana, its population, or both), then compute it from that.
- [ ] Training units: followers turned into warriors, preachers, spies and firewarriors in the training huts (see "Buildings": training huts and training lines).

## Praying (worship) ([worship.md](docs/specs/worship.md))
- [ ] Prayer gauge on the original's rhythm and curve (pop3-rev-analysis.md "The gauge"): every 4 turns, `C² / (C − n + 1)²` per update, drain `C²`; today it steps every tick with `STEP · (n/C)²`.
- [ ] Contested totems: keep one gauge per tribe (our choice; the original has one per trigger, driven by the tribe with the most prayers), but the gifts are shared: a totem's `NumOccurences` counts across all tribes, so a one-gift totem rewards only the first tribe to fill its gauge, and the other gauges stop. Once `NumOccurences` is reached by any tribe, the totem sinks back into the ground for everyone and every tribe's prayers are sent away (`Totem::is_exhausted`: one shared sink, whoever took the last gift). Check `Totem::given` does this and test it with two tribes.
- [ ] Totems: ids (`GameMap::new_id`) and removal from `GameMap::totems` once sunk (exhausted after their gifts, `NumOccurences` times; infinite ones stay), then `Order::Worship` / `Action::Worshipping` by id.
- [ ] Stone totem: check in the game what it does after a completion that is not its last, the timings (turn 2 s, hold 1 s, sink 4 s) and the smoke; other turns to try are listed in worship.md.
- [ ] Totem gifts not handled yet (logged as `Reward::Unhandled`): effects (land bridge, lightning...), revealed scenery and triggers, vehicles, the Angel of Death of type 5 totems; mana once mana exists. Check what `NumOccurences` 0 means.
- [ ] Pyramids: done for the player (`game_core::worship`); rewards per tribe once books are per tribe (today `granted` updates the shared level books and the player's panels); check `PrayTime`'s unit and the drain speed in the game.
- [ ] Prisons: levels where the shaman starts imprisoned; she is locked inside (units.md "Locked units") until freed.
- [ ] Generated pyramid: fold its capstone like the original's top (a separate mesh in `tools/generate_buildings.py`).
- [ ] Pyramids: draw the reward as an icon at the top (cursor sprites 38-57 spells, 58-65 buildings), gone once the reward is granted.
- [ ] HUD: show the gauge over the totem/pyramid and who is praying.

## The tick ([architecture.md](docs/architecture.md) "The tick")
- [ ] Find the original's order of work inside a turn (people, buildings, effects, shots, mana; income comes before spell charging): ours is a guess.
- [ ] Rhythms still on per-object counters or missing: stranded hurt (`STRANDED_HURT_EVERY`, rate unknown), hut breeding every 4 turns and growth every 16 (pop3-rev-analysis.md "Huts: breeding and growth"), mana income every 16 (`MANA_UPDATE_FREQ`), the other status timers every 8 (shield and bloodlust done).
- [ ] World events without behaviour yet: `Raid` (occupants thrown out). Panic: the screaming, and how the original turns at an obstacle.
- [ ] Burning buildings (`fire`): the igniter's tribe and the destroyed-building count (`stats[tribe][7]`), the fire sound.
- [ ] Shots (fireballs 8/4, 8/7, 8/8): a typed list ticked before effects like `GameMap::bolts`, integer flight, spawning a simple blast on arrival.
- [ ] World views (architecture.md "Client: world visuals"): the landing dust (`units::dust`) as a `WorldView` once
  touchdowns are kept in game-core (a unit's landing tick), so a rejoining client draws it too; the sinking totem's
  smoke once totems have ids; units, building views and the unit glide (`SimClock::prev_ids`) still keyed by index.
- [ ] Coordinators: the infrastructure is done ([coordinators.md](docs/specs/coordinators.md)); each kind to specify and implement (preacher's sermon, formation, guard control, soul convert; the challenge and the melee are done), with its scan in `GameMap::detect_coordinators`.
- [ ] The hurt path (`GameMap::damage`): the last attacker, the death cause, and death held off while tumbling, so no emitter repeats them.
- [ ] World events still to add with their kinds: `ChangeOwner { unit, tribe }` (conversion).
- [ ] Internal model 10/6: Preacher Attract (wiki) or a formation (pop3-rev-analysis.md "Combat"): decide from the original.

## Combat
- [ ] Melee (combat.md "Melee"): the ground check moving the middle off unsuitable ground, the winner's count and the killed count per tribe, the moves' sounds, the original's option with no shove, the third approach (a target at its building), the fight music or alert.
- [ ] Challenge (combat.md "Challenge"): the target facing the attacker and its taunts (client, 1 in 8 per turn while waiting), ground sliding instead of `SHOVE_LIFT`.
- [ ] Fight poses in the generated (CC0) unit sheets: strike, trade, kick, reel, shoved (`Pose::fallback` stands in: idle, fall).
- [ ] Spotting (combat.md "Spotting"): the preacher's conversion look (it fights what it sees meanwhile), the shaman's look during some orders, invisible people unseen, allies.
- [ ] Preachers convert enemies.
- [ ] Attackers inside a building walk out first (combat.md "Attacking": they set off from where they stand).
- [ ] Firebolts (combat.md "Firebolts"): the line of fire, from drum towers and vehicles, the smoke trail (our own) and the ground light, ghost firewarriors' bolts, the shaman inside a building or vehicle losing half her life, the angel's reaction.
- [ ] Health, hit feedback and death animations for every unit type.
- [ ] Healing (combat.md "Health"): no healing while struck by lightning once it has its own state; the angel of death never heals; the original lets life go past full by up to one heal (ours stops at full).

## Spells ([spells.md](docs/specs/spells.md))
- [ ] Spell effects with their own animations/visuals (swarm, whirlwind...; lightning has our own bolt, the original's bolt sprites are not identified); casting (C) only uses a charge and makes the shaman jump.
- [ ] Aim the other spells on the terrain like Teleport, Blast and Land Bridge (`hud::spells::ground_spell`), within the cast range (`GameMap::in_cast_range`, done: `SP_W_RANGE` by altitude band, +1/3 in a drum tower, none on god levels).
- [ ] Every spell flies from the shaman as a shot before it takes effect (6 turns into the cast; standard shot 1400 units per turn, blast's fireball 1000), homing on a targeted unit for the spells that follow one; burn, blast, lightning, whirlwind and insect shots are deflected 8 cells away by a shielded enemy on the target cell. A 1 s cooldown after any human cast, no per-spell cooldown (`SPELL_x_OPT_S` is not one).
- [ ] Blast (`game_core::blast`, an effect: pushes, and 50 damage per turn to other tribes), to the original (pop3-rev-analysis.md "Blast push and flight", "Components"): push on each of the 3 turns a person is still within 2.5 cells, cumulative (ours flings once); scale the horizontal push by the slope along it (`len ± len × clamp(|h|, 1, 16) / 16`, h over 32 units, at least 3); allies pushed only on the last turn, at most `BLAST_FRIEND_AFFECT` 5, never damaged; angels of death and units inside something never pushed; the fireball shot flying from the shaman first (1000 units per turn, 6 turns into the cast); buildings 40 once (buildings have no health yet); ghosts in the centre cell removed; the burn cell, smoke and explosion.
- [ ] Spell cursors: confirm 40 = Armageddon and 43 = Ghost Army in the game (`virtual_cursor::spell_sprite`); check the gold arrow's frame order and speed against the game; the pause menu releases the mouse, so it shows the system cursor, not the gold arrow.
- [ ] Burn (1), bloodlust (20) and the original teleport (21), now traced (pop3-rev-analysis.md "Per spell"): burn sets the target cell's trees alight and is not castable; bloodlust (hidden) gives the 6 nearest own followers ×3 damage, /8 damage taken and faster firewarrior reloads for 120 s; teleport (hidden) moves the caster's own shaman. Decide which ones we expose.
- [ ] Erode to the original's rules (pop3-rev-analysis.md "Per spell"; the land bridge and flatten are done, `land_bridge`, `flatten`): erosion as 63 hydraulic-erosion passes on 8 × 8 cells (ours: radius 4 lowered by 120). Flatten: trace the original's per-turn curve (ours divides the gap by the turns left) and whether the disc is centred on a point or a cell. The land bridge raises a smoke puff on every bridge cell every turn while it builds (a `WorldView` of the effect); balloons rise with it; its steps do not re-level or collapse buildings yet.
- [ ] Level spells: keep each discovery's availability (permanent / this level / one shot) for when it is discovered; `SpellsAvailableLevel` / `SpellsNotCharging` and `SpellsAvailableOnce`.
- [ ] A "once" spell discovery should give a single shot (`Provided { shots: 1 }`) when found (e.g. Convert in levels 4 and 5).

## Mana ([spells.md](docs/specs/spells.md) "Mana")
- [ ] Mana bar in the HUD for the player (`GameMap::mana`: `bank`, `received`, `used`).
- [ ] Generated by every follower, busy or idle (huts-and-training.md rule 7), instead of the fixed `mana::INCOME`.
- [ ] Delayed reservoir (gifts, shaman kills), shaman death losses (pop3-rev-analysis.md "Mana rules").
- [ ] A cast's mana cost: the original also takes the cost from `incoming` when casting; our charges are prepaid. Check whether a cast takes both.
- [ ] Spell share inside the distribution: the original also splits evenly between the learned, unblocked spells that are not full (pop3-rev-analysis.md "Distribution"); check where it puts the odd mana (ours: the first ones in book order).
- [ ] Spell charges from `SP_1_OFF_MAX` instead of `max_charges()` from the cost.
- [ ] Spent on training units: the stores are filled as in the original (`production::charge` in `ManaPool::distribute`: half of `incoming` before the spells, at most `cost / 32` each, a second share with every spell full, 100 a turn given back while making nothing). Left: a negative `incoming` (casts) taking mana back from the stores, the stores counted in the shaman death loss.
- [ ] Swamp, Earthquake (`map::Lcg`), Volcano, Firestorm, Angel of Death, Armageddon..., as traced in pop3-rev-analysis.md "Per spell" and "Components": swamp kills (not a surface type), the earthquake sinks a half-disc with a lava fissure, the volcano raises a cone with lava and throws 26 decorative rocks (pop3-rev-analysis.md "Volcano rocks": hfx 346-349 billboards rolling down its slope, client only), firestorm drops fireballs (each a simple blast that panics). Lightning is done (`lightning`) but its victims carbonized (anim 97). Burning trees (`Tree::burning`, 127 turns then burnt to size 0) and the ground fire (`ground_fire`, 48 turns) have guessed lengths: trace the original's scenery state 5 and effect 7/0x0A; plants, wood piles and fires burning too (burn cell 7/0x05).

## Tribes
- [ ] A tribe model in game-core: `TribeId` instead of bare `u8` owners (blue 0 .. green 3, wildmen 255), its colour
  in the engine instead of the client's `sites::tribe_color` / `procedural::tribe_rgb`, the tribes playing the level
  (header byte 88: 1v1, 1v1v1, 2v1...), allies (header byte 92, see "Default allies" below), human / computer,
  defeated; holding the player's mana pool, spell book and build book (today `GameMap::mana`, `spell_books`,
  `build_books` by owner).

## Winning and losing
- [ ] Conditions to win or lose a level: find them in the original (tribes wiped out, the shaman unable to reincarnate, level scripts and triggers), then end the game with a victory or defeat screen.

## Things and level data ([level-format.md](docs/specs/level-format.md), [objects.md](docs/specs/objects.md))
- [ ] Start camera from the header's start position (cell) and angle (2048ths) instead of looking at the shaman.
- [ ] Level flags: fog of war (0x01, levels 9 and 18), shaman omni (0x02, level 25; `GameMap::shaman_omni`: spells start from the view and have no range, the rest of god mode to find), no guest spells (0x10), no reincarnation time (0x20).
- [ ] Default allies per tribe (`.hdr` 92): alliances in the simulation (no attacking allies), e.g. level 14's three tribes against blue.
- [ ] `NoAccessSquares` (non-zero only in levl2002 and levl2079): probably cells nobody walks on; check them on the map and feed them to the `path` blocked mask.
- [ ] Load the object bank the level header names (byte 97: 6 for levels 3, 5, 16, 22, 2120; 7 for 2110; 2 for 2127) instead of always bank 0 (`original_models`): either load bank N with the index table of objects.md, or stay on bank 0 (trees already picked by bank, `tree_object_in_bank`).
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
- [ ] Attacking buildings (combat.md "Attacking a building"): enemies on a plan's cells count as its people; blasts (40) and firewarrior bolts (100) add wear; the flying faces of the partial explosion; the original's jitter is ±2 units (ours tilts); the player warned once.
- [ ] Ashes (combat.md "Ashes"): the hover text ("damaged ground", our words), the long damage smoke over them, the whirlwind's destruction, lava and earthquake ground as the same mark.
- [ ] Construction, see [buildings.md#construction-planned](docs/specs/buildings.md): only hut (size 1), drum tower, training huts, boat/airship huts; never the reincarnation site, prison, vault, totems.
- [ ] Build tab: icons, discoveries unlocking "?".
- [ ] Blueprint: tune `STEEP_SPREAD`; check the door side of the other kinds against the original objects.
- [ ] Construction pace: check in the game how long a jump and building one piece take (`JUMP_TICKS`, `JUMP_STEP`, `BUILD_TICKS`, guessed) and whether braves gather and watch first (a plan on level ground is flat at once today).
- [ ] Walls: replan only walkers whose route crosses a changed wall (every walker replans today); a boat hut's jetty side stays open to boats, to check with vehicles.
- [ ] Units inside a building are hidden but still selectable with a box; check what the original allows. Units walking out of several buildings: the door may be blocked by another wall or the sea.
- [ ] CC0 sheets: a jump pose for flattening (`Pose::Jump` falls back on arms up) and a hammer pose (`Pose::Hammer` falls back on idle).
- [ ] Find the original hammering anim (the axe swing, anim 11, stands in for `Pose::Hammer`).
- [ ] Huts: auto-housing of idle braves, breeding, mana from people inside, Eject (huts-and-training.md rules).
- [ ] Wood claimed across sites: two sites' braves can head for the same tree piece; braves of another task may take wood a site counts on. Refine the dispatch rule (nearest brave per piece, gatherers vs builders).
- [ ] Site visuals: views are redone whenever any building changes; update them per building (stable ids) instead; compare the part order (from the ground up) and the structure with the original; chimney position per hut model; busy look for the other kinds; the pile by the door could be drawn as a stack rather than loose pieces.
- [ ] Dismantling (switched by `Command::Dismantle`, which today only empties the building and stops its braves): braves remove one piece at a time, dropped as a wood piece (circle on the ground) near the door; the building disappears when empty and frees its ground.
- [ ] Wood pieces lying on the ground (`GameMap::wood`, drawn): picked up by any construction; hovering one could show "Wood". Maybe draw the original's shadow under them (`hfx0-0.dat` 22, unconfirmed).
- [ ] Destruction of buildings on uneven/flooded ground (spells, erosion), as in the original (pop3-rev-analysis.md "Terrain changes: what reacts"): after a terrain change near it, a building takes the average height of its footprint as its base, pulls the footprint cells within 200 back towards it, and collapses if a cell is more than 200 away; check the second destroy reason (flooding?). Flatten and the land bridge leave buildings floating or buried today. The check goes in `WorldEvent::Terrain` (`GameMap::apply_event`); cast terrain spells and the editor brushes write the heightmap directly today and must go through it too.
- [ ] Damage from moving ground: check in the game what sets the damage (level 10's atlantis island comes back with its buildings missing 1 to 3 pieces of wood, no common percentage), and whether repairs need fetched wood ([buildings.md#damage-and-repair](docs/specs/buildings.md)).
- [ ] Effects 89 atlantis set / 90 atlantis invoke (level 10): sink the island at start, raise it back when the stone head trigger fires; 83 boat hut repair.
- [ ] Towers (drum tower): hold one unit (going in is done, `game_core::enter`), which gets a longer range from there: firewarriors throw farther, the shaman casts spells farther, a preacher converts enemies around the tower. No other tower effect.
- [ ] Training huts, see [huts-and-training.md](docs/specs/huts-and-training.md): queue around the door (folded snake), one follower inside at a time, transformed into the hut's kind, mana along the training, cancel by ordering it out; the trainee and the queued units count as Working in the headcount.
- [ ] Training as in the original (pop3-rev-analysis.md "Training buildings"): cost `TRAIN_MANA[kind] × trained × BAND%[count of that kind]` (`HUMAN_` 3500/4000, `CP_` 1000; bands 0-3 100 %, 4-7 125 %, 8-11 150 %, 12-15 175 %, 16-20 200 %, 21+ 250 %); no duration, the trainee comes out when the building's store reaches the cost (at least 256-512 turns); the trainee is deleted and a new unit of the building's kind created at the door (new id, full life, queued orders and ghost flag kept) instead of changing its kind in place; `CONV_*` 4000 is not a training time. One trainee inside at a time from an unlimited queue, as in our design. Still to measure: the training time.
- [ ] Reincarnation sites from pop3-rev-analysis.md "Reincarnation sites": the ground levelled to the site height over a 5-cell disc in about 21 turns (ours: radius 3, blend 4) with its effects on enemies, wildmen, trees and swamps; pillars rising out of the ground, deleted and moved; the 32 sparkles.
- [ ] Houses, see [huts-and-training.md](docs/specs/huts-and-training.md): green (birth) and red (grow) bars in the tooltip, a star animation at birth; capacity 3/4/5, supply 3/5/7 (cap min(supply, 200)), breeding by occupants and population band, mana from the people inside.
- [ ] Houses grow (small -> medium -> large: `villager_hut` sizes): red bar to 100 %, then if the population has room braves fetch wood and the hut becomes a construction site of the next size, still usable (huts-and-training.md rule 6). Growing takes 3 pieces (1 consumed, 2 added to the hut). Settle the red bar speed, the wood per size (3/5/7?) and who works on it before implementing.
- [ ] `game_core::balance`: `Turns` (original game turns) converted to ticks, `Balance::ours()` defaults, later `Balance::from_constants`; every rule reads it from `GameMap` (huts-and-training.md "Balance and game turns").

## Wood ([trees.md](docs/specs/trees.md))
- [ ] Original scenery models 7 plant 1, 8 plant 2: not drawn yet. Scenery 9 (the totems, 98 in the levels) is drawn as the stone head (object 82), a guess: find its real model. Trees have no size in the thing record: full size is right.
- [ ] Wood for repair and hut growth, like construction (`game_core::work`: assigned braves fetch, carry to the door, build). Cutting, carrying, fetching and construction are done (units.md "Wood", buildings.md "Construction").
- [ ] Check in the game whether a brave ordered on a tree with no wood to spare goes to another one.
- [ ] A tree does not grow back while a building stands on it; buildings can only be placed over size-0 (invisible) trees.
- [ ] Trees as obstacles for walking (around full trees?), to check against the original.

## Terrain ([terrain.md](docs/specs/terrain.md), [terrain-textures.md](docs/specs/terrain-textures.md))
- [ ] Curvature in a vertex shader sampling an R16 height texture (static grid).
- [ ] Dirty-rect re-bake of the texture/normals instead of full rebuilds.
- [ ] Check the height -> colour row scale against the real game, and how the original maps its sky (backdrop scrolling with the camera?).
- [ ] Cliffs: `cliff0-X.dat` turns land colours to rock by level (what picks the level: slope? damage?); `fade0-X.dat` palette light table (object/sprite shading, fog of war). Load both in `pop3_format::theme`.

## UI and editor ([ui-and-editor.md](docs/specs/ui-and-editor.md))
- [ ] Spell and building icons, tooltips.
- [ ] Vehicles fill the In boat / In balloon rows (`headcount::State::InBoat`, `InBalloon`, always 0 for now).
- [ ] Spies: hovering one shows a "spy indicator" (in the original a cursor with a punch, to check) instead of its disguise.
- [ ] Original keymap ([shortcuts.md](docs/specs/shortcuts.md), dev mode off): add the missing shortcuts (F1, S status, groups 1-6, camera points Z/X/C/V, `\\` track, Shift+Enter, Space skips the fly-by), and a way to stop and cast self spells without the dev keys.

## Tooltips ([tooltips.md](docs/specs/tooltips.md))
- [ ] Rendered tooltip: done for buildings, trees and totems (`tooltip.rs`: name, slot rows, anchored above, sticky, clicks, right click at once); the toggle to add.
- [ ] People rows filled: totems' counted prayers (once praying exists), vehicles, the trainee (once training exists). Buildings, pyramids and totems' places are done, a click adds the unit to the selection.
- [ ] Wood rows: the pieces still needed on a growing house (once growth exists). Plans, sites, built buildings and trees are done.
- [ ] Bars fed by the simulation: totem prayer progress (once praying exists), house growth and birth, house growth and birth. The widget, its blinking, the pyramid's prayer bar and the training bar (double fill from `GameMap::production`) are done; the training bar moves once something sets a training unit's cost (the training itself).
- [ ] Unload button on a vehicle's tooltip (`catalog::UNLOAD_BUTTON`, same widget as the dismantle toggle, `tooltip::Toggle`), once vehicles exist.
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
- [ ] Network driver: send `Schedule::close_local`'s turn as `Message::Tick` every tick, feed peers' turns to `receive`, 2-3 ticks of delay (the schedule is done, single player only today).
- [ ] Host/join UI, lobby, reconnect.
- [ ] Desync checksum of heightmap + units.
- [ ] Pause and game speed as `Command`s, so every peer agrees (single player only today, `game_speed.rs`).

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
