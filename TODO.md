# TODO

What is left to implement. Tick or remove an item in the commit that does it, add new ones as they
come up. Details live in the linked specs; done work is summarised in [docs/roadmap.md](docs/roadmap.md).

Priorities (2026-10-25): sections below are grouped into six rough tiers, highest first. A tier is a priority ordering, not a gate - later tiers (especially 2 and 5) can and should progress in parallel. Re-group as priorities shift; this is a living ordering, not a plan.

## Priority 1 — core gameplay correctness
What a match plays through today: combat, orders/the tick, units, spells, buildings, worship. Bugs and gaps here are the most visible, and other tiers build on top of them (the AI tier drives units through the same orders/coordinators as a human; the content tier dresses up the same spells and buildings).

### Combat
- [x] Melee (combat.md "Melee", traced in full - pop3-rev-analysis.md "Fight"): the ground check moving the middle off unsuitable ground (`Melee::relocate`) and the per-tribe winner's count and death tally (`GameMap::melee_wins`/`melee_deaths`) are in. Left: the moves' sounds (scoped out for now). Still genuinely open, not traced: what the original's local-player-involved alert flag (`0x89D167`) actually triggers (fight music, an alert, or both) - scoped out for now too.
- [x] Challenge (combat.md "Challenge", traced in full - pop3-rev-analysis.md "Pre-fight"): the target facing the attacker and its taunts (1 in 8 per turn while waiting, confirmed) are in (`WorldEvent::Face`, `challenge::taunts`/`Challenge::taunted` - no pose for the taunt yet, see "Units" below). The earlier "third approach, a target at its building" item is resolved: it is the same `Approach::PulledOut` building pull-out already covered under "Attacking a building", not a separate case. Still open, left for later: the original's no-shove option (a real level/script switch, not a cheat - also disables Melee's own knockback) and ground sliding instead of `SHOVE_LIFT` (not traced at the exe level).
- [ ] Fight poses in the generated (CC0) unit sheets: strike, trade, kick, reel, shoved, and a challenge's taunt (3 animations, `Challenge::taunted` already tracks when - combat.md "Challenge") (`Pose::fallback` stands in: idle, fall).
- [ ] `attack.rs`'s `wildmen_never_fight_and_nobody_attacks_its_own` test makes a wildman entirely unattackable, by anyone, any way. The original (pop3-rev-analysis.md "Wildmen") only confirms its *automatic* proximity auto-attack scan skips a tribeless (0xFF) target - whether an explicit player attack order also refuses a wildman target is unconfirmed; if the original does let a direct attack order through, our blanket rule is stricter than the original and should be relaxed to allow explicit attacks while keeping the auto-scan exclusion.
- [ ] Spotting (combat.md "Spotting"): the shaman's look during some orders, allies. Invisible people are already
  unseen by auto-targeting (`spotting::first_seen`); whether a player ordering an attack directly can still hit one
  is untraced, left open. The tribe flag at `+0x93D` bit `0x8` traced to was a dead end for this: it is "spells
  are free" (a cheat/debug flag, pop3-rev-analysis.md "Spending and other sources"), not "sees invisible people" -
  no tribe flag for seeing through invisibility was found this session; nothing grants it in the exe as far as
  traced, so there may simply be none (a live check with a revealed invisible unit against each tribe type would
  confirm there is truly no such flag before this item is dropped).
- [ ] Joining a fight already starting, not just one going on (combat.md "Spotting": "not in a melee; when idle,
  also someone in a melee (to join it)"): `spotting::first_seen` only special-cases a target already linked to a
  `Link::Melee` (skipped unless the looker is idle, then joined); a target only in a `Link::Challenge` (the
  pre-fight shove-and-charge) falls through to the ordinary case and is attacked directly regardless of whether the
  looker is idle - fine now that doing so escalates the challenge into a melee at once (`GameMap::escalate_challenges`,
  combat.md "Challenge"/"Melee", added to fix units ordered onto an already-challenged target otherwise standing in
  reach doing nothing for the duel's whole charge-shove-charge cycle), itself a guess, not traced against the
  original. Still unconfirmed: whether an idle unit that merely sees a challenge from outside sight range should
  also hold off and only join once it becomes a melee instead of attacking straight in (the way it already leaves an
  ongoing melee's target alone unless idle), and whether a non-idle looker should likewise skip a challenge target
  the same way it skips a melee one - needs a live check (two units challenging while a third, idle and then busy,
  watches from within sight).
- [ ] Sermon (combat.md "Sermon"; full trace, settled across several sessions, in pop3-rev-analysis.md "Preacher
  conversion" and "Command groups" - summary here is the still-open residue only): a failed conversion roll never
  falls back to melee on its own - the roll-failure branch only continues the candidate's listening choreography
  or reverts it to its ordinary default behaviour. The "if the preaching doesn't pass it's then close range
  combat" impression is a third-party attacker engaging the candidate mid-sermon (confirmed live: 8 of 71 observed
  sermons), not the sermon's own exit. A manual preach order (kind 33) never itself converts anyone: the
  preacher's own autonomous auto-scan can self-issue a kind-32 order that out-prioritizes (not destroys) it, and
  that autonomous path is what actually reaches the conversion roll. Our own sermon-ending rule (a freed listener
  strikes back at once, `WorldEvent::StrikeBack`) approximates the third-party-attack case via the preacher's own
  eligibility - worth checking our combat targeting can reach a listener mid-sermon the same way. Model sermon/
  soul-convert as "the preacher's autonomous conversion runs regardless of a manual preach order and can override
  it", not as the manual order itself rolling anything. Still open (same sections): the building-target variant's
  gameplay meaning; the soul flying from preacher to convert; the halving flag of the first check; how many listen
  at once and from how far; whether the preacher's sermon plays its 5 identified phases in order or just one
  (`Pose::Preach` only draws one, anim 22, a guess).
- [ ] Resuming a fight cut short by a push (combat.md "Melee" rule 1, pop3-rev-analysis.md "Resuming a fight cut
  short by a push"): implemented - a fighter's standing attack order now survives a fight (`Unit::join`) and
  resumes, same target, whenever the fight lets go of it, whether through the ordinary roster check or a blast's
  push knocking it clean out (`Unit::release`/`Unit::go_idle`/`resume_attack`, `crates/game-core/src/melee.rs`
  tests `a_fighter_pushed_out_of_leash_resumes_its_attack_order`,
  `a_warrior_blasted_out_of_sight_resumes_its_attack_order`). Still open: exactly how long the resumed order keeps
  trying before giving up is ordinary attack-order behaviour (0x51A2A0), not something special to trace for this.
  **Reported, 2026-10-25**: live play described fights as short, with units "forgetting" they were attacked - this
  gap was the likely single cause; re-watch live play now that it's fixed before trying to explain any of the
  remaining "combat doesn't last" impression some other way.
- [ ] Firebolts (combat.md "Firebolts"): the line of fire, driving a vehicle towards an out-of-reach target (to trace: whether the original's halved speed of a passenger throwing aboard really slows its vehicle, ours does), the smoke trail (our own) and the ground light, ghost firewarriors' bolts, the shaman inside a building or vehicle losing half her life, the angel's reaction.
- [ ] Health, hit feedback and death animations for every unit type.
- [ ] Healing (combat.md "Health"): no healing while struck by lightning once it has its own state; the angel of death never heals; the original lets life go past full by up to one heal (ours stops at full).

### The tick ([architecture.md](docs/architecture.md) "The tick")
- [ ] Find the original's order of work inside a turn (people, buildings, effects, shots, mana; income comes before spell charging): ours is a guess.
- [ ] Rhythms still on per-object counters or missing: stranded hurt (`STRANDED_HURT_EVERY`, rate unknown), hut breeding every 4 turns and growth every 16 (pop3-rev-analysis.md "Huts: breeding and growth"), mana income every 16 (`MANA_UPDATE_FREQ`), the other status timers every 8 (shield and bloodlust done).
- [ ] World events without behaviour yet: `Raid` (occupants thrown out; also needed for Earthquake's own building-shake, spells.md). Panic: the screaming, and how the original turns at an obstacle.
- [ ] Burning buildings (`fire`): the igniter's tribe and the destroyed-building count (`stats[tribe][7]`), the fire sound.
- [ ] Shots (fireballs 8/4, 8/7, 8/8): a typed list ticked before effects like `GameMap::bolts`, integer flight, spawning a simple blast on arrival.
- [ ] World views (architecture.md "Client: world visuals"): the landing dust (`units::dust`) as a `WorldView` once
  touchdowns are kept in game-core (a unit's landing tick), so a rejoining client draws it too; the sinking totem's
  smoke once totems have ids; units, building views and the unit glide (`SimClock::prev_ids`) still keyed by index.
- [ ] Coordinators: the infrastructure is done ([coordinators.md](docs/specs/coordinators.md)); each kind to specify and implement (preacher's sermon, formation, soul convert - see pop3-rev-analysis.md "Preacher conversion" for soul convert's traced numbers (`PREACHEE_CONV_FREQ`/`_CHANCE`, the population cap, the "no breeding" tribe bypass); the challenge and the melee are done), with its scan in `GameMap::detect_coordinators`. Guard control is **not** one of these any more (coordinators.md's own table, row 10): it turned out to be ordinary move/attack orders stashed across a unit's own order slots, not a dedicated controller - see "Order model" below.
- [ ] Hardening, optional, found while tracing the above: `Melee`/`Sermon` both still manually re-check `is_alive()` on every member inside their own `tick()` rather than relying on `tick_kind`/`participants()` to have already filtered dead members out of `members` - a convention, not something the shared infrastructure enforces today. Teaching `tick_kind` to pre-filter would let every future `Coordinator` impl, this one included, drop its own by-hand check.
- [ ] The hurt path (`GameMap::damage`): the last attacker, the death cause, and death held off while tumbling, so no emitter repeats them.
- [x] Internal model 10/6 (and 10/1) resolved: they are a **formation** system (pop3-rev-analysis.md "Formations"), not Preacher Attract. A unit joins/creates one (`add_unit_to_formation`) whenever a global flag (now traced: a "formations off" switch, normally clear) is clear and its current order kind "wants formation" - true for plain move (kind 3), false for attack (kinds 19/21). Up to 12 members are held at fixed per-slot offsets around a controller that jitters its own heading periodically. Implemented (`formation.rs`, `Command::Formation`, `GameMap::move_orders`): a plain group move order (not attack, not entering a building, etc. - kind 3's own clearest traced case only) still gives each walker its own one-shot standing spot via `slots.rs`'s grid-based spot search as before (never on a tree or another unit), but when there is more than one walker they also join one shared formation that keeps nudging their target towards a slot grid turning slowly around the order's destination. Left open, guessed or simplified rather than traced: the slot table itself and its 16-unit pitch (ours reuses `slots.rs`'s own standing pitch instead, which the original's literal number would stand members on top of each other at), the jitter's rhythm, whether the controller itself advances across the map towards its target (ours keeps it fixed at the destination), the exact "≥ 568 units off its slot with a bad heading and an unreachable path" drop condition (ours drops a member once ordinary pathing gives up on its slot, `Action::Stranded`, instead of a separate distance check), and whether several units given one move order join the same new formation together or trickle in one at a time (ours: together, in one `Command::Formation` alongside the group's own `Command::OrderUnit`s).
- [ ] Order model: the original's "unit order queue" (pop3-rev-analysis.md "Command groups", now fully traced across several sessions) is structurally different from `Unit::queue` (`crates/game-core/src/unit.rs`). Ours is a single `Vec<Order>` FIFO of future player-authored steps (push/pop, cleared by a fresh direct order). The original instead gives a unit one exclusive "primary" order - reserved EXCLUSIVELY for self-issued autonomous behaviour (auto-attack/auto-scan, confirmed exhaustively: only 5 functions in the whole exe ever use it) and always outranking secondary when both are populated, without erasing it - plus 8 independent "secondary" slots. A freshly given direct order (move/attack/etc., all 8 "give an order" entry points) wipes everything and lands in secondary slot 0 - in solo play this is functionally equivalent to our own "a direct order replaces the chained ones", so that part already lines up.
  **Implemented** (`game_core::patrol`, `game_core::hidden_point`; `docs/specs/units.md` "Patrol
  orders"): `0x4BA9B0`'s behaviour (fills secondary slots one at a time without wiping earlier ones,
  `N` releases the held group, each point an ordinary move-via-invisible-marker or attack order, points
  set on several selected units share one id) without the tribe-wide point counter, deliberately not
  reproduced (scoped per-selection instead, client-side `PatrolSession`, units/selection.rs). Still
  open: whether the original really has a deliberate "attack this specific unit" patrol slot at all
  (vs. reactive, non-stored engagement along plain waypoints), and separately whether an attack-type
  point can ever target a *building* rather than only a unit - unverified either way (units.md's own
  caveat); a follow-up clean-room pass against the exe trace should settle this. Also still open, an
  `[ours]` idea not followed through on: "the whole patrol group engages an enemy together" once one
  member spots it - today only the spotter itself breaks off (`spotting::spot_enemies`, unaffected by
  this work), its co-patrollers keep cycling obliviously; needs a patrol-point-aware hook there. How the
  AI script's own `GUARD_AT_MARKER`/`GUARD_BETWEEN_MARKERS`/`SET_MARKER_ENTRY` (docs/specs/ai-scripts.md)
  drives this same mechanism for computer tribes is also still open.

### Units ([units.md](docs/specs/units.md))
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
- [ ] Wildmen (neutral "gaia" braves, spawned from the levels, standing still today): implement the original's
  confirmed, fully traced idle loop (pop3-rev-analysis.md "Wildmen", "Drowning"): state 1 → 8 → (5 "eating" at a
  tree/plant within 18 cells, or 6/16 "drinking" at a shore cell within 12 cells if none found) → 8 → ...; no
  hunger/thirst resource exists anywhere, both searches only ever pick a destination for an idle animation. A
  wildman idling at its shore spot genuinely drowns fairly often - no special mechanic, the shore search only
  checks the coarse per-cell shape while the fine-grained per-turn drowning check has no bias toward the dry side.
  The Convert spell turns them into the caster's braves (unchanged); no periodic respawn exists, they only ever
  decrease. A wildman's death always plays the "climbing + spinning" spirit-rise variant, never the calm ghost
  pose (see the death/spirit item below). Still open: the literal VSTART id each state resolves to (eat-fruit=2,
  drink=4, already known from sprite inspection, not yet pinned to a line of code); whether an explicit attack
  order is honoured by the original despite its auto-attack-scan exclusion (see the
  `wildmen_never_fight_and_nobody_attacks_its_own` item above).
- [ ] When the shaman reincarnates at her site, the wildmen within range of it are converted into braves of her tribe for free, each with a conversion animation. Done at level start without animation (`ReincarnationSite::welcomes`); check the range in the game (the levels only use the ring of 8 cells) and whether it happens on every reincarnation.
- [ ] Animate the wildmen: eating fruit (2) and drinking (4), wandering around their spot, instead of standing still.
- [ ] Selection of buildings (without the people inside), and of units inside them (`selection::selectable`). Vehicles: a click with nothing selected selects the people aboard (vehicles.md).
- [ ] Walking trails: footprints / worn paths left on the ground where units walk (fading over time).
- [ ] Statuses (`Unit::statuses`: Magical Shield, Bloodlust, Invisibility and Hypnotism now cast for real,
  `GameMap::cast_nearest_status`/`hypnotize`; only Ghost Army's left unset by any spell): **found this session,
  tracing the exe's own Hypnotism code (0x50EB00) while resolving "Preacher conversion"'s GONE/NEW contradiction**:
  the original explicitly carries the captured person's current life, max life and carried wood across the
  capture (and again across the later revert) - ours (`GameMap::change_owner`, used by `hypnotize`) instead resets
  health to full and carrying to 0. A damaged enemy hypnotised in the original keeps its existing damage and any
  wood it was carrying; ours heals and empties it. The timer of a ghost's
  status (not traced: no expiry found for Ghost Army), other options to define; read by combat, selection and
  drawing. Invisibility's own early end (attacking does, walking does not) is a guess (pop3-rev-analysis.md "Per
  spell" 6: "some actions end it early", not traced), implemented in `attack::go_attack`. Drawing: the markers are
  done (`units/markers.rs`); left: the low-health looping sound 0xCF, own invisible and ghost people translucent
  (invisible flickering faster near the end) and invisible enemies hidden (sprite, shadow, hover, selection), the
  original's colour tables (our shield tint is a guess), the optional health bar over own damaged people (commands
  100/101).
- [ ] Physics (`game_core::physics::step`: velocity, gravity 32, touchdown; landing in the sea or on walkable ground done, `Unit::touch_down`), to the original's rules (pop3-rev-analysis.md "Blast push and flight"): velocity caps (192 per horizontal axis, 512 vertical), air drag 2 per turn per horizontal axis, the unit spinning 26° per turn in the air, a scream on take-off; landing: dust and a sound at a vertical speed <= -100, `FALL_OUT_OF_WW_DAMAGE` 700 at <= -200 (`Touchdown::speed`), none on water cells; on the ground: sliding down the slope (gradient over ±76 units) with friction 28 per turn per axis (72 on special terrain), facing where it slides (`Tumble` pose; a grounded tumbling unit does not move yet). Speeds are per original turn: convert to our ticks (`game_core::balance`). Ordinary falls (ground dropping under a standing or walking person: earthquake, erosion, flatten) fall with the same gravity and drag, without fall damage; rising ground lifts people. **Resolved this session (pop3-rev-analysis.md "Person physics"): landing on a building gets no special handling at all.** The original's `ground(pos)` (`0x44E940`) is a pure terrain-mesh height interpolation (a cell's 4 corner heights, diagonal-split-aware) with no building/unit/class check anywhere in it - a cliff is just handled correctly as steep terrain (same function), but a building sitting on the landing cell is invisible to it. A thrown person's touchdown height is always the raw ground height there, same as open ground - no "stands up on the roof" behaviour exists to port; a landing that happens to fall on a building's footprint should just land at ground level (through/inside the building's base), matching ours needing no special case either. Still open: what the original does to a unit landing in water (ours dies at once), boats landing on the sea once vehicles exist, bumping into buildings and cliffs, a task dropped or kept when flung.
- [ ] Flying: units thrown by Whirlwind / Blast / explosions follow a ballistic or carried path (flying 61, tumbling 73 anims), land with damage, drown if they land in the sea.
- [ ] Ground recovery after landing (person state 19, pop3-rev-analysis.md "Tumbling and ground recovery", live-checked 2026-10-27): the original does not go straight from a grounded tumble back to Idle - it stays locked (no orders) for anywhere from a handful to several dozen turns (longer with bad luck on two small-odds per-32-turn rerolls) while it picks a nearby clear spot and settles, before resuming its ordinary per-type behaviour. No new animation asset needed - it plays ordinary Idle/Walk poses throughout (resolved from `0x5A6D50`'s action table, statically, not confirmed on the one live trial so far), switching to holding-wood/walk-with-wood instead if the person happens to be carrying wood (`+0x78`) at the time, with a few dropped-wood debris puffs along the way (one per 100 wood) if so. Visually this likely just reads as the person standing/walking around unresponsive, not visibly struggling on the ground. Ours (units.md "Tumbling") goes idle (and responsive) the instant it stops moving - should add a short randomized locked/unresponsive grace period instead. Not measured: real-world duration distribution (only one live trial so far, ~90 turns on a mild landing).
  - **Follow-up live check needed**: blast/tumble a brave that is actively carrying wood (`+0x78` nonzero) and watch
    whether it (a) shows the holding-wood/walk-with-wood pose instead of plain idle/walk while relocating, and (b)
    drops a small debris puff with a sound once per 100 wood while settling, down to 0. The one live trial done so
    far did not confirm this - the unit's wood status wasn't logged, so this is a static-table reading only.
- [ ] Vehicle huts (vehicles.md "Vehicle huts", done with guesses): check in the game the crew size (4), the work per vehicle (1200 crew-turns), whether mana is spent, where the vehicle appears, and a hut's queue at the door (types 13-16 have one, pop3-rev-analysis.md "The queue at the door").
- [ ] Whirlwind spell (to implement), on a vehicle: everyone aboard is put out, it spins (roll +204, pitch +153 /2048 per tick) for 10 ticks (boat; ends early when the ground reaches it) or 2 (balloon), then explodes like a destroyed boat on land (pop3-rev-analysis.md "Life, damage and destruction", state 9).
- [ ] Vehicle destruction (vehicles.md; health, damage from blasts and bolts, sinking, rising and exploding done): the partial explosion of a boat on land (its faces flying off, `explosion.rs`).
- [ ] Balloons: bobbing while moving, and the original's climb rate and look-ahead; check the hover over trees and totems in the game (450 guessed; buildings 560 from the exe) (vehicles.md).
- [ ] The shaman's health is lowered while she is in a vehicle and goes back to normal when she leaves it.
- [ ] The shaman can cast only some spells from a vehicle: Land Bridge is refused (done); check the others in the game (fire-type probably yes).
- [ ] Drowning animation: the original drowning pose is not identified (tumbling is used).
- [ ] Use the shaman's wand gesture (81-84), the conversion sit (18), the spy's juggling idle (92), the carbonized brave (97) for lightning; check the tribe layer flag `0x10` mapping.

- [ ] The shaman's reincarnation is not instant: ours waits a fixed `RESPAWN_TICKS` (3 s). Find in the original how long she waits and what it depends on (her tribe's mana, its population, or both), then compute it from that.
- [ ] Training units: followers turned into warriors, preachers, spies and firewarriors in the training huts (see "Buildings": training huts and training lines).

### Spells ([spells.md](docs/specs/spells.md))
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
        can work. Not just a hypothetical Armageddon state either: `DO SET_REINCARNATION OFF` is a real PopScript
        command a tribe's own AI script runs at turn 0 (confirmed in level 1's red script, ai-scripts.md "Worked
        example: level 1's opening flyby") — once script interpretation below exists, an ordinary campaign tribe
        can reach the "no site for this owner" state outside Armageddon too, so this fix is load-bearing sooner.
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
- [ ] Spell effects with their own animations/visuals (whirlwind...; lightning has our own bolt, the original's bolt sprites are not identified); casting (C) only uses a charge and makes the shaman jump.
- [ ] Aim the other spells on the terrain like Teleport, Blast and Land Bridge (`hud::spells::ground_spell`), within the cast range (`GameMap::in_cast_range`, done: `SP_W_RANGE` by altitude band, +1/3 in a drum tower, none on god levels).
- [ ] Every spell flies from the shaman as a shot before it takes effect (6 turns into the cast; standard shot 1400 units per turn, blast's fireball 1000), homing on a targeted unit for the spells that follow one; burn, blast, lightning, whirlwind and insect shots are deflected 8 cells away by a shielded enemy on the target cell. A 1 s cooldown after any human cast, no per-spell cooldown (`SPELL_x_OPT_S` is not one).
- [ ] Blast (`game_core::blast`): the cumulative, slope-scaled push and the allies-only-on-the-last-turn rule are done (pop3-rev-analysis.md "Blast push and flight"). Still to the original: the fireball shot flying from the shaman first (1000 units per turn, 6 turns into the cast, TODO.md "Shots" above); buildings 40 once (buildings have no health yet); angels of death and ghosts (neither spell/unit kind exists yet); the burn cell, smoke and explosion.
  - **Reported, 2026-10-25**: live play feels blast as inconsistent, "sometimes too powerful, sometimes not
    enough". Two of the still-missing pieces above are the likely explanation rather than wrong numbers: no
    building damage yet (a blast against/near a building does nothing where the original would wear it down, read
    as "too weak") and no fireball-flight-first (ours applies the full effect the instant the cast releases, no
    6-turn travel window, which could read as "too sudden/too strong" compared to a flight players may remember
    giving targets time to react or flee). Before changing any number: audit our own `blast::effect` against the
    "Simple blast" row in "Components" and "Blast push and flight" turn by turn (radius 2/4/5 cells across the 3
    turns, `BLAST_DAMAGE_PERSON` 50/turn up to 150 total near the centre, push only within 2.5 cells/`PUSH_RANGE`
    1280 cumulative, ally push only the last turn) - this is a verify-matches-trace pass, not a new measurement,
    since the exact numbers are already live-confirmed there. Only once that audit comes back clean should a
    fresh live check of the original's own blast be requested, and only for whatever the audit can't explain.
  - **Resolved, live-checked 2026-10-27** (pop3-rev-analysis.md "A blasted person fighting back"): no, there is no
    spell-specific or caster-specific "fight back" mechanism, and the guess already in this TODO was right - it is
    nothing but the generic Auto-attack scan (`look_every` rhythm, independent of being struck) happening to find
    an enemy once the person lands from the push. Watched a brave hit by two separate Blast casts: the first
    landed it nowhere near anyone and it just sat idle for 90 turns with no order; the second landed it a few
    cells from an enemy shaman, and ~4 turns later the ordinary auto-attack scan self-issued a cell-targeted
    attack order (not caster-targeted - it walked to whichever enemy the scan's square found, which here happened
    to be the shaman) that resolved into a real melee. Today's implementation already matches this (a blasted idle
    person goes idle and only re-engages via its own passive scan) - no code change needed here. Same mechanism
    should hold for Lightning/Swarm/Whirlwind (being idle-with-no-order after any push/stun, not Blast-specific),
    not separately re-checked.
- [ ] Lightning panic, open question (not yet traced either way): **reported, 2026-10-25**, live play describes
  lightning as "less effective" than remembered, specifically fewer panicking units around the strike. Per the
  current trace (pop3-rev-analysis.md "Per spell" row 3 and "Components"), neither Lightning's own strike (an
  outright kill in the target cell, a building set alight, trees burnt/ground fire) nor its companion "simple
  blast" causes panic - panic is only traced for Firestorm/Volcano's *panicking* fireballs, Swarm's stings, and
  occupants thrown from a burning or raided building (a delayed effect, not an immediate ring around the strike).
  So either: (a) the player's expectation is carrying over Firestorm/Volcano/Swarm's panic onto Lightning, nothing
  to fix; or (b) the original's Lightning strike (or its companion blast specifically) does panic nearby survivors
  and the current trace is incomplete. Needs a live check in a spec session (watch an original Lightning cast with
  several followers standing just outside the kill cell) before assuming either way - do not guess a panic radius
  into `EffectKind::Lightning` without it.
- [ ] Spell cursors: confirm 40 = Armageddon and 43 = Ghost Army in the game (`virtual_cursor::spell_sprite`); check the gold arrow's frame order and speed against the game; the pause menu releases the mouse, so it shows the system cursor, not the gold arrow.
- [ ] Burn (1), bloodlust (20) and the original teleport (21), now traced (pop3-rev-analysis.md "Per spell"): burn sets the target cell's trees alight and is not castable; bloodlust (hidden) gives the 6 nearest own followers ×3 damage, /8 damage taken and faster firewarrior reloads for 120 s; teleport (hidden) moves the caster's own shaman. Decide which ones we expose.
- [ ] Flatten: trace the original's per-turn curve (ours divides the gap by the turns left) and whether the disc is centred on a point or a cell. The land bridge raises a smoke puff on every bridge cell every turn while it builds (a `WorldView` of the effect); balloons rise with it; its steps do not re-level or collapse buildings yet.
- [ ] Level spells: keep each discovery's availability (permanent / this level / one shot) for when it is discovered; `SpellsAvailableLevel` / `SpellsNotCharging` and `SpellsAvailableOnce`.
- [ ] A "once" spell discovery should give a single shot (`Provided { shots: 1 }`) when found (e.g. Convert in levels 4 and 5).

### Mana ([spells.md](docs/specs/spells.md) "Mana")
- [x] Mana bar in the HUD for the player (`GameMap::mana`: `bank`, `received`, `used`): `hud/mana.rs`, a strip under
  the population strip showing the bank against `MAX_MANA`; `received`/`used` aren't drawn yet, only available for
  later (e.g. a tooltip) if wanted.
- [x] The bar's own fill rule now matches the original's shape (pop3-rev-analysis.md "The mana bar (HUD)",
  `ManaPool::advisor_fraction`/`advisor_eta`): not `bank` against `MAX_MANA` at all, always full with nothing
  charging, otherwise `received` (last turn's income) against an estimate for `SpellBook::charging_candidate`,
  scaled the same ~1.78x (`456 / 256`) the exe does, so a short bar means income fell behind the estimate, not that
  the bank is low. A two-colour fill/track bar too: the tribe's own accent colour filled (`sites::tribe_color`), a
  fixed colour for the rest — still a single solid colour, no flash.
- [ ] `advisor_eta` itself is `[ours]`: the exe scales its own pacing estimate by an undecoded global byte
  (`0x89D161`, "The mana bar (HUD)") that this reading skips, spreading `SpellStats::advisor_seconds`
  (`SPELL_<X>_OPT_S`) directly over its own `cost` instead of the exe's "share the picker returns" - close in
  shape, not in the exe's own numbers; revisit once that byte is decoded. Also not done: the separate
  flat-colour flash condition (two more undecoded flags, "not confirmed to be the same signal" even to each other)
  and the one-shot "mana had nowhere to go this turn" advisor message. In generated/no-original mode, `cost` is
  still our own tiny placeholder (`SpellStats::default_for`), so a cheap spell's own whole-period target
  (`cost / periods`, periods from `advisor_seconds`) still rounds down to 1-2 mana and the bar reads full long
  before the spell is anywhere near its real charge count - cosmetic only there (`advisor_fraction` doesn't
  drive the actual charging, `mana_income` below does). Fixed for a real install's own `constant.dat`
  (`SpellBook::apply_constants`), where `cost` is the real number and this mostly goes away.
- [x] Generated by every follower, busy or idle (`GameMap::mana_income`, headcount.rs, huts-and-training.md rule 7):
  busy (housed or working, `headcount::state_of`) or idle followers at `MANA_BUSY_BRAVES`/`_IDLE_BRAVES`,
  `MANA_BUSY_SPECIALS`/`_IDLE_SPECIALS`, the shaman always at `MANA_F_SHAMEN`, summed and scaled by
  `HUMAN_MANA_ADJUST` (every tribe, human or not: no AI/controller type modelled yet, same gap `training.rs`'s
  `train_cost` already has). Replaces the old fixed `mana::INCOME` placeholder, which made every spell charge in
  a single income tick regardless of population - checked live: a lone shaman now takes Blast (4 charges, cost 40)
  about 11 s, not instantly.
- [ ] Delayed reservoir (gifts, shaman kills), shaman death losses (pop3-rev-analysis.md "Mana rules").
- [ ] Shaman-death mana dispatch (pop3-rev-analysis.md "Shaman death"): not implemented at all yet (no code in
  `mana.rs`). The tribe loses `base * SHAMEN_DEAD_MANA_%_LOST` (`base` = bank, or `needed + stored building mana`
  for a human tribe); if the shaman's last attacker (now tracked, `Unit::last_attacker`/`hurt.rs`) is another
  tribe, that tribe is owed `base * SHAMEN_DEAD_MANA_%_GAIN` through the delayed reservoir above (`give_mana_delayed`,
  1000 turns/83 s). Note the file/exe split already caught in the trace: the level file declares 25 %/25 % for
  `_LOST`/`_GAIN`, but the exe's own disassembly-confirmed value is 25 %/**0 %** - spells.md's current summary line
  only cites the file's 25 %/25 % and should be corrected once this lands, since matching the exe faithfully means
  no mana ever actually reaches the killer.
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

### Buildings ([buildings.md](docs/specs/buildings.md))
- [x] Generated building kit (AI-written script) instead of labelled boxes for every named kind in generated mode: 16 reproducible GLBs, tribe accents, timber construction frames; see [kit workflow](assets/3d/buildings/README.md). Unknown IDs remain labelled boxes; see Sandbox > Buildings.
- [x] Generated procedural building textures: clay, timber grain, straw, stone and cloth; shared mipmapped atlas linked from every GLB, UVs preserved during construction.
- [ ] Generated buildings: coherent part-by-part assembly instead of height-sorted triangles, damage/fire/rubble, working-production animation, hut style variants and building add-ons.
  - **Reported, 2026-10-25**: live play reads construction and destruction as "off" compared to the original -
    the generated kit's progressive reveal is already specced (buildings.md "Building it": "more and more parts of
    the real model show and replace the wood", pieces used / wood needed picking how many parts; "Dismantling and
    damage go the other way: parts back to wood, then removed") but today is approximated by sorting the whole
    model's triangles by height and revealing them bottom-up, not by swapping in or hiding the kit's own named
    parts - so a part can appear half-cut instead of whole, and nothing reads as a distinct, recognisable stage of
    the building. No new original measurement needed for the *mechanism* (the reveal order above is already
    traced-adjacent/specced); what's missing is implementation: give the generated kit real per-part meshes keyed
    to the same piece count the simulation already tracks (`Building`'s wood used/needed), reveal/hide whole parts
    in that order both ways (building up and dismantling/damage down), and only then layer the separate
    damage/fire/rubble visual states (buildings.md "Damage and repair", "Explosion") on top, which do need their
    own live checks per that section's own open questions (e.g. the island's lost-wood-piece rule, the lean/
    oscillator shape of flooded sinking - "Foundation").
- [ ] Check temple = prayer hut object. Reconversion, wall, gate, guard post have no object and no use in the levels: the generated models stand in even with the original files (objects.md).
- [ ] Villager hut style (3 styles in the objects, style 0 always drawn): find what picks it.
- [ ] Check the facing of the other kinds against the game (boat huts are settled: jetty local +z, door -z, from the 11 in the levels), and the cells a footprint takes (used by the walking mask).
- [ ] General model 9 "building add-on" (one near each of 91 medium/large huts): find what it is (hut extension?) and draw it.
- [ ] Sea level: only height 0 is sea in the simulation, but the original draws very low ground (height 1, e.g. the inlet by level 19's boat hut) like water; find the original's threshold. **Partially addressed 2026-10-23, render-only**: `terrain_texture::looks_like_sea`/`VISUAL_SEA_HEIGHT` (4, a guess, not the confirmed threshold) now makes the GROUND SHADER treat height <= 4 as "sea" for its land/water branch choice, separately from `game_core::unit::is_sea` (still exactly `SEA_LEVEL` for all of pathing/drowning/building placement, untouched) - reported (level 38, original assets): a coastal cell at very low but nonzero height was rendering through `land_colour`'s own `cliff` remap instead of as water, reading as lava/rock (a steep drop to the seabed there legitimately pushes `blend` high, and that theme's own cliff table apparently has reddish "damaged ground" entries at high blend - terrain-textures.md "cliff"). The real threshold (and whether it's even a flat height cutoff rather than something else) is still unconfirmed - `VISUAL_SEA_HEIGHT` is picked to clearly cover the one concretely cited case (height 1) with a little margin, nothing more.
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
- [x] Destruction of buildings on uneven/flooded ground: implemented (`game_core::foundation`, docs/specs/buildings.md "Foundation"), both destroy reasons, matching the trace (pop3-rev-analysis.md "Terrain changes: what reacts", "Building sinking (flooded destruction)"). **[ours] a periodic per-building scan** (every `CHECK_EVERY` turns, every turn for a water-sited kind) instead of a reactive hook on `WorldEvent::Terrain` - this doc's own earlier plan - so it never needed routing every direct `Heightmap` write (`Spell::Erode`/`Raise`, construction settling, a reincarnation site's own re-levelling) through that event first; it just re-reads the ground under each building whenever its own rhythm fires, up to `CHECK_EVERY`'s own latency (0.33 s) behind whatever changed it. Sub-state 1 (uneven) shatters at once (`Collapse::Shattering`, removed the tick after so the client can still catch it for `explosion::BuildingExploded` - no more need for `EXPLODE=building:N` to be the only way there, though the dev hook still works); sub-state 2 (flooding) sinks over 80 turns (`Collapse::Sinking`), drawn client-side dropping on the ramping curve and tilting with a decaying wobble (`buildings.rs`'s `sink_depth`/`sink_tilt`) - **the original's own compass-histogram lean target, oscillator formula and final spiral/splash flourish are still not traced**, so the wobble is our own approximation and the flourish isn't drawn at all. Neither reason leaves ashes, matching the trace.
- [ ] A tree whose cell becomes water after any terrain change (land bridge, flatten, erosion, earthquake...) must drop to size 0 (dormant/invisible, like a burnt one) and not grow back while submerged (reported, not traced): `Tree::tick` (`tree.rs`) takes no terrain/water input today, unlike the building check above; needs the heightmap threaded in, likely alongside the same `WorldEvent::Terrain` handling in `GameMap::apply_event`.
- [ ] Damage from moving ground: check in the game what sets the damage (level 10's atlantis island comes back with its buildings missing 1 to 3 pieces of wood, no common percentage), and whether repairs need fetched wood ([buildings.md#damage-and-repair](docs/specs/buildings.md)).
- [ ] Effects 89 atlantis set / 90 atlantis invoke (level 10): sink the island at start, raise it back when the stone head trigger fires; 83 boat hut repair.
- [ ] Towers (drum tower): hold one unit (going in is done, `game_core::enter`), which gets a longer range from there: firewarriors throw farther, the shaman casts spells farther, a preacher converts enemies around the tower. No other tower effect.
- [ ] Training huts (huts-and-training.md "Done"): the queue's ripple (2 turns per rank) and those already of the building's kind letting the others pass, five trained together (the exe's count; ours holds one), queued orders carried over, damage cancelling the queue and sending the trainee out.
- [ ] Training as in the original, what is left (pop3-rev-analysis.md "Training buildings"): the computer players' cost (`CP_` 1000) once tribes have a controller; the trainee's queued orders given to the new unit; the training time to measure in the game.
- [ ] Reincarnation sites from pop3-rev-analysis.md "Reincarnation sites": the ground levelled to the site height over a 5-cell disc in about 21 turns (ours: radius 3, blend 4) with its effects on enemies, wildmen, trees and swamps; pillars rising out of the ground, deleted and moved; the 32 sparkles.
  **Related bug found and partly fixed, 2026-10-21** (TODO.md "Terrain", `docs/archived-plans/terrain-white-patch.md`):
  `ReincarnationSite` had no stored height at all - `flatten_for_spawn` recomputed a *live average* of the
  surrounding ground every time (floored at a too-low `MIN_SPAWN_HEIGHT=32`), not the exe's own fixed, captured-
  once `tribe+0x915` (measured 127-128 in the spec). A site whose local average happened to be low/flat could
  read as water once `row_jitter` became a genuine row-scale mover - reported on `sandbox_spells` and real
  level 10. **Fixed**: added `ReincarnationSite::height`, captured once from the real terrain at site creation
  (`sites_from_level`/`generated_sites`/every sandbox), `flatten_for_spawn` levels to it (`MIN_SPAWN_HEIGHT` now
  only a last-resort floor). Confirmed on level 10: the player's site at (18,126) now correctly captures its
  real height (88), not a floored average.
  **Tried and reverted, same session**: widening `SPAWN_FLAT_RADIUS`/`SPAWN_BLEND_RADIUS` from 3/4 to 5/6 to
  match the 5-cell disc above (the user still saw a nearby house on water after the height fix alone) -
  broke `placement::the_original_levels_boat_huts_fit_the_shore_rule`: on `levl2010.dat`, the player's own site
  is exactly 5 cells from an unrelated boat hut's own shore *only* via the map's torus wrap (123 cells the other
  way), and the wider radius hard-levelled solid dry land over the open sea that boat hut's dock placement
  depends on. Reverted rather than trade one visual bug for a functional regression on a real, already-validated
  level. Left open: does the real exe's own 5-cell effect also reach across this exact wrap (needs a spec
  session - clean room) - if not, something else about how we apply it in one shot instead of growing it over
  21 turns is likely the real difference, in which case the animated-growth half of this same TODO item might
  fix both the "house on water" symptom and this radius question together, rather than a flat radius bump.
- [ ] Houses, see [huts-and-training.md](docs/specs/huts-and-training.md): green (birth) and red (grow) bars in the tooltip, a star animation at birth; capacity 3/4/5, supply 3/5/7 (cap min(supply, 200)), breeding by occupants and population band, mana from the people inside.
- [ ] Houses grow (small -> medium -> large: `villager_hut` sizes): red bar to 100 %, then if the population has room braves fetch wood and the hut becomes a construction site of the next size, still usable (huts-and-training.md rule 6). Growing takes 3 pieces (1 consumed, 2 added to the hut). Settle the red bar speed, the wood per size (3/5/7?) and who works on it before implementing.
- [ ] `game_core::balance`: `Turns` (original game turns) converted to ticks, `Balance::ours()` defaults, later `Balance::from_constants`; every rule reads it from `GameMap` (huts-and-training.md "Balance and game turns").

### Wood ([trees.md](docs/specs/trees.md))
- [ ] Original scenery models 7 plant 1, 8 plant 2: not drawn yet. Scenery 9 (the totems, 98 in the levels) is drawn as the stone head (object 82), a guess: find its real model. Trees have no size in the thing record: full size is right.
- [ ] Wood for repair and hut growth, like construction (`game_core::work`: assigned braves fetch, carry to the door, build). Cutting, carrying, fetching and construction are done (units.md "Wood", buildings.md "Construction").
- [ ] Check in the game whether a brave ordered on a tree with no wood to spare goes to another one.
- [ ] A tree does not grow back while a building stands on it; buildings can only be placed over size-0 (invisible) trees.
- [ ] Trees as obstacles for walking (around full trees?), to check against the original.

### Praying (worship) ([worship.md](docs/specs/worship.md))
- [ ] Prayer gauge on the original's rhythm and curve (pop3-rev-analysis.md "The gauge"): every 4 turns, `C² / (C − n + 1)²` per update, drain `C²`; today it steps every tick with `STEP · (n/C)²`.
- [ ] Contested totems: keep one gauge per tribe (our choice; the original has one per trigger, driven by the tribe with the most prayers), but the gifts are shared: a totem's `NumOccurences` counts across all tribes, so a one-gift totem rewards only the first tribe to fill its gauge, and the other gauges stop. Once `NumOccurences` is reached by any tribe, the totem sinks back into the ground for everyone and every tribe's prayers are sent away (`Totem::is_exhausted`: one shared sink, whoever took the last gift). Check `Totem::given` does this and test it with two tribes.
- [ ] Totems: ids (`GameMap::new_id`) and removal from `GameMap::totems` once sunk (exhausted after their gifts, `NumOccurences` times; infinite ones stay), then `Order::Worship` / `Action::Worshipping` by id.
- [ ] Stone totem: check in the game what it does after a completion that is not its last, the timings (turn 2 s, hold 1 s, sink 4 s) and the smoke; other turns to try are listed in worship.md.
- [ ] Totem gifts not handled yet (logged as `Reward::Unhandled`): effects (land bridge, lightning...), revealed scenery and triggers, vehicles, the Angel of Death of type 5 totems; mana once mana exists. Check what `NumOccurences` 0 means.
- [ ] Pyramids: done for the player (`game_core::worship`); rewards per tribe once books are per tribe (today `granted` updates the shared level books and the player's panels); `PrayTime`'s unit is now confirmed live (worship.md "Open", pop3-rev-analysis.md "The vault"): the same 4-turn tick as a totem's gauge, cap = `PrayTime` directly. Still to check in the game: the drain speed with nobody praying (not observed live yet).
- [ ] Prisons: levels where the shaman starts imprisoned; she is locked inside (units.md "Locked units") until freed.
- [ ] Generated pyramid: fold its capstone like the original's top (a separate mesh in `tools/generate_buildings.py`).
- [ ] Pyramids: draw the reward as an icon at the top (cursor sprites 38-57 spells, 58-65 buildings), gone once the reward is granted.
- [ ] HUD: show the gauge over the totem/pyramid and who is praying.
- [ ] HUD sparkle/flash when a discovery is granted at a totem or pyramid (not traced: no sparkle effect or
  timing pinned down yet, just a gap noticed - a reward landing today has no feedback of its own beyond whatever
  the reward itself does, e.g. a spell appearing in the book). Needs a live check of what the original actually
  shows at the moment of the grant before implementing anything beyond a placeholder.

## Priority 2 — `--no-original` asset completeness (copyright safety)
AGENTS.md's own bar is that everything shippable works with `--no-original`. Sound is a hard zero today (format reverse-engineered, no playback, no CC0 assets sourced at all); several spell VFX, building/spell icons and ritual scenery are still original-only. Largely independent of the other tiers, so it can run in parallel with them rather than waiting its turn.

### Sound ([sound.md](docs/specs/sound.md))
- [ ] `pop3_format::sound`: SDT banks (count, offsets, 40-byte headers), signed 16-bit PCM mono/stereo, the 4 looping sounds, `0x80` placeholders skipped; music `PopDrones22.SDT` as MPEG Layer II.
- [ ] Play SFX in the client (Bevy audio): spells, chopping, building, follower voices, ambiences; drums and music. Free sounds for `--no-original`.

### Art ([assets.md](docs/specs/assets.md))
- [x] Audit the original-data-free path and missing assets across buildings, units, scenery, VFX, UI and audio ([inventory](docs/specs/assets.md)).
- [ ] Next open art: building icons, ritual scenery/reincarnation stones, vehicles and worker action clips (inventory P1). CC0 trees/unit sheets, the generated building kit and 14 of 20 spell icons already ship.

### Original sprites and blending ([sprites.md](docs/specs/sprites.md))
- [ ] Spell effects from the alpha sprites (`hfx0-0.dat`, decoded by `blend::AlphaTable`, blended on the GPU): map each effect to its frames (sprites.md).
- [ ] Ghost table `ghost0-X.dat` (66% mix) for see-through units (Invisibility).
- [ ] `sprites` parser: test the empty (0 x 0) entries and the `0x7F` trailer seen in the real banks.

## Priority 3 — computer players (AI) and what it needs
The single biggest remaining system, and the one that turns this from a sandbox/multiplayer tech demo into an actual single-player game. Sequenced after priority 1 on purpose: AI scripts can invoke any spell/building/unit action through the same order/coordinator machinery, so building the interpreter against still-incomplete systems means repeated rework. Tribes and level scripting are its direct prerequisites (per-tribe state, triggers the scripts fire).

### Tribes
- [ ] A tribe model in game-core: `TribeId` instead of bare `u8` owners (blue 0 .. green 3, wildmen 255), its colour
  in the engine instead of the client's `sites::tribe_color` / `procedural::tribe_rgb`, the tribes playing the level
  (header byte 88: 1v1, 1v1v1, 2v1...), allies (header byte 92, see "Default allies" below), human / computer,
  defeated; holding the player's mana pool, spell book and build book (today `GameMap::mana`, `spell_books`,
  `build_books` by owner).
  - Reported: red has no reincarnation site on `levl2001.dat` (level 1) in play, suspected to be this same gap
    (header byte 88 unread). Level data itself does place a red (owner 1) shaman Thing (`#38 model 7 kind 1
    owner 1 at (2304,7424)`, alongside blue's `#35 owner 0`) - `site::sites_from_level` takes any owner with a
    placed shaman Thing with no regard for which tribes the header's own byte 88 (`tribes` field, read as `2`
    for this level, i.e. a 1v1 code) actually selects, so if that byte means a specific pair of slots rather
    than "first 2 owners found", we may be including/excluding the wrong one. AI scripts (blue/red/yellow/green)
    read `[0, 10, 4, 5]` for this level - 3 non-zero, more than a "1v1" would suggest are active. Not root-caused
    this session; needs byte 88 actually decoded and cross-checked against which owners the original instantiates.

### Level scripting (triggers)
- [ ] Trigger semantics (layout and types known, level-format.md): what each type waits for (proximity, timed, player death, shaman proximity, library, shaman + angel of death), how `TriggerCount`, `NumOccurences`, `PrayTime` and `InactiveTime` play, and what activating a target does (discovery granted, effect fired, hidden thing revealed).
- [ ] Praying totems, stone heads (scenery 9), vault of knowledge: which trigger/discovery each uses, once or repeatedly.
- [ ] Level 5: a countdown (about 15 min) starts at some point; the winged death totem must be prayed at before it reaches 0. In the level: trigger #98, type 5 (shaman only, brings an Angel of Death), on cell 83, 65 (the start cell, marker 11), `PrayTime` 10, `CreatePlayerOwned` 1, targets effect 91 (same cell) and effect 88 (cell 106, 116). The timer is not in the level things: probably red's script cpscr058 (`SET_TIMER_GOING`, `HAS_TIMER_REACHED_ZERO`; the spec says level 5's script ends the level as won). Decompile it (`ai_script` above) to learn when the timer starts, what reaching 0 does and how the totem stops it; then implement its reward (not done yet, unlike the other totems).
- [ ] Implement the triggers and discoveries as deterministic simulation (through `Command`/state, no floats); AI scripts can fire them too (`TRIGGER_THING`).

### Computer players (AI)
- [ ] AI for the computer-controlled tribes: their shaman and followers act on their own through `Command`s (deterministic, like a player's input): gather wood, build and grow the village, train units, pray, cast spells, attack and defend.
- [ ] `pop3_format::ai_script`: parse `cpscrNNN.dat` (12 552 B only) into fields and a statement tree per [ai-scripts.md](docs/specs/ai-scripts.md) (grammar, token, parameter and internal variable tables are there), with a decompiled listing; `cpatr` name and masks. Then `just level-info` prints the scripts of the level's tribes decompiled next to its things, triggers and markers, so a level's events can be read in one place.
- [x] `pop3_format::constants`: decode `levels/constant.dat` (XOR key, 2-byte marker, `P3CONST_` lines) per
  [constants.md](docs/specs/constants.md) into a plain `name -> value` map (`Constants::parse`/`load`,
  `[ours]`-tagged guess at which of the two forms — encoded or the community's plain text — a given file is
  in, since the real loader's own test is unconfirmed; a malformed line is skipped, not fatal). Wired through
  for spells only so far (`game_core::spell_book::SpellStats::from_constants`/`SpellBook::apply_constants`,
  `GameMap::load_original`): cost, charge count, advisor pacing and range, with our own defaults
  (`SpellStats::default_for`) when an install isn't present or its file doesn't parse.
- [ ] The rest of `constant.dat` (units life/speed/damage, training costs, wood, hut breeding) still uses its
  own hardcoded placeholders across the codebase, not `pop3_format::constants::Constants` — reconcile one
  group at a time the same way the spell group was (`cost`'s own split stays: our placeholder scale by
  default, real numbers only from a loaded file). `CONV_SPY`'s own duplicate entry (unit group, then building
  group) is read "by position" per constants.md, unconfirmed and irrelevant to every group read so far:
  check if any future group actually needs position-sensitive duplicate handling before adding it.
- [ ] Lockstep: two peers resolving different `SpellStats` for the same spell (different installs, or one
  with and one without) would silently diverge mid-match — not checked anywhere today. Needs either a
  same-constants precondition at game start (like level/version matching presumably already is) or hashing
  the resolved values into the handshake; `game-net` doesn't touch `SpellBook`/`Constants` at all yet.
- [ ] Script interpreter in `game-core`: user variables, internal variables read from the simulation (`headcount`: units by state and kind, `housing`), `EVERY n m` firing when `(turn + m) % n == 0` (stored minus 1), DO commands mapped to AI states and `Command`s; start with the states, `ATTACK` and `SET_SPELL_ENTRY`. Levels 2100, 2110, 2131 name missing scripts: fall back to no script.
- [ ] Script timing: measure the original turn rate (wiki: about 8 turns/s) against our 10 ticks/s, and whether `EVERY` adds a per-tribe phase, before porting script timings (see [ai-scripts.md](docs/specs/ai-scripts.md) "What this means here").
- [ ] Script side effects outside the AI: messages, flybys, `GIVE_ONE_SHOT`, `GIVE_MANA_TO_PLAYER`, `TRIGGER_LEVEL_WON/LOST`, user input lock (campaign scripts).

## Priority 4 — remaining fidelity and content
Level/world correctness that is mostly stable already (terrain especially: extensive, but most of it recently fixed) rather than blocking anything above.

### Terrain ([terrain.md](docs/specs/terrain.md), [terrain-textures.md](docs/specs/terrain-textures.md))
- [ ] Curvature in a vertex shader sampling an R16 height texture (static grid).
- [x] Dirty-rect re-bake of the texture/normals instead of full rebuilds: mostly already true -
  `terrain_texture::rebake_cells` (channels/altitude) and `terrain_mesh::patch` (vertices, and the normals of
  every touched vertex plus its 4 neighbours) were both already dirty-rect-limited, only falling back to a full
  `bake_cells`/`build` on a level switch or a camera move (legitimately need the whole visible disc). The one
  remaining full-map scan was `terrain_texture::walkable_cells` (the channels texture's own walkable/coastal
  flag), re-run over the entire 128x128 map every tick that touched terrain at all (an ongoing Volcano rise or
  lava flow dirties a handful of cells almost every tick). **Fixed 2026-10-21**: `rewalkable_cells` updates just
  the cells whose own `is_sea` (corners `(x,z)..(x+1,z+1)`) could have changed from a dirty rect - one cell short
  of it on the min side, two new unit tests proving it matches a full recompute and leaves cells outside its
  reach untouched. Wired into `world.rs::rebuild_terrain` in place of the old whole-map call.
- [x] `terrain-textures.md` "disp"'s per-theme byte ranges (theme 1 45..127, 5 130..193, 7 150..170, c 73..106)
  were measured against a corrupted install (`AppData/Local/Bullfrog/Populous/data/`, a leftover from a
  badly-uninstalled texture-pack extension - deleted). **Re-measured 2026-10-21, all 36 `disp0-X.dat` files
  against the genuine 1998-dated `Program Files (x86)/Bullfrog/Populous - A l'aube de la création/` install:
  every single theme spans the full raw byte range 0..255** (one, `f`, 0..254) - none is narrow, none stays on
  one side of 128 either, so the old "no file crosses 127/128"/"narrow off-centre range is the bias" reasoning in
  terrain-textures.md was wrong too, not just the numbers. Real means differ per theme (82.9 `s` to 167.5 `g`/`i`)
  - a gentler distributional skew, not a hard range restriction. Several theme letters share one file byte-for-
  byte: `0`/`q`/`v`/`w`/`x`/`y`/`z` (placeholder for unused slots), `c`/`d`, `l`/`m`. No code change needed -
  `pop3_format::theme::disp_mean()` is already dead code (called only by its own unit test). `bigf0-X.dat`/
  `pal0-X.dat`/level `.dat` files were confirmed byte-identical between the two installs, so this was `disp`-only.
  terrain-textures.md "disp" updated with the corrected figures.
- [x] Height -> colour row scale (pop3-rev-analysis.md "Terrain land colour", terrain-textures.md "bigfade"/
  "disp"/"Our bake"): fixed — `POP3_ROW_SCALE` is gone; the row is now `clamp(height + 75 + (75 if blend != 0
  else 0), 0, 1022)` (`game_core::shading`'s per-cell `blend`, see below), `disp`'s row-jitter follows the
  altitude-dependent sensitivity curve (`pop3_format::theme::disp_sensitivity`), and the brightness column comes
  from a precomputed per-cell `light` byte (`game_core::shading::CellShading`, a height-gradient dot the level's
  sunlight-derived vector, fed by `pop3_format::level::Sunlight`) instead of a baked Lambert light of the
  rendered mesh slope. `land_start`'s own floor was kept at first as a renderer-side safety net on top of the
  faithful formula (a freshly flattened, perfectly flat low platform has blend 0, so the bare formula alone can
  read close to the sea's own rows there) - since removed, see below.
  `SunParams::default` now returns the live-measured, disassembly-confirmed flat constant `sunX = sunZ = sunBase
  = 147` (pop3-rev-analysis.md "Terrain land colour", 2026-10-20) instead of scaling `ShadeRange`/`ShadeStart` -
  the level's sunlight block plays no role in it, matching the exe's `sunlight_init_default`. Left open: whether
  the gated day/night sun-angle tick (0x401350) or the fixed-Inclination-64 path (0x401920) ever actually run in
  some mode (both measured OFF this session); how the original maps its sky (backdrop scrolling with the
  camera?), not traced this session either.
- [x] `game_core::shading::cell_light`'s slope sample overshot: it read `height(x, z) - height(x + 2, z)` /
  `height(x, z + 2) - height(x, z)`, a two-real-cell baseline, where the original's `compute_cell_light`
  (0x4BE230) reads a one-real-cell baseline (pop3-rev-analysis.md "Terrain land colour", 2026-10-20 correction —
  proven from `load_level2`'s height-table load loop, not assumed) — fixed, now `height(x + 1, z)` /
  `height(x, z + 1)`. Confirmed independently correct (unit tests updated, `just test` passes), but on
  `levl2001.dat`'s own decorative hill (theme `c`) the near-white shoreline band this was found from is only
  partly better after the fix: raw heights there are steep even over a single real cell (z=11: x=9..12 is
  412, 353, 191, 0), so some clamping still happens, and separately `bigf0-c.dat` rows 128-139 (the
  `land_start_row()`-floored "lowest dry land" band) are pale/sandy-grey across most of the realistic
  brightness range, not just at the clamped extremes - any shore cell floored into that band reads pale
  regardless of `cell_light`. See the next entry: `land_start_row()`'s own floor has no original basis at all,
  and is the more likely remaining source of this paleness, not a miscalibrated value to retune.
- [x] `pop3_format::theme::Theme::land_start_row()`'s floor had no counterpart in the original (`0x4BE330`
  corner trace): removed (`land_start_row`/`sea_like`/`MAX_SHORE_ROWS` gone from `pop3_format::theme`, the
  `land_start` uniform dropped from `ground.wgsl`/`ground_material.rs`/`world.rs`). Didn't fix the white/pale
  patch on levl2001.dat's hill (theme `c`) by itself - see below.
- [x] **White/pale patch on steep slopes, root-caused and fixed 2026-10-21** (`crates/game-client/src/
  ground.wgsl`, "Our bake" below, pop3-rev-analysis.md "Terrain land colour"): two compounding implementation bugs
  in the land branch, both confirmed against instruction-level disassembly (`pop3-rev/docs/functions.tsv`
  0x4C0EC0/0x4BF860/0x4C0380) rather than guessed:
  1. The `row`/`row + 1` bilinear RGB blend (`7a6d98d`) had no basis in the exe at all - it reads `bigf0-X.dat`
     exactly once per pixel, at a single flat index, never blending two rows. Fixed: a single hard-floored
     `row = floor(altitude)`, no blend.
  2. `row_jitter` (disp's own row-moving term) was being summed directly alongside `brightness` as if it were
     column-scale, but the exe's `and ecx,0xFFFFFC03; sar ecx,0x2` masking-before-shift makes it always a
     multiple of 256 - i.e. it moves the picked **row**, not the column, by up to dozens of rows on steep/high
     terrain. Under-scaling it by 256x left a steep hill's whole altitude-driven row range far more static/
     exposed than the original, which is what kept the patch visible even after fix 1. Fixed: `row = altitude_int
     + row_jitter_rows`, multiplied by `BIGFADE_WIDTH` once.
  `cell_light`'s own formula/divisor/clamp were separately confirmed faithful along the way (not the cause) - a
  live measurement of a known steep row matched our prediction exactly (109/109, 165/165, 54/54, 128/128).
  Verified both by a pixel-level Rust-side replica of the shader against the real `levl2001.dat`/theme `c` data
  (pale-pixel count in the repro's own hotspot: 32 -> 0, the few remaining hits relocated far away) and visually
  by the user against the real 1998 install. Two earlier wrong attempts along the way, both reverted: folding the
  altitude's live fraction into the column axis (produces a dense zigzag - the exe explicitly discards that
  fraction with its own mask) and accepting that same zigzag as a "hedgehog noise" trade-off (unnecessary once
  `row_jitter`'s scale was fixed instead). Full blow-by-blow: `docs/archived-plans/terrain-white-patch.md`
  (archived per AGENTS.md "Plans" - this investigation took real time and iteration).
  Also found along the way, still open, separate from the above:
  - `game_core::shading::CellShading`'s `ambient_nibble` is hardcoded `0`, but it's really a dynamic 0-15 shadow
    cast by buildings/trees/scenery on or near a cell (`landscape_move_1` 0x403A00, `landscape_set_unit_shadow`
    0x403C10, `calc_unit_shade_value` 0x450D50) - wrong on any cell near such an object. Needs
    `calc_unit_shade_value`'s per-object-type weighting traced and a real per-cell source wired in.
  - A runtime table at 0x96EAE0/0x970AE0, read once more after `cliff0-X.dat`, indexed by the cliff output plus a
    fourth, unidentified per-corner fixed-point attribute (present in `landscape_texture_32` too, not 16-bit-depth
    specific) - untraced, role and whether our ground shader needs it both unknown.
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
  A **different** aliasing surfaced later (2026-10-21), once the white/pale patch fix above removed the
  row/row+1 RGB blend: that blend had been incidentally smoothing the `bigfade`/`cliff`/`palette` chain's own
  categorical `textureLoad` (no filtering), so a faithful single hard-floored row showed a visible hard edge
  wherever `row`/`row_jitter` changes between neighbouring fragments on steep ground - this is a genuine
  per-fragment aliasing issue, not a mip/distance one, so the "nothing left to mip" framing above doesn't cover
  it. Fixed with a renderer-only anti-aliasing pass (`ground.wgsl::land_colour`, factored out and evaluated at a
  small jittered 2x2 sub-pixel grid per fragment, averaged) rather than reintroducing any row-blending into the
  formula itself - same spirit as `soften_light`, confirmed by the user visually.
- [x] Snow appears at high elevation in every theme today + `BANDS` row boundaries never recalibrated after the
  row scale changed - tackled together (2026-10-21), as this entry's own earlier note asked: `procedural_theme.rs`
  renamed `BANDS` to `BAND_STARTS` (row boundaries only) with a separate `band_colours(seed)`. Row boundaries
  recalibrated for the real `height + 75 + (75 if blend != 0)` formula (`blend` alone adds ~150 to almost any
  non-flat cell, which the old 0/64/128/180/320/560/760/960 boundaries - picked for the since-dropped
  `POP3_ROW_SCALE` - never accounted for, pushing ordinary modest terrain into "dark grass"/"rock" and reading as
  unexpected near-black patches): now 0/64/128/170/260/620/860/990, so "grass" covers the bulk of ordinary
  terrain instead. The top band is no longer a hardcoded white "snow" - `band_colours` picks one of 4 plausible
  extreme-ground tones (pale snow, pale blue-grey rock, sun-bleached sandy rock, plain weathered rock) per seed,
  matching that real themes diverge a lot at the row ceiling (terrain-textures.md "Themes seen" - only one
  sampled theme clearly goes near-white there). Four new/updated unit tests (varies across seeds, still reads
  lighter than rock whichever tone, ordinary modest terrain lands in "grass"). Verified visually (`AERIAL=1 just
  shot --no-original`): ordinary green terrain with sand shorelines, no near-black patches, no forced snow caps.
  Left for later, not addressed here: `procedural_theme::synthetic_cliff`'s own shape (a separate, `blend`-driven
  axis, not height/row - this entry's "needs its own pass" was about both, only the row/band half is done).
  **Regression found and fixed, 2026-10-21**: `sandbox_spells`' reincarnation site (flat land, height 64, blend 0
  -> altitude 139, just 11 rows above `BIGFADE_LAND_ROW`) rendered as deep water, then as a fine blue "vein"
  pattern threading through otherwise-dry ground after a first attempt. Root cause: `row_jitter`'s own scale fix
  (above) made it a genuine per-ROW mover, and `disp_sensitivity`'s shared, faithful 320 floor (never smaller,
  even on the lowest/flattest land) means there's no "gentle near the shore" to lean on - any land near the
  water threshold can dip into bigfade's own water-authored rows. Fixed in `procedural_theme.rs` only (the one
  thing fully ours to shape; `ground.wgsl`'s shared formula must stay exe-faithful, no clamp to add there):
  `BAND_STARTS`'s own "sand" moved well below `BIGFADE_LAND_ROW` (100, not 128) so a dip still reads as sand, not
  water; and - the real fix for the vein pattern - `disp_noise` was found to be centring its output on the
  *unsigned* midpoint 128, which is actually the *signed* range's own extreme once read as `i8` (raw 128 decodes
  to -128, not a small value), so "narrowing" it that way never shrank the signed jitter at all. Reworked into a
  separate `water_noise` (unsigned, unchanged) and a `disp_noise` centred on the signed zero instead, with its
  own `DISP_AMPLITUDE` (0.35) taming the swing directly. Two new unit tests, verified visually (`POP3_START=
  sandbox-spells`): clean sandy ground, no water patch, no vein pattern, real shoreline still reads as water.
- [ ] Same "flat, low platform dips into bigfade's own water-coloured rows" class as the `sandbox_spells` entry
  above, now also seen on a **real level/theme** where there is no equivalent lever: `levl2010.dat` (theme 25),
  reincarnation site at cell (18,126), stored height 88. The plaza cells around it (roughly x=17..19, z=122..125)
  render patches of the same sea-like colours (`cell_debug.rs`'s new hover readout can point at the exact cell;
  `levl2010.dat` reported fresh "corners" at 88/88/88/88, so not a coast/`is_sea` case - this is the land branch
  picking a water-authored row). Confirmed this is raw, *unmodified* level data, not our own `flatten_for_spawn`
  reshaping it: the level author's own height table already reads flat 88 there before any site leveling runs
  (checked both against `Level::height` pre-flatten and `GameMap::terrain` post-flatten - identical in this
  region). With blend 0 (perfectly flat) and height 88, `altitude_cell` = 163, only 35 rows above
  `BIGFADE_LAND_ROW`; `disp_sensitivity(163)` = 425, so a per-pixel `disp` byte below roughly -85 (about a sixth
  of the signed range) alone drags the picked row under 128 - measured 60-96% of one cell's own 8x8 sub-pixel
  grid dipping there, matching the visible blue blotches. Unlike `sandbox_spells`, there is no per-theme table
  here that's ours to retune (theme 25's `bigf0-*.dat` is the original 1998 file, unmodified, and "Our bake"
  above already confirms from the exe trace that a flat low platform reads sea-like in the original too, with no
  floor guarding it) - so this may simply be faithful to what the real exe also shows at this exact spot, not a
  remaining bug in `ground.wgsl`. Left open for a spec session to confirm against the real exe (clean room: a
  coding session doesn't run/trace it itself) before concluding either way.
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
- [x] The sea's own ripple (pop3-rev-analysis.md "Water rendering (watdisp.dat)"), after several same-day
  attempts (2026-10-22) each reported as still looking wrong - full history, so a future session doesn't repeat
  it:
  1. Drove colour straight from the exe pseudocode's own `height_offset`/`light` (a sum of two `watdisp` reads
     scrolled in opposite directions as `turn % 256` advances): stayed anchored in place at first (summing
     cancels the translation - looked frozen, "a different rotation" only between separate level loads); once
     fixed to scroll a single sample instead, it swept into a theme's own shore-coloured `bigfade` rows (tan
     patches afloat in open sea) and read too dark against the coast ("coast water color is correct... the water
     is too dark").
  2. A synthetic time-only "boiling" brightness nudge (each cell's own fixed `watdisp` byte seeding a per-cell
     sine phase): genuinely not a scroll (cross-correlating far-apart video frames gave an optimal alignment
     shift of exactly `(0, 0)`), but still read as motion with no real geometric basis ("do you alter the water
     altitude... do it way slower").
  3. A real vertex-shader height displacement with an ad-hoc slope-driven brightness nudge: reported as still
     looking like motion, and reverted to fully static (no time input anywhere) on that report - verified by
     cross-diffing every frame of a recorded video, zero pixel change in the sea region across 8 real seconds.
  4. A static watdisp.dat ripple texture (sampled at a single fixed position, no time input) layered onto the
     static brightness, after "insert the water0.bin effect... read as a flat 256x256 byte grid, it resolves into
     a smooth, periodic 2D field".

  **Root cause of 1-3 reading as wrong, found: the user's own recollection that the real game's sea never visibly
  moves was itself mistaken** - corrected by the user citing a third-party source, [peterderivaz/populous](
  https://github.com/peterderivaz/populous) (README "References"), which explains the real mechanism in detail:
  the sea IS rendered with vertex altitude and lighting that change continuously, via the exact two-counter-
  scrolling-sample formula attempt 1 and 3 already used - the source explains WHY two samples, not one: a single
  scrolling sample would show a fixed wave pattern moving in a fixed direction (an obvious prevailing wind);
  summing two samples scrolling in OPPOSITE directions cancels that net directional drift while still producing
  "lots of local swirls and seemingly non-uniform motion". So attempts 1 and 3 were not wrong to animate the sea -
  attempt 1's bug was only ever picking colour from a shifted sample (mathematically a scroll regardless of
  intent, terrain-textures.md has the detail) and attempt 3's row/lighting formula were ad-hoc guesses, not what
  made the animation itself wrong.

  **Final implementation, matching the confirmed mechanism**: `GroundMaterial` has a real vertex shader
  (`ground.wgsl`'s `vertex`, `Material::vertex_shader`) that displaces each pure-sea vertex's own `y` by
  `water_height_offset` (the two-counter-scrolling-sample formula, mirroring `Theme::water_shimmer`'s
  `height_offset = sum / 8`), converted via the live `terrain_mesh::CurveParams::height_scale` (`ground_material::
  set_height_scale`, kept in sync every frame by `world.rs::animate_water` alongside `set_turn`) and faded to zero
  approaching the coast. The normal is tilted by that height field's own local slope; `water_colour`'s brightness
  (`water_light`) now mirrors `game_core::shading::cell_light`'s own formula exactly (same sun constants, same
  ambient term) fed the live ripple height instead of the real terrain height - "the dynamic lighting is based on
  the adjusted altitudes instead of being distance based", replacing the land's precomputed `light` sample for
  this one purpose rather than nudging it. Row stays fixed at 0 (never a shore colour, any theme). Verified: no
  shader errors; cross-correlating far-apart video frames finds only a marginal improvement at a nonzero shift
  (not the dramatic drop a real net scroll would show) - no overall directional motion; consecutive frames show
  real, moderate, ongoing local variation (10-40% of sea pixels changing a few units each, every quarter second) -
  genuine animation, not frozen, not a uniform flow either. Left open:
  - Not yet re-confirmed against the original's own files - no original install is available in this environment;
    checked only against the procedural theme here and against peterderivaz/populous's written description, not
    the real game directly.
  - The exact bit layout of `cell_coord x8 -+ (turn % 256) x 257` (pop3-rev-analysis.md's own pseudocode) isn't
    pinned down - `Theme::water_shimmer`'s/`ground.wgsl`'s own reading (a plain diagonal pixel shift, reusing the
    existing `px`/`py` `PX_PER_CELL` coordinate space) reproduces the doc's own stated property ("(1,1) per turn")
    but doesn't claim the original's bit-for-bit packed-word addressing. A spec session could pin this down from
    the disassembly directly.
  - `water_light`'s own formula (mirroring `game_core::shading::cell_light`, sun constants 147/147/147) has since
    been replaced entirely by a continuous normal/slope shading - see the next entry.

- [x] The sea's own colour banded into hard, concentric contour rings instead of the original's soft diffuse
  shading, and its ripple pattern read as a few giant smooth blobs instead of choppy water - both confirmed
  against a clean-room visual description of a real screenshot (terrain-textures.md "Water: visual reference",
  2026-10-23, kept deliberately independent of this session's own implementation knowledge): individual features
  measured 150-300 screen px across in a comparably-framed shot of ours, 30-50x larger than the reference's own
  3-8 px, and "only two colours are visible, with crisp edges between them" - textbook symptoms of (1) a sampled
  pattern tiled far too coarsely and (2) colour quantised/thresholded before shading rather than lit continuously
  from the surface slope. Both confirmed as bugs, not readings of real original behaviour: (1) `water_height_offset`
  was sampled at `PX_PER_CELL`'s own per-cell granularity (tiling the 256-texel `watdisp` once every ~32 cells)
  with no further scaling; (2) `water_colour` rounded its brightness to an integer and looked it up through the
  same discrete `bigfade`/`palette` chain land uses - categorical by construction, so adjacent fragments whose
  brightness crossed an integer boundary did show a hard edge, same mechanism `land_colour` already needs
  supersampling to hide, but applied here to a signal with enough contrast to read as rings rather than subtle
  aliasing. Fixed: `WATER_FREQUENCY` (24x) tiles the pattern well below one cell; `water_normal` (a centred finite
  difference, more accurate than the one-sided difference the vertex displacement still uses for itself) builds a
  continuous surface normal, lit by a fixed direction and mixed smoothly between two colours - no palette/row
  lookup for water's colour at all any more, so nothing left to band. Caught a second bug applying the fix: the
  two mix colours are sRGB (gamma) values, like any hex code read off a screenshot, but were first used directly
  as linear values - the swapchain target being itself sRGB meant the GPU encoded them through the sRGB curve a
  second time on store, washing the water out much paler than the intended deep navy (confirmed visually: a
  washed-out pale blue, not reasoned from first principles) - fixed with an explicit `srgb_to_linear` conversion,
  the same class of bug "Our bake"'s own "Colour-space bug, fixed" entry already covers for the palette texture.
  Verified: measured pixel colours land in the reference's own colour family and order of magnitude (not pixel-
  perfect - the reference's own figures are approximate, and lighting/zoom weren't identically reproduced); a
  close shot shows fine mottled texture with no banding; the "no net directional motion, local swirls" property
  from the previous entry still holds with the new frequency/normal-scale constants.

- [x] The sea's two shading colours were universal constants (measured off one reference screenshot) - reported
  as visibly clashing with a generated theme whose own land colours happened to be teal/brown, not blue-adjacent:
  a fixed water hue can't harmonise with every theme's own, independently-chosen palette. Tried sourcing both
  colours from the THEME's own `bigfade` row 0 instead (the historical "sea row" convention, never confirmed
  against the exe) with a "fall back to a fixed blue unless the theme's own colour is actually blue-dominant"
  guard against a theme whose row 0 isn't water-coloured - reported as the wrong fix in turn: on another level
  (38) the water rendered red, which turned out NOT to be `water_colour` reading a bad theme colour at all, but a
  separate, already-documented class of bug (very-low-but-nonzero ground reading as land, not sea - the "Sea
  level" entry above, since fixed). Left as: both endpoints come from the theme's own `bigfade` row 0 at two fixed
  brightness columns (`WATER_DARK_BRIGHTNESS`/`WATER_LIGHT_BRIGHTNESS`), no fallback guard - the guard was solving
  a problem that didn't originate in this function, and reintroducing it without evidence it's still needed would
  just be guessing twice. If a genuinely bad per-theme row 0 turns up again (as opposed to a misclassified cell
  reaching this function at all), revisit then with that concrete case in hand.

- [x] Small/coastal water visibly rippled when it shouldn't have, fixed twice the same day:
  1. Reported: a small 2x2 pond; confirmed, level 8 cell 89/90 - a coast cell, not deep sea. Root cause:
     `vertex`'s own coast fade (`sea = 1.0 - smoothstep(0.0, COAST_THRESHOLD, walk)`) reused `COAST_THRESHOLD`
     (0.2), a value tuned purely for the land/water COLOUR boundary's own shape - but `walk` (a bilinear average
     of up to 4 corners' own walkable flag) only ever takes the exact values 0, 0.25, 0.5, 0.75, 1.0, so ANY
     land-adjacent vertex (walk >= 0.25) already sat well clear of that 0.2 margin and still got a small but
     visible fraction of the full ripple - wrong by the exe's own documented rule (pop3-rev-analysis.md "Water
     rendering": "land and shore vertices take the real heightmap value instead, no watdisp involved" - a hard
     per-vertex binary, not a graded fade). Fixed: a separate, much tighter `WATER_ANIMATION_MARGIN` (0.02) gated
     both the vertex displacement/normal-tilt and the fragment's own lighting off the SAME single-point `walk`
     value. Verified at the time: individual near-shore water pixels varied only by GPU dithering noise across 10
     consecutive frames, the same magnitude flat land shows; deep water still animated.
  2. Reported again: a small pond kept changing elevation even inside its own shore (level 1, cell 6/101) - a
     single point's own `walk` value can't tell "deep ocean" from "the interior of a small pond, fully surrounded
     by water-classified neighbours at this one point", since both read `walk == 0` there. Fixed:
     `water_sea_factor` replaces the single-point test with a coarse multi-sample one - a ring of 8 points
     `WATER_OPENNESS_RADIUS` (3) cells out, plus the centre; only when EVERY one of those is also still "sea"
     does the ripple reach full strength, so a body of water has to be open for several cells around a point, not
     just at that exact point. Approximate (a ring can miss a thin obstacle between two sampled directions, and
     this measures "no land at roughly this radius", not the water body's true size/shape) but much closer to
     "only open ocean ripples". Verified: confirmed near-shore water pixels at zero variation across a fixed-seed,
     20 s capture; a confirmed-deep ocean region (identified from an aerial shot, several cells from any island)
     still shows substantial ongoing change over that same window (11.7% of pixels, mean nonzero delta ~7, max
     122) - a short capture window can under-sample how fast a specific deep-water location's own local swirl
     phase happens to be changing, so use a window of several seconds, not under two, when checking this again.

### Winning and losing
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

### Things and level data ([level-format.md](docs/specs/level-format.md), [objects.md](docs/specs/objects.md))
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

## Priority 5 — presentation and UX polish
Additive and mostly independent of the tiers above: chip away at these alongside anything else.

### UI and editor ([ui-and-editor.md](docs/specs/ui-and-editor.md))
- [ ] Notifications (ui-and-editor.md "Notifications", reported, nothing implemented yet): a stacking icon area
  right of the panels - information notices open a modal dialog, "preaching" and enemy-spell-cast notices jump
  the camera to the event instead (enemy-spell ones also show a small vertical bar in the casting tribe's own
  colour), and a separate tower alert (sound only) plays when an occupied tower/building spots a nearby enemy.
  pop3-rev-analysis.md "Notifications" has the exe-traced advisor-message system (`fire_advisor_message_once`, a
  plausible match for the plain "information" dialogs) and the now fully-read `0x44D7F0` (10 categories, one of
  which draws a per-tribe-coloured bar - a strong match for the enemy-spell indicator) - but its trigger for an
  automatic "an enemy just cast a spell" alert wasn't found (only reached from the level-script interpreter so
  far), nor was whoever draws the stacking icon itself from the record it writes. The tower alert and the preach
  notice's own trigger still need tracing before this can be built.
- [ ] Building icons, tooltips (spell icons done: `assets/spell-hud`).
- [ ] Spies: hovering one shows a "spy indicator" (in the original a cursor with a punch, to check) instead of its disguise.
- [ ] Original keymap ([shortcuts.md](docs/specs/shortcuts.md), dev mode off): add the missing shortcuts (F1, S status, groups 1-6, camera points Z/X/C/V, `\\` track, Shift+Enter, Space skips the fly-by), and a way to stop and cast self spells without the dev keys.
- [ ] Debug overlay (`F4`, `crates/game-client/src/debug_overlay.rs`): reported bug, not yet found - "invisible overlay across the globe". A read-only audit of `draw()` itself this session found nothing wrong (every unit-to-cell conversion and every `torus_delta_cells` usage checked out), so the cause is elsewhere; next suspects, not yet investigated: `crates/game-client/src/grounded.rs`'s `ground_point()`, `CurrentMap`/map-loading staleness, or a `rig.focus`/`CurveParamsRes` mesh-to-cell scale mismatch. Pre-existing and unrelated to the patrol-points work above (Units, "Order model") - check this independently before/alongside adding the patrol visualization there, since that addition reuses `draw()`'s existing helpers and would inherit the same bug if it's actually in one of them after all.

### Tooltips ([tooltips.md](docs/specs/tooltips.md))
- [ ] Rendered tooltip: done for buildings, trees and totems (`tooltip.rs`: name, slot rows, anchored above, sticky, clicks, right click at once); the toggle to add.
- [ ] People rows filled: totems' counted prayers (once praying exists), the trainee. Vehicles and vehicle huts are done. Buildings, pyramids and totems' places are done, a click adds the unit to the selection.
- [ ] Wood rows: the pieces still needed on a growing house (once growth exists). Plans, sites, built buildings and trees are done.
- [ ] Bars fed by the simulation: totem prayer progress (once praying exists), house growth and birth, house growth and birth. The widget, its blinking, the pyramid's prayer bar and the training bar (double fill from `GameMap::production`) are done
- [ ] Editor: brushes under the mouse (`grounded::pick_ground` exists), brush radius UI, object placement.
- [ ] Editor: save back to the original `.dat`/`.hdr`/`.ver` (rules in level-format.md "Writing levels": things packed from slot 0, 1-based trigger links, buildings on corners).
- [ ] Editor: smooth brush and the raise/lower levelling step of the ALACN editor (ui-and-editor.md).

### Menu and sandboxes ([ui-and-editor.md](docs/specs/ui-and-editor.md))
- [ ] Sandbox Spells: test ground to cast every spell freely (no mana, no charges).
- [ ] Sandbox Buildings: placing and constructing buildings on its free ground (braves, wood and trees are there; every model and every construction stage are shown).
- [ ] More sandboxes as features come (combat...).
- [ ] Sandbox Worship: totems are drawn but nothing can be prayed at yet; the generated stand-ins are identical pillars.
- [ ] Sandbox Walk: the info line still shows "level 1/n" for sandbox maps.

### Camp fire ([buildings.md](docs/specs/buildings.md) "Camp fire")
- [ ] Guard posts' torches (objects 190-193, blended tile 92): draw them with `flame.rs` once guard posts have an original object (the firewarrior and prayer huts' torches burn).
- [ ] Generated firewarrior hut: drop the static ember cone from its braziers (tools/generate_buildings.py) now that an animated flame stands on them.
- [ ] Check in the game: what a camp fire is for (people gathering? mana?), its burn time when left alone (`ABANDON_TICKS`, 60 s guessed), the ring size, the flatness rule, and when object 12 (other logs) is used.
- [ ] Hover/selection of a camp fire (tooltip with who is around it), a hover cursor when units are selected over one.

## Priority 6 — infrastructure with no dependents
Real work, but nothing else in this file waits on it and neither has an immediate player-facing payoff until the tiers above are further along: performance only matters once there is more to render/simulate at once, and multiplayer's lockstep codec already works in tests with no UI or dependents yet.

### Performance (60 fps)
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
- [ ] Loading screen freezes near the end: two unyielded main-thread stalls, both reading as "the bar reaches
  100% then hangs". `boot::poll_job` (`boot/mod.rs`) runs each `BootJob`'s CPU work off-thread, but once it
  finishes, `J::install(output, world)` (GPU uploads, resource inserts) runs synchronously with no progress
  reporting, and `BootStatus::fraction()` caps a job at 0.99 while running, so the bar sits near 100% for the
  whole `install` (the `"boot: {label} installed in {ms}"` log line says which job's `install` is slow). Separately,
  `menu::begin_game`/`boot::GameStep` is explicitly documented as blocking ("then built, which holds the main
  thread"): after `SHOW_FRAMES` the map is built synchronously (`LevelList::load_current`: parse, build `GameMap`,
  recompute shading), zero progress reporting, no frames drawn until it returns - likely the dominant freeze for a
  real original level. Fix direction: make a slow `install` (or the map build) incremental, a bounded chunk of work
  per frame via a small state machine, instead of one unyielded call. Separate from the existing stale-terrain-
  during-instant-bulk-tick bug above (that one is post-load desync, not a loading-time stall).

### Multiplayer ([multiplayer.md](docs/specs/multiplayer.md))
- [ ] Network driver: send `Schedule::close_local`'s turn as `Message::Tick` every tick, feed peers' turns to `receive`, 2-3 ticks of delay (the schedule is done, single player only today).
- [ ] Host/join UI, lobby, reconnect.
- [ ] Desync checksum of heightmap + units.
- [ ] Pause and game speed as `Command`s, so every peer agrees (single player only today, `game_speed.rs`).

