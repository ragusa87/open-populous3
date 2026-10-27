# Combat

Our rules for health, fights, attacks on buildings and firewarriors' fire. The facts come from
pop3-rev-analysis.md (sections named below); the names here are ours. Tags: **[exe]** traced in the executable,
**[measured]** read live from the running game, **[ours]** our choice, **guess**.

## Health
- Per kind (`UnitKind::stats`, `kind_stats.rs`): full life (constant.dat values) and the life regained per heal
  ("Health and healing" [exe, measured]):

| Kind | Life | Heal |
|---|---|---|
| shaman | 2000 | 32 |
| brave | 1000 | 4 |
| warrior | 1800 | 6 |
| preacher | 1100 | 4 |
| spy | 600 | 4 |
| firewarrior | 700 | 6 |
| wildman | 1000 (guess) | never |

- Everyone heals on every eighth turn of the game (`unit::HEAL_EVERY`, phase 0: the same turn for all) [exe],
  whatever it does, fighting included; not in the air, drowning, stranded or dead.
- Never past full [ours]: the original adds the heal while below full, so life can end up to one heal over.
- In the air, a deadly hit does not kill at once: the unit flies on and dies on landing [exe].
- Not yet: no healing while struck by lightning (we have no such state yet), the angel of death never heals.

## Fight numbers
- Per kind (`KindStats`): a blow's strength at full life (constant.dat values: 60, the warrior 360), a rank (brave,
  preacher, spy 1; warrior, shaman 2; firewarrior 3: the lower gives way in a full melee), a pace (turns per
  animation frame: the firewarrior 2, so its moves last twice as long) [exe, measured]. Wildmen never fight;
  firewarriors never join a melee already going on, but fight when challenged [exe].
- A blow hits `max(32, strength × life / max_life)`, × 3 when the attacker is bloodlusted (`combat::strength`) [exe];
  a bloodlusted defender then takes `>> BLOODLUST_HEALTH_SHIFT` (÷8) of that, and of a firewarrior's bolt too
  (`GameMap::damage`, independently of the attacker's own multiplier). Not yet: per-person maximum life
  ("Measured": not always the kind's).
- Move lengths in turns (`combat::move_turns`), × pace [exe, measured]:

| Move | Followers | Shaman |
|---|---|---|
| shove (the challenge's opening) | 6 | 4 |
| knocking blow | 6 | 4 |
| trading blow | 7 | 4 |
| clean blow | 7 | 5 |
| reeling | 7 (4 when knocked back) | 7 |

- `DamageKind::Melee`: hurts anyone alive; the shield blocks spells, not blows [ours, from the game].

## Attacking
- `Order::Attack { target }` (`Target::Unit` or `Target::Building`), from a left click on an enemy person or
  building with own people selected (`units::selection::attack_click`). Only those that may attack it go
  (`attack::may_attack`): both fight (never a wildman), other tribes, the target alive. A building only if it
  belongs to another tribe and wears down (`may_attack_building`): never a neutral one (a vault of knowledge, a
  totem), which is prayed at; a click on it gives the prayer orders as before. A firewarrior's attack throws fire
  from afar ("Firebolts").
- `Action::Attacking { target, to }`: walks a path to `to`, the person or the building's door, and stands there.
  Each turn before the coordinators `GameMap::tend_attacks` aims `to` at where the target is, planning again once it
  is a cell off; a target gone, dead or no longer an enemy stops the attack (Idle). Once within reach of a person, a
  challenge starts (below).

## Challenge
A coordinator (`challenge::Challenge`, coordinators.md) for one attacker and one target, before a melee
("Pre-fight" [exe, measured]):
- Started in `detect_coordinators` (`GameMap::start_challenges`): an attacker within `FIGHT_REACH` (224) of a
  target that is in no coordinator, outside and on the ground. Both join it (`Unit::join`): the attacker
  `Role::Charging` at its walking speed + a draw below a quarter of it, the target `Role::Waiting`. A target already
  in a melee is left alone for now (joining it comes with it); a target already in a challenge instead turns that
  challenge straight into a melee at once (`GameMap::escalate_challenges`, right after `start_challenges` in
  `detect_coordinators`, before `join_melees` brings the new attacker and anyone else in reach into it) - guessed,
  not traced against the original (see "Spotting" and TODO.md "Joining a fight already starting"). A shaman in her
  cast jump until it ends: a cast during a fight takes her out of it (an order), goes off, and the attacker
  challenges her again [ours: the original's rule for casting in a fight is not traced]. Joining drops a spell she
  held (`Unit::join`).
- Each turn: it ends, both released, when one is gone, dead, no longer linked (an order took it away) or of the same
  tribe, or when a charge lasts `CHARGE_TICKS` (40). The charger runs straight at the target (`Placed` gives the
  target's place when it moved). Within `SHOVE_REACH` (315) on both axes of a waiting target: `Shove`, 6 turns
  (shaman 4, × pace); with 2 left the target is thrown (`Push`, `PushKind::Melee`: the shield does not stop it, the
  target stays `Shoved` in the challenge) at 100 + a draw below 100 per turn, turned a third to two thirds of a turn
  from the way to the attacker (away from it), lift `SHOVE_LIFT` 48 [guess: ours does not slide on the ground]; no
  damage. Still again, it waits. The attacker charges again; reaching it again ends the challenge with `StartMelee`: a melee
  at the target's place.
- Already in reach when it starts, the attacker shoves at once.
- While waiting, the target faces the attacker (`WorldEvent::Face`, `Unit::face`: ours has no separate "held
  look-at" field - with nothing else moving its body while it only waits, setting the one `facing` field each turn
  reads the same) and taunts on a 1 in 8 chance per turn (`challenge::taunts`; pop3-rev-analysis.md "Pre-fight",
  confirmed): kept on `Challenge::taunted` (the last turn it fired) for when there is a pose and the three taunt
  animations to show for it (TODO.md "Units").
- Not yet: the original's option with no shove (a real level/script option, not a cheat - the same flag also
  disables Melee's own knockback variant, see "Melee" below); ground sliding instead of `SHOVE_LIFT` (push
  mechanics unread at the exe level either way). The "third approach, a target at its building" is not a separate
  case: it is the same
  `Approach::PulledOut` building-occupant pull-out already described under "Attacking a building" below, just
  reached through a slower wait-then-approach-the-door lead-in instead of being thrown out immediately
  (pop3-rev-analysis.md "Pre-fight", modes 1/2) - nothing left to implement here beyond what `PulledOut` already
  covers.


## Melee
A coordinator (`melee::Melee`) holding up to `MAX_FIGHTERS` (6) people of two tribes: one alone on its side in the
middle, the others in a ring around it ("Fight" [exe, measured]). Fighters are `Action::Melee { melee, place, stance }`.
- Started by a challenge (`GameMap::start_melee`): the middle where the target stands, the ring's turn drawn
  [guess: the original's first angle is not traced].
- Each turn, in this order:
  1. fighters gone, dead, unlinked or farther than `MELEE_LEASH` (2048) from the middle leave (`Release`);
  2. fewer than 2 left or one tribe left: it ends, everyone released, and the surviving tribe's win is tallied
     (`WorldEvent::FightWon`, `GameMap::melee_wins`: "Fight" [exe] `stats[winner][0] += 1`), separate from a
     per-tribe death tally a death increments for its OWN tribe (`GameMap::melee_deaths`, counted straight off a
     killing `DamageKind::Melee` blow in `apply_event` - the only source of that kind, so equivalent to the
     exe's own blow-resolution-step count: `stats[dead's own tribe][1] += 1`, pop3-rev-analysis.md "Fight" - it
     counts losses suffered, not kills scored);
  3. the middle is the fighter alone on its side (the last one first) and follows it;
  4. on `RING_TURN` (every 32 turns, at its own phase): the fight relocates off unsuitable ground (`Melee::relocate`
     - water, a cliff, or inside a building's walls, to that building's door or else the nearest walkable cell; the
     exe's own suitability test, `0x51E4B0`, is unread, so this stands in with the walk-passability test everything
     else already uses, and the spiral search for the nearest cell, `0x49A2F0`, is a plain ring search outward -
     also run once when the fight/pre-fight is created, `GameMap::start_melee`), then the ring turns half a turn,
     then by 1 chance in 2 another 20° to 80° either way (`RING_EXTRA`);
  5. places (`ring_place`): the middle, the others at `RING_RADIUS` (180) around it at the ring's turn, + 90° with
     3 people, + 120° and 240° with 4, the 5th and 6th in the middle; `Placed` only when a place changed;
  6. each fighter on guard draws 0..16 and picks its opponent (the middle one any of the ring, the ring the
     middle one). Against an opponent on guard (`blow_on_guard`): the shaman 7/16 knocking, 9/16 clean; others
     5/16 knocking, 9/16 clean, 2/16 trading. Against a busy one near its place (`BUSY_REACH` 360), 4/16 of the
     time (`blow_on_busy`): clean, or with more than 2 people clean or trading by the draw's parity.
     A blow (`Struck`): the target loses the attacker's strength, and except a clean blow the attacker the target's
     (both at once, `DamageKind::Melee`); lives already lost this turn count. Both are then busy for the turn.
- A fighter's own cycle (`Stance`): `ToPlace` (walks to its place) → `OnGuard` within `PLACE_REACH` (12) →
  `Striking` (the blow's length) or `Reeling` (7 turns, 4 when knocked) → to its place again. A knocking blow
  against an on-guard follower (never the shaman) throws it back after it reeled: 35 + a draw below 70 per turn,
  away from the attacker, along the ground (`Staggering` until still).
- Fighters heal as anyone (see "Health").
- Joining (`GameMap::join_melees`, in `detect_coordinators` after the challenges and after `escalate_challenges`): an
  attacker within `FIGHT_REACH` of a target in a melee joins it if it is of one of its two tribes and not a
  firewarrior, while there are fewer than 6; with 3 or more of its tribe there it joins only by replacing the one of
  lowest rank below its own (released). Then, both sides with 2 or more, the last of each leaves for a new
  one-to-one melee at the same place: a melee stays one against several ("Fight" [exe, measured]). A target reached
  while still only in a challenge (not yet a melee) is turned into one at once instead ("Challenge" above), so a
  third attacker joins the same turn rather than waiting out the duel's own pace.
- A fighter released from the melee (rule 1, or the fight ending) does not simply go idle if it had come from its
  own standing attack order: that order goes dormant for as long as the fight lasts and resumes, same target, once
  it is back on its feet (`Unit::join`/`Unit::release`, `resume_attack`) - this is how a target blasted out of
  reach gets chased again instead of forgotten (pop3-rev-analysis.md "Resuming a fight cut short by a push"). A
  target with no attack order of its own (the common case) still just goes idle.
- Not yet: the move's sounds (a generic catch-all for two of the six per-fighter stance values, plus for Striking
  specifically: a warrior-vs-warrior "clang", a warrior-vs-non-warrior sound, a non-warrior's own blow sound, and a
  sound layered on the receiving opponent whenever a warrior lands a blow regardless of its own type -
  pop3-rev-analysis.md "Fight" has the exact ids). Still genuinely open: what the original's `0x89D167` flag (set
  whenever a fight/pre-fight involves the local player's own tribe) actually triggers - plausibly the fight music or
  an alert, unconfirmed.

## Attacking a building
`crate::wreck` ("Attacking a building" [exe]); the attacker's own work, no coordinator except the pull-out.
- `Order::Attack` on an enemy building walks the attacker to its door. Arrived (`ARRIVED` 32 per axis): it hacks
  (`Action::Hacking`), `HACK_AT_DOOR` (16) turns at the door, the building shaking. Then, if people are inside, the
  first is pulled out: thrown from the door outwards at `PULL_SPEED` (100) with a small lift `PULL_LIFT` [guess: ours
  does not slide], and a challenge (`Approach::PulledOut`) starts with it already `Shoved`; the attack on the
  building is chained after it. Empty: it hacks for 8 + a draw below 16 turns, adding its kind's `sabotage` (brave,
  preacher, spy 7; warrior 12; shaman 10; firewarrior 16) to the building's `wear` each turn, then walks to a spot
  `WANDER` (360) from the building's middle (outside the walls, else the door) and hacks again.
- The building (`Building::tend_wear`, every turn, `GameMap::tend_building_wear` after the fires): it shakes
  `SHAKE_TICKS` (6) after each blow; its wear fades by one every 4 turns; at its limit (`wear_limit`: 2000; walls
  and gates 16 000; the prison 22 000; a vault never) the wear drops by the limit and it loses a piece of wood
  (`lose_a_piece`): damage smoke, a large building's people thrown out [which kinds are large: guess], no repairs
  for `REPAIR_DELAY` (1200 turns); with no wood left it is destroyed (its people thrown out) and leaves ashes.
- Not yet: enemies standing on a plan counting as its people, blasts' and firewarriors' wear, the flying faces of
  the partial explosion, the player warned once, the attack going on from where it stands when no way leads to its
  spot.

## Ashes
`crate::ashes` ("Building ashes" [exe]): a building destroyed by losing its last piece of wood (worn down by
attackers, burnt down) is removed and leaves its footprint cells (`walled_cells`) in ashes (`GameMap::burn_down`);
one removed any other way (a plan cancelled) leaves none.
- `GameMap::ashes`: up to `ASHES_SLOTS` (100) patches, oldest first; a new one when full clears the oldest.
- On `ASHES_EVERY` (every 16 turns, phase 0) every patch ages by 16; past `ASHES_FULL_TICKS` (6000) its darkness
  (127 when fresh) drops by 2 each time; at 92 or below the patch is gone: 6000 turns dark, then 18 steps (288 turns).
- Nothing is built on a cell with ashes (`placement::Blocked::Ashes`).
- Drawn: the terrain texture chars those cells (`terrain_texture::char_ashes`, our colour), less as they fade; the
  map's dirty rects redraw them when a patch comes, fades a step or goes. The base terrain texture itself now also
  reflects an ashed cell's darkness through the shared blend/cliff mechanism (`game_core::shading`,
  terrain-textures.md "cliff") feeding `Theme::cliff`'s remap — `char_ashes` stays wired alongside it for now,
  pending visual verification before removal (TODO.md "Terrain").
- Lava and earthquake ground share the same mark once they cool/fade (`lava::Lava::turn`, `WorldEvent::Scorch`):
  Volcano's flowing lava, its whole footprint in one shot once fading ends; Earthquake's static fissure, all its
  cells at once (they share one blend clock) once they drop below `GONE_BLEND`.
- Not yet: the hover text over ashes; the long damage smoke at the ashes; the whirlwind's destruction.

## Spotting
`crate::spotting` ("Auto-attack" [exe]): people who see an enemy attack it on their own, before the attacks are
followed each turn (`GameMap::spot_enemies`).
- Who: braves, warriors and preachers (`looks_for_melee`; a preacher only starts a fight with a preacher or a shaman,
  `attack::may_attack`, and defends itself like anyone when attacked), and firewarriors once reloaded, as far as they throw (`GameMap::looking`; any enemy person, in a melee or
  not: their own look is not traced); the preacher's sermon is its own look ("Sermon"); the shaman's look during some orders is TODO; spies and wildmen never look. On its kind's `look_every` (4 turns; preacher 2),
  phase 0.
- The preacher's own look (pop3-rev-analysis.md "Preacher conversion") only fights another preacher or a shaman;
  anyone else it finds gets preached to instead, a separate state machine on the target
  (`PREACHEE_CONV_FREQ`/`_CHANCE`, a 1 in 5 roll every 41.7 s) that destroys the target and recreates it, same
  type, in the preacher's tribe. The link from its scan to that state machine is not traced yet.
- How far (`sight_now`, cells; per kind `sight` and `sight_at_work`: brave and warrior 5 and 1, preacher and shaman
  3 and 1, firewarrior 11 and 5, spy and wildman 0): idle, holding wood or hacking at a building, its full sight;
  walking (every walk is on an order, the original's "going to the place") or attacking on its way, nothing;
  working (cutting, picking up, flattening, building, hammering, round a fire, praying), `sight_at_work`, its own
  cell; busy (fighting, panicking, thrown, casting, drowning, entering, inside, dead), no look at all [the order
  kinds' split is the analysis' guess].
- What: the square of cells (sight rounded down to even, + 1) around it, row by row from a corner (`square`); the
  first match wins, not the nearest: an enemy person it may attack, alive, outside, on the ground, not thrown and
  not listening to a sermon [guess: not traced, but it would undo the preacher's sit-down on its very next look],
  and not in a melee; when idle, also someone in a melee (to join it) or an enemy building it can wear down. It then
  attacks it (`Order::Attack`'s path) [ours: the original gives itself an "attack around this cell" order].
- `UnitGrid`: the living people outside by cell, sorted, built only on a look turn.

## Firebolts
`crate::firebolt` ("Firewarrior fire" [exe, measured]). Firewarriors (`KindStats::throws`) never charge, hack or
join a melee on their own attack (they still fight when challenged).
- Reach per axis (`bolt_reach`): 11 cells × the altitude band's percent (`cast_range::ALT_BAND_PERCENT`, the band
  `height / 128`), + 4 in a drum tower (not used yet), made odd, × 256 + 56 units: 9 cells low down, 11, 13, 15, up
  to 17 at the top.
- Attacking: it walks until its target (a person, or a building's door) is in reach, then stops there
  (`Unit::hold`); reloaded, it throws (`Action::Throwing`, 5 turns): two bolts start 96 units to each side at its
  height + 16, only the right one hits. Reload `RELOAD` 25 turns (36 from a tower, >> 3 bloodlusted), its last step
  only on a turn of four (`reload_step`): 25 to 28 turns between throws.
- A bolt (`Registry<Firebolt>` on `GameMap::bolts`, ticked first in the turn, `tend_bolts`): 512 units per turn in
  3D, homing on the target's middle (+ 16) or a building's door on the ground, arriving within one turn's move. The
  hitting one: every enemy person outside in that cell loses `CELL_DAMAGE` 100 (× 3 bloodlusted), the target also
  `TARGET_DAMAGE` 100 × its kind's share (`bolt_share`: the warrior 12/256, so 4; others all of it): a brave 200, a
  warrior 104, anyone else in the cell 100; `DamageKind::Fire` (a shielded target takes none, and its knockback is
  stopped too, `PushKind::Spell`: a magical ranged attack, like any other spell [reported, not traced]).
  Those hit are thrown 64 per turn away from the thrower and 110 up; a warrior or preacher that lives only hops 36.
  Trees in the cell burn; a targeted building takes 100 wear and burns. A firewarrior target fires back at the
  thrower (`fires_back`, `WorldEvent::StrikeBack` after its push) unless it is already attacking or in a fight: whatever
  else it does (walking, working) it drops; thrown, the attack is chained for when it lands [ours: that part of the
  original is not read in detail].
- Drawn: our own glowing ball per bolt (`firebolts` view); the throw pose (anim 7).
- From a drum tower or a vehicle [exe]: the firewarrior looks for enemies and throws without getting out (it is the
  only kind that looks from inside or aboard). Its reach follows its altitude (`GameMap::altitude`: aboard, its
  vehicle's, so a balloon throws further; in a building, its level), the bolts start at that height; in a finished
  drum tower (`GameMap::in_tower`) it reaches 4 cells more across, reloads in `RELOAD_IN_TOWER` (36) and its target
  takes `TARGET_DAMAGE_FROM_TOWER` (400). An attack order reaches a passenger (`order_aboard`); aboard or inside, a
  target out of reach is waited for, it does not walk [ours: driving it towards the target is not done]. While a
  passenger throws, its vehicle goes at half speed (`Vehicle::speed_now`) [exe: the thrower's speed is halved
  aboard; that this slows the vehicle is ours]. Drawn pedalling aboard while it waits, its
  throw pose while it throws.
- Not yet: the line of fire (a building in the way), the smoke trail and
  the ground light, a ghost firewarrior's harmless bolts, the shaman inside a building or vehicle taking half her
  life, an angel's reaction.

## Sermon
`crate::sermon`: a preacher sits the enemies it sees down and preaches to them; now and then one joins its tribe
("Preacher conversion" [exe]; [game]: the user's account of the game).
- Who may be preached to (`may_preach`): an enemy that fights, alive, on the ground in the open, not a preacher or a
  shaman (those the preacher fights), never a wildman [game: the trace converts them at once, the user's game never],
  never someone aboard a balloon or a boat [game], not taken by another coordinator already.
- The start [guess: not traced]: on the preacher's look turn (every 2 turns), a preacher that is free (idle, holding
  wood, walking on any order, preaching, or ordered to attack someone it may preach to) sits down every such enemy in
  its look square (3 × 3 cells, `spotting::square`) at once; there is no walk-up, so attackers coming at it sit before
  they reach it. Unlike anyone else's own look (`spotting::sight_now`), walking does not blind the preacher to this:
  its conversion look is a separate scan, not the melee one, so it keeps working on the way somewhere, not only once
  idle [game]. Ordered at someone further, it also checks that one target every turn rather than only on its look
  turn, so the target's own spotting cannot challenge it first while the preacher is between look turns [game]. It
  then stands `Action::Preaching`, facing a listener, and goes idle again once it has no listener left.
- A sermon per listener, kept as state on the two units involved rather than a separate coordinator object - the
  original does the same: no backing object of any kind, just a sub-state on the preacher and a resist-timer and
  striker reference carried directly on the listener itself [exe]. `Action::Listening { preacher, sitting, left,
  attempts }` on the listener carries everything a sermon needs, ticked by `GameMap::tend_sermons`
  (docs/specs/coordinators.md), not a coordinator. The listener sits (anim 18) and is locked [game]: no orders,
  dropped from its player's selection, until converted, freed or dead; a brave puts down the wood it carries
  [game]. It still heals and can be attacked:
  an attacker challenges it as anyone, which takes it out of its sermon, the others listen on [game]. More listeners
  on one preacher convert faster only in the aggregate, never individually: each listener's own countdown and roll
  are entirely independent of every other listener's - this is a plain property of several independent timers
  running at once, not a group bonus.
- Each turn the sermon ends (the listener freed) when the preacher is gone, dead or no longer preaching (an order took
  it, or it was attacked or thrown (a bolt, a blast): every one of its sermons ends [game], and each freed listener attacks it back at once,
  `WorldEvent::StrikeBack` [game]; nothing else protects them: whether the preacher, free again, sits them down before
  they reach it is up to the usual rules: a bolt only makes a preacher hop (`firebolt::steady`), so a listener a cell
  away is sat down again on the preacher's next look, about 20 units before it could challenge it, while one right
  next to it challenges it first), or the listener is gone, dead or taken by something
  else. Its countdown: the first chance after `CONV_FIRST_CHECK` (250) turns, then every `CONV_CHECK` (500), each give
  or take a sixteenth [exe]; a chance converts 1 time in `CONV_CHANCE` (5) [exe], unless the preacher's tribe is at
  the population cap (`headcount::MAX_POPULATION`, 200), when it waits the next check.
- Conversion (`WorldEvent::ChangeOwner`, `Unit::change_owner`): the original destroys the person and makes one of
  the same kind in the preacher's tribe at its place [exe]; ours keeps the unit (same id and kind) and starts it
  afresh the same way: new tribe, plans, orders, work and wood gone, full health, no status, idle.
- Drawn: listeners sit down once (anim 18 over `SIT_TICKS`, 6 turns, the `Listening` countdown) and stay seated on
  its last frame (the generated sheets stand in with the prayer pose), the preacher gestures with its free arm
  while holding the book (`Pose::Preach`, anim 22 of its 5 identified phases, animations.md "`HSPR0-0.DAT` by
  index" [guess: the original 20-24 sequencing is not traced, one phase stands in for it]); the dev overlay draws
  a line from each preacher to each listener.
- Not yet: the building variant (preaching to a building's people), what really starts a sermon, the soul flying
  from the preacher to the converted, the halving flag of the first check.
