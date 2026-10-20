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
- [ ] More sandboxes as features come (combat...).
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
- [ ] Boarding vehicles: people of the same group within 440 units join the vehicle's group (pop3-rev-analysis.md "Boarding and driving"); the original's boarding clip (our jump uses the `Jump` pose, unit-art.md).
- [ ] Open-source units: models closer to the original look (feathers and staff for the shaman, tribal outfits); a prayer clip (SitDown's end stands in) and a swim/drown clip (RecieveHit sunk stands in). CC0 poses to render as each action lands: [unit-art.md, Planned poses](docs/specs/unit-art.md#planned-poses-cc0-stand-ins).
- [ ] CC0 sheets for the wood poses (`chop`, `carry_walk`, `carry_idle`: axe and log props, unit-art.md "Planned poses"); the braves show idle / walk meanwhile.
- [ ] CC0 sheets for tumbling (`flung`: in the air, arms spread; `tumble`: falling down a slope); the fall loops meanwhile.
- [ ] Wildmen: find their death anim (47, falling down a slope, stands in) - confirmed shared with warrior/preacher's death-anim group, not tied to any wildman-specific lifecycle (pop3-rev-analysis.md "Wildmen").
- [ ] Load open-source unit sheets (`assets/units/<kind>/<pose>.png`, see [unit-art.md](docs/specs/unit-art.md)): 64x64 cells, feet at (32, 58), 5 directions mirrored to 8, magenta key ramp swapped per tribe; fall back to the generated figure per missing sheet.
- [ ] `just bake-units` is not bit-reproducible: two bakes differ by 1-2 edge pixels in a few frames (GPU rasterisation), so there is no `--check` like the building kit's; render on the CPU or compare with a tolerance.
- [ ] Original unit animations: who the pointed-helmet attacker of 20-24 is, and anim 13; use the fighting anims (pop3-rev-analysis.md "Combat": 15 move 2 and the opening blow, 13 move 3, 25 move 4, 14 hit, 16 struck before the flight; taunts 48/49/50 while waiting), throw-fire (7), spy fire-setting (17), tornado tilt (16), pedalling (51), running from bees (52) anims once those actions exist; idle for too long: braves scratch (89), warriors do push-ups (90/91) - traced as far as a confirmed, rhythm-gated (16-turn) RNG-weighted picker (pop3-rev-analysis.md "Idle behaviour"), but only confirmed for wildmen (state 8 → 5) this session; whether ordinary followers share it is still open, so scope any implementation to wildmen first and extend if a later check confirms followers too. `Action::Idle` today has no animation-variant state or rhythm counter at all (`unit.rs`).
- [ ] Their own behaviour: braves build/gather, warriors fight, preachers convert, spies disguise, firewarriors throw fire (today every kind only walks, prays, drowns and dies).
- [ ] Spies: the player can disguise one in another tribe's colour; it then walks through that tribe unnoticed, except when hovered (spy indicator) or when it passes by a spy of the tribe it imitates. Its attack on a building: it sets a fire (anim 17), walks away, the building then takes damage; the spy stays unnoticed and can attack again later.
- [ ] Death: `SPIRIT_TICKS` (currently one guessed 3 s constant, `unit.rs`) is confirmed wrong as a single variant (pop3-rev-analysis.md "Death and the spirit rise"). The original spawns a separate spirit object with TWO variants, chosen by whether the corpse died in a drowning-eligible spot: a brief 4-turn (0.33 s) stationary "calm" rise (object id → VSTART 38 for followers, 85 shaman - not drawn in post as a translucent copy for this one specifically, re-check), or - unconditionally for a wildman regardless of cause of death, and for anyone else only if drowning - a 32-turn (2.67 s) "climbing" rise with a REAL continuous upward velocity (40 world units/turn, no gravity, VSTART 31 for a wildman reusing the "flung in the air" tumble clip, 40 followers, 45 shaman) plus, for a wildman only, a continuous spin (91/2048 facing per turn) found nowhere else - this is the "flying into the sky" look. Implement both variants and the wildman-always/others-if-drowning split; `DYING_TICKS` (the fall before the spirit spawns) was not re-examined this session. Unit views are indexed by unit position and rebuilt whenever a unit leaves the map: key them by unit id once there are many units.
- [ ] Wildmen (neutral "gaia" braves, spawned from the levels, standing still today): the original (pop3-rev-analysis.md "Wildmen") runs a confirmed, fully traced loop, state 1 → 8 → (5 or 6) → 16 → 8 → ...: from the idle tick (state 8) it FIRST searches up to 18 cells for a tree or plant (scenery types 1-6/7-8 specifically, confirmed from the exe's own scenery-type flags table) and, if one is found, idles beside it (state 5) - this is the "eating" stop. Only if no tree/plant is nearby does it fall back to a SECOND, also deliberate search (radius 12) for a shore cell (the same coastal-shape table used elsewhere for `is_land_at`, explicitly rejecting pure inland and pure open-sea cells), walks there (state 6, re-validating every turn that the destination is still shore-shaped) and idles there (state 16) - this is the "drinking" stop. There is still no hunger/thirst/food resource or stat anywhere - both searches only ever decide a DESTINATION for an idle/ambient animation, nothing is consumed. (Corrects an earlier, wrong pass through this code that called the destination "random, not a tree or water object" - it is deliberately one or the other.) Confirmed in play [observed]: always eating at the tree stop (state 5), always drinking at the shore stop (state 16) - the animation isn't a separate choice, it follows from which search succeeded; only the literal VSTART id each state resolves to for wildmen (already known from sprite inspection: eat-fruit=2, drink=4) is still not pinned to a specific line of code. The Convert spell turns them into the caster's braves (unchanged). No hunger/thirst/age death exists, but a wildman idling at its shore spot genuinely drowns fairly often [observed and traced] - not a special mechanic, the shore search only checks the coarse per-cell shape while the generic drowning check re-tests the unit's exact fine-grained position every turn with no bias toward the dry side (pop3-rev-analysis.md "Drowning"). Ordinary units' automatic auto-attack scan never targets a wildman (tribeless, tribe 0xFF) - only an explicit attack order, an area spell, or drowning at the shore can hurt one; whether an explicit attack order is honoured by the original is still open (see our blanket "unattackable" rule below). No periodic respawn exists in the original either: wildmen are placed once at level load and only ever decrease. Whatever kills it, a wildman's death always plays the "climbing + spinning" spirit-rise variant, never the calm ghost pose - see the new item below.
- [ ] When the shaman reincarnates at her site, the wildmen within range of it are converted into braves of her tribe for free, each with a conversion animation. Done at level start without animation (`ReincarnationSite::welcomes`); check the range in the game (the levels only use the ring of 8 cells) and whether it happens on every reincarnation.
- [ ] Animate the wildmen: eating fruit (2) and drinking (4), wandering around their spot, instead of standing still.
- [ ] Selection of buildings (without the people inside), and of units inside them (`selection::selectable`). Vehicles: a click with nothing selected selects the people aboard (vehicles.md).
- [ ] Walking trails: footprints / worn paths left on the ground where units walk (fading over time).
- [ ] Statuses (`Unit::statuses`: Magical Shield, Bloodlust, Invisibility and Hypnotism now cast for real,
  `GameMap::cast_nearest_status`/`hypnotize`; only Ghost Army's left unset by any spell): the timer of a ghost's
  status (not traced: no expiry found for Ghost Army), other options to define; read by combat, selection and
  drawing. Invisibility's own early end (attacking does, walking does not) is a guess (pop3-rev-analysis.md "Per
  spell" 6: "some actions end it early", not traced), implemented in `attack::go_attack`. Drawing: the markers are
  done (`units/markers.rs`); left: the low-health looping sound 0xCF, own invisible and ghost people translucent
  (invisible flickering faster near the end) and invisible enemies hidden (sprite, shadow, hover, selection), the
  original's colour tables (our shield tint is a guess), the optional health bar over own damaged people (commands
  100/101).
- [ ] Physics (`game_core::physics::step`: velocity, gravity 32, touchdown; landing in the sea or on walkable ground done, `Unit::touch_down`), to the original's rules (pop3-rev-analysis.md "Blast push and flight"): velocity caps (192 per horizontal axis, 512 vertical), air drag 2 per turn per horizontal axis, the unit spinning 26° per turn in the air, a scream on take-off; landing: dust and a sound at a vertical speed <= -100, `FALL_OUT_OF_WW_DAMAGE` 700 at <= -200 (`Touchdown::speed`), none on water cells; on the ground: sliding down the slope (gradient over ±76 units) with friction 28 per turn per axis (72 on special terrain), facing where it slides (`Tumble` pose; a grounded tumbling unit does not move yet). Speeds are per original turn: convert to our ticks (`game_core::balance`). Ordinary falls (ground dropping under a standing or walking person: earthquake, erosion, flatten) fall with the same gravity and drag, without fall damage; rising ground lifts people. Still open: landing on a cliff or a building (stands up meanwhile), what the original does to a unit landing in water (ours dies at once), boats landing on the sea once vehicles exist, bumping into buildings and cliffs, a task dropped or kept when flung.
- [ ] Flying: units thrown by Whirlwind / Blast / explosions follow a ballistic or carried path (flying 61, tumbling 73 anims), land with damage, drown if they land in the sea.
- [ ] Vehicle huts (vehicles.md "Vehicle huts", done with guesses): check in the game the crew size (4), the work per vehicle (1200 crew-turns), whether mana is spent, where the vehicle appears, and a hut's queue at the door (types 13-16 have one, pop3-rev-analysis.md "The queue at the door").
- [ ] Whirlwind spell (to implement), on a vehicle: everyone aboard is put out, it spins (roll +204, pitch +153 /2048 per tick) for 10 ticks (boat; ends early when the ground reaches it) or 2 (balloon), then explodes like a destroyed boat on land (pop3-rev-analysis.md "Life, damage and destruction", state 9).
- [ ] Swarm spell (to implement), on people aboard a vehicle: the bees put them out (`GameMap::put_out`): they fall out of a balloon; off a boat they drown, unless it is by the shore, where they get off onto the ground (`GameMap::unload_spots`).
- [ ] Vehicle destruction (vehicles.md; health, damage from blasts and bolts, sinking, rising and exploding done): the partial explosion of a boat on land (its faces flying off, `explosion.rs`).
- [ ] Balloons: bobbing while moving, and the original's climb rate and look-ahead; check the hover over trees and totems in the game (450 guessed; buildings 560 from the exe) (vehicles.md).
- [ ] The shaman's health is lowered while she is in a vehicle and goes back to normal when she leaves it.
- [ ] The shaman can cast only some spells from a vehicle: Land Bridge is refused (done); check the others in the game (fire-type probably yes).
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
- [ ] World events without behaviour yet: `Raid` (occupants thrown out; also needed for Earthquake's own building-shake, spells.md). Panic: the screaming, and how the original turns at an obstacle.
- [ ] Burning buildings (`fire`): the igniter's tribe and the destroyed-building count (`stats[tribe][7]`), the fire sound.
- [ ] Shots (fireballs 8/4, 8/7, 8/8): a typed list ticked before effects like `GameMap::bolts`, integer flight, spawning a simple blast on arrival.
- [ ] World views (architecture.md "Client: world visuals"): the landing dust (`units::dust`) as a `WorldView` once
  touchdowns are kept in game-core (a unit's landing tick), so a rejoining client draws it too; the sinking totem's
  smoke once totems have ids; units, building views and the unit glide (`SimClock::prev_ids`) still keyed by index.
- [ ] Coordinators: the infrastructure is done ([coordinators.md](docs/specs/coordinators.md)); each kind to specify and implement (preacher's sermon, formation, guard control, soul convert - see pop3-rev-analysis.md "Preacher conversion" for soul convert's traced numbers (`PREACHEE_CONV_FREQ`/`_CHANCE`, the population cap, the "no breeding" tribe bypass); the challenge and the melee are done), with its scan in `GameMap::detect_coordinators`.
- [ ] The hurt path (`GameMap::damage`): the last attacker, the death cause, and death held off while tumbling, so no emitter repeats them.
- [ ] Internal model 10/6 (and 10/1) resolved: they are a **formation** system (pop3-rev-analysis.md "Formations"), not Preacher Attract. A unit joins/creates one (`add_unit_to_formation`) whenever a still-untraced global flag is on and its current order kind "wants formation" - true for plain move (kind 3), false for attack (kinds 19/21). Up to 12 members are held at fixed per-slot offsets around a controller that jitters its own heading periodically. Not implemented at all today; `slots.rs`'s grid-based spot search for a group move has no equivalent in the original and isn't gated by anything - either replace it with (or gate it behind) a formation-shaped system, or keep it as a documented simplification.

## Combat
- [ ] Melee (combat.md "Melee"): the ground check moving the middle off unsuitable ground, the winner's count and the killed count per tribe, the moves' sounds, the original's option with no shove, the third approach (a target at its building), the fight music or alert.
- [ ] Challenge (combat.md "Challenge"): the target facing the attacker and its taunts (client, 1 in 8 per turn while waiting), ground sliding instead of `SHOVE_LIFT`.
- [ ] Fight poses in the generated (CC0) unit sheets: strike, trade, kick, reel, shoved (`Pose::fallback` stands in: idle, fall).
- [ ] `attack.rs`'s `wildmen_never_fight_and_nobody_attacks_its_own` test makes a wildman entirely unattackable, by anyone, any way. The original (pop3-rev-analysis.md "Wildmen") only confirms its *automatic* proximity auto-attack scan skips a tribeless (0xFF) target - whether an explicit player attack order also refuses a wildman target is unconfirmed; if the original does let a direct attack order through, our blanket rule is stricter than the original and should be relaxed to allow explicit attacks while keeping the auto-scan exclusion.
- [ ] Spotting (combat.md "Spotting"): the shaman's look during some orders, allies. Invisible people are already
  unseen by auto-targeting (`spotting::first_seen`); whether a player ordering an attack directly can still hit one
  is untraced, left open. The tribe flag at `+0x93D` bit `0x8` traced to was a dead end for this: it is "spells
  are free" (a cheat/debug flag, pop3-rev-analysis.md "Spending and other sources"), not "sees invisible people" -
  no tribe flag for seeing through invisibility was found this session; nothing grants it in the exe as far as
  traced, so there may simply be none (a live check with a revealed invisible unit against each tribe type would
  confirm there is truly no such flag before this item is dropped).
- [ ] Sermon (combat.md "Sermon"; pop3-rev-analysis.md "Preacher conversion" and "Command groups"): the preacher's own look scan and its preach order are now traced - the scan only ever issues an ordinary attack order or the disguise/building-visit order, never preach directly, and the preach order itself walks up to one target and ends by putting it in a generic "idle near a reference unit" state, not the "struck" state the conversion reroll actually runs in. The missing link is most likely one flag bit on the attack order that lets a preacher engage a non-preacher target at all (see "Preacher conversion"'s open questions) - still to trace (spec session): which code sets that bit, the building variant (preaching to a building's people), the soul flying from the preacher to the converted, the halving flag of the first check, how many listen at once and from how far, and whether the preacher's sermon really plays its 5 identified phases in order (`Pose::Preach` only draws one, anim 22, a guess) or just one.
- [ ] Attackers inside a building walk out first (combat.md "Attacking": they set off from where they stand).
- [ ] Firebolts (combat.md "Firebolts"): the line of fire, driving a vehicle towards an out-of-reach target (to trace: whether the original's halved speed of a passenger throwing aboard really slows its vehicle, ours does), the smoke trail (our own) and the ground light, ghost firewarriors' bolts, the shaman inside a building or vehicle losing half her life, the angel's reaction.
- [ ] Health, hit feedback and death animations for every unit type.
- [ ] Healing (combat.md "Health"): no healing while struck by lightning once it has its own state; the angel of death never heals; the original lets life go past full by up to one heal (ours stops at full).

## Spells ([spells.md](docs/specs/spells.md))
- [ ] Armageddon (full mechanic, `crates/game-core/src/armageddon.rs`; design in `docs/plans/spells.md`
      "Armageddon"; v1 only pauses, cancels fights, and settles instantly by combined headcount):
  - [ ] Clear the map: remove every building/totem/vehicle globally; clear trees within the arena radius only.
  - [ ] Tally headcount by kind before the wipe; spawn that many fresh fighters per tribe directly in the arena
        (grouped by team/wedge), not by teleporting each existing unit; relocate (don't respawn) each tribe's
        shaman.
  - [ ] Two-phase `ArmageddonMatch`: `Gathering` (camera orbit, combat held off) then `Fighting`.
  - [ ] Per-tribe permanent shield (`WorldEvent::Shield`, continuously refreshed) while a tribe's own fighters live.
  - [ ] Lightning-kill-without-reincarnation when a tribe's own fighters hit zero: `WorldEvent::Shield{steps:0}` +
        `WorldEvent::RemoveSite` + the existing `WorldEvent::Damage`/`DamageKind::Lightning`.
  - [ ] Related fix needed by the above: `Unit::tick`'s `Action::Dead` branch gets stuck forever (never
        reincarnates, never dies) when there's no `ReincarnationSite` for that owner — should fall through to
        `Action::Spirit` instead. Independent bug, not Armageddon-specific, but required before the lightning-kill
        can work.
  - [ ] Client: an `armageddon.rs` plugin beyond the v1 camera/lock — cast-confirmed screen flash, arena atmosphere
        tint, per-tribe survivor HUD bars (headcount visible as the decider), the continuous camera orbit (v1 only
        flies to the target and holds).
  - [ ] Dev/test hooks: `ARMAGEDDON_TICKS=N` to preview the full sequence headlessly (`just shot`/`just video`).
  - [ ] Replace `armageddon::best_side`'s placeholder "highest combined headcount wins instantly" decision rule
        with the real one (last side standing after the fight actually plays out).
- [ ] Bloodlust (hidden spell, model 20; its cast is implemented, docs/specs/spells.md): which levels give it (header
  bit 20 is among levels 1 and 2's leftover bits, so only discoveries give it today).
- [ ] Spell icons for `--no-original` (`assets/spell-hud`, `hud::spells::generated_glyph_path`): 14 of 20 drawn and wired in; still missing Convert, Volcano, Armageddon, Magical Shield, Teleport and Bloodlust (their names stand in). Building icons for `--no-original` are still unstarted.
- [ ] Find Magical Shield's and Bloodlust's original panel icons (none in `hfx0-0.dat` 1057-1074; Bloodlust's looks like a crane, in no PSFB bank nor the old `.DAT`/`.TAB` pairs; their names stand in with the original files too).
- [ ] Spell effects with their own animations/visuals (swarm, whirlwind...; lightning has our own bolt, the original's bolt sprites are not identified); casting (C) only uses a charge and makes the shaman jump.
- [ ] Aim the other spells on the terrain like Teleport, Blast and Land Bridge (`hud::spells::ground_spell`), within the cast range (`GameMap::in_cast_range`, done: `SP_W_RANGE` by altitude band, +1/3 in a drum tower, none on god levels).
- [ ] Every spell flies from the shaman as a shot before it takes effect (6 turns into the cast; standard shot 1400 units per turn, blast's fireball 1000), homing on a targeted unit for the spells that follow one; burn, blast, lightning, whirlwind and insect shots are deflected 8 cells away by a shielded enemy on the target cell. A 1 s cooldown after any human cast, no per-spell cooldown (`SPELL_x_OPT_S` is not one).
- [ ] Blast (`game_core::blast`, an effect: pushes, and 50 damage per turn to other tribes), to the original (pop3-rev-analysis.md "Blast push and flight", "Components"): push on each of the 3 turns a person is still within 2.5 cells, cumulative (ours flings once); scale the horizontal push by the slope along it (`len ± len × clamp(|h|, 1, 16) / 16`, h over 32 units, at least 3); allies pushed only on the last turn, at most `BLAST_FRIEND_AFFECT` 5, never damaged; angels of death and units inside something never pushed; the fireball shot flying from the shaman first (1000 units per turn, 6 turns into the cast); buildings 40 once (buildings have no health yet); ghosts in the centre cell removed; the burn cell, smoke and explosion.
- [ ] Spell cursors: confirm 40 = Armageddon and 43 = Ghost Army in the game (`virtual_cursor::spell_sprite`); check the gold arrow's frame order and speed against the game; the pause menu releases the mouse, so it shows the system cursor, not the gold arrow.
- [ ] Burn (1), bloodlust (20) and the original teleport (21), now traced (pop3-rev-analysis.md "Per spell"): burn sets the target cell's trees alight and is not castable; bloodlust (hidden) gives the 6 nearest own followers ×3 damage, /8 damage taken and faster firewarrior reloads for 120 s; teleport (hidden) moves the caster's own shaman. Decide which ones we expose.
- [ ] Erode to the original's rules (pop3-rev-analysis.md "Erosion passes"; the land bridge and flatten are done, `land_bridge`, `flatten`): 64 turns (5.3 s), one small steepest-descent pass per turn, each jittered within a few cells of the target and touching only a small local window - not a single instant, flat, radius-4, -120 drop as ours does today. The original follows each cell's own steepest of its 8 neighbours (diagonals distance-weighted), so the eroded shape hugs the real slope instead of being a uniform disc; the exact per-pass height delta is still untraced (open: finish 0x4983A0's write-back, or a live measurement on flat ground). Depends on the tree/water bullet above for trees left on ground eroded down to sea level. Flatten: trace the original's per-turn curve (ours divides the gap by the turns left) and whether the disc is centred on a point or a cell. The land bridge raises a smoke puff on every bridge cell every turn while it builds (a `WorldView` of the effect); balloons rise with it; its steps do not re-level or collapse buildings yet.
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
- [x] Earthquake and Firestorm (`spells.md`): Firestorm's scheduled `panicking_blast` fireballs are done in full; Earthquake's shake-and-sink is done for its terrain part (sink + lava fissure, reusing `lava`/the `raise` brush) but still missing its three building-shake checks (needs `Raid`, below) and the trace's "possibly two lava flows". Angel of Death, Armageddon (full mechanic, see below)... still to do, as traced in pop3-rev-analysis.md "Per spell" and "Components". None of Earthquake's lava, Volcano's lava/turn-110 kill or Swamp route through `effect::hurts`/the shield check: a shielded unit still drowns or burns when the ground under it changes (reported, not traced), unlike a blast, lightning, swarm or hypnotise. Lightning is done (`lightning`) but its victims carbonized (anim 97). Burning trees (`Tree::burning`, 127 turns then burnt to size 0) and the ground fire (`ground_fire`, 48 turns) have guessed lengths: the original's own ground-fire object (scenery class 5 type 0xA, not an effect — "Fireball impact") gets its burn countdown from 0x4A8C60 with param 22, the exact resulting turn count not yet decoded; trace that function and the original's scenery state 5; plants, wood piles and fires burning too (burn cell 7/0x05).
- [x] Firestorm's own look (pop3-rev-analysis.md "Firestorm fireballs"): fixed — `fireball_shot.rs` [client] gives `panicking_blast`'s own effect a view of its own (both Firestorm's and Volcano's fireballs, which share it): a bright ball falling from above, shrinking as it nears the ground, a few dimmer embers trailing behind it (not `smoke.rs`'s grey puffs), landing in a flash that grows and fades into a last puff of smoke; `ground_fire.rs`'s standing flame is unchanged. Our own invented flight and look throughout (the original's hfx 1120-1127/90-93/~1180-1210 sprites, confirmed by eye last session, are not traced closely enough to follow pixel for pixel). `panicking_blast::VISUAL_TICKS` keeps the effect (and so this view) alive well past its own 3-turn blast purely so the fall/trail/burst has room to play out — the blast's own damage/push/panic are unaffected (already a no-op past turn 3). Not yet: the same view wired up for Blast's own `SimpleBlast` fireball (TODO.md "Blast push and flight").
- [ ] Firestorm (and Blast) fireballs landing on water (pop3-rev-analysis.md "Fireball impact"): the original checks the landing cell's terrain type against the coastal-shape table and, on water, skips the fire entirely and plays an unnamed splash/hiss effect instead (sound 0x55, never traced in detail); `panicking_blast`/`ground_fire` always spawn regardless of what the point is over today, so a fireball landing in the sea still starts a (pointless) ground fire there.
- [x] Volcano/Earthquake lava (`lava.rs`, `volcano.rs`; pop3-rev-analysis.md "Volcano lava flow"): fixed — `Lava::turn` and Volcano's own turn-110 destruction now catch fire (`WorldEvent::Ignite`/`Unit::catch_fire`, a new person state: loses `BURN_DIVISOR` of its own max health a turn for up to `BURN_TICKS`) instead of an outright kill. The flow (`lava::flowing`) now grows along each cell's real downhill neighbour, capped by `GROWTH_TURNS`/an accumulator, then scorches the whole (now fixed) footprint into permanent ash (`WorldEvent::Scorch`) right away rather than waiting for `FADE_TURNS` of fading to finish — those turns are now only a visual cooldown, `lava.rs` [client]'s own alpha fade revealing the already-existing ash mark underneath instead of bare ground that used to turn to ash in one pop on the last tick; the original's own per-cell direction is now traced too ("Volcano lava flow") - it is frozen at the last height edit by a general terrain routine, not recomputed live every turn like `downhill` does, which could let a channel's direction drift turn to turn near actively-rising/falling ground, unlike the original's. The static fissure (`lava::effect`, "lava square") decays and self-destructs on its own blend clock, scorching its cells into the same permanent ash once they're gone (all at once: they share one blend clock), same as the flow's own. Left as a guess, not traced: `Unit::catch_fire`'s own movement reuses Panic's ("runs about" is all the trace gives); confirm separately if a spec session ever traces state 31's own movement.
- [x] Lava's own "pour-in" look (`lava.rs` [client]): fixed — a cell an actual spread step just added (`game_core::lava::CellArrival`, the tick it arrived and the direction back to the neighbour it spread from) briefly flashes a directional highlight sweeping from the corner facing that neighbour (`pour_in_alpha`), before settling into the existing blend-based fade: without it, cells only ever lit up in place, one tile after another, instead of reading as a single flow pouring downhill from tile to tile. On top of that flash, the cell's own UVs now keep drifting upstream for as long as it carries its `arrival` (`flow_offset`, effectively its whole life, `grow` never clears it): the molten texture itself visibly creeps downhill the whole time a cell is hot, not just in the brief moment it arrives — closer to how the original reads even though its own spread is a thin single-direction channel per cell, not a filled area (`game_core::lava::grow`). That single-direction spread also leaves a checkerboard of plain, untouched ground cells fully surrounded by hot ones; `fill_gaps` draws those too, client-side only, each at its hottest touching neighbour's own alpha and drift (not dimmed — the same lava, not a fake halo), so the patch reads solid. Client-side only, same "our own invented flight/look" spirit as `volcano_rocks.rs`.
- [x] Ground Shake rewritten as a camera shake (`ground_shake.rs` [client]): fixed — it was a trembling terrain-mesh patch, which can look like the ground itself glitching underfoot; now a kick and a roll on the camera itself (`camera_shake`, chained strictly after `camera::apply_rig` so it never feeds back into that system's own frame-to-frame smoothing), summed over every simultaneously active `GroundShake` effect so two quakes shake harder together. `game_core::effect::GroundShake`'s own intensity/radius curves (Volcano's rumble, Earthquake's tremor) are unchanged.
- [x] Volcano's fireballs now land on their own traced window (`volcano.rs`; pop3-rev-analysis.md "Volcano rocks": `ebp` in `[0x23,0x46]` + 80 = cast turns 115-150, distinct from the rocks' own 105-130): fixed — they used to start only once the crater finished rising (`SHAPING_TURNS`, 139) and run to the controller's own end (160); now `FIREBALL_START`/`FIREBALL_END` spread them 115-150 instead, starting while the cone is still rising, overlapping but not matching the rocks' window.
- [ ] Swamp (`swamp`): its kill and overlap-claiming (ours: the most recently cast one on a contested cell) are done; still missing is the 30-per-tribe, oldest-goes bookkeeping and ending once most of its cells are unsuitable (pop3-rev-analysis.md "Per spell" 11), and any world or aerial-map mark (no sprite, hfx 81; pop3-rev-analysis.md "Swamp: drawing, and the reedy-grass / mist ground dressing") — client only.
- [ ] Bug, client only, found wiring up `lava::LavaView`: `just shot`/`HEADLESS=1 SCREENSHOT=...` with a dev var that instantly runs many ticks at once (e.g. `VOLCANO_TICKS=139` or more) can leave the terrain's own ground mesh and texture (`world.rs`'s `rebuild_terrain`) frozen on its pre-cast, flat state forever, while anything reading `CurrentMap` live (confirmed with `LavaView`'s own draped mesh) correctly shows the real, risen terrain — so the two visibly disagree (a textured lava patch floating over flat "ocean"). Confirmed: the simulation's own `GameMap.terrain` is correct and stable the whole time (direct `GameMap`-only test, no client); `just video` from a live-ticking start does not show it (its terrain updates correctly as Volcano rises, `target/video.mp4` frames checked). Only reproduced via the instant bulk `map.run(N)` dev-var path with a static `FOCUS`; happens consistently right around `volcano::SHAPING_TURNS` regardless of `VOLCANO_TICKS`'s exact value, survives disabling Volcano's lava/fireball spawn there (so it's not those effects specifically), and persists at `SHOT_FRAME` up to 200. Likely in `rebuild_terrain`'s dirty-tracking (`TerrainDirty`/`TerrainPatches`, `TerrainPatches` itself is otherwise never written anywhere) or in how `dev.rs`'s per-var instant catch-up interacts with the normal per-frame `SimClock` catch-up, not yet root-caused.

## Tribes
- [ ] A tribe model in game-core: `TribeId` instead of bare `u8` owners (blue 0 .. green 3, wildmen 255), its colour
  in the engine instead of the client's `sites::tribe_color` / `procedural::tribe_rgb`, the tribes playing the level
  (header byte 88: 1v1, 1v1v1, 2v1...), allies (header byte 92, see "Default allies" below), human / computer,
  defeated; holding the player's mana pool, spell book and build book (today `GameMap::mana`, `spell_books`,
  `build_books` by owner).

## Winning and losing
- [x] A generic, hookable `Outcome` (`game_core::outcome`): a tribe/side wiped out (by current owner, team-aware
  via `tribes.rs`) is detected automatically every tick and declares a winner (or a draw); `GameMap::declare_outcome`
  is the one entry point anything else (Armageddon, below, or a future level-script interpreter) can also call.
- [ ] Level scripts and triggers also ending the game (`TRIGGER_LEVEL_WON`/`LOST`, timers) — needs the PopScript
  interpreter below; will call the same `GameMap::declare_outcome` hook, not a separate mechanism.
- [x] A victory/defeat screen in the client (`game_end.rs`, `AppState::Ended`): headline, reason, every tribe's
  survivor count; `GAME_END=victory|defeat|draw` and `GAME_END=eliminate:<tribe>` dev hooks. Wording/art are our
  own (AGENTS.md licence) — not traced from or compared against the original's screens.
- [x] Once the match ends, every unit still acting stands down to Idle right away (a death already under way
  finishes on its own, and a lone shaman never gets a last-stand reincarnation out of it, `GameMap::tick`'s own
  freeze) and the economy/combat/wrecking/reincarnation ticks stop for the rest of the game (`outcome.rs`,
  `map.rs`'s `frozen`, same freeze `armageddon.rs` already used) — corrects the earlier "keeps running, not
  frozen". Only the winning side's living units (and its allies, `Tribes::same_team`) are kept shielded
  (`GameMap::shield_everyone_once_ended`/`on_winning_side`, `outcome.rs`) — directly sets `Unit::statuses.shield`,
  not a real cast of Magical Shield (now implemented, `GameMap::cast_nearest_status`, docs/specs/spells.md); the
  losing side gets none, and a draw shields nobody. Left as a direct status poke rather than reusing the spell's
  own cast path, since this epilogue shields a whole side at once, not the nearest 6: a different shape, not the
  same mechanic under another name.
- [x] A "victory lap" visual once the match ends: every unit on the winning side (`GameMap::on_winning_side`) plays
  a small repeating happy jump (`units/mod.rs::victory_bounce`/`bounce_start`, added to `pull_to_eye`'s lift) — the
  losing side doesn't celebrate. There's no such animation traced in the original's identified sprites, so this is
  our own invented stand-in, not a reproduction.
- [ ] The client assumes the local player is always tribe 0/blue (`units::PLAYER`, `units/mod.rs:36`) throughout —
  multiplayer needs this determined per-session instead; affects win/lose presentation (`game_end::verdict`'s
  `local`), spell casting, camera, and HUD alike. Pre-existing, not introduced by the win/lose work above.

## Things and level data ([level-format.md](docs/specs/level-format.md), [objects.md](docs/specs/objects.md))
- [ ] Start camera from the header's start position (cell) and angle (2048ths) instead of looking at the shaman.
- [ ] Level flags: fog of war (0x01, levels 9 and 18), shaman omni (0x02, level 25; `GameMap::shaman_omni`: spells start from the view and have no range, the rest of god mode to find), no guest spells (0x10), no reincarnation time (0x20).
- [x] Default allies per tribe (`.hdr` 92) read into `game_core::tribes::Tribes` and used for: elimination/win-lose
  grouping by side, and the two places that gate whether an attack may start (`attack::attack_goal`,
  `spotting::first_seen`) — allies no longer spot or attack each other. Still open: the rest of the alliance-aware
  nuances below (Blast's own friend-sparing rules, spotting during orders, preacher conversion targeting).
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
- [ ] Ashes (combat.md "Ashes"): the hover text ("damaged ground", our words), the long damage smoke over them, the whirlwind's destruction. Lava and earthquake ground sharing the mark is done (`lava.rs`'s `WorldEvent::Scorch`, both `Mode::Static` and `Mode::Flowing`).
- [ ] Construction, see [buildings.md#construction-planned](docs/specs/buildings.md): only hut (size 1), drum tower, training huts, boat/airship huts; never the reincarnation site, prison, vault, totems.
- [ ] Build tab: icons, discoveries unlocking "?".
- [ ] Blueprint: tune `STEEP_SPREAD`; check the door side of the other kinds against the original objects.
- [ ] Construction pace: check in the game how long a jump and building one piece take (`JUMP_TICKS`, `JUMP_STEP`, `BUILD_TICKS`, guessed) and whether braves gather and watch first (a plan on level ground is flat at once today).
- [ ] Walls: replan only walkers whose route crosses a changed wall (every walker replans today); a boat hut's jetty side stays open to boats, to check with vehicles.
- [ ] Units inside a building are hidden but still selectable with a box; check what the original allows. Units walking out of several buildings: the door may be blocked by another wall or the sea.
- [ ] CC0 sheets: a jump pose for flattening (`Pose::Jump` falls back on arms up) and a hammer pose (`Pose::Hammer` falls back on idle).
- [ ] Find the original hammering anim (the axe swing, anim 11, stands in for `Pose::Hammer`).
- [ ] Huts: auto-housing of idle braves, breeding, mana from people inside, Eject (huts-and-training.md rules). No autonomous wood-gathering by an idle person was found this session (pop3-rev-analysis.md "Idle behaviour"): a person with no command group does nothing at all, forever; the one confirmed autonomous, non-player assignment (a training building's own per-turn code, 0x405B80) looks like occupant/queue positioning, not wood-fetching, pending its unread "doing it" half - don't add an idle-brave-fetches-wood behaviour without a further live check confirming it.
- [ ] Wood claimed across sites: two sites' braves can head for the same tree piece; braves of another task may take wood a site counts on. Refine the dispatch rule (nearest brave per piece, gatherers vs builders).
- [ ] Site visuals: views are redone whenever any building changes; update them per building (stable ids) instead; compare the part order (from the ground up) and the structure with the original; chimney position per hut model; busy look for the other kinds; the pile by the door could be drawn as a stack rather than loose pieces.
- [ ] Wood pieces lying on the ground (`GameMap::wood`, drawn): picked up by any construction; hovering one could show "Wood". Maybe draw the original's shadow under them (`hfx0-0.dat` 22, unconfirmed).
- [ ] Destruction of buildings on uneven/flooded ground (spells, erosion), as in the original (pop3-rev-analysis.md "Terrain changes: what reacts"): after a terrain change near it, a building takes the average height of its footprint as its base, pulls the footprint cells within 200 back towards it, and collapses if a cell is more than 200 away; check the second destroy reason (flooding?). Flatten and the land bridge leave buildings floating or buried today. A collapse sends `explosion::BuildingExploded` to the client (the explosion drawing is done, started by `EXPLODE=building:N` for now); the original's smoke and blast with it are not drawn yet. The check goes in `WorldEvent::Terrain` (`GameMap::apply_event`); cast terrain spells and the editor brushes write the heightmap directly today and must go through it too.
- [ ] A tree whose cell becomes water after any terrain change (land bridge, flatten, erosion, earthquake...) must drop to size 0 (dormant/invisible, like a burnt one) and not grow back while submerged (reported, not traced): `Tree::tick` (`tree.rs`) takes no terrain/water input today, unlike the building check above; needs the heightmap threaded in, likely alongside the same `WorldEvent::Terrain` handling in `GameMap::apply_event`.
- [ ] Damage from moving ground: check in the game what sets the damage (level 10's atlantis island comes back with its buildings missing 1 to 3 pieces of wood, no common percentage), and whether repairs need fetched wood ([buildings.md#damage-and-repair](docs/specs/buildings.md)).
- [ ] Effects 89 atlantis set / 90 atlantis invoke (level 10): sink the island at start, raise it back when the stone head trigger fires; 83 boat hut repair.
- [ ] Towers (drum tower): hold one unit (going in is done, `game_core::enter`), which gets a longer range from there: firewarriors throw farther, the shaman casts spells farther, a preacher converts enemies around the tower. No other tower effect.
- [ ] Training huts (huts-and-training.md "Done"): the queue's ripple (2 turns per rank) and those already of the building's kind letting the others pass, five trained together (the exe's count; ours holds one), queued orders carried over, damage cancelling the queue and sending the trainee out.
- [ ] Training as in the original, what is left (pop3-rev-analysis.md "Training buildings"): the computer players' cost (`CP_` 1000) once tribes have a controller; the trainee's queued orders given to the new unit; the training time to measure in the game.
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
- [x] Height -> colour row scale (pop3-rev-analysis.md "Terrain land colour", terrain-textures.md "bigfade"/
  "disp"/"Our bake"): fixed — `POP3_ROW_SCALE` is gone; the row is now `clamp(height + 75 + (75 if blend != 0
  else 0), 0, 1022)` (`game_core::shading`'s per-cell `blend`, see below), `disp`'s row-jitter follows the
  altitude-dependent sensitivity curve (`pop3_format::theme::disp_sensitivity`), and the brightness column comes
  from a precomputed per-cell `light` byte (`game_core::shading::CellShading`, a height-gradient dot the level's
  sunlight-derived vector, fed by `pop3_format::level::Sunlight`) instead of a baked Lambert light of the
  rendered mesh slope. `land_start`'s own floor is kept, deliberately, as a renderer-side safety net on top of
  the faithful formula (`terrain_texture::pixel_rgb`'s doc comment): a freshly flattened, perfectly flat low
  platform has blend 0, so the bare formula alone can still read close to the sea's own rows there.
  `SunParams::default` now returns the live-measured, disassembly-confirmed flat constant `sunX = sunZ = sunBase
  = 147` (pop3-rev-analysis.md "Terrain land colour", 2026-10-20) instead of scaling `ShadeRange`/`ShadeStart` -
  the level's sunlight block plays no role in it, matching the exe's `sunlight_init_default`. Left open: whether
  the gated day/night sun-angle tick (0x401350) or the fixed-Inclination-64 path (0x401920) ever actually run in
  some mode (both measured OFF this session); how the original maps its sky (backdrop scrolling with the
  camera?), not traced this session either.
- [x] Baked terrain texture minification aliasing (blocky look and bright-speckle `row_jitter` at distance,
  since a single-mip baked 8 px/cell texture can't area-average many texels into one screen pixel): fixed by the
  structurally faithful route this entry already pointed at — the ground colour formula (terrain-textures.md
  "Our bake") now runs per fragment in a shader (`ground.wgsl`/`ground_material.rs`), at the engine's own output
  resolution, the same as the exe's own `set_texture_landscape_main` (pure CPU rasterization, no D3D texture
  sampling), instead of being pre-baked into one coarse texture the GPU then had to filter/minify. Nothing left
  to mip: the mip-chain patch this entry used to propose is moot, not merely unneeded. `terrain_texture.rs` keeps
  the small, still-unit-tested per-cell data prep (`bake_cells`/`rebake_cells`, `altitude_cell`, `ash_coverage`,
  the theme LUT packing); the colour formula itself moved to WGSL and isn't unit-tested any more (no GPU in
  `cargo test`) — verified visually instead (`just shot`/`just profile`). Still separate: "Curvature in a vertex
  shader over a static height texture" below (geometry, not colour).
- [ ] Snow appears at high elevation in every theme today, which the original never does: `procedural_theme.rs`'s `BANDS` table hardcodes a universal top band (`[236, 236, 240]` "snow", row 960) applied regardless of theme/seed whenever the procedural fallback runs (no original data, or `--no-original`) — a project invention, not traced original behaviour. A rough first-pass catalogue of all 36 `pal0-X.dat`/`bigf0-X.dat` files now exists (terrain-textures.md "Themes seen"): of the themes sampled, only `h` clearly goes near-white right at the row ceiling the way the fallback assumes, `i`/`k` lean pale blue there instead, and theme `c`'s own "snow" shows up at a brighter column on LOW ground, not the row ceiling at all — i.e. whether a cell reads as snow-like depends on the brightness column too, not just height. Fix: make the fallback's top band(s) theme-aware (or at least not force "snow" universally) instead of one fixed palette for every theme; the catalogue is a first pass (one column, a handful of rows per theme), not a verified per-theme palette audit, so re-check a theme's own file before hardcoding its family. Now also sits on a much wider live row range than when this was written (see the new bullet below on `BANDS`'s own calibration) — worth doing together.
- [ ] `procedural_theme.rs`'s `BANDS` row boundaries (0/64/128/180/320/560/760/960, chosen to roughly span
  0..`BIGFADE_ROWS` for whatever row the old, flat `POP3_ROW_SCALE`-scaled height produced) were never
  recalibrated after the row scale was dropped above: ordinary terrain now reaches a much higher raw-height row
  far more easily (e.g. height 400 + blend alone already reaches row ~550, "dark grass"/"rock" territory),
  observed as unexpectedly dark, near-black patches on steep or tall `--no-original` terrain (a small sandbox
  mesa, `sandbox_walk`) that look fine with real `bigf0-X.dat` data loaded. The fallback is lower-fidelity by
  design, but its own bands should still track the same height range the real formula now uses; needs its own
  pass (band boundaries, and `procedural_theme::synthetic_cliff`'s shape) rather than being guessed further here.
- [x] Cliffs, lava and ash share one mechanism (`cliff0-X.dat`, 64 rows): fixed — a general per-cell `blend` byte
  now exists (`game_core::shading::CellShading`, pop3-rev-analysis.md "Building ashes" "Drawn"), normally
  `clamp((max neighbour height − min) / 8, 1, 127)` over the cell and its 8 neighbours, recomputed after every
  height edit (`GameMap::recompute_shading`, hooked into `apply`/`tick` centrally, never left to an individual
  spell/effect author to call); lava (`lava::Lava::cells_lit`) and building ashes (`ashes::Ashes::darkness`) take
  priority over it for their own cells, exactly as the original's own recompute skips flagged cells. `cliff0-X.dat`/
  `fade0-X.dat` are now loaded (`pop3_format::theme::Theme::with_cliff`/`with_fade`, a synthetic identity-ish
  table in `--no-original` mode), and `terrain_texture::pixel_rgb` remaps the bigfade colour through
  `Theme::cliff(blend / 4, colour)`. Left for a follow-up, once this is visually verified further (`just shot`/
  `just video` against more original levels and effects): removing the two stand-ins this was meant to replace -
  `terrain_texture::char_ashes`/`ash_k`/`ash_k_bilinear` (a flat RGB darken, still wired alongside the new path)
  and `game-client/src/lava.rs`'s separate `LavaView` 3D overlay mesh (its own module doc already calls it a
  stand-in for exactly this) - and switching `game_core::lava::downhill`'s own live 8-neighbour recompute to read
  `GameMap::shading.downhill_dir` instead, now that the same precomputed field exists (more faithful to the
  original's frozen, precomputed downhill direction; needs its own check against `lava.rs`'s existing spread-shape
  tests first). `fade0-X.dat` is loaded but still not consumed by anything (object/sprite shading, fog of war).
- [x] Ground rendered too bright/washed-out next to the original, with the shoreline's darker "cracked" look
  barely visible and a stray near-white patch next to a shore (reported from a level-1 screenshot comparison,
  2026-10-20; terrain-textures.md "Our bake" has the full writeup): fixed — `ground_material.rs::palette_image`
  was the only original-palette colour texture in the codebase stored as `Rgba8Unorm` rather than `Rgba8UnormSrgb`
  (every sibling - `sky.rs`, `original_models.rs`, `units/mod.rs`, `lightning.rs`, `sparkles.rs`, `flame.rs`,
  `lava.rs`, `tooltip.rs` - uses the sRGB format for the same kind of data), so `ground.wgsl`'s `palette_rgb`
  returned an already gamma-encoded byte as if it were linear, which the sRGB swapchain target then encoded a
  second time - compounding, so darker colours lifted far more than bright ones, flattening contrast and washing
  out exactly the shoreline's own dark/rocky colouring. Confirmed with a `just shot` before/after on level 1
  (storing `palette_image`, and its placeholder, as `Rgba8UnormSrgb` restores ground contrast).

## UI and editor ([ui-and-editor.md](docs/specs/ui-and-editor.md))
- [ ] Building icons, tooltips (spell icons done: `assets/spell-hud`).
- [ ] Spies: hovering one shows a "spy indicator" (in the original a cursor with a punch, to check) instead of its disguise.
- [ ] Original keymap ([shortcuts.md](docs/specs/shortcuts.md), dev mode off): add the missing shortcuts (F1, S status, groups 1-6, camera points Z/X/C/V, `\\` track, Shift+Enter, Space skips the fly-by), and a way to stop and cast self spells without the dev keys.

## Tooltips ([tooltips.md](docs/specs/tooltips.md))
- [ ] Rendered tooltip: done for buildings, trees and totems (`tooltip.rs`: name, slot rows, anchored above, sticky, clicks, right click at once); the toggle to add.
- [ ] People rows filled: totems' counted prayers (once praying exists), the trainee. Vehicles and vehicle huts are done. Buildings, pyramids and totems' places are done, a click adds the unit to the selection.
- [ ] Wood rows: the pieces still needed on a growing house (once growth exists). Plans, sites, built buildings and trees are done.
- [ ] Bars fed by the simulation: totem prayer progress (once praying exists), house growth and birth, house growth and birth. The widget, its blinking, the pyramid's prayer bar and the training bar (double fill from `GameMap::production`) are done
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
- [ ] Next open art: building icons, ritual scenery/reincarnation stones, vehicles and worker action clips (inventory P1). CC0 trees/unit sheets, the generated building kit and 14 of 20 spell icons already ship.
