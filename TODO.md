# TODO

What is left to implement. Tick or remove an item in the commit that does it, add new ones as they
come up. Details live in the linked specs; done work is summarised in [docs/roadmap.md](docs/roadmap.md).

Priorities (2026-10-28): sections below are grouped into seven rough tiers, highest first. A tier is a priority ordering, not a gate - later tiers (especially 4 and 6) can and should progress in parallel. Re-group as priorities shift; this is a living ordering, not a plan.

## Priority 1 — core gameplay correctness
What a match plays through today: combat, orders/the tick, units, spells, buildings, worship. Bugs and gaps here are the most visible, and other tiers build on top of them (the AI tier drives units through the same orders/coordinators as a human; the content tier dresses up the same spells and buildings).

### Combat
- [x] Melee (combat.md "Melee", traced in full - pop3-rev-analysis.md "Fight"): the ground check moving the middle off unsuitable ground (`Melee::relocate`) and the per-tribe winner's count and death tally (`GameMap::melee_wins`/`melee_deaths`) are in. Left: the moves' sounds (scoped out for now). The local-player-involved alert flag (`0x89D167`, pop3-rev-analysis.md "Watchtower sentry scan and alarm" and the paragraph above it) is now fully traced: a continuous "currently in a fight involving you" signal (set every turn by `pre_fight_turn`/`fight_turn`) plus an edge-triggered "a new fight involving you just started" pulse (`attack_order_handler`), copied each turn into `+0x8` of a HUD-widget-shaped struct (`0x895DBB`) - this is also confirmed to be the real mechanism behind the tower alert (manning a tower never plays its own siren; it just starts a fight via order kind 19, and this same flag fires). The reader is found too (pop3-rev `functions.tsv` row `0x489a30`): it turns `+0x8` and 3 sibling counts into 4 segments of a HUD gauge widget, no sound call anywhere in it - visual only, no separate fight-music trigger found (a different consumer of the same underlying state can't be ruled out, but this specific field's reader is settled).
- [x] Challenge (combat.md "Challenge", traced in full - pop3-rev-analysis.md "Pre-fight"): the target facing the attacker and its taunts (1 in 8 per turn while waiting, confirmed) are in (`WorldEvent::Face`, `challenge::taunts`/`Challenge::taunted` - no pose for the taunt yet, see "Units" below). The earlier "third approach, a target at its building" item is resolved: it is the same `Approach::PulledOut` building pull-out already covered under "Attacking a building", not a separate case. The no-shove option (`0x89D17C` bit `0x40`, a level/script token at `0x48E3D5`, not a cheat - also disables Melee's own knockback) and the lift-vs-ground-slide question (resolved: genuinely airborne via the same BLOWN push physics as Blast, no ground slide - `SHOVE_LIFT` was right) are both now traced - see combat.md/pop3-rev-analysis.md "Pre-fight"/"Fight". Still open: actually wiring the script-level no-shove option into our own level/script handling.
- [ ] Fight poses in the generated (CC0) unit sheets: strike, trade, kick, reel, shoved, and a challenge's taunt
  (`Pose::fallback` stands in: idle, fall). The taunt's original-asset side is done (combat.md "Challenge",
  `Pose::TauntHide`/`Wave`/`Punch`, anims 48-50, brave only): `Challenge::taunted` now carries which of the 3 too
  and copies it onto the target's own `Unit::taunted` (`WorldEvent::Taunted`) for the client's pose, held for a
  short guessed display window - *which* of the 3 plays is only confirmed "at random" [exe], the draw that picks
  one here is `[ours]`, not itself traced. Left: the 3 CC0 clips themselves (unit-art.md "Planned poses" - "new
  clips authored in Blender", out of scope without the baking pipeline), and the other kinds' original art, if any
  (today redirected to idle like every other brave-only anim).
- [x] `attack::may_attack` relaxed, `spotting::first_seen` still excludes it: a wildman is no longer entirely
  unattackable, only unreachable by the automatic proximity scan (now confirmed `[exe]` to skip it,
  pop3-rev-analysis.md "Wildmen") - an explicit player attack order now goes through
  (`a_direct_order_may_attack_a_wildman_but_nobody_attacks_its_own`, `attack.rs`). **`[ours]`, not itself traced**:
  whether the original's own explicit-order path really does let one through was never confirmed either way, only
  that the auto-scan excludes it - this is a guess that the stricter blanket rule (nobody, ever) was wrong, kept
  for a later spec session to confirm or correct.
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
  original. **Answered, traced this session (pop3-rev-analysis.md "Starting a melee", new paragraph)**: the
  original does not special-case a challenge from a melee here at all - the auto-attack scan's own candidate
  filter treats person states 25 (fight) and 29 (pre-fight/challenge) identically (neither is excluded), and the
  scan runs every tick regardless of whether the looker itself is idle or busy. So the original's own answer is
  the OPPOSITE of our current `Link::Melee`-only special case: a unit already challenging should be just as
  attackable by a third party, idle or not, as one already fighting - ours should likely drop the "skipped
  unless idle"/`Link::Melee`-only gate rather than extend it to challenges, not add a matching hold-off for
  `Link::Challenge`. No live check needed to settle this one.
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
- [x] Resuming a fight cut short by a push (combat.md "Melee" rule 1, pop3-rev-analysis.md "Resuming a fight cut
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
- [x] Healing (combat.md "Health"): implemented - life can go past full by up to one heal (`Health::heal_past_max`), and a lightning hit that doesn't kill now blocks healing for a while (`Unit::statuses.struck_by_lightning`, a tagged guess for the duration since every lightning hit today is a kill). Still not yet: the angel of death never heals (that unit kind doesn't exist).

### The tick ([architecture.md](docs/architecture.md) "The tick")
- [ ] Find the original's order of work inside a turn (people, buildings, effects, shots, mana; income comes before spell charging): ours is a guess.
- [x] Audited (2026-10-29): every rhythm this item named already runs on the `Every`/phase-0 convention, most
  already done despite the item's own wording - stranded hurt (`STRANDED_HURT_EVERY`, each unit at its own phase,
  documented intentionally, units.md; the damage amount and rhythm are now known and both wrong, pop3-rev-
  analysis.md "Stranded": the original deals 56 life every 8 turns (not 20 every 4), and only after an
  untimed grace period first - a climbing timer up to a per-tribe ceiling of 352 or 864, ours has none),
  mana income (`MANA_EVERY`, `mana.rs`), and
  every status timer, not just shield and bloodlust (`status::STEP`, `Statuses::tick` already steps invisible and
  hypnotized the same way). Only hut breeding (every 4) and growth (every 16) are still genuinely missing, not a
  convention slip: see "Buildings" below, "Houses growth/breeding".
- [x] `Raid` (occupants thrown out) was already implemented (added for Swarm's own building raids, `GameMap::throw_out`)
  despite this item's own wording - only Earthquake's own building-shake hadn't been wired to anything yet,
  now done (`WorldEvent::Shake`, `earthquake::Earthquake::shake`, `SHAKE_TURNS`/`SHAKE_CHANCE` - 20/128 per
  building in the 16 × 16 square at turns 24/56/88 `[exe]`; one piece of wood lost outright per hit, same
  primitive as fire's own burn tick, not the ordinary wear-threshold model; a large building's people out on any
  hit, a partial explosion only on its last piece - both confirmed, pop3-rev-analysis.md "Per spell" 14 and its
  function-level row). Panic's obstacle turning also already existed and is tested
  (`a_panic_run_turns_away_from_the_sea_and_leaves_by_the_door_first`, `unit.rs`), now documented (units.md
  "Panicking" row, newly added - the table had no row for it at all): a quarter turn away from the blocking
  heading, **`[ours]`, not traced** - checked this session (pop3-rev-analysis.md person_states.tsv state 26): the
  original's entry just rolls one random heading and a fixed speed/duration, with no obstacle-turning logic of
  its own in either the entry or per-turn code read so far - movement itself must go through the universal
  per-unit step/block-detection shared by ordinary walking (state 33's own row), which has no "turn away"
  behaviour, just a halt into state 33. Inferred, not proven (the exact step-execution code for a panicking unit
  specifically wasn't traced to confirm it really is the same shared path) - a live check (push a unit toward a
  wall/cliff with Blast via Firestorm's impact, watch whether it turns or just stops) would settle it outright.
  Left open either way, needing the whole audio system first, out of scope here: the original's own panic
  scream (also unconfirmed to exist at all - only an ambient crackle sound was found, see person_states.tsv).
- [x] Burning buildings (`fire`): the igniter's tribe and the destroyed-building count are in - `Building::ignite`
  now takes the lighting tribe (`ignited_by`, kept past the fire itself like a unit's own `last_attacker`), and
  `fire::tend_fires`'s `FireTurn::Hit { destroyed: true }` tallies it into `GameMap::fires_put_out` via
  `WorldEvent::BuildingBurnedDown` ("The tick" [exe] `stats[tribe][7]`, mirroring `FightWon`/`melee_wins`;
  `fire.rs` test `a_building_burnt_down_tallies_the_igniting_tribe_s_fires_put_out`). `burn_down`/`ashes.rs` itself
  stays untouched (cause-agnostic: an explosion or terrain destruction never raises it). Left: the fire sound
  (needs the whole audio system, `pop3_format::sound` below, not written yet) and a stats-tab reading
  `fires_put_out` (nothing surfaces the number yet, same gap as `melee_wins`/`melee_deaths`).
- [ ] Burning buildings, edge cases left after a live trace confirmed the timeline (pop3-rev-analysis.md "Burning
  building", `[exe, measured]` 2026-11-12): `fire.rs`'s `BURN_TICKS` 127 / `THROW_OUT_AT` 8 / `HIT_AT` 48 and the
  single one-piece hit leaving the building "under construction" match the original exactly (its turns counted
  from the ignition turn as 1). Differences: (1) a kind with no wood cost - the original lazily creates the
  wood stock at the type's full cost from its own table on the hit turn, so a 0-cost kind goes to -100 and is
  DESTROYED by its first burn unless the kind is fireproof, where ours keeps it standing ("its wood unknown");
  check which 0-cost kinds can burn at all, and the gate (500 wood in the original's table, 0 in ours); (2) the
  visible fire: the original lights 4 flames around a big hut's footprint (ours: one), and smoke puffs plus
  glowing embers stream up along the building from the throw-out turn on (`smoke::damage` covers the hit only).
- [x] Shots, Blast's own cast-to-effect fireball (8/4) only: `blast::Fireball`/`GameMap::fireballs`, a typed list ticked before effects like `GameMap::bolts` (`GameMap::tend_fireballs`), integer 3D flight at `FIREBALL_SPEED` 1000 units/turn from the shaman's own position, spawning the simple blast on arrival - the "+6 turns: a shot flies from the shaman" piece of pop3-rev-analysis.md "From cast to effect", previously missing entirely (the blast landed the instant the cast released). This item originally lumped three shot ids together (8/4, 8/7, 8/8), but they turn out to be two different mechanisms: Firestorm's and Volcano's own fireball rain (8/4 shared with Blast for Firestorm's, 8/7+8/8 for Volcano's two sizes) are each a `panicking_blast` scheduled instantly by their own effect every few turns (`firestorm::Firestorm::turn`, `volcano::Volcano`) - confirmed-correct scheduling already, no per-fireball flight ever modelled, by design (pop3-rev-analysis.md: "the already-correct scheduling... is not where the rain-of-fire look is missing", only the client's own visual is). Blast's own single shot, flying once from the shaman to the cast point, was the only piece with no implementation at all - that's what this item covers now. The general shot machinery every OTHER spell (not Blast/Firestorm/Volcano) shares for its own single cast-to-effect shot - homing on a targeted unit, the shielded-enemy deflection - is still open (the item below).
- [ ] World views (architecture.md "Client: world visuals"): the landing dust (`units::dust`) as a `WorldView` once
  touchdowns are kept in game-core (a unit's landing tick), so a rejoining client draws it too; the sinking totem's
  smoke once totems have ids; units, building views and the unit glide (`SimClock::prev_ids`) still keyed by index.
- [x] Coordinators: the infrastructure is done ([coordinators.md](docs/specs/coordinators.md)); the challenge and
  the melee are coordinators proper, with their scan in `GameMap::detect_coordinators`. Formation is its own
  thing (`formation.rs`), not a `Coordinator` impl, matching the trace. **Soul convert was already fully
  implemented** (`sermon.rs`: `CONV_FIRST_CHECK`/`CONV_CHECK`/`CONV_CHANCE`, the population cap gate,
  `WorldEvent::ChangeOwner`) - this item's own earlier wording wrongly listed it as still to specify, and it is
  deliberately NOT a `Coordinator` either way: combat.md's own "Sermon" section already says the original keeps
  it as state on the two units involved, not a backing object, same as ours. Guard control is **not** one of
  these any more (coordinators.md's own table, row 10): it turned out to be ordinary move/attack orders stashed
  across a unit's own order slots, not a dedicated controller - see "Order model" below, and units.md "Guard the
  shaman" for the G-key escort order itself (a different, now-implemented mechanic with a similar name).
  **Caution for whoever next touches `sermon.rs`'s own target eligibility**: pop3-rev-analysis.md "Preacher
  conversion" contains an internal, still-unresolved contradiction between two of its own traced facts - a
  static per-type table read says person state 23 (the actual conversion-roll candidate state) is reachable
  ONLY for type 4/6 (preacher/firewarrior), while combat.md's own `may_preach` (ordinary followers, never
  preachers/shamans) matches the community/game-observed behaviour instead. The trace's own text explicitly
  flags this as not settled by either pathway found so far - do not "fix" `may_preach` to the type-4/6-only
  reading without a dedicated spec session resolving which one is actually real; `may_preach`'s current
  followers-convert rule matches observed play and is left as is.
- [ ] Hardening, optional, found while tracing the above: `Melee`/`Sermon` both still manually re-check `is_alive()` on every member inside their own `tick()` rather than relying on `tick_kind`/`participants()` to have already filtered dead members out of `members` - a convention, not something the shared infrastructure enforces today. Teaching `tick_kind` to pre-filter would let every future `Coordinator` impl, this one included, drop its own by-hand check.
- [x] The hurt path (`GameMap::damage`): the last attacker and the death cause were already tracked
  (`Unit::last_attacker`/`death_cause`, `hurt.rs`) - this item's own wording was stale. Death held off while
  tumbling was also already in place (`take_damage`'s own `!self.thrown()` guard), but a genuine gap was found
  auditing it: once the hold-off ended (tumbling actually settling, not just landing), nothing re-checked health,
  so a unit fatally hit mid-tumble went to Idle at zero health instead of ever actually dying - fixed
  (`Action::Tumbling`'s own tick arm now checks `health.is_zero()` before going Idle, units.md "Tumbling").
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
- [x] Guard the shaman implemented (units.md "Guard the shaman", `game_core::guard`): `G` with followers selected
  gives each non-shaman one a continuous escort order (`Order::Guard`/`Action::Guarding`, `GameMap::go_guard`/
  `tend_guards`), re-aiming at her live position every tick, within `GUARD_RANGE` (824 units [exe]); selection
  clears, matching the original. `G` with nothing selected (or only her) instead cancels every one of the
  player's own guards (`Command::CancelGuard`). Bound to `G` in the original keymap; dev mode uses Shift+G
  instead, since plain `G` there is already the cell grid toggle (`keymap.rs`). The shaman is confirmed NOT protected while guarded
  (joins the same fight ring as her guards); no protection logic needed, already covered by ordinary
  auto-attack/melee-join behaviour.
  **Fixed, 2026-11-10** (reported live: several guards all walked to her exact position and drew superposed,
  one on top of another, instead of standing apart): each guard now circles a fixed point of its own on a ring
  around her instead of her exact position once within range (`crate::guard::orbit_point`, `[ours]`, not
  traced), `slot` assigned once at `GameMap::go_guard` and never reshuffled while it keeps guarding (same
  principle as a formation's own fixed member slots) so several guards spread out and keep slowly turning
  around her, catching up again (straight to her current position, same as before) if she walks far enough
  to pull any of them past `GUARD_RANGE`.

### Units ([units.md](docs/specs/units.md))
- [x] **Fixed, 2026-11-10** (reported live: a unit standing frozen in the walking pose, never losing life,
  no feedback at all - "stuck"): traced to `Action::Attacking`'s own `approach` having no fallback once
  `path::route` comes back empty (an enemy walled off by new construction, say) - `self.route` just stays
  empty and every tick after does nothing, silently, forever. A unit `Walking`, or `Attacking` while in the
  open, making less than `STUCK_MIN_PROGRESS` headway over `STUCK_WINDOW` is now treated as `Action::Stranded`
  instead (arms up, losing life), same as a target genuinely found unreachable up front (`Unit::stuck_watch`,
  units.md's state table, the `Stranded` row). `[ours]`: a safety net over that specific cause
  and over whatever subtler one might still strand an ordinary walk short of actually failing to plan at all,
  not a diagnosis of a single root cause - the window and progress-floor numbers are both guesses. Guarding is
  deliberately left out (a guard settled on its own orbit point legitimately stands still between nudges,
  `crate::guard::orbit_point` - this was first tried covering it too and promptly starved a guard to death in
  testing).
  **Checked further, 2026-11-11** (user question: does `approach` retry a fresh path before stranding, or
  only on a ground change?): it already retried on a ground change AND on every blocked step (resets its
  cache, which the very next tick's own top-of-function check then rebuilds) - the one real gap found was
  that `approach` lacked `follow_route`'s own immediate cell-by-cell correction on a blocked step, replanning
  with the full straightened search again instead, which could in principle repeat the exact same failure.
  Hardened to match `follow_route` (`Unit::route_to`, shared by both). **Not a confirmed fix for a specific
  repro**: an extensive search for a `(start, to, wall)` case where `path::leg_is_clear` approves a
  straightened leg that then actually blocks on unchanged ground found none - it already walks every cell
  the leg crosses exactly, including the corner-pinch rule, so this gap may not be reachable in the current
  code at all. Also checked: a pyramid of knowledge (`BuildingKind::Vault`)'s own footprint is an ordinary
  rectangle, same as any other building, no special corner handling needed or missing found - no evidence of
  a pyramid-specific pathing bug; needs a more specific repro (level, position) to look at further.
- [ ] Health: read the lives from `constant.dat` once parsed (`KindStats::life` holds the file's values); find the wildman's life; check in the game the stranded rate (`STRANDED_DAMAGE`, scaled x20 from our old 100 HP guess, still unconfirmed). `DROWN_DAMAGE` is answered and is **not** a per-tick damage value at all (pop3-rev-analysis.md "Drowning"): the original always takes a fixed 16 turns (1.33 s) to kill once truly submerged, forcing life to exactly 0, independent of current/max health - a flat countdown, not `-60`/tick. Matters most for the shaman: her much larger health pool means ours currently lets her survive drowning far longer than the original allows.
- [ ] Vehicles: feet/hull on the ground and never cut by nearby slopes, like units (`units::toward_eye`).
- [ ] Vehicles ignore the walking slope speed (flying ones ignore the ground, the sea is always at height 0).
- [ ] Walk animation rate could follow the slope speed (slower steps uphill).
- [ ] Replanning: every walker replans on any terrain write; only replan when the change touches its route (`DirtyRect`) once there are many units. Group moves could share a flow field.
- [ ] Worshipping the shaman: check in the game the range (3 cells guessed) and the states she must be in (idle or walking today). Confirmed real (not invented): animations.md's pose 8 is explicitly shared between praying-at-a-structure and "worshipping the shaman nearby". The person-state-5 hypothesis from an earlier pass (the generic "idle-fidget beside a paired reference unit", `pop3-rev/docs/person_states.tsv`) was checked further and is **not supported**: a full sweep of every literal "set state to 5" write in the exe turns up only the two already-known setters (wildman-beside-tree, the preach order) - no third, shaman-specific one. So this is either a cosmetic pose roll from a genuinely different state/mechanism not yet identified, or the "worshipping the shaman" description itself needs re-checking against what the original actually shows - needs a live check (watch an idle follower near its own shaman for the kneeling pose) before any more static searching, since the state-machine angle has now been ruled out rather than just unconfirmed.
- [ ] Stranded units: find the game's anim (frame 1 of anim 12, the flattening jump, stands in today).
- [ ] Boarding vehicles: people of the same group within 440 units join the vehicle's group (pop3-rev-analysis.md "Boarding and driving"); the original's boarding clip (our jump uses the `Jump` pose, unit-art.md).
- [ ] Open-source units: models closer to the original look (feathers and staff for the shaman, tribal outfits); a prayer clip (SitDown's end stands in) and a swim/drown clip (RecieveHit sunk stands in). CC0 poses to render as each action lands: [unit-art.md, Planned poses](docs/specs/unit-art.md#planned-poses-cc0-stand-ins).
- [ ] CC0 sheets for the wood poses (`chop`, `carry_walk`, `carry_idle`: axe and log props, unit-art.md "Planned poses"); the braves show idle / walk meanwhile.
- [ ] CC0 sheets for tumbling (`flung`: in the air, arms spread; `tumble`: falling down a slope); the fall loops meanwhile.
- [ ] Wildmen: find their death anim (47, falling down a slope, stands in) - confirmed shared with warrior/preacher's death-anim group, not tied to any wildman-specific lifecycle (pop3-rev-analysis.md "Wildmen").
- [ ] Load open-source unit sheets (`assets/units/<kind>/<pose>.png`, see [unit-art.md](docs/specs/unit-art.md)): 64x64 cells, feet at (32, 58), 5 directions mirrored to 8, magenta key ramp swapped per tribe; fall back to the generated figure per missing sheet.
- [ ] `just bake-units` is not bit-reproducible: two bakes differ by 1-2 edge pixels in a few frames (GPU rasterisation), so there is no `--check` like the building kit's; render on the CPU or compare with a tolerance.
- [ ] Original unit animations: who the pointed-helmet attacker of 20-24 is, and anim 13; use the fighting anims (pop3-rev-analysis.md "Combat": 15 move 2 and the opening blow, 13 move 3, 25 move 4, 14 hit, 16 struck before the flight; taunts 48/49/50 while waiting), throw-fire (7), tornado tilt (16), pedalling (51), running from bees (52) anims once those actions exist.
- [x] Bored poses (pop3-rev-analysis.md "Bored poses"): braves scratch (89), warriors (91) and firewarriors (90)
  do push-ups, spies juggle (92), 3/64 every 32 turns, one loop - client-only (`units::art::bored`). Left open:
  the spy's juggling trigger is untraced (ours rolls it like the others, `[ours]`); the `+0x82` gate on the
  push-ups; CC0 clips (`Pose::fallback` to `Idle`).
- [ ] Idle shuffle (`game_core::idle`): the length of the step is a guess (an eighth of a cell); a spec question.
- [ ] Their own behaviour: braves build/gather, warriors fight, preachers convert, spies disguise, firewarriors throw fire (today every kind only walks, prays, drowns and dies).
- [ ] State transitions: `Unit::panic`/`Unit::catch_fire` (`unit.rs`) both call `drop_plans()`, cancelling the route, destination, teleport/spell targets, every pending "go do X" field, the order queue, the patrol list and any work assignment outright. **Confirmed, not just suspected, as a real divergence** (pop3-rev-analysis.md "Resuming a fight cut short by a push", addendum): the original's state-transition mechanism itself has no generic "forget your plans" step anywhere (`0x4ED6F0`, always called on any transition, is a no-op; `0x4ED640` only runs the new state's own entry case), Fight is independently confirmed to never release a participant's command group, and person states 26 (panic) and 31 (on fire) were each read exhaustively, start to finish, this session - neither touches the order/command-group slot (`+0x9B`/`+0x8B`/`+0xA6`) or calls `group_release` anywhere. Fix: `panic`/`catch_fire` should stop calling the full `drop_plans()` and instead just interrupt the current task the way Fight already does (`resume_attack`-style), so it resumes unprompted afterward instead of leaving the person with nothing queued. Also resolve the existing inconsistency this surfaced: the `Drowning` transition (`unit.rs` `tick()`) does a hand-picked partial reset (`teleport_to`/`spell` only) rather than calling `drop_plans()` at all - whichever way `panic`/`catch_fire` are fixed, make `Drowning`'s own reset consistent with it rather than its own third, narrower variant. (Unrelated to the above but found alongside it: on-fire's own entry in the original also forces the SAME random run panic does, for the whole 70-turn burn, gated by a ghost-check rather than panic's blast flag - `catch_fire`'s existing `run_in_panic`-for-the-whole-burn already matches this, no change needed there.)
- [ ] Spies, left open after the implementation below (units.md "Spies"): the real windup tables and the
  owners' notice percent after an arson (`arson::NOTICED_PERCENT`, a guess); the original's flagged wall cell
  (ours stands just outside the middle of a wall); a spec session to reconcile "arson is not player-reachable"
  (pop3-rev-analysis.md) with play, where a spy's attack on a building sets it on fire (user-confirmed, implemented
  so); how the computer fills its arson worklist (ours: nearest enemy
  buildings) and what `SPY_CHECK_FREQUENCY`/`MAX_SPY_ATTACKS` really mean (`ai_spy.rs`, guessed readings); the
  discovery sweep's radius (3 cells, a guess); whether a settled spy is also drawn with the imitated look of a brave
  (animations.md 17 shows a "disguised as a brave" variant; ours keeps the spy outfit in the disguise colour); the
  original's own reveal cursor sprite (ours: a generated punch badge); the throw sound (0x36, no sound events yet).
- [x] Spies (implemented: `disguise.rs`, `arson.rs`, `ai_spy.rs`, client `units/spy.rs`, `hud/disguise.rs`; the
  delay is simply 63 of our ticks, one per original turn): disguise is one packed byte per spy (apparent tribe + a transition countdown, `SPY_DISGUISE_DELAY`
  exe default 63) consumed by one gameplay-side check reused everywhere a tribe-match decides hostility/targeting
  (pop3-rev-analysis.md "Spy disguise and infiltration") - while the countdown runs after disguising, the spy
  still counts as its real tribe everywhere (a genuinely vulnerable window, not cosmetic); once it settles, it
  counts as the disguise tribe and is left alone by ordinary targeting, matching "unnoticed... except when
  hovered or passed by a spy of the tribe it imitates". Its building attack (anim 17) is a confirmed separate
  order (kind 15: approach, aim, throw a firebrand, the building catches fire, then a discovery-roll withdrawal
  during which it can be outed) - but every issuer found in the exe is the native AI tasking its own spies
  against a worklist of buildings, never a player command. The player-facing DISGUISE trigger is now
  live-confirmed (pop3-rev-analysis.md "Spy disguise and infiltration", the confirmed path block): clicking
  the per-tribe HUD square with spies selected queues an ordinary per-tribe command that becomes one shared
  order (kind 16, target tribe in the order) on every selected unit - so our implementation should model it as
  a queued unit order (one sim-turn latency, applies per selected spy, re-clicking re-arms the full transition),
  not an instant HUD effect, available only while a spy is in the selection (the original gates the option on selected-unit types). ARSON is settled as NOT player-reachable (the arming machinery never offers it, 2026-11-12) - ours need not expose it either; a second, fully orphaned wood-pile arson order also exists in the original and needs no implementation. The transition is 63 turns
  (~5.25 s at the original's 12 turns/s; our 10 ticks/s equivalent is ~52-53 ticks), decremented -1/turn by the
  per-person turn step. The complete reveal rules are mapped (pop3-rev-analysis.md, "What un-disguises a spy"):
  an attack order targeting the spy (revealed the turn the order starts, before contact), any combat contact (hit, Swarm), a
  enemy-AI discovery task (45% per check, certain once the spy has stood more than 255 turns in one state), an enemy spy on the same map cell, being hypnotised (lost both ways), or the explicit reveal command -
  and nothing else. Targeting rule (user-confirmed): manual attack targets by REAL tribe (a player can always attack
  a disguised spy by hand, revealing it), automatic targeting by APPARENT tribe (a settled spy is never
  auto-attacked).
- [x] Death: the single guessed `SPIRIT_TICKS` is replaced with the two confirmed variants (`Action::Spirit`'s
  own new `climbing: bool`, `Unit::new_spirit`, units.md "Chained orders" area): `SPIRIT_CALM_TICKS` (4 turns,
  stationary) or, unconditionally for a wildman and for anyone else only if it died in a drowning-eligible spot
  (reusing `Action::Drowning`'s own `is_sea` check), `SPIRIT_CLIMB_TICKS` (32 turns) with a real sustained
  `SPIRIT_CLIMB_RATE` (40/turn) rise on `motion.height`, no gravity - its own branch in `Unit::tick`, bypassing
  the general physics step entirely (which would otherwise apply gravity and a touchdown check neither variant
  wants). Still open: the per-type VSTART object ids this TODO item used to list (38/85 calm, 31/40/45 climbing)
  are a pose/art-asset wiring question, not a simulation one, left for whenever the original-asset animation
  pass reaches them; the wildman-only continuous facing spin (91/2048/turn) is not implemented - our sprite art
  is 8 discrete directions, not a smoothly rotating model, so this needs its own client-only approximation, not
  attempted this session; `DYING_TICKS` (the fall before the spirit spawns) was not re-examined either. Unit
  views are indexed by unit position and rebuilt whenever a unit leaves the map: key them by unit id once there
  are many units.
- [x] Wildmen idle loop implemented (`game_core::wildman`, units.md "From the levels"): state 1 → 8 → (5 "eating"
  at a tree within `TREE_SEARCH_RADIUS_CELLS` (18), or 6/16 "drinking" at a shore cell within
  `SHORE_SEARCH_RADIUS_CELLS` (12) if none found) → 8 → ..., `Action::Grazing`, anims 2/4 wired
  (`Pose::Eat`/`Pose::Drink`, original-asset only, CC0 falls back to idle). No plant scenery (types 7/8) is
  modelled in this engine at all, so only trees are searched - a narrower destination set than the original's own,
  `[ours]`; "shore" substitutes `wildman::is_shore_cell`'s own neighbour-based approximation for the original's
  untranslated coastal-shape table, also `[ours]`. `wildman::SEEK_EVERY` (how often it looks) and `GRAZE_TICKS`
  (how long it lingers) are both guesses: the trace gives neither a number. Drowning at the shore needs no code
  of its own, confirmed to fall out of the generic per-turn check already. Still open, unrelated to the loop
  itself: the literal VSTART id each state resolves to was already known and is now wired (eat=2, drink=4); a
  wildman's death spirit-rise variant (see the death/spirit item below); whether an explicit attack order is
  honoured (see the wildman-attack item above, now resolved as a guess).
- [ ] When the shaman reincarnates at her site, the wildmen within range of it are converted into braves of her tribe for free, each with a conversion animation. Done at level start without animation (`ReincarnationSite::welcomes`); check the range in the game (the levels only use the ring of 8 cells) and whether it happens on every reincarnation.
- [ ] Selection of buildings (without the people inside), and of units inside them (`selection::selectable`). Vehicles: a click with nothing selected selects the people aboard (vehicles.md).
- [ ] Walking trails: footprints / worn paths left on the ground where units walk (fading over time).
- [ ] Statuses (`Unit::statuses`: Magical Shield, Bloodlust, Invisibility and Hypnotism now cast for real,
  `GameMap::cast_nearest_status`/`hypnotize`; only Ghost Army's left unset by any spell): **fixed** (found tracing
  the exe's own Hypnotism code, 0x50EB00, while resolving "Preacher conversion"'s GONE/NEW contradiction): the
  original explicitly carries the captured person's current life, max life and carried wood across the capture
  (and again across the later revert). `Unit::change_owner_keeping_body` now does this for both `hypnotize` and
  `revert_expired_hypnosis` (soul-convert and the Convert spell keep the ordinary full-reset `change_owner`,
  unaffected: this was specific to Hypnotism's own capture/revert, checked at every `change_owner` caller first).
  The timer of a ghost's
  status: **resolved, there is no timer** - pop3-rev-analysis.md "Ghost Army mechanics" traced the spawn code
  directly and confirmed no duration field is ever written; implement `Statuses::ghost` as permanent until killed
  (or culled by the 60-per-tribe cap). Other options to define; read by combat, selection and
  drawing. Invisibility's own early end (attacking does, walking does not) is a guess (pop3-rev-analysis.md "Per
  spell" 6: "some actions end it early", not traced), implemented in `attack::go_attack`. Drawing: the markers are
  done (`units/markers.rs`). Own invisible and ghost people are now drawn translucent (`units::ghost_material`/
  `GhostMaterials`, docs/specs/sprites.md "Ghost"): a cached alpha-blended clone of the unit's own material (shared
  per kind+tribe+alpha-mode since the atlas-sharing work above) at alpha 0.34, picked by eye against
  `INVISIBLE=1 just shot` as the reading that looks see-through rather than solid or gone - the ghost table's own
  which-operand-is-the-sprite ambiguity is left unresolved, just answered empirically here, not traced further.
  Still open from this same item: the "flickering faster near the end" nuance as invisibility is about to expire.
  Left otherwise: the low-health looping sound 0xCF, invisible enemies hidden (sprite, shadow, hover, selection),
  the original's colour tables (our shield tint is a guess), the optional health bar over own damaged people
  (commands 100/101).
- [ ] Physics (`game_core::physics::step`: velocity, gravity 32, velocity caps and air drag now done too, touchdown; landing in the sea or on walkable ground done, `Unit::touch_down`), to the original's rules (pop3-rev-analysis.md "Person physics"): still open - the unit spinning 26° per turn in the air, a scream on take-off; landing: dust and a sound at a vertical speed <= -100, `FALL_OUT_OF_WW_DAMAGE` 700 at <= -200 (`Touchdown::speed`), none on water cells; on the ground: sliding down the slope (gradient over ±76 units) with friction 28 per turn per axis (72 on special terrain), facing where it slides (`Tumble` pose; a grounded tumbling unit does not move yet). **Correction found closing this item: `game_core::balance` did not exist as code yet at the time (it does now,
huts-and-training.md "Balance and game turns", `Balance::ticks` the identity) - one of our ticks already is one
original turn 1:1 (`time.rs`, `TICKS_PER_SECOND`), so the original's per-turn numbers (caps 192 horizontal/512
vertical, drag 2) apply to `physics::step` directly, no conversion needed.** Ordinary falls (ground dropping under a standing or walking person: earthquake, erosion, flatten) fall with the same gravity, caps and drag, without fall damage; rising ground lifts people. **Resolved this session (pop3-rev-analysis.md "Person physics"): landing on a building gets no special handling at all.** The original's `ground(pos)` (`0x44E940`) is a pure terrain-mesh height interpolation (a cell's 4 corner heights, diagonal-split-aware) with no building/unit/class check anywhere in it - a cliff is just handled correctly as steep terrain (same function), but a building sitting on the landing cell is invisible to it. A thrown person's touchdown height is always the raw ground height there, same as open ground - no "stands up on the roof" behaviour exists to port; a landing that happens to fall on a building's footprint should just land at ground level (through/inside the building's base), matching ours needing no special case either. Still open: what the original does to a unit landing in water (ours dies at once), boats landing on the sea once vehicles exist, bumping into buildings and cliffs, a task dropped or kept when flung.
- [ ] Flying: units thrown by Whirlwind / Blast / explosions follow a ballistic or carried path (flying 61, tumbling 73 anims), land with damage, drown if they land in the sea.
- [x] Ground recovery after landing (person state 19, pop3-rev-analysis.md "Tumbling and ground recovery", live-checked 2026-10-27): **implemented as a scoped-down approximation** (`Action::Settling`, `Unit::settle`/`settle_ticks`) - a grounded tumble now locks (no orders, `Unit::locked`) for a randomized while before resuming its own per-type behaviour (`go_idle`, so a standing attack order a fight put on hold still resumes same as before), playing its ordinary Idle (or holding-wood) pose throughout, no new animation asset. Simplifications, each tagged in code/docs rather than guessed silently: the duration is one `4..=31`-turn round [exe, measured] plus up to one more at the original's own combined reroll odds (our own cap on how many rounds, not traced - kept low enough that the common case clears well under a firewarrior's own ~27-turn reload, matching "a handful" as typical and "several dozen" as the rare unlucky tail) rather than the original's own open-ended per-32-turn reroll loop; it settles in place rather than relocating to a nearby clear spot (the spot-picking logic - 48/216-unit thresholds, an 8-direction offset table - is left unimplemented); carried wood is put down immediately rather than after the first wait round, and the debris puff/sound itself isn't drawn (no such effect exists yet). Getting hit again while settling still interrupts it into a fresh tumble, same as any other hit - a locked unit can in principle get juggled this way by a sufficiently persistent attacker (seen while testing, `firebolt::tests::a_firewarrior_hit_fires_back_even_walking`), which reads as a real consequence of the traced numbers rather than a bug, but isn't independently confirmed either way live. Randomized off a per-unit/per-tick seed (`settle_ticks`), not the shared `map::Lcg`: `Unit::tick` has none of its own and giving it one would mean threading `&mut Lcg` through every caller and test in the crate for this detail alone - still deterministic and lockstep-safe (a pure function of simulation state).
  - **Follow-up live check needed**: blast/tumble a brave that is actively carrying wood (`+0x78` nonzero) and watch
    whether it (a) shows the holding-wood/walk-with-wood pose instead of plain idle/walk while relocating, and (b)
    drops a small debris puff with a sound once per 100 wood while settling, down to 0 - neither is implemented yet
    beyond the immediate, undramatic wood-down (`Unit::settle`). The one live trial done so far did not confirm
    this either - the unit's wood status wasn't logged, so this is a static-table reading only.
  - Not yet: relocating to a nearby clear spot before settling (currently settles exactly where it lands); the
    exact per-32-turn reroll odds/cap (ours is a bounded approximation, not traced); whether getting hit again
    while settling, and the resulting "juggled indefinitely by a fast enough attacker" possibility, matches the
    original at all - unconfirmed live either way.
- [ ] Vehicle huts (vehicles.md "Vehicle huts", done with guesses): mostly answered this session, traced from the exe (pop3-rev-analysis.md "Vehicle huts: how the hut itself produces a vehicle"). **Work per vehicle is NOT 1200 crew-turns**: it's a plain wood-delivery cost, already in the Vehicles table (`WOOD_VEHICLE_*`) - 600 for either boat hut type, 1000 for either balloon hut type, via a per-hut accumulator compared against the vehicle's own table cost (what increments that accumulator per turn wasn't found, so a real-world rate still needs either a live watch or a further static session). **Crew size is NOT 4**: the occupant array is the generic building `+0x86` field, confirmed elsewhere as a 6-slot array (though a different already-written row describes up to ~20 on the same field for training buildings - an unresolved discrepancy, flagged in functions.tsv, not settled this session). **Where the vehicle appears**: a per-type, door-relative offset point near the hut (not an arbitrary nearest-water search), gated on a clearance check. Mana: not referenced anywhere in the traced code - the mechanism appears to be wood-only, no mana cost found (matches a [player] recollection that vehicle creation is wood-only). The door queue question (types 13-16) is untouched by this pass, still open. **Settled by a [player] description this session** (supersedes the earlier, hedged 2/6-unsure guess and the "no free vehicle" question, vehicles.md "Vehicle huts"): crew is 4 for a boat hut, 6 for an airship hut (now `VehicleKind::crew`, split from the old shared `CREW` constant) - still not independently traced from the exe, so a live check is still worth doing; no vehicle is ever free, every one including the first is crewed and paid for in wood; the crew that isn't driving the finished vehicle off keeps working and starts the next one, looping until nobody is left assigned to the hut - already how `GameMap::work`/`crew_step` behaved, so no code change was needed for the looping part.
- [ ] Whirlwind spell (to implement), on a vehicle: everyone aboard is put out, it spins (roll +204, pitch +153 /2048 per tick) for 10 ticks (boat; ends early when the ground reaches it) or 2 (balloon), then explodes like a destroyed boat on land (pop3-rev-analysis.md "Life, damage and destruction", state 9).
- [ ] Vehicle destruction (vehicles.md; health, damage from blasts and bolts, sinking, rising and exploding done): the partial explosion of a boat on land (its faces flying off, `explosion.rs`).
- [ ] Balloons: bobbing while moving, and the original's climb rate and look-ahead; check the hover over trees and totems in the game (450 guessed; buildings 560 from the exe) (vehicles.md).
- [ ] The shaman's health is lowered while she is in a vehicle and goes back to normal when she leaves it.
- [ ] The shaman can cast only some spells from a vehicle: Land Bridge is refused (done); check the others in the game (fire-type probably yes).
- [ ] Drowning animation: the original drowning pose is not identified (tumbling is used).
- [ ] Use the shaman's wand gesture (81-84), the conversion sit (18), the spy's juggling idle (92); check the tribe layer flag `0x10` mapping. The carbonized brave (97) for lightning is done (`Pose::Carbonized`, "Spells" below).

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
        **Correction**: `WorldEvent::Shield{steps:0}` named below as an already-existing event does **not** exist -
        checked, only `Unit::statuses.shield`, a per-unit `Timed` status set directly by `cast_nearest_status`/
        similar; this needs new plumbing (a `WorldEvent` variant, or a direct per-tick status refresh inside
        `armageddon.rs`) once it's built, not a quick wire-up like the item below.
  - [x] Lightning-kill-without-reincarnation when a tribe's own fighters hit zero: **implemented** (`armageddon.rs`
        `losers`) - wired into v1's existing instant-resolution instead of a real ongoing fight (there isn't one
        yet, see the two-phase item above): the moment the match resolves, every tribe not on the winning side has
        its reincarnation disabled (`WorldEvent::RemoveSite`/`GameMap::remove_reincarnation`) and its shaman, if
        still alive, struck by the ordinary `WorldEvent::Damage`/`DamageKind::Lightning` path (so a shield still
        blocks it, same as any other lightning strike) - a draw (`best_side` returns `None`) strikes nobody down.
  - [x] Related fix, done ahead of Armageddon itself: `Unit::tick`'s `Action::Dead` branch no longer gets stuck
        forever when there's no `ReincarnationSite` for that owner - it falls through to `Action::Spirit`, same as
        `Dying`'s own non-shaman case (`unit.rs`). Not just a hypothetical Armageddon state: `DO SET_REINCARNATION
        OFF` is a real PopScript command a tribe's own script runs at turn 0 (level 1's red script, ai-scripts.md
        "Worked example: level 1's opening flyby"), now wired end to end (see "Computer players (AI)" below).
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
- [ ] Every spell flies from the shaman as a shot before it takes effect (6 turns into the cast; standard shot 1400 units per turn, blast's fireball 1000 - **Blast's own fireball flight is now implemented**, `blast::Fireball`, TODO.md "Shots" above), homing on a targeted unit for the spells that follow one; burn, blast, lightning, whirlwind and insect shots are deflected 8 cells away by a shielded enemy on the target cell. Still open for every spell other than Blast: the standard 1400-unit/turn shot itself (every other spell still takes effect the instant it releases, same gap Blast had), the homing, and the shield deflection. A 1 s cooldown after any human cast, no per-spell cooldown (`SPELL_x_OPT_S` is not one).
- [ ] Blast (`game_core::blast`): the cumulative, slope-scaled push, the allies-only-on-the-last-turn rule, buildings' 40 wear once per building reached, and the fireball shot flying from the shaman first (1000 units per turn, 6 turns into the cast, TODO.md "Shots" above) are done (pop3-rev-analysis.md "Blast push and flight", "Components", "From cast to effect"). Its own cast chain, now traced in full (pop3-rev-analysis.md "Blast's own cast chain"), is also done: the fireball's landing spawns the burn cell (`blast::impact_events`/`ground_fire::burn_cell`, shared with Lightning's own strike - a tree there catches fire, or else a short ground fire) and a 9-turn explosion burst (`blast::explosion_effect`, `EffectKind::Explosion`) alongside the damaging blast; client-side, the flying fireball is a small flickering flame (`blast_fireball.rs`) and the burst a small flash (`blast_impact.rs`, deliberately sized about a firewarrior bolt's own impact, not literally the blast's own multi-cell damage radius — that would read as a crater). Still to the original: the plain grey smoke puff the chain also spawns (7/0x03, 4-turn life) — a generic trail/puff primitive shared with the firewarrior's own bolt and other shots, not modelled yet (see "Shots" above); angels of death and ghosts (neither spell/unit kind exists yet); the fireball's own homing/shield-deflection (not applicable to Blast's static ground target per the trace, so likely moot here).
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
- [x] Lightning panic, open question: **resolved via static tracing, 2026-10-29** (no live check needed - and a
  same-day false start corrected in place, see below). "Panic" (person state 0x1A/26) is real: entering it forces
  a 64-turn random-heading run at speed 110 (`init_unit_class_1`'s own per-state ENTER dispatch, not just the
  cosmetic scorched/fire-trail visual a first pass this session found in the per-turn tick body and wrongly
  concluded was the whole story - see `pop3-rev/docs/person_states.tsv` row 26 for the full citation and the
  correction). **The actual answer to the reported gap**: this forced run is gated per-caller by a blast
  instance's own `+0x7A` flag, confirmed SET by Firestorm's own impact blast (`0x511800`) and confirmed NOT set
  by Lightning's own companion blast (`0x511AE0`) - a real, deliberate difference between the two spells, not a
  trace gap. So Lightning's own strike and companion blast still do not panic anyone near the strike cell (that
  part of the original trace was right, just for an incompletely-understood reason) - the only panic a Lightning
  cast can cause is the building-eviction case (unconditional, "Burning building" below), 8 turns after it sets a
  building alight, not an immediate ring effect. No panic radius needed for `EffectKind::Lightning`. Firestorm's
  own panic ("Per spell" row 8) is confirmed as this same mechanism. Swarm's own panic (row 5) is now also traced,
  2026-10-29, and is NOT the same `+0x7A` mechanism at all - a third, independent gate: insect plague's per-turn
  handler (`process_insect_plague`/`0x510120`) does two separate things, both using the same unconditional
  `0x4ED6F0`/state=0x1A/`0x4ED640` transition as the building-eviction case (no `+0x7A` check anywhere in this
  spell's own code). Every 64 turns it raids a nearby enemy building (occupants evicted one at a time, forced to
  panic, then damaged - "up to 3 times" matches); every 8 turns it scans its own 3x3-cell neighbourhood for enemy
  persons (shamans/angels excluded by a type table, matching "immune") and, for each one whose own `+0xe` bit
  `0x10` is clear (now confirmed, not a Swarm-specific flag: a general "not already being removed this tick"
  re-entrancy guard, pop3-rev-analysis.md's Angel-of-Death area panic and Preacher-conversion eviction both check
  the exact same bit with the same meaning - not Blast's own `+0x7A`) and who isn't a ghost (ghosts are
  deleted outright instead, matching "ghosts destroyed"), forces the same panic then applies exactly
  `SWARM_PERSON_DAMAGE` (100, confirmed via `0x5AA514`) - matching "Per spell" row 5's "100 damage and panic"
  precisely. Full citation: `pop3-rev/docs/functions.tsv`'s `0x510120` row. Volcano's own fireballs (row 16, "22
  small and 6 big fireballs (simple blasts)") were not re-checked against their own code this session, nor was
  Blast's own direct spell-cast blast (row 2) - worth confirming whether they set the same `+0x7A` trigger
  Firestorm does before relying on their "panics" wording.
- [x] Ghost Army (docs/specs/spells.md "Ghost Army", `game_core::ghost_army`): the spell, the priority scan and
  scatter spawn, the 60-cap cull, every destroy-outright chokepoint (damage/conversion/hypnotism), the
  shaman-uniqueness exclusions, headcount/mana exclusion, restricted auto-targeting and cosmetic-only bolts are
  all done. Left, both deferred as cosmetic-only (docs/specs/units.md "Status markers"): the original's own
  sound cue + short generic flash where a ghost dies (no sound system, no reusable flash primitive exist in this
  codebase yet) and the cast's own tick-1 cosmetic mist burst (81 puffs). Still open, not traced (queued in
  pop3-rev's own `docs/trace-backlog.md`): the exact `CAST_RELEASE` delay for this spell specifically, and whether
  auto-attack *target selection* scores a ghost any differently from an ordinary target (distinct from what a
  ghost itself may attack, already implemented: only enemy ghosts/shamans).
- [ ] Spell cursors: confirm 40 = Armageddon and 43 = Ghost Army in the game (`virtual_cursor::spell_sprite`); check the gold arrow's frame order and speed against the game; the pause menu releases the mouse, so it shows the system cursor, not the gold arrow.
- [ ] Burn (1), bloodlust (20) and the original teleport (21), now traced (pop3-rev-analysis.md "Per spell"): burn sets the target cell's trees alight and is not castable; bloodlust (hidden) gives the 6 nearest own followers ×3 damage, /8 damage taken and faster firewarrior reloads for 120 s; teleport (hidden) moves the caster's own shaman. Decide which ones we expose.
- [ ] Flatten: trace the original's per-turn curve (ours divides the gap by the turns left) and whether the disc is centred on a point or a cell. The land bridge raises a smoke puff on every bridge cell every turn while it builds (a `WorldView` of the effect); balloons rise with it; its steps do not re-level or collapse buildings yet.
- [x] Level spells: keep each discovery's availability (permanent / this level / one shot) for when it is
  discovered: implemented (`SpellSlot::discovery_availability`, `SpellBook::from_level`/`discover`, spells.md
  "Spell book"). **Correction**: the TODO's own `SpellsAvailableLevel`/`SpellsNotCharging`/`SpellsAvailableOnce`
  pointer named the wrong data source - those are level-header fields, confirmed always 0 in every campaign
  level (level-format.md); the real per-discovery availability is `pop3_format::level::Discovery::availability`
  on the discovery thing itself (already parsed, `DiscoveryType 11`), which is what's wired up now. Permanent and
  ThisLevel both still resolve to `Known` and are indistinguishable in practice: there is no cross-level save/
  progression system to carry a discovery's `Known` state into a later level, so this is a real, explicitly
  deferred gap, not a bug.
- [x] A "once" spell discovery should give a single shot (`Provided { shots: 1 }`) when found (e.g. Convert in
  levels 4 and 5): implemented as part of the same fix - `discover` now branches on `discovery_availability`
  (`spell_book.rs` tests `discover_applies_the_level_s_own_discovery_availability`,
  `level_loadout_from_header_and_discoveries`).

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
- [x] Shaman-death mana dispatch done for the loss side (`GameMap::shaman_died`, spells.md "Mana"): the tribe
  loses `base * SHAMEN_DEAD_MANA_PCT_LOST` (25 %, file and exe agree; `base` = bank only - the file's own "needed
  + stored building mana" alternative for a human/controller-type-2 tribe is still the same unmodelled-controller
  gap `training::train_cost` already has). Caught by comparing every shaman's alive-before/alive-after across a
  tick (`Unit::die` sets `Action::Dying` synchronously wherever damage lands, not through a tick-returned event,
  so there is no single call site to hook instead) - this catches every cause (melee, a spell, drowning,
  stranded, lava...), not just combat. The killer's own share (`SHAMEN_DEAD_MANA_PCT_GAIN`) is exe-confirmed
  **0 %, not the file's 25 %** - spells.md corrected to the exe's own 25 %/0 % split. Left deliberately
  unimplemented: the delayed reservoir (`give_mana_delayed`, 1000 turns/83 s) the killer's share would go
  through - with the exe's own 0 % it would always move nothing, so building it now would be dead code; revisit
  if a later session finds the file's 25 % is what actually governs instead.
- [ ] A cast's mana cost: the original also takes the cost from `incoming` when casting; our charges are prepaid. Check whether a cast takes both.
- [ ] Spell share inside the distribution: the original also splits evenly between the learned, unblocked spells that are not full (pop3-rev-analysis.md "Distribution"); check where it puts the odd mana (ours: the first ones in book order).
- [x] Spell charges from `SP_1_OFF_MAX_<X>` (constants.md), not a cost-banded guess: already done
  (`SpellStats::from_constants`, `spell_book.rs`; `max_charges(Swamp) == 3` test citing `SP_1_OFF_MAX_SWAMP`,
  `from_constants_overrides_only_the_keys_a_file_actually_has` exercising `SP_1_OFF_MAX_BLAST` directly). This
  item's own wording was stale - there is no cost-banded `max_charges()` left anywhere to replace.
- [ ] Spent on training units: the stores are filled as in the original (`production::charge` in `ManaPool::distribute`: half of `incoming` before the spells, at most `cost / 32` each, a second share with every spell full, 100 a turn given back while making nothing). Left, each blocked rather than ready to code: a negative `incoming` (casts) taking mana back from the stores - depends on resolving the still-open "A cast's mana cost" item above (`incoming -= cost` on top of our prepaid charges, or instead of them? - a clean-room question, `Mana` has no subtraction-from-`incoming` path to extend yet); and the stores counted in the shaman death loss - the file's own "needed + stored building mana" alternative applies only to a controller-type-2 ("human") tribe, and no controller type is modelled yet (`GameMap::shaman_died` only ever reads `pool.bank` - the same tribe/controller-type gap `training::train_cost` has, see "Tribes" below).
- [x] Earthquake and Firestorm (`spells.md`): Firestorm's scheduled `panicking_blast` fireballs are done in full; Earthquake's shake-and-sink (terrain part) and its three building-shake checks (`WorldEvent::Shake`, "The tick" above) are both done now, leaving only the trace's "possibly two lava flows". Angel of Death, Armageddon (full mechanic, see below)... still to do, as traced in pop3-rev-analysis.md "Per spell" and "Components". None of Earthquake's lava, Volcano's lava/turn-110 kill or Swamp route through `effect::hurts`/the shield check: a shielded unit still drowns or burns when the ground under it changes (reported, not traced), unlike a blast, lightning, swarm or hypnotise. Lightning is done (`lightning`), its victims now carbonized too (anim 97, "Units" above). Burning trees (`Tree::burning`, 127 turns then burnt to size 0) and the ground fire (`ground_fire`, 9 turns) have guessed lengths: the original's own ground-fire object (scenery class 5 type 0xA, not an effect) is now fully traced (pop3-rev-analysis.md "Ground fire, ignited trees, and a burning building's flame") - its burn countdown is caster-dependent and exact (24 turns Lightning, 22 Firestorm), but that is a DIFFERENT, separate timer from a tree's own burn-to-size-0 length, which remains unconfirmed (not a guessed 127, genuinely untraced - no known trigger arms it explicitly). See that same section, and the two new TODO items below ("Wood", "Buildings"), for the further visual-fidelity gaps this traced (no companion smoke/ember/settling-ash object, flames parented to the structure instead of one independent fire object) found on top of the length question; plants, wood piles and fires burning too (burn cell 7/0x05).
- [x] Angel of Death (model 13, `spells.md`, pop3-rev-analysis.md "Angel of Death mechanics"): the logic is in
  (`game_core::angel_of_death`), and so is its art now (`game-client`'s own `angel_of_death.rs`/`angel_meteor.rs`,
  separate session): `UnitKind::AngelOfDeath` is still deliberately left out of `UnitKind::ALL` (no 2D unit
  sprite, never drawn by the ordinary `units/` pipeline), but it now has its own `views::WorldView`, drawing the
  original's real 3D object-bank mesh instead (like a vehicle) — the already-catalogued "winged death" model
  (`objects.md`/`catalog.rs`), whose `WINGED_DEATH_TOTEM`/`WINGED_DEATH_PERCHED` variants the totem gift's static
  monument already loads. The flying creature itself uses the OTHER two variants, `WINGED_DEATH_BACK` (100, used
  while descending/diving) and `WINGED_DEATH_ALT` (107, used while cruising level), cross-blended whenever its
  vertical motion changes direction rather than swapped instantly (pop3-rev-analysis.md "Angel of Death
  mechanics" "Visuals" — the exact 3-step blend table isn't pinned down, so `angel_of_death.rs` [client] does a
  straight linear crossfade between the two meshes instead, `[ours]`, over `BLEND_TICKS` once its height last
  changed direction, tracked client-side since `Flight` itself keeps no history of its own). Still open, both
  user-reported, playing it: it only ever yaws to face its heading, never pitching or banking (`body_rotation`,
  `[ours]` — the trace's own dive/cruise mesh swap is meant to carry that, but a flat slab never nosing down or
  banking into a turn still reads oddly, "the angle is strange"); and the trace's own confirmed THIRD pose —
  "a dedicated 'launching/accelerating' pose... a climb sequence... when committing to a fast manoeuvre"
  (pop3-rev-analysis.md "Angel of Death mechanics" "Visuals") — is not drawn at all, only the cruise/dive pair
  (closest real counterpart to "flapping wings" read as missing; no literal flap cycle was ever traced). With
  `--no-original`, a stand-in flattened box instead (swept narrower for diving, wide for cruising). Its ground
  shadow needed no new code: `units/shadow.rs` already casts one for every unit regardless of kind. A unit it has
  grabbed (`Action::Grabbed`) is now drawn at its captor's own flight height too (`units::carried`), rather than
  dragged along the ground while the angel carries it off overhead — the simulation only ever copied its `x`/`z`
  to match, not its height. `WINGED_DEATH` itself (7, "front") is not used by the original's own flight code and
  was not needed for this. The cast's own invisible meteor fall now trails a shower of glowing spark particles
  around its line of fall (`angel_meteor.rs` [client], pop3-rev-analysis.md: "36... orbiter spark particles...
  scattered around it with staggered lifetimes" — approximated as each born at its own point along the fall and
  fading in and out over its own short life, `[ours]`, the particle-by-particle choreography not traced further).
  The ground shockwave itself needed no client code at all: it is a real terrain change
  (`TerrainChange::AngelOfDeathShockwave`, an expanding-then-settling ring, 9 ticks), redrawn by the terrain mesh
  the same as any other. Still not drawn separately: the spawn-moment blast (`SimpleBlast`, like Lightning's and a
  building's own destruction, already invisible by convention — a push/damage effect, not a decal). Cast: the
  meteor falls for `FALL_TICKS` (8), then the ripple and the blast fire at once, and the real creature appears
  (`WorldEvent::SpawnAngelOfDeath`). From there it runs entirely on its own (`GameMap::tend_angels_of_death`),
  never through `Command` (`Action::can_take_orders`'s own exclusion
  list keeps it, and whoever it grabs, unorderable and unselectable): hunts a rival angel first, else the nearest
  eligible enemy anywhere on the map (shamans included); grabs it once close (`Action::Grabbed`, no orders, no
  movement of its own, no dedicated "held" pose — none was found either) and carries it `CARRY_TICKS` (28) before
  releasing it to die through the ordinary damage pipeline. User-reported, playing it, and confirmed in code: "once
  close" (`tend_closing`'s own `GRAB_RANGE` check) is `x`/`z` distance alone, `flight.height` never factored in and
  never deliberately lowered towards a grounded victim (only ever clamped to a *minimum* clearance above terrain,
  `MIN_CLEARANCE`) — so it grabs someone on the ground from `CRUISE_HEIGHT` without ever visibly diving down to
  reach them. The trace itself doesn't resolve this either way (only the separate "launching/accelerating" climb
  pose above is confirmed, not a literal dive-to-grab manoeuvre), so whether the original actually swoops down
  before a grab is itself an open question for a future trace session; until then this is `[ours]`, tagged rather
  than guessed at silently. Against a rival angel, fights by health instead
  (both take a cut each clash, `FIGHT_COOLDOWN` apart, the weaker knocked back) — see the module doc's own
  "Simplified from the trace" list for exactly which choreography details (`[ours]`) aren't literal. Every tick it
  also forces any eligible enemy within `PANIC_RADIUS_CELLS` (8) to panic, excluding shamans and other angels
  (confirmed, see "Units" below). Never pushed by a Blast (`blast.rs`, confirmed); excluded from Swarm's own sting
  (confirmed). Left undone, same as the trace itself leaves open: the literal kill-count/duration expiry rule
  (`AOD_KILL_COUNT`/`AOD_DURATION`), whether it can attack a building, whether it can grab someone riding in a
  vehicle, and the firewarrior-bolt ~10% damage factor (`fights: false` already keeps it out of the ordinary
  attack/spotting pipeline entirely, simplest reading of "cannot be ordered" extended to "cannot be ordered to
  attack it either"). Also still open: totem gifts' own Angel of Death reward, trigger type 5 and level 5's
  winged-death totem (`TODO.md` "Totem gifts", "Trigger semantics", "Level 5") don't spawn one yet — nothing
  calls `angel_of_death::cast` but the spell book/panel UI (already wired before this session) and tests.
- [x] Firestorm's own look (pop3-rev-analysis.md "Firestorm fireballs"): fixed — `fireball_shot.rs` [client] gives `panicking_blast`'s own effect a view of its own (both Firestorm's and Volcano's fireballs, which share it): a bright ball falling from above, shrinking as it nears the ground, a few dimmer embers trailing behind it (not `smoke.rs`'s grey puffs), landing in a flash that grows and fades into a last puff of smoke; `ground_fire.rs`'s standing flame is unchanged. Our own invented flight and look throughout (the original's hfx 1120-1127/90-93/~1180-1210 sprites, confirmed by eye last session, are not traced closely enough to follow pixel for pixel). `panicking_blast::VISUAL_TICKS` keeps the effect (and so this view) alive well past its own 3-turn blast purely so the fall/trail/burst has room to play out — the blast's own damage/push/panic are unaffected (already a no-op past turn 3). Blast's own `SimpleBlast` fireball now has its own, separate pair of views instead of reusing this one (`blast_fireball.rs`, `blast_impact.rs` [client], TODO.md "Blast push and flight"): its flight is a straight line from the shaman rather than a fall from above, and its landing is sized small on purpose rather than matching the blast's own multi-cell damage radius.
- [x] Firestorm fireballs landing on water no longer start a ground fire (`firestorm::impact`, spells.md): matches
  the original's own coastal-shape check (pop3-rev-analysis.md "Fireball impact") via `unit::is_sea` on the
  landing cell - the blast itself and a building's own ignite (if any) still happen either way, confirmed. The
  splash/hiss effect itself (sound 0x55, hfx 1152) is still not modelled, "not traced further" even in the exe
  analysis - a water landing is silent and fireless rather than wrong. Blast has no fireball-impact mechanic of
  its own yet at all (a separate item, "Every spell flies from the shaman as a shot" above), so this only ever
  applied to Firestorm in practice; Volcano's own fireballs (`volcano.rs`) don't spawn `ground_fire` either,
  unaffected by this fix, not re-examined here.
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
- [ ] Sea level: only height 0 is sea in the simulation, but the original draws very low ground (height 1, e.g. the inlet by level 19's boat hut) like water; find the original's threshold. **Partially addressed 2026-10-23, render-only**: `terrain_texture::looks_like_sea`/`VISUAL_SEA_HEIGHT` (4, a guess, not the confirmed threshold) now makes the GROUND SHADER treat height <= 4 as "sea" for its land/water branch choice, separately from `game_core::unit::is_sea` (still exactly `SEA_LEVEL` for all of pathing/drowning/building placement, untouched) - reported (level 38, original assets): a coastal cell at very low but nonzero height was rendering through `land_colour`'s own `cliff` remap instead of as water, reading as lava/rock (a steep drop to the seabed there legitimately pushes `blend` high, and that theme's own cliff table apparently has reddish "damaged ground" entries at high blend - terrain-textures.md "cliff"). **Confirmed this session: it is NOT a flat height cutoff at all.** `set_texture_landscape_main`'s own land/water branch (`0x4BE3F4`) tests the SAME precomputed per-cell coastal-SHAPE classification (map-cell shape nibble indexed into table `0x5AA328`) that `is_land_at`/`0x44F980` and the drowning predicate (`0x4EEFF0`) already use elsewhere - a shore cell's exact sub-cell shape, not a height threshold. `VISUAL_SEA_HEIGHT` is the wrong shape of fix: the real rule needs the terrain's own precomputed coastal-shape class (already derivable the same way our existing `is_sea`/pathing code presumably could be, or by computing the same shape classification client-side) rather than a tunable height constant - a height-based approximation will always have cases like the reported one.
- [ ] Buildings do not heal: damaged ones need repairs by braves, using wood.
- [ ] Attacking buildings (combat.md "Attacking a building"): enemies on a plan's cells count as its people; the flying faces of the partial explosion; the original's jitter is ±2 units (ours tilts); the player warned once. Blasts (40, `blast::DAMAGE_BLDG`) and firewarrior bolts (100, `firebolt::TARGET_DAMAGE`) adding wear is now implemented - only Blast's own `SimpleBlast` wears buildings this way (once per building it ever reaches, confirmed - not every
  turn like people/vehicles), not `panicking_blast` (Firestorm's fireball blast): the trace's "blasts
  (BLAST_DAMAGE_BLDG 40)" cites the constant by the Blast spell's own name, and nothing confirms Firestorm's
  separate blast variant does the same, so left unextended rather than guessed.
- [x] Burning building's visual (pop3-rev-analysis.md "Ground fire, ignited trees, and a burning building's
  flame"): fixed - the footprint-grid flame billboards parented to the building (`fire_bases`, `Fire`,
  `burn_down`, all removed from `buildings.rs`) are replaced by one independent companion fire
  (`fire_flame::CompanionFire`, shared with the tree fix below), its own `burning_buildings.rs` world view
  spawning it once at the building's own door (`Building::door`), not parented to the building's model. Its own
  visual life is assumed to share the building's structural fire (`fire::BURN_TICKS`, 127 turns - "assumed",
  since no separate re-arming call was traced for it): full height throughout (band 1, flat, confirmed for a
  duration this long), continuous drifting ground smoke the whole time (`fire_flame::FIRE_SMOKE`, a `Puffs`
  loop), one brighter ember exactly `EMBER_BEFORE_END` (4 turns, confirmed) before it ends, then a settling-ash
  sink (`fire_flame::height`, `SETTLE_TICKS` - `[ours]`, a guess: the trace only confirms the sink happens, not
  its own length) before it disappears, all within the building's own already-burning window (no game-core
  change needed). Tested (`fire_flame.rs`) and checked live (`just shot`, original and `--no-original`). Left
  open, same as before: whether a second, not-yet-traced mechanism (same section, `0x4089AC`) also drives a
  building-local flame-intensity distinct from this door-point object - not resolved, so not a basis for a fix.
- [ ] Ashes (combat.md "Ashes"): the hover text ("damaged ground", our words), the long damage smoke over them, the whirlwind's destruction. Lava and earthquake ground sharing the mark is done (`lava.rs`'s `WorldEvent::Scorch`, both `Mode::Static` and `Mode::Flowing`).
- [ ] Construction, see [buildings.md#construction-planned](docs/specs/buildings.md): only hut (size 1), drum tower, training huts, boat/airship huts; never the reincarnation site, prison, vault, totems.
- [ ] Build tab: icons, discoveries unlocking "?".
- [ ] Blueprint: tune `STEEP_SPREAD`; check the door side of the other kinds against the original objects.
- [ ] Construction pace: **the overall shape is now known and is NOT a per-worker timer** (pop3-rev-analysis.md "Building construction and wear"): the original drives the building's 0-4 visual stage directly from a wood-stock counter against a per-type total cost (300/900/1100/800/200/500/1100 wood by kind, exe-confirmed) via `stage = clamp((stock*4-1)/(cost-1), 0, 3)`, not a fixed jump-then-build animation length. The assigned-brave wood-fetch throttle is now concrete too: at most half of a site's currently-assigned braves (rounded down, minimum one), minus however many are already out fetching, are ever recruited to fetch wood at once, clamped to how much is still needed - a separate "plan" controller per site (not the enforcement point for "each kind has only a maximum", which is elsewhere), and not on a recurring 16-tick cycle (that rhythm belongs to a different function, the huts' growth-check cadence, not this controller - see "Huts: breeding and growth"). Our own `work.rs` throttle ("at most needed - delivered braves are out at once") is a different rule than this and could be brought in line with it, not yet done. Still open: `JUMP_TICKS`/`BUILD_TICKS`/`JUMP_STEP` as literal per-worker durations likely don't correspond to anything in the original at all; how a dropped wood pile next to a site actually becomes stock progress (the final delivery hop) was not resolved and needs a live memory check, not further static search. Whether braves gather and watch first before flattening is untouched by this pass, still open.
- [ ] Walls: replan only walkers whose route crosses a changed wall (every walker replans today); a boat hut's jetty side stays open to boats, to check with vehicles.
- [ ] Units inside a building are hidden but still selectable with a box; check what the original allows. Units walking out of several buildings: the door may be blocked by another wall or the sea.
  **Partly addressed 2026-10-30** (reported: a warrior-training site's door facing a neighbouring house, its
  workers stuck idle however they were ordered once the door was sealed): `placement::can_place` now refuses a
  *new* placement that would wall shut a standing building's door, in either direction (`nothing_may_wall_a_door_
  shut_either_ones`).
  **Client preview fixed, 2026-11-10** (reported live: a door-blocking spot still showed the ordinary white
  footprint, with the click then silently doing nothing - `can_place` itself was already right, only the
  blueprint's own red-or-white preview in `game-client` had never been taught the door rule, predating it):
  `placement::door_blocked` made `pub`, `blueprint.rs`'s `all_red` now also checks it
  (`a_plan_that_would_wall_a_neighbours_door_shut_is_refused_and_shows_red`).
  Still open: a door sealed some other way (a layout already on the map, e.g. an original
  level, or two plans placed before either has a door yet) still traps whoever is inside for good - no order ever
  frees them, since `Unit::tick`'s inside-a-building branch always retries the door and never looks for another
  way out (repro: `work::tests::a_worker_trapped_when_its_only_door_cell_gets_walled_never_gets_out`, `#[ignore]`d
  pending this). Needs its own recovery, not just prevention - e.g. relocating a unit whose door has had no free
  cell for some sustained time to the nearest free cell outside its building, the way `buildings.md`'s own
  (currently unimplemented) "a unit already standing on a new site's footprint is moved to the nearest free cell"
  design note already describes for a related case.
- [ ] CC0 sheets: a jump pose for flattening (`Pose::Jump` falls back on arms up) and a hammer pose (`Pose::Hammer` falls back on idle).
- [ ] Find the original hammering anim (the axe swing, anim 11, stands in for `Pose::Hammer`).
- [x] Huts: auto-housing of idle braves implemented (huts-and-training.md rule 3, `GameMap::tend_auto_housing`,
  `enter.rs`): still open from this item - breeding (done separately, `hut_growth.rs`), mana from people inside.
  **Eject implemented, 2026-11-10** (user question: is "selection + Shift + click a building dispatches to a
  free spot" implemented? Answer: mostly yes already, but via the Build tab icon's own Shift+click
  (`send_in.rs`/`GameMap::send_into`, ui-and-editor.md "Build tab"), not by Shift-clicking a specific building
  directly - that gesture did nothing extra until now. shortcuts.md's own "Buildings" table already names what
  it should do instead: "one follower comes out" - `Command::Eject`/`GameMap::eject` (lowest-id occupant,
  `[ours]`, not itself traced which one), `selection::eject_click` wires Shift + left click on one of the
  player's buildings to it, independent of the current selection. This also corrected huts-and-training.md's
  own older, wrong description of `Command::Eject` as emptying the whole building - never actually
  implemented, and contradicted by shortcuts.md's sibling row for a vehicle using the same "one ... out"
  wording). Still open, flagged by the same shortcuts.md row and left out of this pass: "Shift + left click on
  a vehicle: one follower gets out" - same mechanism, applied to `Vehicle::passengers` instead of a building's
  occupants. No autonomous wood-gathering by an idle person was found this session (pop3-rev-analysis.md "Idle
  behaviour"): a person with no command group does nothing at all, forever; the one confirmed autonomous,
  non-player assignment (a training building's own per-turn code, 0x405B80) looks like occupant/queue
  positioning, not wood-fetching, pending its unread "doing it" half - don't add an idle-brave-fetches-wood
  behaviour without a further live check confirming it. Auto-housing itself is `[ours]`, not traced at all
  (no corresponding exe behaviour found or looked for this session) - the delay and search radius are both
  guesses, flagged as such in `enter.rs`.
  **Auto-work implemented, 2026-11-11** (huts-and-training.md "Huts" rule 8, `GameMap::tend_auto_work`,
  `enter.rs`): the same idle brave also joins a nearby construction or dismantling site on its own
  (`GameMap::assign`, same as `Order::Build`), within `SITE_SEARCH_RADIUS_CELLS` (3) and up to
  `MAX_AUTO_WORKERS` (4) per site. Pop3-rev-analysis.md's own closest match for this (`0x4D9B50`) reads as
  AI-tribe-only - a player's own recollection of this happening for a human-controlled idle brave contradicts
  that reading; a coding session cannot recheck the gate against the exe itself (clean-room), so this is
  flagged as an open question for a spec session rather than resolved, and implemented for the player's own
  tribe regardless, `[ours]`. Specifically still left out, since the recollection was only about construction:
  autonomous wood-gathering (the caution above stays as is - no new evidence on that one).
  **Found along the way, not fixed here, worth its own session**: `work_on`'s own per-tick order (`fetch_wood`
  before `flatten_step` whenever the site still `wanted` more wood) can starve the very last flatten point of a
  site that is crowded with workers - a brave standing exactly on it, freshly done with its own task, can lose
  its claim to a wood-fetch errand before `flatten_step` ever re-considers it, while nothing else is actually
  finishing that point either; `MAX_AUTO_WORKERS` above only lowers how often this is reached in practice, it
  does not fix the underlying race. Reproduced live at 16 workers sharing 8 flatten points on a training hut
  (`enter::tend_auto_work`'s own cap exists because of this); 5 workers on the same site never reproduced it in
  the time spent on it this session, so it's plausibly rare in ordinary play but real.
- [ ] Wood claimed across sites: two sites' braves can head for the same tree piece; braves of another task may take wood a site counts on. Refine the dispatch rule (nearest brave per piece, gatherers vs builders).
- [ ] Site visuals: views are redone whenever any building changes; update them per building (stable ids) instead; compare the part order (from the ground up) and the structure with the original; chimney position per hut model; busy look for the other kinds; the pile by the door could be drawn as a stack rather than loose pieces.
- [ ] Wood pieces lying on the ground (`GameMap::wood`, drawn): picked up by any construction; hovering one could show "Wood". Maybe draw the original's shadow under them (`hfx0-0.dat` 22, unconfirmed).
- [x] Destruction of buildings on uneven/flooded ground: implemented (`game_core::foundation`, docs/specs/buildings.md "Foundation"), both destroy reasons, matching the trace (pop3-rev-analysis.md "Terrain changes: what reacts", "Building sinking (flooded destruction)"). **[ours] a periodic per-building scan** (every `CHECK_EVERY` turns, every turn for a water-sited kind) instead of a reactive hook on `WorldEvent::Terrain` - this doc's own earlier plan - so it never needed routing every direct `Heightmap` write (`Spell::Erode`/`Raise`, construction settling, a reincarnation site's own re-levelling) through that event first; it just re-reads the ground under each building whenever its own rhythm fires, up to `CHECK_EVERY`'s own latency (0.33 s) behind whatever changed it. Sub-state 1 (uneven) shatters at once (`Collapse::Shattering`, removed the tick after so the client can still catch it for `explosion::BuildingExploded` - no more need for `EXPLODE=building:N` to be the only way there, though the dev hook still works); sub-state 2 (flooding) sinks over 80 turns (`Collapse::Sinking`), drawn client-side dropping on the ramping curve and tilting with a decaying wobble (`buildings.rs`'s `sink_depth`/`sink_tilt`) - **the original's own compass-histogram lean target, oscillator formula and final spiral/splash flourish are still not traced**, so the wobble is our own approximation and the flourish isn't drawn at all. Neither reason leaves ashes, matching the trace.
- [x] A tree whose cell becomes water drops to size 0 and stops regrowing while submerged, done (trees.md
  "Simulation"): `Tree::tick` now takes a per-turn `submerged` flag (`GameMap::tick`, `unit::is_sea` against the
  live heightmap), the same periodic-recheck shape as the building foundation check above rather than routing
  through `WorldEvent::Terrain`.
- [ ] Damage from moving ground: check in the game what sets the damage (level 10's atlantis island comes back with its buildings missing 1 to 3 pieces of wood, no common percentage), and whether repairs need fetched wood ([buildings.md#damage-and-repair](docs/specs/buildings.md)). Likely candidate, not independently confirmed as THIS trigger: the ordinary terrain-reaction foundation check (pop3-rev-analysis.md "Terrain changes: what reacts", `0x408080`) explicitly does no damage for its slow pull-back case (only two all-or-nothing destroy reasons, uneven/flooding - neither matches "missing a few pieces"), so a probabilistic partial loss matching "1 to 3, no common percentage" is much better explained by the Earthquake spell's own already-traced building-shake (pop3-rev-analysis.md "Per spell" 14 / Reference `0x50D9A0`/`0x50FE60`/`0x4980A0`): 3 independent 20/128 rolls (turns 24/56/88), each a flat −100 wood (one piece) on a hit - 0 to 3 pieces lost, no fixed percentage, exactly the reported shape. Atlantis rising is plausibly scripted as an actual Earthquake-type effect rather than a plain terrain write, which would make this a match - not confirmed; check the level's own script/trigger for an Earthquake effect, or live-watch for the spell's own 120-turn shake cadence during the rise.
- [ ] Effects 89 atlantis set / 90 atlantis invoke (level 10): sink the island at start, raise it back when the stone head trigger fires; 83 boat hut repair.
- [ ] Towers (drum tower): hold one unit (going in is done, `game_core::enter`), which gets a longer range from there: firewarriors throw farther, the shaman casts spells farther, a preacher converts enemies around the tower. No other tower effect.
- [ ] Training huts (huts-and-training.md "Done"): the queue's ripple (2 turns per rank) and those already of the building's kind letting the others pass, five trained together (the exe's count; ours holds one - a [player] note, reaffirmed, says it's always one at a time in play, so this needs a live check before five-at-once is implemented), queued orders carried over, damage cancelling the queue and sending the trainee out.
- [ ] Training as in the original, what is left (pop3-rev-analysis.md "Training buildings"): the computer players' cost (`CP_` 1000) once tribes have a controller; the trainee's queued orders given to the new unit. The training time is already answered, no live measurement needed: the original has no fixed duration at all, just the mana cost filled at `cost / 32` per 16-turn income tick (same section) - 256-512 turns (21-43 s) at best, scaling with cost like everything else in the mana-income system.
- [ ] Reincarnation sites from pop3-rev-analysis.md "Reincarnation sites": the ground levelled to the site height over a 5-cell disc in about 21 turns (ours: radius 3, blend 4) with its effects on enemies, wildmen, trees and swamps; pillars rising out of the ground, deleted and moved; the 32 sparkles.
- [x] The site's build/rebuild is not instant, and is driven by the shaman, not level load directly
  (pop3-rev-analysis.md "Build/rebuild ritual"): she walks to the anchor, settles 12 turns, triggers the ~21-turn
  ground levelling above, then fires one projectile per pillar slot a turn apart (slot order 7..0), each pillar
  appearing 1-3 turns after its own shot lands and rising independently from there (so up to 3 stones can be
  mid-rise at once) - total spread about 15 turns (1.25 s) for all 8. Ours (game-client `sites.rs`) places all 8
  stones at once with no rise or stagger at level load. The same ritual (walk/settle/flatten/fire) also runs on
  a relocation, e.g. a new site forming at a dead shaman's last position - not just a one-time level-load step;
  replacing a pillar in place reuses a smaller version of it (destroy with a burst, spawn the new one, a second
  burst as a "pop-in" flash). Precondition: only run this ritual for a tribe whose site is currently enabled
  (`ReincarnationSite::enabled`/`SET_REINCARNATION`, already implemented) - a level's own AI script can turn it
  off before this would run, as level 1's red tribe does at turn 0 even though it has a shaman thing (ai-scripts.md
  "Worked example: level 1's opening flyby"), so a disabled tribe must get no site and no build animation at all.
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
  **Shaman-driven ritual implemented, 2026-11-08**: `ReincarnationSite::animated` (true for a real level,
  `sites_from_level`; false for every sandbox/generated/dev site, `at_cell`, so none of them lose their
  instantly-usable shaman) gates a new `Action::BuildingSite`/`SiteBuildState` on the shaman (`site.rs`,
  `unit.rs`) - settle 12 turns, then the ~21-turn levelling (still instant, not the growing disc: left to
  the radius/disc-growth question above and its own animated version), then one `SiteShot` (standard shot,
  1400 u/turn, `site.rs`) per pillar slot a turn apart in order 7..0, each slot's own pillar built
  (`ReincarnationSite::pillars`) the instant its shot lands, with a burst (`blast::explosion_effect`) first
  if it's replacing one already standing. Runs at level load and every reincarnation alike, not just once;
  aborts to Idle with no pillars at all if the site is disabled first (`site.is_none()` check each tick),
  covering the precondition above. Client (`game-client` `sites.rs`): each pillar mesh rises from buried to
  its resting height over `pillar_height`'s own guessed `RISE_SECS`/`RISE_DEPTH` (not separately measured);
  input is locked while the player's own shaman is in `Action::BuildingSite` (`sites::site_build_active`,
  alongside `armageddon::input_locked`/`flyby::flyby_active` at the same 3 call sites - closed a pre-existing
  gap there too: `select_and_order`/the spell-casting systems weren't checking `flyby_active` before). **Not
  done**: site relocation (a new site forming at a dead shaman's last position) - `ReincarnationSite`'s
  position stays fixed as documented, so every reincarnation reruns the ritual in place instead; a replaced
  pillar rises the same way a fresh one does (buried to resting height) rather than the original's own
  "smaller version" of itself.
  **Bugs found testing on level 1, 2026-11-08, first two fixed same day**:
  1. **Fixed**: pillars no longer poke above ground while buried - `sites.rs`'s `RISE_DEPTH` was `STONE.y`
     (0.6 cells), the *generated placeholder's* own height, reused as the bury depth for the original mesh
     too; the real `REINCARNATION_STONE` object measures taller (points y=0 to y=400 of 512/cell, 0.78 cells,
     origin at its own base), so 0.6 left its own tip showing. Bumped to a flat 1.2 (margin over the measured
     0.78) rather than computing either mesh's exact bounds at runtime - covers the generated block's smaller
     need too, confirmed clean on a `levl2001.dat` video capture through the whole settle/level/fire sequence
     (no sliver, no premature pop, the pop-in burst and staggered rise all read correctly slot by slot).
  2. **Fixed by the same change**: the rise animation itself (`pillar_height`, `RISE_SECS`/`RISE_DEPTH`) was
     never actually visible correctly before - issue 1's shallow bury depth hid it behind a visible sliver the
     whole time. Confirmed by the same video: each pillar now reads as buried, then rises cleanly over
     `RISE_SECS`, staggered one slot after another with its own burst.
  3. **Investigated, not this feature's bug**: reported laggy/low fps through the sequence. Profiled
     (`just profile`) both `levl2001.dat` (through the ritual) and the `sandbox-walk` sandbox (no ritual at
     all, every site `!animated`) - both show the same dominant cost, `bevy_render`'s own per-frame
     `allocate_and_free_meshes`/`prepare_assets<GpuImage>` (~1.5-2 ms/call, every single frame of either run,
     not just during the ritual), and comparable total `update` time per frame (~10-11 ms) in both. So this
     isn't the ritual's own systems (`rise_site_pillars` is a handful of transform writes, not mesh/image
     work): it's the already-tracked "Priority 5 - performance" / "Performance (60 fps)" item below, whose own
     notes already name the likely cause (`animate_views`/`respawn_views` swapping `Mesh3d`/`MeshMaterial3d`
     handles every frame instead of true instancing) - this session's numbers are one more data point for that
     item, not a new one of its own.
  4. **Re-opened, 2026-11-08: the 2026-11-08 "fix" below used the wrong look - a fresh trace caught it before
     it reached the original's own real visual.** pop3-rev-analysis.md "Build/rebuild ritual" has been
     corrected: the shot's own in-flight decoration is NOT a lightning-bolt shape at all - it is a tiny,
     tribe-coloured, four-point twinkle, the same generic small-sparkle look the Reincarnation Centre's own
     ground-levelling ring already shows nearby (just a different tint), not a jagged bolt/crackle of any
     kind. The previous entry below (now struck through in spirit, kept for the record) reused `lightning.rs`'s
     zigzag/ribbon bolt - the wrong asset for this: that component is a big jagged ribbon mesh meant to read as
     a dramatic lightning strike, nothing like the subtle twinkle the original actually shows. **The right
     asset to build from is `sparkles.rs`** (the small twinkling-star view already used for terrain-reshaping
     spells - land bridge, flatten, volcano): same general look (a tiny star/point-light that grows, rises and
     fades on a loop, `Twinkle`/`LOOP`/`SIZE`/`RISE`), but that component is currently wired specifically to
     `Effect::reshaping` (height points still moving) and uses one fixed warm-gold `COLOUR`, neither of which
     fits a travelling shot - it needs a generalised/parameterised version (driven by the shot's own jittered
     sub-step positions instead of reshaping cells, tribe-coloured instead of fixed-gold) rather than being
     reused as-is. `site_shot.rs` should go back to spawning a handful of small tribe-coloured twinkles
     scattered near the shot's own current position each tick, not jagged bolt segments - revert the
     `lightning.rs` reuse (the `pub(crate)` exports added for it can go too, unless something else now depends
     on them) in favour of a small twinkle view built the same way `sparkles.rs` is. Left as the original's own
     further nuance, not pursued: the exact 20-point density and the shot's own slight per-sub-step wobble
     (subtle at this short range, pop3-rev-analysis.md's own "may not be worth it for the visual alone").
     - **Original (incorrect) entry, kept for context**: "Fixed, 2026-11-08: the shot's own look (`site_shot.rs`)
       was a guess (a straight glowing ball on a straight glowing rope tethered to the shaman the whole flight)
       ... reusing `lightning.rs`'s own `zigzag`/`ribbon`/`cross_texture` ... to spawn `CRACKLES` (3) short
       jagged bolt segments ... confirmed on a `levl2001.dat` video capture (a small scattered crackle shows
       near the shaman while a shot is in flight, not a straight line)." That video capture only confirmed the
       code ran as written, not that it matched the original - the trace it was built from had never actually
       checked what the original draws, only inferred a shape from an internal component name.
     - **Fixed for real this time, 2026-11-08**: `sparkles.rs` generalised as asked above - `twinkle`'s own pose
       maths factored into `twinkle_at(key, period, now)` (`pub(crate)`, period no longer hardcoded to this
       effect's own `LOOP`), `mix` made `pub(crate)` too, `star_texture` was already `pub`. `site_shot.rs`
       rewritten around these instead of `lightning.rs`: `FIREFLIES` (4) small tribe-coloured twinkles
       (`tribe_color(s.owner)`), each independently jittered (`firefly_jitter`) within `JITTER` cells of the
       shot's own current position (not spread along a path to the shaman - matching the corrected spec's "near
       its own current position", a cluster drifting with the shot rather than a line), on a short `PERIOD`
       (4 ticks, our own guess sized to fit a whole grow-rise-shrink cycle into the shot's brief 1-2 tick
       flight) instead of `sparkles.rs`'s own many-tick `LOOP`. The `lightning.rs` reuse from the previous
       (wrong) attempt was left in place rather than reverted (its `pub(crate)` exports are harmless unused
       weight, not worth churning back out). **Bug caught before shipping**: the quad mesh was first built
       pre-scaled to `SIZE` and then scaled *again* by the twinkle's own size each frame - double-shrinking it
       to near-invisible; fixed to build a unit quad (`sparkles.rs`'s own `SparkleAssets.quad` does the same
       split). Confirmed building and running clean against `levl2001.dat`; a clean, uncluttered look at the
       very first shot (before the landing burst starts overlapping it) wasn't fully isolated on screen this
       session - the existing landing-burst flash (`blast_impact.rs`, unrelated to this item) is far brighter
       and briefly dominates the same spot once a shot lands, same as it already did before this change.
- [x] Houses breeding and growth implemented (`game_core::hut_growth`, huts-and-training.md rules 5-6, fully
  traced `[exe]` and corrected where this entry's own earlier draft had guessed wrong - the band formula, the
  "stops at 100%" vs "resets to 0" behaviour, and growing's own wood seeding). Capacity 3/4/5, supply 3/5/7
  (cap min(supply, 200)) were already done (`BuildingKind::capacity`, `headcount::{SUPPLY,Housing}`). Mana from
  people inside was already done too (rule 7, `GameMap::mana_income`). Left for the client: the tooltip's green
  (`breed`) and red (`grow`) bars, and a star animation where a bred brave appears. `SPROG%`'s full 20-band table
  is now confirmed from `constant.dat` directly (pop3-rev-analysis.md "Huts: breeding and growth"), replacing the
  earlier `[ours]` linear-interpolation guess - check `hut_growth.rs`'s own table against it.
- [ ] A hut that already exists when the map loads (an original level's starting village, not one built during
  play) starts its breeding bar cold: `buildings_from_level` goes through the plain `Building::new` (`used:
  kind.wood_cost(), breed: 0, ...`, `building.rs`), never `hut_growth::on_completed`'s own small head start for a
  hut whose construction finishes mid-game ("the first brave comes quickly", huts-and-training.md rule 5) - so a
  level's own starting huts need a full breeding cycle before their first brave, same as one just placed by a
  player. **Guess, not traced or observed, flagging for a live check**: existing houses at map start are probably
  already well along, maybe around 80% of their target, not freshly reset - matches the general shape of
  `on_completed`'s "head start, not a cold 0" reasoning, just a bigger one for a hut that's presumably been
  standing and breeding for a while already when the level opens. If confirmed, seed `breed` this way wherever a
  hut comes from a loaded level (`buildings_from_level`), not `Building::new` generally (a freshly placed one
  during play should stay cold, or use the existing smaller `on_completed` head start once its construction
  finishes).
- [x] **Population cap was missing the per-tribe base of 6** (pop3-rev-analysis.md "Huts: breeding and growth",
  "Impact on our specs" rule 5): the original's `can_breed` cap is `min(200, 6 + sum(hut supply))` - a flat 6
  added per tribe, hardcoded in `load_level` (0x42B428) for every tribe regardless of hut count, not in
  `constant.dat`. Fixed: `headcount::BASE_POPULATION: u16 = 6` added and included in `Housing::room()`
  (`game-core/src/headcount.rs`); `ai.rs`'s `housing.room()` read and the client's population strip
  (`hud/population.rs`) inherit the fix automatically. Does not affect `convert.rs`/`sermon.rs`'s own
  `MAX_POPULATION` (200) checks - those match a separate, genuinely flat exe check (preacher conversion's own
  tribe+0x89DAE5 population counter), not `can_breed`'s hut-based cap.
- [x] `game_core::balance`: `Turns` (original game turns) converted to ticks (`Balance::ticks`, the identity
  today), `Balance::ours()` defaults, grouped by topic (`Huts`, `Training`, `Income`), stored on `GameMap` and
  read by `hut_growth.rs`, `training.rs`, `headcount::mana_income` and `mana::ManaPool::tick` instead of their
  own scattered constants (huts-and-training.md "Balance and game turns"). Left: `Balance::from_constants` (needs
  `pop3_format::constants`, not built yet), the wood topic (still plain constants - `BuildingKind::wood_cost` and
  trees' own regrowth are enum methods/free functions called from many places, not a `GameMap` tick), and
  `Balance` is not yet part of the lockstep desync checksum.

### Wood ([trees.md](docs/specs/trees.md))
- [ ] Original scenery models 7 plant 1, 8 plant 2: not drawn yet. Scenery 9 (the totems, 98 in the levels) is drawn as the stone head (object 82), a guess: find its real model. Trees have no size in the thing record: full size is right.
- [x] Wood for repair and hut growth, like construction (`game_core::work`: assigned braves fetch, carry to the door, build): **already falls out of the existing construction system, no special-casing needed** - damage (`wreck.rs`) and growth (`hut_growth.rs`) both just lower `used`/change `kind` and leave the building in `Stage::UnderConstruction`, which `work.rs`'s ordinary fetch/carry/build loop treats exactly like a fresh site once braves are assigned to it (`Command::Assign`/`Order::Build`); confirmed by a new test, `work.rs`
  `a_damaged_building_is_repaired_like_a_fresh_construction_once_braves_are_assigned`. Left open, tracked separately
  (huts-and-training.md "Open questions"): whether a growing hut's own occupants are assigned to the job
  automatically or the player must send braves by hand, same as any other repair.
- [x] **Bug fixed while checking the above**: reusing the construction system for repair/growth had surfaced a real
  regression - `work::work_event`'s own completion handler (`UnitEvent::Built`) called `empty_building`, which
  walks out *everyone* inside, not just the construction workers. Fine the first time a building goes up (nobody
  can be a resident of one not built yet), wrong for a repair or a hut's growth: a damaged-but-occupied hut, or
  one mid-regrowth, would have its own long-standing residents evicted the instant the rebuild finished, instead
  of staying housed "all along" as huts-and-training.md rule 6 already says. Fixed with a narrower
  `GameMap::release_builders` (only `u.work == Some(site)`, i.e. an actually assigned worker) used at completion,
  keeping `empty_building` (evicts everyone) for dismantling starting and `throw_out`'s own destruction/fire/
  collapse cases, which do mean to clear the building out entirely. Tests:
  `work::tests::residents_stay_housed_through_a_repair_only_the_builder_is_released`.
- [x] Burning tree's visual (pop3-rev-analysis.md "Ground fire, ignited trees, and a burning building's flame"):
  fixed - `burning_trees.rs` no longer climbs several billboards up the trunk/crown scaled by the tree's size;
  it spawns one independent companion fire (`fire_flame::CompanionFire`) at the tree's own base instead, fixed
  size, not scaled by the tree. Its own visual duration is the trace's confirmed FIXED 76 turns
  (`burning_trees::VISUAL_TICKS`), decoupled from `Tree::burning`/`BURN_TICKS`'s own, still-unconfirmed 127-turn
  burn-to-size-0 length: the tree keeps "burning" (no wood, eventually snaps to size 0) for however long is left
  after the flame's own 76 turns with nothing shown at all, matching the trace. Continuous ground smoke, a final
  ember 4 turns before the end, and a settling-ash sink before it disappears all come from the same shared
  `fire_flame` module as the building fix above. Tested, checked live (`just shot`: a flame at a lit tree's
  base, gone by turn 76 with the tree still standing unlit until its own much later timer runs out).
- [x] `fire_flame::SIZE` (was 0.35)/`ground_fire::SIZE` (was 0.45) (pop3-rev-analysis.md "Ground fire, ignited
  trees, and a burning building's flame", "Flame billboard size" bullet): fixed - both confirmed WRONG, not just
  guesses - a live ground-fire object (spawned by a real Blast cast, read via pop3-rev's
  `scripts/scan_ground_fire.py`) draws via a real 3D `OBJS0` mesh (table `5A6AF8` row 2, model id 5 in bank 2 =
  bank 0's object 12, a camp-fire sibling), whose own flame-board faces (`FLAME_TILE`) give the exact same
  160x160-unit = **0.3125-cell** bounding box as the ordinary camp fire. The ground-fire/tree-ignition/burning-
  building companion flame is THE SAME SIZE as a camp fire, not bigger - both constants now reuse
  `campfire::SIZE` directly (made `pub`) instead of their own independent, oversized numbers. Full trace:
  pop3-rev's `docs/functions.tsv` row `0x4a66c0`.
  **Correction, traced further and now wrong - see the new item right below**: `BURN_TICKS` is not merely
  "unconfirmed", it is now known to be the SAME fixed 76 turns as the flame, not 127, and the tree is not meant
  to stand full-size and unlit for 51 turns after the flame goes out.
- [x] Burning tree's own size during its fire (`tree::Tree`/`BURN_TICKS`; pop3-rev-analysis.md "Ground fire,
  ignited trees, and a burning building's flame", "Impact on our specs"): fixed - `BURN_TICKS` is now 76, the
  SAME fixed length as the companion flame (`burning_trees::VISUAL_TICKS`), set directly by `ignite()` rather than
  a separate/longer timer. A burning tree now tracks its own wood stock (`burn_stock`, `WOOD_PER_SIZE` units,
  seeded from `size` at ignite) and drains it by 4 every turn (`BURN_DRAIN`), continuously recomputing the
  discrete 0-4 `size` from it instead of holding it constant until the last tick - size 0 is reached as soon as
  the stock crosses `WOOD_PER_SIZE` (the original's own absolute delete-floor), which for anything short of a
  full tree is well before the 76-turn clock ends, and for a full tree a turn before it (burning itself still
  runs the full 76 turns regardless - matches the trace's "disappears... while the flame is still burning").
  No change to how "gone" is represented (`size == 0`/`is_visible() == false`, regrows in place) - that part was
  already right. Test: `tree.rs`
  `a_burning_tree_gives_no_wood_and_its_size_drains_before_the_fire_goes_out`.
  **Drawn scale fixed, 2026-11-10** (reported live: the model visibly snapped to a smaller size every 100 units
  instead of shrinking continuously): the discrete 0-4 `size` above is correctly a whole-piece gameplay unit
  (how much wood is left to give), but `nature.rs` was reading it directly for the draw scale too, so the model
  only ever changed size once every `WOOD_PER_SIZE / BURN_DRAIN` (25) turns and jumped a full 20% of its height
  each time. `Tree::visual_size` now gives a continuous value (`burn_stock` itself while burning, `tree.rs`
  test `visual_size_shrinks_every_tick_while_a_whole_tree_s_size_jumps_at_once_then_holds`),
  `nature::size_factor`/`tree_scale` take it instead of `size` directly (every call site in `nature.rs` and
  `tooltip.rs`), interpolating smoothly between whole sizes.
- [x] `fire_flame::FIRE_SMOKE` (pop3-rev-analysis.md "Ground fire, ignited trees, and a burning building's
  flame", "The ground-fire's own smoke puff" bullet): fixed - the old `Puffs{count:4, life:2.4, emit:Loop,
  spread:(0,0.35), ...}` modelled the wrong SHAPE of emission, not just wrong numbers (the original spawns a
  brand-new, short puff almost every tick for as long as the fire lives, not a few puffs that each drift for
  2.4s). Kept as `FIRE_SMOKE_OURS` (unchanged numbers, `[ours]`, not traced) and used only without the original
  game's own data loaded; a new `fire_smoke_traced(visual_secs)` models the trace instead whenever
  `LevelList::original` (the same `!levels.original` idiom as `flame.rs`/`campfire.rs`/`wood.rs`/etc.): `gap` 1
  tick, `life` ~0.58s/7-tick, `until` the fire's own `visual_secs` (76 turns for a tree, 127 for a building -
  these differ, so `until`/`count` are sized per call, not one shared constant), a near-flat `spread`
  `(0.06,0.06)`, and `rise` matching the trace's 20 world-units/turn over the puff's own life - a retune along
  `SINKING`'s own pattern, not a new emission mode, picked via a new `fire_flame::smoke_pattern(original,
  visual_secs)` (tested, `fire_flame.rs`: `smoke_pattern_switches_between_our_own_loop_and_the_traced_original_
  run`, `traced_smoke_sizes_itself_to_a_different_fire_s_own_life`). `CompanionFire::spawn` takes `original`/
  `visual_secs` now, threaded from `burning_trees.rs`/`burning_buildings.rs`'s own `Res<LevelList>` (already in
  scope there for `flame::FlameFrames`). Checked live (`just shot`, `--no-original`: a burning hut's own flame
  and drifting grey smoke at its door, `LIGHTNING=52,70 LIGHTNING_TICKS=10` on `sandbox-buildings`); the traced
  branch is parameter-tested only, not live-checked, with no original install in this session. Full trace:
  pop3-rev's `docs/functions.tsv` rows `0x50bf60`/`0x50bd70`/`0x50beb0`, `docs/effect_states.csv` row 3.
- [ ] `cut()`'s own wood-removal (braves chopping a tree) likely shares the same underlying mechanism as burning in
  the original (one function used by both, `pop3-rev-analysis.md`'s own Reference `0x4A79F0`) - worth checking
  whether our own `cut()`'s single discrete step vs. a continuous per-chop drain matters for how a chopped tree
  looks, not just a burning one.
- [ ] Check in the game whether a brave ordered on a tree with no wood to spare goes to another one. The gather-wood state's own code (`0x495D70`, the chop-completion branch at `0x4960CC`) has **no tree-depletion check at all once already chopping**: on countdown expiry it unconditionally calls `transfer_wood(tree, person, WOOD_<PERSON>)`, which on an empty tree just moves 0 and the brave starts the same 20-turn countdown again, forever, with no reroute logic anywhere in this function. Whether a NEW order onto an already-empty tree is refused/redirected at assignment time (a different, not-yet-traced code path, outside this per-turn state) is still open - that's where any "goes to another one" behaviour would have to live, if it exists at all.
- [x] A tree does not grow back while a building stands on it; buildings can only be placed over size-0 (invisible)
  trees: both already done and tested - `placement::blocked_at` refuses a spot with a visible tree
  (`nothing_may_wall_a_door_shut_either_ones`'s sibling test `sea_buildings_sites_trees_fires_and_totems_block`,
  "a cut down tree does not block"), and `work::tests::trees_under_a_building_do_not_grow_back` covers the
  invisible tree staying put under a built hut; also already listed as done in huts-and-training.md ("Done":
  "Trees under a building (plans included) do not grow back.") - this entry was just stale.
- [ ] Trees as obstacles for walking (around full trees?), to check against the original.

### Praying (worship) ([worship.md](docs/specs/worship.md))
- [x] Prayer gauge on the original's rhythm and curve (pop3-rev-analysis.md "The gauge", worship.md "Shared: the
  prayer gauge"): moves every 4 turns (`gauge::RHYTHM`, not every tick), gains `C² / (C − n + 1)²` per update and
  drains `C²`, scaled into our existing `STEP`-per-`PrayTime`-unit steps (`gauge::gain`/`step`); wired into both
  `Vault::tick` (now takes an `update` flag) and `GameMap::tend_totems`.
- [x] Contested totems: keep one gauge per tribe (our choice; the original has one per trigger, driven by the tribe with the most prayers), but the gifts are shared: a totem's `NumOccurences` counts across all tribes, so a one-gift totem rewards only the first tribe to fill its gauge, and the other gauges stop. Once `NumOccurences` is reached by any tribe, the totem sinks back into the ground for everyone and every tribe's prayers are sent away (`Totem::is_exhausted`: one shared sink, whoever took the last gift). **Truth sweep: already correct** (`Totem::given`/`is_exhausted`, `GameMap::tend_totems`) - confirmed and pinned down with a two-tribe test (`worship.rs` `two_tribes_contest_a_totem_the_first_to_fill_wins_the_gift_and_both_are_swept_once_exhausted`), no fix needed.
- [x] Totems: ids and removal implemented (worship.md "Totems"). `TotemId`/`GameMap::add_totem` give every
  totem a stable id, the same `new_id_of` convention as buildings/units (`totems_from_level` stays a pure
  `Level -> Vec<Totem>` function, assigned ids only once added via `with_totems`/`add_totem`). `Order::Worship`/
  `Action::Worshipping` now carry a `WorshipAt` (`Vault { corner }` or `Totem { id }`) instead of a bare world
  point, and `GameMap::go_worship` resolves the exact totem/vault an order named instead of re-deriving "which
  one" from a shared position. `GameMap::tend_totems` drops a totem from `GameMap::totems` outright once
  `Totem::is_gone()` (`worship.rs`'s `an_exhausted_totem_sends_its_prayers_away`), safe without extra
  bookkeeping since exhaustion already clears its queue and stops every assigned unit well before sink
  finishes. Incidentally fixes a real bug: level 23's two totems sharing one cell (worship.md) were previously
  indistinguishable by `go_worship`'s exact-position match, so an order to the second totem always resolved to
  the first instead - proven fixed by `worship.rs`'s
  `two_totems_sharing_one_point_are_each_reachable_by_their_own_id`. `game-net`'s wire codec updated to encode/
  decode `WorshipAt` (tag byte: 0 vault corner, 1 totem id). Left out of scope: the client's `totems.rs`
  `TotemView` still keys by `Vec` index, not the new id - not a correctness bug (it is fully rebuilt on any map
  change), just the same known "key views by stable id" cleanup already flagged elsewhere in this file for
  buildings/units.
- [ ] Stone totem: check in the game what it does after a completion that is not its last, the timings (turn 2 s, hold 1 s, sink 4 s) and the smoke; other turns to try are listed in worship.md.
- [ ] Totem gifts not handled yet (logged as `Reward::Unhandled`): effects (land bridge, lightning...), revealed scenery and triggers, vehicles, the Angel of Death of type 5 totems; mana once mana exists. Check what `NumOccurences` 0 means.
- [ ] Pyramids: done for the player (`game_core::worship`); rewards per tribe once books are per tribe (today `granted` updates the shared level books and the player's panels); `PrayTime`'s unit is now confirmed live (worship.md "Open", pop3-rev-analysis.md "The vault"): the same 4-turn tick as a totem's gauge, cap = `PrayTime` directly. The drain speed with nobody praying is also answered, from the shared gauge code itself (pop3-rev-analysis.md "Prayer at triggers" "The gauge" - `gauge -= C²` every 4-turn step when nobody counts, same magnitude as a full-staffed fill, not the `(C−n+1)²`-divided partial-attendance rate) - the vault is confirmed to run through that exact code path with no vault-specific math ("The vault"), so no separate live trial is needed to settle this one.
- [ ] Prisons: levels where the shaman starts imprisoned; she is locked inside (units.md "Locked units") until freed.
  Level 15 has one (so far the only level seen with a prison). Observed live: a pyrotechnic hit knocked the
  imprisoned shaman outside the prison and killed her, so being locked inside does not make her unattackable/
  unkillable as assumed - check what actually protects her (if anything) and what ejects her from the building.
- [ ] Generated pyramid: fold its capstone like the original's top (a separate mesh in `tools/generate_buildings.py`).
- [ ] Pyramids: draw the reward as an icon at the top (cursor sprites 38-57 spells, 58-65 buildings), gone once the reward is granted.
- [ ] HUD: show the gauge over the totem/pyramid and who is praying.
- [ ] HUD sparkle/flash when a discovery is granted at a totem or pyramid (not traced: no sparkle effect or
  timing pinned down yet, just a gap noticed - a reward landing today has no feedback of its own beyond whatever
  the reward itself does, e.g. a spell appearing in the book). Needs a live check of what the original actually
  shows at the moment of the grant before implementing anything beyond a placeholder.

## Priority 2 — computer players (AI) and what it needs
The single biggest remaining system, and the one that turns this from a sandbox/multiplayer tech demo into an actual single-player game. Sequenced after priority 1 on purpose: AI scripts can invoke any spell/building/unit action through the same order/coordinator machinery, so building the interpreter against still-incomplete systems means repeated rework. Tribes and level scripting are its direct prerequisites (per-tribe state, triggers the scripts fire).

### Tribes
- [x] `TribeId` (an enum: blue/red/yellow/green) instead of bare `u8` owners everywhere in game-core, game-client
  and the game-net wire format (`Unit`/`Building`/`Command`/`WorldEvent`/mana/spell/build books), plus `Owner`
  (`Owner::Tribe(TribeId)` or `Owner::Wild`, the wildman/neutral sentinel the old `255` stood in for) wherever a
  unit, building or effect can be nobody's. `game_core::tribes::Tribes` (`GameMap::tribes`) holds, per tribe
  present, its `computer` flag (`Tribes::with_ai_scripts`, loaded from the level header's `ai_scripts` in
  `GameMap::load_original`: nonzero means a tribe is computer-controlled) and allies (`with_allies`, header byte
  92). Left open from the original ask:
  - [ ] The tribe colour still lives in the client (`sites::tribe_color`, taking `impl Into<Owner>`), not the
    engine; `procedural::tribe_rgb` and the other pure sprite-recolouring helpers (shapes/frames/original_anim/
    sheets) still take a plain `u8` index, never seeing an `Owner`/`TribeId` — left alone since nothing feeds them
    one today.
  - [ ] The tribes actually playing the level (header byte 88: 1v1, 1v1v1, 2v1...) are still not decoded/applied -
    see the open report below.
  - [ ] `GameMap::mana`/`spell_books`/`build_books` are still separate `[T; TRIBES]` arrays indexed by tribe, not
    folded into a `Tribe` struct alongside `computer`/allies.
  - Reported: red has no reincarnation site on `levl2001.dat` (level 1) in play, suspected to be this same gap
    (header byte 88 unread). Level data itself does place a red (owner 1) shaman Thing (`#38 model 7 kind 1
    owner 1 at (2304,7424)`, alongside blue's `#35 owner 0`) - `site::sites_from_level` takes any owner with a
    placed shaman Thing with no regard for which tribes the header's own byte 88 (`tribes` field, read as `2`
    for this level, i.e. a 1v1 code) actually selects, so if that byte means a specific pair of slots rather
    than "first 2 owners found", we may be including/excluding the wrong one. AI scripts (blue/red/yellow/green)
    read `[0, 10, 4, 5]` for this level - 3 non-zero, more than a "1v1" would suggest are active. Not root-caused
    this session; needs byte 88 actually decoded and cross-checked against which owners the original instantiates.
    **The reincarnation-site part of this is now explained and implemented, not a bug**: red's script here is
    010, and cpscr010's own turn-0 setup opens with `DO SET_REINCARNATION OFF` (ai-scripts.md "Worked example:
    level 1's opening flyby") - `game_core::ai_script` now runs that for real (see "Computer players (AI)"), so
    red having no site on level 1 is the intended, scripted behaviour. The byte 88/tribe-selection question above
    is separate and still open.

### Level scripting (triggers)
- [ ] Trigger semantics (layout and types known, level-format.md): what each type waits for (proximity, timed, player death, shaman proximity, library, shaman + angel of death), how `TriggerCount`, `NumOccurences`, `PrayTime` and `InactiveTime` play, and what activating a target does (discovery granted, effect fired, hidden thing revealed).
- [ ] Praying totems, stone heads (scenery 9), vault of knowledge: which trigger/discovery each uses, once or repeatedly.
- [ ] Level 5: a countdown (about 15 min) starts at some point; the winged death totem must be prayed at before it reaches 0. In the level: trigger #98, type 5 (shaman only, brings an Angel of Death), on cell 83, 65 (the start cell, marker 11), `PrayTime` 10, `CreatePlayerOwned` 1, targets effect 91 (same cell) and effect 88 (cell 106, 116). The timer is not in the level things: probably red's script cpscr058 (`SET_TIMER_GOING`, `HAS_TIMER_REACHED_ZERO`; the spec says level 5's script ends the level as won). Decompile it (`pop3-ai-script` above) to learn when the timer starts, what reaching 0 does and how the totem stops it; then implement its reward (not done yet, unlike the other totems).
- [ ] Implement the triggers and discoveries as deterministic simulation (through `Command`/state, no floats); AI scripts can fire them too (`TRIGGER_THING`).

### Computer players (AI)
- [x] A first native engine for the computer-controlled tribes (`game_core::ai`, pop3-rev-analysis.md
  "Native AI: how a computer-controlled tribe actually decides what to do"): `GameMap::tend_native_ai`
  runs, per computer tribe (one `NativeAi` per tribe `LevelHeader::ai_scripts` assigns a script to,
  same set `tribe_scripts` tracks), a priority sweep every 64 ticks (`SWEEP_EVERY`, phased by owner) —
  construct an essential building (drum tower first, then a hut once housing is nearly full, then
  warrior/temple/firewarrior training by population), train people towards the script's
  `INT_ATTR_PREF_*_PEOPLE` shares, then post a spare defender up to a quota — the first that can act,
  acts, matching "position 0 is always construct buildings". Every decision goes through `GameMap`'s
  own public order/builder helpers (`place_building`, `build_orders`, `send_into`, `set_patrol`,
  `apply`) exactly like a human player's input, which is also the "easily switched" seam: a different
  engine only has to produce the same kind of calls. `DO ATTACK` (`ai::AttackGoal`, `GameMap::ai_attack`)
  is the one task the sweep never enqueues on its own, confirming the spec's "native AI never decides
  to attack"; it launches the chosen fighters straight at their target instead of the original's own
  muster-at-the-drum-tower-then-march sequence (simplified, not traced). Deliberately not modelled at
  all yet: garrison and vehicle tasks, scripted patrol groups and one-off guards
  (`SET_MARKER_ENTRY`/`GUARD_AT_MARKER`/... - specified, ai-scripts.md "Markers, guards and the base":
  `GameMap::markers` now threads the level header's `Markers[256]` table in (`docs/plans/ai-script-markers.md`),
  but nothing here reads it for a standing guard/patrol task yet, plus an own-guess movement order since the
  exe's own one wasn't found), and the real worker-selection/site-search tie-breaking rules (ours: a bounded
  random site search, idle braves in id order). `DO ATTACK`'s own `ATTACK_MARKER` case is implemented
  (`ai::AttackGoal::Marker`, below). See the next few items for what reaches it from a script.
- [x] Shaman auto-cast, `ATTACK`'s damage-then-withdraw threshold, and a reactive counter-attack
  (`game_core::ai`, previously all three "deliberately not modelled" above): `SET_SPELL_ENTRY`'s own
  slots (`NativeAi::spell_entries`, `GameMap::ai_set_spell_entry`) are checked every `REACTIVE_EVERY` (8
  ticks, our own guess at "fairly responsive") by `tend_ai_spellcasting`/`ai_cast_if_ready` - she casts
  the first slot whose `min_people` enemies are within the spell's own `cast_range` of her and whose
  `base` flag matches whether she is near the tribe's own base (`AI_BASE_RADIUS_GUESS`, a guessed
  stand-in radius for the still-unmodelled `SET_BASE_MARKER`/`SET_BASE_RADIUS`), reusing the same
  `Command::Cast` a human player's click issues (`Spell::at_point`, shared with the client's own
  `hud::spells::ground_spell`). `mana`'s own argument is read and discarded: the book's own cost/charges
  already gate casting, and the wiki never says it is a separate threshold. `ATTACK`'s `damage` parameter
  (0..999, "how much damage to deal before withdrawing") now tracks a `Sortie` per launched force
  (`ai_attack`, `GameMap::ai_record_sortie_damage` hooked into `WorldEvent::Damage`'s own handling,
  `tend_ai_sorties` calling survivors home with a plain `Order::MoveTo` once the threshold is dealt or
  the target is dead) - **only for a `Shaman` goal**: a `Building` goal's own `WorldEvent::Wear` carries
  no attacker at all (`event.rs`), so there is nothing to attribute a building sortie's damage to yet,
  and it still fights on regardless of `damage`. The reactive counter-attack itself is
  `ai_defend_base`/`tend_ai_defense` (`STATE_DEFEND`/`STATE_DEFEND_BASE`): on the same `REACTIVE_EVERY`
  rhythm, pulls one idle fighter from anywhere in the tribe to the nearest intruder within the whole
  guessed base radius - distinct from `spotting.rs`'s own always-on per-unit auto-engage, which only
  looks a handful of cells around a unit already idle. None of this is traced against the exe beyond
  what ai-scripts.md already gives; every guessed number says so in its own doc comment.
- [x] The level header's `Markers[256]` table is now threaded into `game-core` (`GameMap::markers`,
  `marker_cell`/`marker_pos`, `docs/plans/ai-script-markers.md`), and `GET_HEIGHT_AT_POS`/
  `COUNT_PEOPLE_IN_MARKER`/`IS_SHAMAN_IN_AREA` and `ATTACK`'s own `MARKER` target case (`ai::AttackGoal::Marker`)
  are implemented on top of it (`ai_script.rs`'s `do_call`). This closes the 2026-11-02 report that level 1
  (`levl2001.dat`, red = `cpscr010`) attacked the human in play: red's own two scripted `DO ATTACK`s are gated
  on a `GET_HEIGHT_AT_POS` land-connection check (ai-scripts.md "Worked example: level 1's opening flyby") that
  can now actually run (regression test: `ai_script.rs`'s
  `a_level_1_shaped_script_withholds_its_raid_until_its_marker_cluster_is_land_bridged`).
  **Still open, and likely the actual source of what was originally observed**: `ONLY_STAND_AT_MARKERS`/
  `SET_MARKER_ENTRY`/`MARKER_ENTRIES`/`GUARD_AT_MARKER`/`GUARD_BETWEEN_MARKERS`/`CLEAR_GUARDING_FROM` (specified,
  ai-scripts.md "Markers, guards and the base", traced from the exe) are still not modelled - nothing confines
  red's own people to their assigned markers yet, and `ai_defend_base`'s own guessed base radius still has no
  marker-confinement to stay narrow against - so a human who gets within that radius, or near a red unit, can
  still draw it into a fight before the script would allow on levels that rely on this. Two pieces of the
  guard/patrol mechanic are genuinely open even once someone picks this up, not just unwired: the actual
  walk/circle movement order for a gathered group was never found in the exe trace (implement a reasonable guess
  - e.g. reuse our own patrol-point mechanism, units.md "Patrol orders" - and flag it as such, don't block on
  finding the original's exact one), and whether a guard group ever gets replenished after losses is unconfirmed
  (current best reading: no, so don't auto-replace).
- [ ] Shaman positioning and a drum-tower rally point (live-observed on level 25: red's script builds a
  drum tower at its `SET_DRUM_TOWER_POS`, the shaman goes inside it, and followers gather around it;
  when it is destroyed the shaman waits beside it while braves repair it, instead of the tower staying
  wrecked or her going elsewhere). None of this is modelled yet:
  - `PUT_PERSON_IN_DT model x z` (ai-scripts.md "People, buildings and orders") is a recognized `DO`
    command but still falls through `ScriptEnv::do_call`'s catch-all no-op (`ai_script.rs`) - wiring it
    needs the native engine to treat the shaman as a unit it can order into a building at all, which it
    currently never does (she is only ever a spellcaster, `ai_cast_if_ready`, or an attack target,
    `ai_attack_target` - nothing issues her a move/enter order).
  - The muster-then-march sequence `ai_attack`'s own doc already flags as simplified away ("the chosen
    fighters set off for their target right away instead of gathering at the drum tower first"): the
    original stages the attack force at the reincarnation site or `SET_DRUM_TOWER_POS`'s own spot first
    (ai-scripts.md "Attacks and spells", `INT_ATTR_DONT_GROUP_AT_DT` to skip it - not read anywhere).
    This would also be the natural rally point idle followers default to standing at, distinct from
    `ai_post_defender`'s own single stationary guard spot.
  - Automatic repair for a computer tribe: `wreck.rs`'s wear-down already reverts a worn building to
    `Stage::UnderConstruction` on a `REPAIR_DELAY` once it loses a piece of wood, but once a building is
    `Built` its workers are released (`work.rs`'s `empty_building`) and the native AI's own
    `ai_next_building` only ever picks a kind the tribe doesn't `has()` yet - a worn-but-still-standing
    building of a kind it already owns (the drum tower included) is never re-noticed, so nothing sends
    braves back to it on its own. Needs `ai_next_building` (or a sibling check) to also catch "own a
    built one of this kind, but it needs repairing" and reuse the same `build_orders` assignment
    `ai_construct_building` already does for a fresh plan.
  - Whether the shaman specifically stands guard beside a building under repair (vs. following whatever
    general "defend base"/idle behaviour already puts her there): a shaman-SPECIFIC "stand guard" person
    state does exist (state 39, `pop3-rev/docs/person_states.tsv`), but it's confirmed gated behind a
    global debug/option flag (`0x89D17C` bit 2) that normal play doesn't set - without it she falls back
    to her ordinary per-type default like anyone else, same mechanism everyone's idle fallback uses. So the
    person-state machine itself has no shaman-specific "guard a repair site" behaviour active in a real
    game; what's actually seen in play is more likely the generic AI positioning logic the item's own
    "defend base" guess points at, not traced further this session - that's native-AI/ai-scripts.md
    territory (DEFEND/guard logic), a separate and substantially bigger investigation than the person
    states checked here.
- [x] `pop3-ai-script` (its own crate, not a `pop3-format` module): parses `cpscrNNN.dat` (12 552 B
  only) into fields and a statement tree per [ai-scripts.md](docs/specs/ai-scripts.md) (grammar,
  token, parameter and internal variable tables are there), folds a recognized
  `FLYBY_CREATE_NEW..=FLYBY_START` run into one `Stmt::Flyby` (`flyby.rs`), and runs the tree's
  control flow (`IF`/`EVERY`/`SET`/`INCREMENT`/`DECREMENT`/`MULTIPLY`/`DIVIDE`) through an
  `interp::Env` trait that every `DO` command is handed to undecided; `cpatr` name and masks
  (`cpatr.rs`). Still open:
  - a decompiled-listing pretty-printer, so `just level-info` can print the level's tribes'
    scripts decompiled next to its things, triggers and markers.
  - `cpscr081`/`cpscr099`'s smaller layouts, if ever needed (no active level slot uses them).
  - the actual AI: a growing but still partial set of `DO` commands reach real behaviour through
    `game_core::ai_script::ScriptEnv::do_call` now (see the next two items); most still don't (markers,
    guard groups, vehicles, messages/camera/tutorial UI).
  - `Interpreter::resolve_call` now resolves every `Field` a `DO` call carries (including one nested
    in a `Team`/`TeamWild` argument) to a plain `Field::Const` before `do_call` ever sees it - a
    consumer's `Env` impl doesn't own the interpreter's user-variable storage, so it had no way to
    read a `$variable`-valued argument itself before this. **A query command's own `var_field` output
    is wired now too**: `Env::do_call` returns `Option<i32>` instead of `()`, and `Interpreter::exec_stmt`
    writes a `Some` straight into the call's own last `var_field` argument itself (the interpreter
    already knows which one that is, from the resolved call) - replacing the old `Env::write_user_var`
    stub, which no `Env` impl ever actually had a way to reach the interpreter's own storage through.
    `I_HAVE_ONE_SHOT` (`SPELL_TYPE` form only; the `BUILDING_TYPE` form's meaning is still unknown),
    `IS_SHAMAN_AVAILABLE_FOR_ATTACK`, `COUNT_ANGELS` and `IS_BUILDING_NEAR` now answer for real
    (`ai_script.rs`). Still no-ops: every query that needs the marker table
    (`COUNT_PEOPLE_IN_MARKER`, `GET_HEIGHT_AT_POS`, `IS_SHAMAN_IN_AREA`, `NAV_CHECK`), `GET_SPELLS_CAST`/
    `GET_NUM_ONE_OFF_SPELLS` (no cumulative-casts counter exists yet), `SET_ATTACK_VARIABLE` (the wiki
    itself only guesses at its meaning), `PARTIAL_BUILDING_COUNT` (toggles a count mode for
    `INT_x_BUILDING_*` internal variables we don't model at all yet) and the many UI/human-watching
    queries (`IS_PRISON_ON_LEVEL`'s own single use looked too uncertain to guess at - it has no
    `var_field` in its own spec table at all, unlike every other query).
- [x] First real `DO` commands, end to end: `game_core::ai_script` (`load_tribe_scripts`,
  `ScriptEnv`, `GameMap::tend_ai_scripts`) loads each computer tribe's `cpscrNNN.dat`
  (`LevelHeader::ai_scripts`) in `GameMap::load_original`, runs its "turn 0" setup block once right
  there, then once more per later `tick()` (the `ELSE { EVERY ... }` branch). Real so far: `SET_REINCARNATION
  ON/OFF` (`GameMap::remove_reincarnation`/`restore_reincarnation`,
  `WorldEvent::RemoveSite`/`RestoreSite`, for the script's own owner), `SET_NO_BLUE_REINC` (the
  human/blue equivalent, no on/off argument, always targets owner 0 regardless of which computer
  tribe's script calls it, to the same `RemoveSite`), every `STATE_*` toggle (`crate::ai::state::bit_for`
  -> `NativeAi`'s own bitset), `SET_DRUM_TOWER_POS`/`BUILD_DRUM_TOWER` (places/tops up a drum tower plan
  right away, `GameMap::ai_build_at`) and `DELAY_MAIN_DRUM_TOWER`/`BUILD_MAIN_DRUM_TOWER` (gates the
  native engine's own automatic one), `TRAIN_PEOPLE_NOW` (`GameMap::ai_train_now`), `ATTACK`
  (`GameMap::ai_attack`, `ai::AttackGoal` - `ATTACK_PERSON`/`ATTACK_BUILDING` only, `ATTACK_MARKER`
  needs the marker table; its own `damage` parameter now reaches `ai_attack` too, see above) and
  `SET_SPELL_ENTRY` (`GameMap::ai_set_spell_entry`, see above). Also real now, the level-event gifts
  (`GIVE_MANA_TO_PLAYER` -> `ManaPool::add`; `GIVE_ONE_SHOT` -> `worship::one_more_shot`, the same
  helper a totem's own one-shot reward uses; `GIVE_PLAYER_SPELL` -> `SpellBook::set(.., Known)`;
  `REMOVE_PLAYER_THING` -> hides a spell or a buildable kind, whichever `model` resolves to), the level
  outcome (`TRIGGER_LEVEL_WON`/`TRIGGER_LEVEL_LOST` -> `GameMap::declare_outcome` with
  `Reason::Scripted`, blue winning or the script's own tribe winning respectively - our own reading of
  "ends the level", not traced) and the timer trio (`SET_TIMER_GOING`/`REMOVE_TIMER`/
  `HAS_TIMER_REACHED_ZERO` -> `NativeAi::timer_end`; the UI countdown itself still isn't drawn anywhere).
  Every other `DO` command is still a no-op (see the query ones two items up). `ScriptEnv::int_var` backs
  `INT_GAME_TURN`, `INT_MY_NUM_PEOPLE` and every `INT_ATTR_*` (round-tripped through `NativeAi`); any
  other internal variable reads back as its own numeric token id rather than 0 - a deliberate, safe
  default for the many purely identifier-like ones (`INT_WARRIOR`, `INT_TEMPLE`,
  `INT_NO_SPECIFIC_BUILDING`...), which is how `do_call` recovers "which model/unit kind was named"
  from an already-resolved `Field::Const` (see `resolve_call` above).
- [x] `pop3_format::constants`: decode `levels/constant.dat` (XOR key, 2-byte marker, `P3CONST_` lines) per
  [constants.md](docs/specs/constants.md) into a plain `name -> value` map (`Constants::parse`/`load`,
  `[ours]`-tagged guess at which of the two forms — encoded or the community's plain text — a given file is
  in, since the real loader's own test is unconfirmed; a malformed line is skipped, not fatal). Wired through
  for spells only so far (`game_core::spell_book::SpellStats::from_constants`/`SpellBook::apply_constants`,
  `GameMap::load_original`): cost, charge count, advisor pacing and range, with our own defaults
  (`SpellStats::default_for`) when an install isn't present or its file doesn't parse.
- [ ] The rest of `constant.dat` (units speed/damage, training costs, wood, hut breeding) still uses its
  own hardcoded placeholders across the codebase, not `pop3_format::constants::Constants` — reconcile one
  group at a time the same way the spell group was (`cost`'s own split stays: our placeholder scale by
  default, real numbers only from a loaded file). `CONV_SPY`'s own duplicate entry (unit group, then building
  group) is read "by position" per constants.md, unconfirmed and irrelevant to every group read so far:
  check if any future group actually needs position-sensitive duplicate handling before adding it.
  **Units' own `life` sub-group is now done** (`balance.rs`'s new `Units`/`Balance::apply_constants`,
  `kind_stats.rs`'s own `KindStats::life` stays the compiled-in default that `Unit::new`/`change_owner`
  reset to; the live simulation corrects it against `LIFE_<P>` right after, in `GameMap::spawn`/
  `with_shamans`/`change_owner`, the only three places a unit's health is ever set from its kind) - since
  our defaults already are the traced real numbers (`kind_stats.rs`), this only matters for a modded
  install with different ones. **`LIFE_<P>`'s own per-kind suffixes are `[ours]`, a guess** (constants.md
  only confirms the group and two example values, not every kind's exact spelling) - worth checking
  against a real decoded file or the community's own dump; a wrong guess is harmless (that kind's default
  just stays, same lenient fallback as every other group here). `speed`/`damage` next would follow the
  same shape, but need their own per-kind suffixes confirmed too.
- [x] Lockstep: two peers resolving different `SpellStats` would silently diverge mid-match - a hashing
  mechanism now exists (`SpellBook::constants_hash`, every spell's resolved `cost`/`max_charges`, the two
  fields that actually feed simulation state; `advisor_seconds`/`range` left out, neither can desync anything)
  and a wire message to carry it (`game_net::Message::Handshake { constants_hash }`, tag 3,
  `constants_match` helper, multiplayer.md "Wire format"; spells.md "Mana"). **Correction**: the TODO's own
  "like level/version matching presumably already is" was wrong - `game-net` had no precondition check of any
  kind before this (checked `Message`'s then-3 variants directly). Still open: nothing calls this yet - there
  is no host/join flow or network driver at all (see "Network driver" below), so a real mismatch is not
  actually refused anywhere today, only encodable/decodable and comparable.
- [x] Script interpreter in `game-core`, first pass: user variables (`pop3_ai_script::Interpreter`,
  pre-existing), `INT_MY_NUM_PEOPLE`/`INT_ATTR_*` read from the simulation/`NativeAi` (not the fuller
  `headcount`/`housing` set yet - just what `ai.rs`'s own sweep needed), `EVERY n m` firing when
  `(turn + tribe + m) % n == 0` (`pop3_ai_script::interp::every_fires`/`run_turn`, now including the
  real per-tribe phase, confirmed against the exe and threaded in from `GameMap::tend_ai_scripts` -
  a different clock from the native engine's own `SWEEP_EVERY` phase above), DO commands mapped to AI
  states and `Command`s for `STATE_*` and `ATTACK`
  (previous item). Still open: `SET_SPELL_ENTRY` (shaman auto-cast, not modelled at all), the rest of
  `headcount`/`housing` as internal variables, and whether levels 2100/2110/2131's missing scripts
  (fall back to no script, unverified this falls back cleanly end to end) still need a check.
- [x] Script timing: the original runs 12 turns/s, fixed 83 ms step [exe, measured] (pop3-rev-analysis.md
  "Reference" / "Turns and timing", not the wiki's "about 8"), matching our own one-tick-per-turn at
  `TICKS_PER_SECOND = 12` (units.md, `game_core::time`) - no rescaling needed. `EVERY`'s per-tribe phase is
  confirmed and implemented (see the interpreter item above); message timeouts and flyby times are turns too,
  only `SET_TIMER_GOING` takes seconds (ai-scripts.md "Turn rate").
- [x] Flybys: `FLYBY_CREATE_NEW..=FLYBY_START` drives the camera for real now
  (`game_core::ai_script::ScriptEnv::on_flyby` -> `GameMap::active_flyby`, `crates/game-client/src/flyby.rs`
  reads it to pose `CameraRig` and show the "Flyby"/"Cancelled flyby" banner; Space sends
  `Command::CancelFlyby` when `FLYBY_ALLOW_INTERRUPT` is on, ai-scripts.md "Flybys"). Finishing on its own
  holds at the last keyframe; cancelling eases the camera (over `FLYBY_CANCEL_GRACE_TICKS`) to the same
  place (`end_target` if the script set one, else each channel's own last keyframe) instead of back to
  where the player was before it started. Not done: `FLYBY_SET_EVENT_TOOLTIP` popups (parsed and resolved,
  nothing draws them); the exact keyframe easing/overlap and the zoom -100..100 -> camera distance scale
  are our own guesses, unspecified in the files.
  - [x] **Confirmed live this design is right** (pop3-rev-analysis.md "Flyby cancel"): two captures of level 1's
    flyby, one interrupted mid-flight and one left to finish on its own, settle on the identical camera
    orientation - cancelling really does skip to the flyby's own end rather than reverting. Fixed:
    `FLYBY_CANCEL_GRACE_TICKS` was `Ticks::secs(1)`, now `Ticks::secs(3)` matching the measured original
    ease (only one interrupt point measured, not cross-checked at a different point in the flyby).
- [ ] Camera flyby state: continue the pop3-rev-analysis.md "Camera flyby state (tribe_struct)" investigation.
  All three primary AI-script channels are now traced end to end (pop3-rev/docs/camera.md, pop3-rev/
  docs/functions.tsv):
  - `POS` drives `tribe_struct+0x24` directly: `x`/`z` are raw 0-255 bytes, world units = `(cell+1)*256`
    (256 render units/cell, matches "Landscape projection"), with explicit map-wraparound handling on the
    delta.
  - [x] **Confirmed bug, found via a user's screenshot comparison, now fixed**: the raw byte is at HALF-cell
    resolution over our own 128-cell map (`actual cell = raw / 2`), not a literal cell index - evidence: all
    11 x/z values in level 1's own script (`POS`/`TOOLTIP`/`END_TARGET`) are even (expected if the designer
    placed waypoints on whole cells, doubled by the compiler); `FLYBY_SET_END_TARGET 20 216` halved is cell
    `(10, 108)`, 1.5 cells from this level's own known site position `(8.5, 107.5)`
    (`crates/game-client/src/sites.rs`'s `site_cell_pos`) - matching a screenshot of the camera parked on the
    stone circle/shaman, where the raw byte taken 1:1 (the old code) lands at `(20, 88)`, 20+ cells off over
    the sea. Fixed: `flyby.rs` now has a `half_cell` helper (`raw as f32 * 0.5`) used by `drive_camera`'s
    `POS` keyframes and `final_pose`'s `end_target`/last-keyframe fallback, before the existing
    `rem_euclid(MAP_SIZE)` wraparound. `TOOLTIP` keyframes need the same halving whenever they're implemented
    (currently parsed/resolved but nothing draws them, above). See pop3-rev-analysis.md "The AI-script POS
    and ANGLE channels" and pop3-rev `docs/camera.md` "Correction: POS/TOOLTIP/end-target raw x/z are
    HALF-cell units".
  - `ANGLE` drives `tribe_struct+0x32` directly (the same field the manual mouse-drag rotate code uses) -
    confirms the documented "shortest way" rule is real but has a per-event override flag, not unconditional.
  - [x] **D3D axis/handedness caveat for yaw - resolved, no code change needed**: re-analysing two already-
    captured full-flyby traces (pop3-rev `docs/camera.md`'s `cam5.txt`/`cam7.txt`) against the script's own
    6 `ANGLE` keyframes confirms `yaw_true = -raw * 360/2048` (mod 360) - a pure sign flip, zero offset -
    to within ~0.5-4° on 4 independent keyframes in both captures. That's exactly what `flyby.rs`'s existing
    `render_yaw` (-> `nature::angle_yaw(raw).render()` -> `GameYaw::render()`'s documented left-handed/
    right-handed mirror) already computes - this was an existing, independently-motivated convention, not
    something added to fit this data. See pop3-rev-analysis.md "D3D axis caveat" and pop3-rev `docs/
    camera.md` "D3D axis caveat for YAW - RESOLVED".
    - [x] Regression test added (`flyby.rs`'s `render_yaw_is_calibrated_against_real_flyby_traces`) so the
      mirror negation in `render_yaw` can't be "simplified away" later with nothing to catch it: asserts
      `render_yaw(raw)` within 5° (mod 360) of `(0,0°) (681,240.29°) (744,229.22°) (1438,107.23°)`.
  - `ZOOM` does NOT change camera distance at all: it blends between two named camera PRESETS' render
    parameters (bend/K and a few other render-block fields), not a continuous world-unit zoom.
    `flyby.rs`'s `zoom_to_distance` (-100..100 linear onto `DISTANCE_RANGE`) has the wrong shape entirely,
    not just a wrong scale - read pop3-rev-analysis.md "The AI-script zoom channel" before changing it.
    **Confirmed, a later session, with live round-trip data (not just the static trace above)**: manual
    zoom (`+`/`-`, normal play) uses the same architecture and was caught live moving `K`/pitch together
    while `D` stayed exactly constant across the ENTIRE zoom range, from idle out to the literal
    whole-planet globe view (pop3-rev `docs/camera.md` "Manual zoom is a pitch+bend orbit"). This is
    almost certainly why our flyby camera's elevation looks wrong ("almost sky-view") next to the
    original: `zoom_to_distance` drives `CameraRig.distance` out to `DISTANCE_RANGE.1` (220 cells, ~8.7x
    the real idle 25.4 cells) while pitch stays frozen at the shallow idle angle (`drive_camera` never
    touches `rig.pitch`, see below) - the real camera instead stays at a ~constant radius and orbits
    toward vertical. Fixing this properly means reworking `eye_offset`'s zoom model (pitch-driven, not
    distance-driven) for both the flyby and ordinary manual zoom, not just retuning `DISTANCE_RANGE`.
  - [x] **Confirmed, calibrated, and FIXED in code**: script zoom value 0 means "idle" (~20.4° pitch,
    `camera::GROUND_PITCH_DEG`), not the arithmetic midpoint of `PITCH_RANGE` (~49.5°) the old `zoom_to_pitch(0)`
    returned. Re-running the yaw check's flat-hold-detection pipeline against `cam5.txt`/`cam7.txt`'s matrix
    PITCH instead, matched to level 1's 4 `ZOOM` keyframes, gave real (zoom, pitch) pairs, tight across both
    captures: `(-100, -55.37°) (10, -19.51°) (80, -12.70°) (0, -20.56°)` - vs. the old formula's
    `(-100, 88.60°) (10, 45.64°) (80, 18.31°) (0, 49.55°)`, wrong by 5.6-33.2° on EVERY point. `PITCH_RANGE`
    (10.5°-88.6°) is real but is the range reached by physically holding the manual zoom key to the
    whole-planet globe view - a different input than the script's bounded `-100..100`; level 1's own script
    never gets anywhere near those endpoints. `zoom_to_pitch` now uses a piecewise-linear fit through these
    points (`KNOTS` in `flyby.rs`, zoom 80-100 extrapolated - unconfirmed, no script here exercises past 80;
    pop3-rev `docs/camera.md` item 11 found and decoded the real preset data source this would need, `data/
    VCONFIG0.DAT`, no live capture required - not fully closed yet, still needs tracing what advances the
    "current preset" index during a real zoom/flyby before the 80-100 numbers can be read off directly),
    with a regression test (`zoom_is_calibrated_against_real_flyby_pitch_traces`) pinning it within 6° so a
    later cleanup can't flatten it back into a straight `PITCH_RANGE` span without a test catching it. See
    pop3-rev-analysis.md "The AI-script zoom channel" and pop3-rev `docs/camera.md` "`flyby.rs::zoom_to_pitch`
    calibrated against the same traces".
  - [x] **SOLVED, a later session - it's FOV, confirmed, and it moves with zoom**: a user also felt camera
    distance "should be ~10 cells" from a screenshot (distance and FOV are confounded in a still image,
    `1/(distance*tan(fov/2))`). `D` (`GROUND_DISTANCE`=25.4 cells) is one of the best-measured constants here
    (confirmed constant three independent ways: live manual-zoom round trip, static zoom-chain trace, and the
    `0x417510`/`CREATE_MSG_INFORMATION_ZOOM`/`ZOOM_TO`/`TRACK_TO_MARKER` alternate-mechanism lead checked
    against level 1's real `cpscr010.dat` via `ai_script_info` and RULED OUT - none of those opcodes appear
    anywhere in the script, only the one known `FLYBY_SET_EVENT_ZOOM`-driven flyby). `DEFAULT_FOV` (35°), by
    contrast, was never traced from the exe at all ("matched by eye") - confirmed the actual culprit, and it's
    not even a single wrong constant: **the per-axis screen-scale step "Landscape projection" used to leave open
    is now fully traced** (`focal_px = Q · 2^(S-16)`, isotropic - pop3-rev `docs/camera.md` "The per-axis screen
    scale SOLVED"), and **`Q` is itself one of the flyby `ZOOM` channel's blended fields**, live-confirmed
    swinging from ~40786 (idle) to ~71207 (level 1's own "zoomed in" extreme) - i.e. the original's "zoom"
    changes effective FOV on top of pitch+bend, a real telephoto effect, not just an orbit (pop3-rev
    `docs/camera.md` "Zoom changes effective FOV too").
    **Fixed in code**: `flyby.rs` now has a `zoom_to_fov` next to `zoom_to_pitch`, driven by the same script
    zoom value, both sharing one `piecewise_deg` helper. `CameraRig.fov` is the vertical FOV already
    (window-size-independent once in degrees), so no window dimensions are needed in our own code even though
    the original's own `focal_px = Q/32`, `FOV_y = 2*atan(half_height/focal_px)` derivation used its capture's
    640x480 window to get there. `FOV_KNOTS`: `(-100,21.6°) (0,21.3°) (80,12.3°) (100,10.05°)` - the last point
    extrapolates the 0..80 trace's own slope, same spirit as `PITCH_KNOTS`' own 80..100 guess (zoom 80-100 is
    unconstrained, no level script here exercises it). Regression tests:
    `zoom_is_calibrated_against_real_flyby_fov_traces` (pins the 3 real points within 1°) and
    `zooming_in_narrows_the_fov_not_widens_it`. `GROUND_DISTANCE`/`D` itself did NOT change.
    `origin_x`/`origin_y` (screen center) are also now traced (`render_block[+0x2a or +0x2c] + half_window`) -
    Y matches live exactly, X has an unexplained constant ~50px offset not yet resolved, doesn't affect FOV -
    not acted on in code, nothing in `flyby.rs` currently needs the screen origin.
    **Not yet checked**: whether MANUAL zoom (not just the flyby) also moves `Q` the same way - the existing
    "Manual zoom is a pitch+bend orbit" finding above only ever recorded `D`/`S`/`K`/the matrix, not `Q`.
    See pop3-rev `docs/camera.md` "The per-axis screen scale SOLVED" / "Zoom changes effective FOV too".
  - `INT_POINT` (look-at-a-point yaw channel, unused by level 1) and `TOOLTIP` (resolves to a generic
    advisor-notification system; its `idx=0` case walks to a "head"/monument scenery object - almost
    certainly the reported "a building/pyramid is presented" behaviour) were traced on both the creation and
    per-frame rendering side - none of the 5 flyby channels, nor the tooltip/notification pipeline, write
    `tribe_struct+0x2c`, the field a live capture shows actually moving smoothly during a flyby, matching the
    "camera looks closer/farther" impression. The "zoom while a popup is open" hypothesis is RULED OUT.
    Level 1's own `TOOLTIP` keyframe timing (a contiguous `[132,325]`-turn span, with `FLYBY_START` now
    exactly turn 72) still correlates with `+0x2c`'s motion far better than `ZOOM`'s does, but the cause is
    genuinely unknown - no code path found explains it (pop3-rev/docs/camera.md "Open questions" #6).
  - **`FLYBY_START` is now known exactly: turn 72**, read straight from the compiled `cpscr010.dat` script
    via `cargo run -p pop3-ai-script --example ai_script_info -- <path>` (no live memory needed) - solved,
    see pop3-rev-analysis.md "Camera flyby state (tribe_struct)". Worth double-checking `game_core::ai_script`
    reproduces this exact gate (`Every{period:8}` + `INT_GAME_TURN>70` + a one-shot user-var flag) rather than
    just hardcoding turn 72 for this one level.
  [x] The original's camera pitch is NOT fixed during a flyby or manual zoom (animates independently of yaw) and
  distance never moves at all - `flyby.rs`'s `zoom_to_pitch` (was `zoom_to_distance`) and `camera::adjust_view`
  (the player's own wheel/key zoom) now both drive `CameraRig.pitch` across the confirmed `PITCH_RANGE`
  (~10.5° zoomed in to ~88.6° at the whole-planet globe view) at constant distance, fixing the "almost sky-view"
  elevation the flyby used to end up at; the sky/space fade (`camera::space_fade`) now keys off pitch too, since
  distance no longer grows when zooming out. The exact script-zoom-value -> pitch-degrees curve still isn't
  solved (no formula, just a live-sampled table of points, pop3-rev `docs/camera.md`) - linear interpolation
  across the confirmed endpoints is still our own guess, same as before. Not done: the K/bend render-parameter
  blend itself (`terrain_mesh::CurveParams::curvature` stays a fixed constant, not yet tied to zoom) - the
  preset-table fields it blends aren't identified well enough to implement with confidence, and baking it in
  would mean rebuilding the terrain mesh far more often (see the mesh-rebuild-cost item below). Checked and
  ruled out as a lead: the manual mouse-drag pan/rotate code (`move_pos_angle_length` and friends,
  pop3-rev-analysis.md "Manual camera pan/rotate (mouse drag)") is separate from zoom entirely - don't re-chase
  it for this.
- [ ] **Stale, corrected**: `GIVE_ONE_SHOT`, `GIVE_MANA_TO_PLAYER` and `TRIGGER_LEVEL_WON/LOST` are now
  implemented (see "Computer players (AI)" above) - this bullet was outdated. Still genuinely open: messages
  and user input lock (next item).
- [ ] **Checked against a real level script (levl2007/cpscr021, level 7's red+yellow, ai-scripts.md "read IA
  script of level 7" session), found more unimplemented `DO`/query commands it actually uses that weren't yet
  tracked here individually.** All fall through `ScriptEnv::do_call`'s catch-all no-op (`ai_script.rs`); none
  found affects currently-passing tests, since nothing asserts on them:
  - No in-game message/tag queue exists at all: `CREATE_MSG_NARRATIVE`/`CREATE_MSG_INFORMATION`/
    `CREATE_MSG_INFORMATION_ZOOM`/`OPEN_DIALOG`/every `SET_MSG_*`/`CLEAR_ALL_MSG` (ai-scripts.md "Level events",
    "Messages"). cpscr021 alone queues 3 information messages this way. Needs a client-side queue/tag UI plus a
    `game-core` side (which message is "current", its delete-on-ok/timeout/auto-open flags) before any of these
    can do anything.
  - [x] User input lock: `DISABLE_USER_INPUTS`/`ENABLE_USER_INPUTS` (used by the tutorial and level 1's own
    opening flyby gate, ai-scripts.md "Worked example") - **implemented**. `GameMap::user_inputs_disabled`
    is a plain field `ai_script.rs`'s `do_call` sets/clears (same shape as `active_flyby`/
    `player_shaman_building`), and the three call sites' identical, copy-pasted three-condition chain
    (`camera_input` in `camera.rs`, `selection::select_and_order` + `look_at_shaman` in `units/mod.rs`, the
    tile-click/cast/aim-and-cast chain in `hud/spells.rs`) are folded into one shared run condition,
    `input_lock::input_locked` (`crates/game-client/src/input_lock.rs`), composing `armageddon::input_locked`,
    `flyby::flyby_active`, `sites::site_build_active` (the reincarnation-site "RC" build/rebuild ritual) and
    the new flag - referenced once per call site instead of a fourth `.run_if()` bolted onto each.
  - `SET_BUCKET_USAGE`/`SET_BUCKET_COUNT_FOR_SPELL` (ai-scripts.md "Attacks and spells"): the mana/spell-priority
    weighting mechanism cpscr021 uses every 90 turns to rescale all 16 non-hidden spells' priority by its own
    population band. Meaning still unconfirmed against the exe (wiki: "?").
  - [x] `GIVE_UP_AND_SULK`'s confirmed force-success switch (ai-scripts.md "Other ON/OFF switches" already traces
    what it does against the exe - a generic per-tribe flag, not a dedicated surrender routine): used by 44/58
    base scripts (61 call sites counting the Undiscovered Worlds expansion) as a last-resort "losing badly"
    branch. **Implemented**: `ai::NativeAi::give_up_and_sulk`, set by `ai_script.rs`'s `do_call`, read by
    `sermon::tend_sermons` to bypass the `MAX_POPULATION` cap on a successful preacher-conversion roll.
  - [ ] `GIVE_UP_AND_SULK`'s OTHER effect, `0x41CA10`'s own cleanup/punishment sweep (ai-scripts.md "Other ON/OFF
    switches", fully traced): on ON, for the tribe that gave up - force-disembark every non-shaman follower from
    any vehicle; cripple the tribe's own Angels of Death (life rerolled to a random 32-95 of 10 000); re-time
    (typically shorten) the hypnotism-revert countdown of units this tribe currently holds hypnotised; delete the
    tribe's own ghost units; reveal/revert the tribe's own disguised spies; tear down the tribe's own in-progress
    "soul convert 2" conversion coordinators. **The wiki's "everyone retreats to guard the shaman" claim is
    debunked** - confirmed absent from this function (no movement/order/retreat code anywhere in it) and also
    absent from every real script's own `GIVE_UP_AND_SULK` call site (none pair it with a `DO ATTACK`, so there's
    no scripted all-in-attack-on-giving-up baked in either) - do not implement a retreat or group-up-at-shaman
    behaviour for this command. **Implemented, 3 of 6 effects**: force-disembark (`GameMap::ai_force_off_vehicles`,
    vehicle.rs), Angel of Death crippling (`GameMap::ai_cripple_angels_of_death`, angel_of_death.rs), hypnotism
    re-roll (`GameMap::ai_shorten_hypnotism`, hypnotism.rs - exact formula `8 + rand(0..63)`, uniform in `[8, 71]`
    steps, confirmed and now in pop3-rev-analysis.md), wired together by `ai_set_give_up_and_sulk`'s own
    `ai_give_up_sweep` (ai.rs). **4 of 6 now**: ghost-unit deletion (`GameMap::ai_delete_ghosts`) tears down
    every one of the tribe's own Ghost Army copies through the same `destroy_ghost` any other ghost destruction
    goes through, now that the spell itself exists (docs/specs/spells.md "Ghost Army").
    - [x] Spy-disguise reveal: the tribe's own spies drop their disguise (`ai_give_up_sweep`, units.md "Spies").
    - [ ] "Soul convert 2" conversion-coordinator teardown: identified as aborting a tribe's own
      `Action::Listening` candidates (sermon.rs), the same abort idiom `tend_sermons` already uses elsewhere for
      a sermon that can no longer continue - held back for now, not yet implemented.
  - [x] `GIVE_UP_AND_SULK`'s own trigger condition, the `INT_*_KILLED_BY_*` family (ai-scripts.md "Internal
    variables", "Other ON/OFF switches"): every internal variable `ScriptEnv::int_var` didn't special-case read
    back as its own fallback numeric id, so `INT_MY_NUM_KILLED_BY_HUMAN > 10` was always false and
    `INT_MY_NUM_KILLED_BY_BLUE > 5` was always true - a real misfire, not just an inert gap. **Implemented**:
    `GameMap::kills[killer][victim]`, a per-tribe-pair running total kept current by `apply_event`'s
    `WorldEvent::Damage` handling, backs the whole 12-variable family (`ScriptEnv::kill_count`) plus the
    per-colour headcounts `INT_BLUE_PEOPLE`..`INT_GREEN_PEOPLE` (`ScriptEnv::color_headcount`), same unbacked bug.
  - `PREACH_AT_MARKER`/`CONVERT_AT_MARKER`/`SEND_ALL_PEOPLE_TO_MARKER` (ai-scripts.md "People, buildings and
    orders"): the same underlying gap as `PUT_PERSON_IN_DT` above (the native engine never issues the shaman,
    or a whole tribe, a move/order at all) - not individually called out there before.
  - `GET_HEAD_TRIGGER_COUNT`/`REMOVE_HEAD_AT_POS` (ai-scripts.md "Level events"): the query a script uses to
    notice a stone head has fired (cpscr021 gates a message on this) and the command to sink one. Related to,
    but more specific than, "Level scripting (triggers)"'s general trigger-semantics item above.
  - Seven `STATE_*` toggles are parsed and stored (`crate::ai::state::bit_for`) but never read by the native
    engine's own logic, unlike `CONSTRUCT_BUILDING`/`TRAIN_PEOPLE`/`DEFEND`/`DEFEND_BASE`/`AUTO_ATTACK`:
    `FETCH_WOOD`, `SHAMAN_GET_WILDS`, `HOUSE_A_PERSON`, `BRING_NEW_PEOPLE_BACK`, `POPULATE_DRUM_TOWER`, `PREACH`,
    `FIREWARRIOR_DEFEND` (ai-scripts.md "States" table now notes this inline). cpscr021 turns on all seven at
    setup.
  - `INT_ATTR_PREF_{SPY,RELIGIOUS,WARRIOR,FIREWARRIOR}_TRAINS` (as opposed to the already-consumed
    `..._PEOPLE` variants): `ai_next_building`'s own building-choice logic picks by population band only and
    never reads these, so a script's attempt to steer which training building gets built next (cpscr021's main
    mechanism for choosing between temple/warrior-train/firewarrior-train) has no effect (ai-scripts.md
    "Attributes" table now notes this inline).

## Priority 3 — remaining fidelity and content
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
  the level's sunlight block plays no role in it, matching the exe's `sunlight_init_default`. The two paths are
  mutually exclusive on the same bit (`0x895DA5 & 0x8`, checked this session): the day/night tick (`0x401350`,
  called every turn from the main loop) only runs its body when that bit is SET, the fixed-Inclination-64 path
  (`0x401920`) only when it's CLEAR - but `0x401920` also has **zero call sites anywhere in the exe**, and no
  instruction anywhere writes that bit with a literal address either (an indirect/pointer write wasn't ruled
  out, so not airtight) - so with the bit clear, as measured, NEITHER path can ever move the sun away from the
  flat 147 default: the day/night cycle reads as permanently dormant in the shipped game, not just off in the
  levels tried this session. How the original maps its sky (backdrop scrolling with the camera?) is a separate,
  still entirely untraced visual system - not related to this sun-angle code at all, needs its own session.
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
- [x] Audited (2026-10-30): the object bank the level header names (byte 97) was never a live content bug beyond
  what this item's own parenthetical already said. Every consumer but trees (`building_object`, `vault.rs`,
  `vehicles.rs`, `totems.rs`, `sites.rs`) resolves its model through a fixed, named bank-0 `pop3_format::catalog`
  constant, never a raw level-encoded index - so which bank a level's header declares makes no difference to
  anything it draws. Trees already translate correctly per bank (`tree_object_in_bank`). objects.md's own open
  "load bank N... or stay on bank 0" sentence is now resolved in writing in favour of staying on bank 0 (the
  "not verified against the game" caveat kept, not a settled trace) - see its "Banks" section. `tree_object_in_bank`
  got a doc-comment signpost for the two still-open raw-model lookups that will need the same convention (plants
  7/8, the building add-on, both separate TODO.md items below) so they don't reinvent one.
- [ ] Other original objects (trees, camp fire logs, reincarnation site stones): now cull their back faces like
  buildings and totems (`nature.rs`, `campfire.rs`, `sites.rs`, `cull_mode: Some(Face::Back)`, objects.md
  "Records"), matching the already-proven-correct pattern exactly. Still open, "checking each for holes" itself:
  no original install was available to screenshot-verify none of the three now shows a new see-through gap (a
  `facs` genuinely missing its back-to-back pair) - needs a `just shot`/`just video` pass at varied
  `YAW`/`PITCH`/`AERIAL` over a tree cluster, a camp fire and a reincarnation site with an install present.
- [ ] Level names instead of the header's "Level N": `pop3_format::language` reads the texts ([language.md](docs/specs/language.md)); find which text number names each level (open there), find the install's `language/` dir like `data/`, pick the language (a setting, English by default).
- [ ] Our own texts in Fluent (`assets/lang/<lang>.ftl`, named keys), a key -> original text number table, lookup from `langNN.dat` with the original files and from our `.ftl` otherwise ([language.md](docs/specs/language.md) "Our texts and the fallback"); when the game first shows translatable text.
- [ ] Sunlight block (ShadeStart 28, ShadeRange 15, Inclination 32/64): find whether it drives the terrain lighting. `LandBlocks` / `LandOrients` are identical stale data: ignore them.

## Priority 4 — presentation and UX polish
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
  far), nor was whoever draws the stacking icon itself from the record it writes. The preach notice's own trigger
  still needs tracing. The **tower alert is now traced** ("Watchtower sentry scan and alarm", 0x40BD20): an
  occupied-by-warrior tower scans a radius (per-occupant `<TYPE>_DT_RADIUS` plus a height bonus) for one visible
  hostile enemy and, on finding one, rallies nearby own units/buildings into an attack order rather than playing
  a sound directly - that order's handler is what sets `0x89D167`, the same byte already identified as a general
  "you're in combat" signal, confirming it as the tower alert's real trigger. The scan is not an unconditional
  per-turn check, but it IS triggered the same way in every game mode: by the player's own **B** keybind
  (shortcuts.md), queued and drained like any other player command regardless of singleplayer/multiplayer - a
  level-script opcode can also fire it but is used in only one level total, so it is not the general mechanism.
  Still open: what UI code reads `0x89D167` to actually play the sound, and where the scan's building-type
  argument (always "tower" in practice) gets set, since the keypress itself doesn't supply it. The original also
  has a "dismiss/clear all notifications" command pair (pop3-rev-analysis.md "Dismiss/clear all notifications")
  - worth exposing as a real action once this system is built, though its own keybind/button wasn't found.
- [ ] Building icons, tooltips (spell icons done: `assets/spell-hud`).
- [x] Spies: hovering another tribe's spy passing as the player's own shows the reveal cursor (`CursorLook::RevealSpy`,
  our own punch badge; the original's sprite not identified), a click unmasks it (units.md "Spies").
- [ ] Original keymap ([shortcuts.md](docs/specs/shortcuts.md), dev mode off): add the missing shortcuts (F1, S status, groups 1-6, camera points Z/X/C/V, `\\` track, Shift+Enter, Space skips the fly-by), and a way to stop and cast self spells without the dev keys.
- [ ] Debug overlay (`F4`, `crates/game-client/src/debug_overlay.rs`): reported bug, not yet found - "invisible overlay across the globe". A read-only audit of `draw()` itself this session found nothing wrong (every unit-to-cell conversion and every `torus_delta_cells` usage checked out), so the cause is elsewhere; next suspects, not yet investigated: `crates/game-client/src/grounded.rs`'s `ground_point()`, `CurrentMap`/map-loading staleness, or a `rig.focus`/`CurveParamsRes` mesh-to-cell scale mismatch. Pre-existing and unrelated to the patrol-points work above (Units, "Order model") - check this independently before/alongside adding the patrol visualization there, since that addition reuses `draw()`'s existing helpers and would inherit the same bug if it's actually in one of them after all.

### Tooltips ([tooltips.md](docs/specs/tooltips.md))
- [ ] Rendered tooltip: done for buildings, trees and totems (`tooltip.rs`: name, slot rows, anchored above, sticky, clicks, right click at once); the toggle to add.
- [ ] People rows filled: totems' counted prayers (once praying exists), the trainee. Vehicles and vehicle huts are done. Buildings, pyramids and totems' places are done, a click adds the unit to the selection.
- [x] Wood rows: the pieces still needed on a growing house: **already done, no special case needed** - growth
  exists now (`hut_growth.rs`) and just leaves the hut in `Stage::UnderConstruction` like any other site, so
  `occupancy::building_wood`'s existing generic case already shows it as provided/cost, confirmed by a new test
  (`occupancy::tests::a_growing_house_shows_the_ordinary_under_construction_wood_row_not_its_old_wood_plus_new`).
  tooltips.md's older "the wood it has plus the pieces still needed" row was a guess from before growth's own
  mechanism was exe-confirmed (huts-and-training.md rule 6: reseeded to 1 piece of the new cost, not old + new) -
  corrected there to match. Plans, sites, built buildings and trees are done.
- [x] Hut tooltip bars: a hut's breeding progress (`Building::breed`, `BarKind::Birth`, green, vertical) and
  growing progress (`Building::grow`, `BarKind::Growth`, red, vertical - huts-and-training.md rules 5-6) are
  wired into `building_model` (tooltip.rs) now, the way `BarKind::Training` already was - fill against the
  live target (`GameMap::hut_breed_target`/`hut_grow_target`, new in `hut_growth.rs`: the breed target moves
  with the tribe's own population band, the grow target is `Huts::grow_target`; both `None` before the hut
  is `Stage::Built`, same gate `tend_huts` itself uses, and `hut_grow_target` is also `None` for an
  already-large hut with no next size). The widget, its blinking, the pyramid's prayer bar, totem prayer
  progress and the training bar (double fill from `GameMap::production`) were already done.
- [x] Editor: brushes now follow the mouse (`grounded::pick_ground`, mirroring `blueprint.rs`'s own
  cursor-ray-pick, falling back to the camera focus when the cursor is off the terrain or over the panel);
  a configurable brush radius (`EditorState.brush_radius`, `[`/`]`, 1..=12, default 4) shown as a ring on the
  ground and on the info line, R/F/T now calling `Heightmap::raise`/`flatten` directly instead of through
  `Spell::cast`'s own hardcoded radius; object placement scoped to trees for v1 (O toggles, left click plants
  one via the new `Command::PlaceTree`/`GameMap::place_tree`, right click/Esc cancels) - see
  [ui-and-editor.md](docs/specs/ui-and-editor.md) "World editor". `EDITOR_BRUSH=x,z:radius` dev var for a
  headless check, confirmed with a screenshot (the ring draped correctly over sloped ground around the
  player's reincarnation site). Left for a follow-up: a second placeable kind (camp fires, already a
  `Command`, per the spec's own note), and the editor's own `Spell::LandBridge`/`B` radius (still the
  original hardcoded 32, a separate concern from this item's brush radius).
- [ ] Editor: save back to the original `.dat`/`.hdr`/`.ver` (rules in level-format.md "Writing levels": things packed from slot 0, 1-based trigger links, buildings on corners).
- [ ] Editor: smooth brush and the raise/lower levelling step of the ALACN editor (ui-and-editor.md).

### Menu and sandboxes ([ui-and-editor.md](docs/specs/ui-and-editor.md))
- [x] Sandbox Spells: already implemented (`GameMap::sandbox_spells`, `SpellBook::unlimited`) - every spell free, no
  mana/charges (`Availability::Unlimited`'s `cast()` arm is a no-op, `ManaPool` never touched since
  `takes_mana()` requires `Known`). Audit found two real gaps, both fixed: (1) Armageddon was included in
  `unlimited()` and its v1 instant resolution declares `GameMap::outcome`, a one-shot latch that rejects every
  further `Command` - stranding the whole sandbox after one cast; now excluded, same as Bloodlust, and
  `dev::armageddon_demo` grants itself one shot directly instead of depending on the sandbox's own book
  (`unlimited_casts_every_spell_at_will`, `armageddon_demo_casts_and_settles_within_the_run`). (2) The south-west
  islet (meant "to bridge or teleport across") sat ~12.2 cells from the home shore, past Land Bridge's own
  ~8.5-cell range at this island height - moved to `(-20, 16)` (~6.6-cell gap, comfortable margin), proven by
  `sandbox_spells_land_bridge_reaches_the_south_west_islet`. The mana bar's "always full with nothing charging"
  rule already covers an all-`Unlimited` book correctly (`charging_candidate` finds no `takes_mana` slot) - no
  change needed there.
- [ ] Sandbox Buildings: placing and constructing buildings on its free ground (braves, wood and trees are there; every model and every construction stage are shown).
- [ ] More sandboxes as features come (combat...).
- [ ] Sandbox Worship: totems are drawn but nothing can be prayed at yet; the generated stand-ins are identical pillars.
- [ ] Sandbox Walk: the info line still shows "level 1/n" for sandbox maps.

### Camp fire ([buildings.md](docs/specs/buildings.md) "Camp fire")
- [ ] Guard posts' torches (objects 190-193, blended tile 92): draw them with `flame.rs` once guard posts have an original object (the firewarrior and prayer huts' torches burn). Checked this session: the only exe mechanism found that spawns a class-5/type-0xA fire scenery object on a building (pop3-rev-analysis.md "Burning building", the visible-flame addition) is specific to the burn-damage state and has exactly one caller in the whole exe - ruled out as the permanent-torch mechanism. Torches are driven by something else, not found this session.
- [ ] Generated firewarrior hut: drop the static ember cone from its braziers (tools/generate_buildings.py) now that an animated flame stands on them.
- [ ] Check in the game: what a camp fire is for (people gathering? mana?), its burn time when left alone (`ABANDON_TICKS`, 60 s guessed), the ring size, the flatness rule, and when object 12 (other logs) is used.
- [ ] Hover/selection of a camp fire (tooltip with who is around it), a hover cursor when units are selected over one.

## Priority 5 — performance (infrastructure with no dependents)
Real work, but nothing else in this file waits on it and it has no immediate player-facing payoff until the tiers above are further along: performance only matters once there is more to render/simulate at once.

### Performance (60 fps)
Measured with `just profile` (units sandbox, dev build, headless, tracing on): about 73 ms a frame. The simulation
(`run_ticks`) takes 0.014 ms a frame and all our systems together about 1 ms: the time goes to Bevy's own CPU work
around rendering (render app about 43 ms, extract and asset preparation about 10 ms, transform propagation in
PostUpdate about 5 ms), driven by how we hand it the world:
- [ ] Unit sprites: one image, material and quad mesh per animation frame (`units::upload_frame`), so each unit is its
  own draw call and bind group, nothing batches. One atlas texture and material per kind and tribe (frames as UV
  rects in a sprite material, instanced); the same for markers, dust and wood. Biggest win expected.
  **Partly done (`docs/plans/11-batch-unit-sprite-rendering.md`)**: `units::atlas::build_atlas` packs a list of
  `art::Frame`s into one RGBA image + a per-frame pixel rect (`AtlasRect::uv`), shelf-packed, unit-tested - pure
  pixel math, no Bevy types. `UnitSprites`' own upload path now uses it (`pack_tribe_atlases`/`upload_tribe_atlases`):
  up to two shared atlases/materials per kind+tribe (opaque `Mask`, blended `Blend` - a `StandardMaterial` can't mix
  both `alpha_mode`s), each frame keeping its own small UV-mapped quad into the shared atlas rather than one fully
  shared mesh with a per-instance UV attribute - `FrameAsset`'s shape is unchanged so no caller needed touching,
  and many more `FrameAsset`s now share one `.image`/`.material` handle (fewer bind groups/texture switches per
  kind+tribe+alpha-mode, the actual complaint). Packing runs off the main thread (`UnitSpritesJob::run`), not
  `install()` - an earlier version that packed in `install()` measured the "Unit sprites installed" boot step at
  ~880 ms instead of ~23 ms. **Step 5 done** for markers/dust/the volcano rock sprite: a new `units::upload_frames`
  (same opaque/blended atlas split as `pack_tribe_atlases`, but flat - no (pose, direction) nesting to key by, so
  it doesn't need the boot job's background-thread split either) replaces their own per-frame `upload_frame` maps
  (`units/markers.rs`'s `load_art`, `units/dust.rs`'s `load_assets`, `volcano_rocks.rs`'s `load_assets`) - each
  call site's own frame set now shares at most two materials instead of one per frame. `wood.rs` deliberately
  stays on plain `upload_frame`: its one sprite has nothing to batch against. Still open: `animate_views`/
  `respawn_views` still swap `Mesh3d`/`MeshMaterial3d` handles every frame rather than true per-instance-UV
  instancing on one shared mesh (step 4) - attempted judging it this session: a real GPU-instancing rework (a
  custom `Material` with per-instance vertex data, Bevy 0.19) is real regression risk for a benefit this plan
  can't yet measure (see next), so left undone rather than rushed. The before/after `just profile` numbers (step
  1/6) still don't exist in a form comparable to this item's own "73 ms/frame" figure: a `POP3_START=sandbox-units`
  capture's own top entries are dominated by the headless video-capture path itself (`video::to_video`,
  `boot::poll_job<UnitSpritesJob>`) even post-sharing, not a steady-state per-frame breakdown - our own systems
  there (`animate_views` 0.115 ms/call, `respawn_views` 0.235 ms/call, `markers::animate_markers` 0.099 ms/call)
  are cheap either way, consistent with the TODO's own complaint being Bevy's render-app bind-group/texture-switch
  overhead rather than our systems' CPU time, but isolating that cleanly from this trace needs a steadier capture
  method than `just profile`'s video path. **Step 1/6 done**: a real before/after measured on the one number this
  work could actually move - `boot::poll_job<UnitSpritesJob>`'s own install cost, the `tools/trace_top.py` top
  entry both before and after - by profiling the pre-atlas-sharing commit (`5bd7e15`, a worktree sharing this
  checkout's own `CARGO_TARGET_DIR` so only the workspace crates rebuilt) against current `HEAD` under the same
  `POP3_START=sandbox-units`/`SELECT=own`/`--no-original` capture: **532.0 ms total / 76.0 ms per call -> 123.1 ms
  total / 17.6 ms per call, a 4.3x cut**, matching the single-run boot log line too (`"Unit sprites installed in
  532 ms"` -> `"...123 ms"`) - the atlas/material sharing (step 2/3/5) is confirmed the real win this item's own
  "biggest win expected" was about, not the still-undone per-instance-UV meshing (step 4). Per-frame steady-state
  systems were never the bottleneck either way (both traces agree, same numbers as above) - step 4's own remaining
  value is real but smaller and riskier than what's already landed, left open rather than forced.
- [x] Health bars: `animate_views` takes every fill's material mutably every frame (`mats.get_mut`), so all of them are
  prepared again each frame; write only when the colour changes, or share a material per colour step. Fixed:
  `HealthFill` now keeps the fraction it last wrote and `mats.get_mut` is only called when the health fraction
  actually changed since (`health_color` is a pure function of it, so comparing the fraction is equivalent to
  comparing the colour, cheaper).
- [ ] Some image is prepared again every frame (`prepare_assets<GpuImage>` about 3 ms, its extract 1.6 ms): find which
  (the UI text atlas of readouts changing every frame? the cursor? the offscreen target?).
- [ ] Mesh allocator about 3 ms a frame: meshes rewritten every frame. **Fixed in part**: the range ring's line
  and blueprint marks now skip `put`/`put_mark` when the mesh they would write is the same as the last one
  written (`RingMesh`/`MarkMesh` now `PartialEq`, a cache on `Ring`/`BlueprintMark`) - a stationary mouse or an
  unmoving, already-fully-shown ring no longer re-uploads their buffers every frame. Left alone: the curtain part
  of the range ring, which is a continuous time-based animation while shown and so has no "unchanged" state to
  skip to begin with. Still open: the 129 x 129 terrain mesh rebuilt on every camera move
  (`world::rebuild_terrain`) - rewrite only on change; the planet curve could move to the vertex shader over a
  static grid.
- [x] Transforms written every frame for everything grounded (`grounded::place_grounded`) and every unit part
  (`units::pull_to_eye`) now skip the write when nothing changed (`docs/plans/12-change-only-transform-writes.md`):
  `place_grounded`'s translation and `tilt_grounded`'s rotation compare before assigning, `Visibility` uses
  `set_if_neq`; `pull_to_eye`'s per-child translation/scale do the same. Transform/Visibility are `Copy +
  PartialEq`, so exact-equality (no epsilon) is the right check - the inputs only change when something real
  does. Not separately re-measured with `just profile` on this pass (see the sibling "Mesh allocator"/"Whole-
  world respawn" bullets below for the other write-every-frame costs in the same section, still open).
- [ ] Unit views carry 7 marker slots and a health bar each, hidden most of the time: spawn markers when shown.
- [ ] Whole-world respawn on terrain changes: when the simulation levels ground (land bridge, flatten, braves
  flattening a site, every few ticks) `run_ticks` marks `CurrentMap` changed, and trees (re-reading their original
  meshes), sites, buildings, unit views, shadows and totems all despawn and rebuild. Those views should only
  re-ground on `world::TerrainPatches`, and rebuild on a real change of their list.
- [ ] Startup: the unit sprites (~45k frames with the original art, Scale2x twice each) are a boot job behind the
  loading screen (`units::UnitSpritesJob`), each kind and tribe prepared on its own thread (~2 s, was ~10 s serial
  in the dev build). Left: its install still costs ~400 ms of render frame per kind (a mesh per frame, ~6.5k per
  kind, plus the atlases uploaded), and the threads slow each other (one tribe ~0.3 s alone, up to ~2 s together);
  next: one shared quad with per-frame UVs, recolour tribes in the shader or a palette swap, or cache the result.
  Other loaders could become boot
  jobs (markers, dust, wood, tooltip icons, cursor: each under 10 ms today), and the map's own work at a game start
  (terrain bake, tree meshes) too.
- [ ] Measure the windowed release build too (`cargo run --release`): the numbers above are the dev build under
  tracing. The simulation only matters with many units: every walker replans A* on any terrain write
  ("Replanning" in Units).
- [x] Loading screen freezes near the end (`docs/plans/13-loading-screen-incremental-build.md`), two unyielded
  main-thread stalls originally lumped as one item, both now fixed.
  - **Fixed**: `menu::begin_game`/`boot::GameStep` used to build a `Start::NewGame` (or sandbox) map synchronously
    inline (`LevelList::load_current`: parse, build `GameMap`, recompute shading - "likely the dominant freeze for
    a real original level"), zero progress, no frames drawn until it returned. `GameMap::load_original`/`generate`
    and the `sandbox_*` constructors are plain `Send` computation with no ECS/GPU access (`game-core` has no Bevy),
    so - unlike `BootJob`'s GPU-touching `install` - the whole build needed no phase-by-phase chunking: it now runs
    as one worker-thread task (`menu::spawn_game_build`, `AsyncComputeTaskPool`), polled each frame
    (`menu::poll_new_game_build`) the same way `boot::poll_job` already polls a `BootJob`'s `run`. `boot::GameStep`
    gained a `Building` step between `Show` and `Built`; `boot::next` can't drive that transition itself (a pure
    function has no way to poll a `Task`), so it just keeps returning `Wait` for it, same as `Show` - the poll
    system flips it to `Built` directly once the task resolves, same pattern `poll_job` already uses for
    `JobPhase`. All `Start` variants (not just `NewGame`) go through the same path now, including the sandboxes
    (previously synchronous too, already fast - unaffected in practice, still correctness-checked:
    `HEADLESS=1 POP3_START=sandbox-units` reaches `Playing` the same as before). No original install was available
    in this environment to reproduce the "real original level" case directly; verified instead that the generated
    fallback goes through the exact same `Building`/`poll_new_game_build` machinery end to end
    (`HEADLESS=1 SHOT_BOOT=1`, reaches `Playing` and screenshots correctly) and that `boot/mod.rs`'s own `next()`
    unit tests cover the new `Building` state.
  - **Fixed, the slow-`install` half**: reported in play against a real original install - the loader hanging
    near the end, unresponsive (no mouse movement: the whole app, not just the sim), occasionally crashing
    outright. `boot::poll_job` used to run `J::install(output, world)` in one unyielded call per `BootJob`; with
    the original art's full frame counts (`UnitSpritesJob`'s own atlas-sharing rework, TODO.md "Unit sprites",
    already cut the generated-art/software-rendering case measured here to ~100-120 ms, but that benchmark has
    nothing like the original's real sprite counts) that call could run long enough to starve the window's own
    event loop - and on some drivers, long enough without a swapchain present to trip the GPU driver's own
    timeout/recovery, crashing the app rather than just freezing it. `BootJob::install` is now `install_step`
    (`InstallProgress::More(output)`/`Done`), called once per frame until done instead of once for everything;
    `JobSlot` gained an `Installing(Output)` state to hold the output between calls. `UnitSpritesJob` installs one
    kind per call (`install_steps` = kind count) instead of all of them in one, bounding a frame's GPU-upload work
    to one kind's atlases regardless of how large the original art makes them. `BootStatus::fraction()` now gives
    the install phase its own climbing share (0.9 to 1.0 by chunks done) instead of a cap stuck at 0.99 until the
    single call returned, so the bar visibly keeps moving instead of appearing to hang right before it does.
    Covered by `boot::tests::a_chunked_install_lands_one_piece_per_frame_and_the_bar_climbs_through_it` (a
    `BootJob` whose `install_step` is called once per fake frame, checked after each); end to end,
    `HEADLESS=1 SHOT_BOOT=1 --no-original` still reaches `Playing` and screenshots correctly (generated art, so
    this doesn't exercise the original's own much larger frame counts - no original install was available in this
    environment to measure the fix's actual effect against the sizes it targets).

## Priority 6 — `--no-original` asset completeness (copyright safety)
AGENTS.md's own bar is that everything shippable works with `--no-original`. Sound is a hard zero today (format reverse-engineered, no playback, no CC0 assets sourced at all); several spell VFX, building/spell icons and ritual scenery are still original-only. Largely independent of the other tiers, so it can run in parallel with them rather than waiting its turn.

### Sound ([sound.md](docs/specs/sound.md))
- [x] `pop3_format::sound` (`crates/pop3-format/src/sound.rs`, `docs/plans/14-sound-format-parser.md`): SDT
  container/40-byte headers (`SoundBank`/`Entry`), signed 16-bit PCM mono/stereo (`Entry::pcm`, interleaved -
  a caller de-interleaves with `is_stereo()`), loop points in bytes (`Entry::loop_range`, `(0,0)` -> `None`),
  `0x80`/`0x10`/`0x20` flags (`is_placeholder`/`is_compressed`/`is_music`), a drum bank's overlapping
  placeholder entry 0 (test proving the parser reads each entry from its own table offset, never assuming
  back-to-back packing), and `PopDrones22.SDT`'s own shorter-header `MusicBank`/`MusicEntry` with its MPEG
  Layer II payload left as an unparsed byte range (`mpeg_data`). `examples/sound_info.rs` dumps either kind.
  Parsing only, as scoped: `soundd2low.SDT`'s `0x10` codec is left undecoded (the spec says to ignore that
  bank and use `Soundd2`), and Layer II decoding/playback is the next bullet, still open.
- [ ] Play SFX in the client (Bevy audio): spells, chopping, building, follower voices, ambiences; drums and music. Free sounds for `--no-original`.

### Art ([assets.md](docs/specs/assets.md))
- [x] Audit the original-data-free path and missing assets across buildings, units, scenery, VFX, UI and audio ([inventory](docs/specs/assets.md)).
- [ ] Next open art: building icons, ritual scenery/reincarnation stones, vehicles and worker action clips (inventory P1). CC0 trees/unit sheets, the generated building kit and 14 of 20 spell icons already ship.

### Original sprites and blending ([sprites.md](docs/specs/sprites.md))
- [ ] Spell effects from the alpha sprites (`hfx0-0.dat`, decoded by `blend::AlphaTable`, blended on the GPU): map each effect to its frames (sprites.md).
- [x] Ghost table `ghost0-X.dat` decoded (`pop3_format::ghost::GhostTable`, sprites.md "Ghost"): the 66% mix,
  identity diagonal and not-symmetric shape, tested against a synthetic fixture (no real game bytes, per
  AGENTS.md). Used for the record rather than replayed per pixel on screen: see the Units "Statuses" entry
  below for the client side (plain GPU alpha blending reads the same as the table's mix, once sprites are
  RGBA rather than live palette indices).
- [ ] `sprites` parser: test the empty (0 x 0) entries and the `0x7F` trailer seen in the real banks.

## Priority 7 — multiplayer
Lowest priority: the lockstep codec already works in tests with no UI or dependents yet, and multiplayer only matters once the single-player game above it is in better shape.

### Multiplayer ([multiplayer.md](docs/specs/multiplayer.md))
- [ ] Network driver: send `Schedule::close_local`'s turn as `Message::Tick` every tick, feed peers' turns to `receive`, 2-3 ticks of delay (the schedule is done, single player only today).
- [ ] Host/join UI, lobby, reconnect.
- [ ] Desync checksum of heightmap + units.
- [ ] Pause and game speed as `Command`s, so every peer agrees (single player only today, `game_speed.rs`).

## Tooling
- [ ] Try [d7vk](https://github.com/WinterSnowfall/d7vk) (Direct3D7-over-Vulkan) as the original's Direct3D layer under
  Wine, for whoever runs the original install to live-check facts for pop3-rev-analysis.md or `just shot`/`just run`
  against it: check whether it runs the original at all and whether it's more correct/faster than Wine's own
  built-in d3d7, document the result here either way. Spec-session/live-check scope only - a coding session never
  runs the original (AGENTS.md "Licence and original data").

