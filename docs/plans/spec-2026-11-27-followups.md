# Follow-ups of the 2026-11-27 spec session (commit c005713)

The spec session traced ~25 open questions; each answer became a "to change" TODO.md item. This plan orders them.
Ranking: TODO.md tier first, then wrong simulation before wrong looks, then small/isolated before cross-cutting
(each step is one commit unless split below; `just test` green between steps).

## 0. Housekeeping
- [x] (b5d2e2a) Tick TODO.md:1052 "Model size": already done by 5e270ea / 3b86204 / ce9f91f (the spec session wrote it
  against an older tree). Check the Buildings `ObjectScale` default stays 1 before ticking.

## 1. P1 simulation, small and isolated (quick wins)
- [x] **Vehicle speed with a thrower** (2b87d79; also did the balloon step's speed 110 and curve) (TODO.md:76, vehicle.rs, firebolt.rs): drop `Vehicle::slowed` and
  `speed_now`'s halving. A boat whose *driver* throws reads as "driver stopped" (thrower speed 0 in a boat):
  slows ×22/32 a turn and parks; in a balloon the thrower's speed is only halved. Fix vehicles.md's `[ours, wrong]`.
- [x] **Ghost attacking a building** (199293b) (TODO.md:463): skip the per-hit damage for a `statuses.ghost` attacker, keep
  the alarm. spells.md "Ghost Army".
- [x] **Idle shuffle** (12a8d54) (TODO.md:239, idle.rs): no step; turn ±135° (768/2048) from the current facing, sign from the
  hold's parity, keep the facing. Drop `SHUFFLE_STEP` (also used in unit.rs, work.rs: check). units.md.
- [x] **Delayed reservoir** (f64c422; `GameMap::multiplayer`, nothing sets it yet) (TODO.md:511, mana.rs): mana discoveries (kind 6) go through `Reservoir` when the
  82-turn reward countdown ends; 50-turn delay when the game is multiplayer. spells.md "Mana".
- [x] **Flatten** (7b0bf0d; Spell::Flatten now world units; sparkle ring left in TODO) (TODO.md:468, flatten.rs): 45 points around the cell corner rounded down, target = interpolated
  height at the exact point, clamp 0..1024, growing radius gate 380 + spread 5120 × remaining / turns_left.
  Sparkle ring follows the radius (client). spells.md "Flatten".

## 2. P1 simulation, medium (one topic each, possibly several commits)
- [x] **Spies** (79115d4, bc9393c, 8c3f268, ed5ff66) (TODO.md:242), split:
  1. look: `Unit::shown_kind` always a brave to other tribes (drop "once settled"); `shown_owner` blink seen by
     everyone in 3D, minimap only shows the real colour until settled;
  2. arson: `arson_spot` = middle of the back wall, opposite the door (TODO.md: "the door when it is not free" -
     re-read pop3-rev-analysis.md "Arson" substate 0 for the fallback); notice roll = owner's `SPY_DISCOVER_CHANCE`, only for a human player's spy;
  3. `ai_spy.rs`: raid target = random non-wall building of a random enemy tribe; discovery = 3×3 cells around the
     caught spy, only spies inside the settlement area (`SET_DEFENCE_RADIUS` → 4/6/8, default 10; parse the
     script token if missing, ai-scripts.md);
  4. drop the spy's juggling from `units::art::bored` (decided: the original never plays anim 92).
- [x] **Health** (ae06c01, 41302e2, 8c71192) (TODO.md:219), split:
  1. wildman life 32, no heal;
  2. stranded: grace 408 (human) / 992 (computer) / 40 (wildman), restarted on entry, then 56 every 8 turns at the
     unit's phase, healing still on, shaman strandable (replaces `STRANDED_DAMAGE` 20/4);
  3. max and starting life rolled at birth in [3/4, 1) of the kind's life with `map::Lcg` - touches every test
     that assumes full health: do last, expect test churn.
- [x] **Worshipping the shaman** (9fae861; now sim state, `game_core::kneel`) (TODO.md:224, worship.rs): her 3×3 cells, any state, one standing follower per 8
  turns at her phase (all at once only at her reincarnation site), stays while within 4.6 cells (re-checked every
  32 turns), faces her every 4; pose: one arm cycle then hold 4-11 turns. worship.md.
- [x] **Angel and vehicle passengers** (jolt done; panicked driver left in TODO) (TODO.md:617, angel_of_death.rs, vehicle.rs): a panicked passenger stays
  seated; a panicked driver sets the vehicle off to its last destination; a grabbed passenger is force-disembarked
  at the exit point, vehicle pushed 40 units along the angel's heading.

## 3. P1 structural (risky: run late in the batch, alone)
- [x] **Order of work inside a turn** (AI moved first; no test churn) (TODO.md:81, `GameMap::tick` map.rs:1569): move `tend_ai_scripts` (and the
  AI family) before vaults/totems/training/huts (the original: queued commands → AI → formations/challenges/fights →
  units → recount/income → terrain → mana last); check a player `Command` never acts in its click turn. Will shift
  replays/checksums: rebaseline tests deliberately, update architecture.md "The tick".
- [x] **Balloons** (35a67fb) (TODO.md:364, vehicle.rs): look-ahead 3 points at 2/4/6 × speed; hover 560 over buildings,
  trees, totems, fires, pillars, portals, 200 elsewhere; linear climb ≤22 up / 16 down, no `CLEARANCE`, no easing;
  bob every turn (32-turn tri wave, passengers too); speed 110 both kinds, accel ×56/32 from 10, decel ×22/32,
  turning ≥10° off aims at 27. Decided: the bob is visual only (client, from `SimNow` and the vehicle id), not
  sim state. vehicles.md.

## 4. P3 terrain (biggest change, own sub-plan if it grows)
- [x] **Sea, shore and land** (185c98c; sub-cell masks, boat-hut water part, land-locked raise left in TODO) (TODO.md:675): a point is sea iff it and its 8 neighbours are 0; cell shape from the
  4 corner bits (land / open sea / 14 shores), a shore cell is land only within 128 units of its land corners;
  one shared function in game-core used by `unit::is_sea`, pathing, drowning, placement, and the ground shader
  (replace `VISUAL_SEA_HEIGHT`); sea points drawn at ripple height ~32; boat hut pulls its water footprint to 0.
  Do sim first (tests), shader second (`just shot` level 19 inlet, level 38 coast). terrain.md, terrain-textures.md.

## 5. P1/P3 looks (client, original assets)
- [x] **Scenery 9 look from its trigger** (27d1570) (TODO.md:935, totem.rs, totems.rs): type 3 → stone totem (1), type 5 →
  winged death (19), "once" discovery target → stone head (82), else totem pole (187); nod/swing only while active
  (pole: while prayed at); scale per the record. Check with `just shot` on levels 2, 5, 15.
- [x] **Plants 7/8** (c4cf22a) (TODO.md:934): hfx 2 / hfx 3 camera-facing sprites, drawn like the volcano rock.
- [x] **Hammering** (e255d10) (TODO.md:730): anim 11 is final (drop the stand-in wording and the CC0 hammer-sheet need);
  one swing every 16-31 turns, sound 0x14; idle 8-15 turns with no building yet.
- [x] **Construction by stage** (4ccfffe) (TODO.md:699): parse `facs` byte 59 in `pop3_format::objects`; draw faces with
  bit `stage`, wood tile 250 for bit `4+stage`; stage 0-3 from wood/cost; damaged buildings reuse it. Decided: original
  objects only drop the edge beams / `built_part`; the generated (CC0) kit keeps its current frame and height cut. objects.md, buildings.md.
- [x] **Building debris** (6ac9a5e; puff+sound left) (TODO.md:696, explosion.rs): a piece vanishes on landing with a dust puff and sound 0x13
  (splash in water).

## 6. P4 input and HUD
- [x] **Holding a plan** (e93e5a0) (TODO.md:650): plain click places and releases; Ctrl/Alt click keeps; walls stay held;
  refused spot → error sound, kept; right click/Esc puts away.
- [x] **Follower picks** (a2150b1) (TODO.md:1908, `GameMap::next_unit`): nearest to the camera not yet selected (activity
  group first, never the shaman); Ctrl 5 (`MULTIPLE_SELECT_NUM`), Shift all.
- [x] **Space as an ordered binding list** (15ea16a; status screen and combat alert don't exist yet) (TODO.md:1933): turn plan → skip fly-by → status once won/lost →
  combat alert. Generalise the keymap to first-match (modifiers, condition) bindings.
- [x] **Right-click query** (73bb21c) (TODO.md:1937), decided: right click with a selection deselects (as today); with
  nothing selected it opens the query on the thing under the cursor (hovered thing, else the cell's occupant),
  kept while held and until the cursor leaves it. Its contents are untraced: reuse the hover tooltip's text for
  now (tooltips.md). Same in dev mode and original keymap. Update shortcuts.md's row.
- [x] **Discovery cinematic** (b4239cb..8d29e46) (TODO.md:1066): large, its own plan. Decided: the sim grants the reward after a fixed
  delay, a game-core constant in turns (`[ours]`, sized to the cinematic's length), only for human
  players' rewards (computer tribes get theirs at once, no cinematic to wait for); the client cinematic is timed from the grant's start tick (`SimNow`) so its landing on the button
  matches the grant turn. One constant shared by both (the client reads game-core's), so they can't drift.
  Determinism holds: the delay is the same for every peer. Pick the length once the cinematic's frame counts are
  converted to time.

## 7. P7
- [ ] **Chat** (TODO.md:2199): wait for multiplayer work.

## Open questions
- Life rolled at birth: same Lcg stream for every tribe, drawn at the spawn call in creation order (lockstep).
