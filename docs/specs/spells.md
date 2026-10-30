# Spells

`game_core::spell::Spell` -> `cast(&mut Heightmap) -> Option<DirtyRect>` (the terrain part). `GameMap::can_cast`
says whether a spell applies at its target; `GameMap::apply` ignores a cast that does not. Implemented:

| Spell | Effect |
|---|---|
| Land Bridge | aimed on the ground (`hud::spells::ground_spell`), within range, at land only (not open sea, `unit::is_sea`: the water on its way is raised); released like a blast 6 turns into her cast jump; starts at the caster's shaman as she stands when the command runs, or where it was aimed from on a god level (`GameMap::shaman_omni`: the ground under the bottom middle of the view). An effect (`land_bridge`, `EffectKind::LandBridge`): 4 height points wide (−1, 0, +1, +2 across the line), each moving every turn towards a straight slope between both ends' heights at cast (at least 90), `h += clamp(T − h, ±256) / (64 − turn)`, for 64 turns (`LAND_BRIDGE_MAX_CHANGE`, `LAND_BRIDGE_DURATION`): sea raised, hills in the way lowered. The editor's B brush is still the instant 1-wide line (min 32). While it builds, a tiny star twinkles at the centre of each cell with a corner not yet at its height (`sparkles::SparkleView`, from `Effect::reshaping`: every terrain effect gets them). Dev: `BRIDGE=x,z` (cells, from the player's shaman), `BRIDGE_TICKS=N` |
| Flatten | aimed on the ground at the nearest height point (`flatten::nearest_point`), released 6 turns into the cast jump like a blast. An effect (`flatten`, `EffectKind::Flatten`): the 45 height points of a disc (within √13 of the target point, about 3.5 cells: the original's count) move each turn towards the target point's height at cast, `h += (T − h) / (15 − turn)`, for 15 turns (the original's length; its curve is not traced, ours lands on the last turn): hills cut, hollows and sea filled. Sparkles on each cell it is reshaping (`sparkles`). Buildings on it are not re-levelled or destroyed yet (TODO). Free in the sandboxes. The editor's T brush is still the instant radius 4. Dev: `FLATTEN=x,z` (height point, another tribe's), `FLATTEN_TICKS=N` |
| Erode | aimed on the ground (`hud::spells::ground_spell`), released like a blast. An effect (`erode`, `EffectKind::Erode`, `DURATION` 64 turns): `PASSES` (63) falloff-brush passes (the existing `Heightmap::raise` brush, radius 4), one per turn except the last (which only ends it), each centred `-4..+3` cells from the cast point on both axes (`map::Lcg`, two draws, `& 7 - 4`) — our own approximation of the original's per-cell steepest-descent slope pick (pop3-rev-analysis.md "Erosion passes"), confirmed sufficient rather than a new hydraulic simulation (docs/plans/spells.md "Erosion": "just a slower/deeper/longer-duration version of the falloff pattern already there"). Floored at sea level by `raise`'s own clamp, like every other terrain spell. Sparkles while passes remain, dropped once a cell is already at sea level (`sparkles`, `Effect::reshaping`). The editor's F brush is still the instant radius 4, -120 drop. Dev: `ERODE=x,z` (height point, another tribe's), `ERODE_TICKS=N` |
| Raise | editor brush, radius 3 +64 |
| Teleport | sandbox only (the original names a spell model 21 teleport, see below, effect unknown): the caster's living shaman does her cast jump, then is at the target (`Unit::cast_teleport`); only onto ground she can walk (`Mobility::Walk`: no sea, no cliff), checked when cast and again when she lands (she stays if it no longer is); another order during the jump cancels it; she lands falling from a cell up (units.md "Landing") |
| Blast | aimed on the ground (`hud::spells::ground_spell`): the shaman's cast jump, and `CAST_RELEASE` (6 turns) into it, a fireball shot flies from her own position to the target (`blast::Fireball`, `GameMap::fireballs`, ticked by `GameMap::tend_fireballs` before effects like `GameMap::bolts`): `FIREBALL_SPEED` 1000 world units per turn in a straight 3D line, no homing or shield deflection yet (the general shot machinery every other spell shares - TODO.md "Shots" - is left for a later round). On arrival, a simple blast effect at the target (`blast::effect`, in `GameMap::effects`): the original's simple blast, 3 turns at radius 2, 4 then 5 cells. Each turn every living person outside buildings in the radius takes `BLAST_DAMAGE_PERSON` 50 (`GameMap::damage`: not the caster's tribe, not the shielded); each living person of any tribe but the caster's own shaman, not inside a building, within the turn's radius and 2.5 cells (`PUSH_RANGE` 1280) is flung (`Unit::fling`): away from the centre at 140 and up at 98 (world / height units per tick), times `(1280 - d) / 1280`, then tumbles and falls (units.md "Physics"). Pushed again on each turn it is still within 2.5 cells, cumulatively (velocities add, `Unit::fling`'s own `saturating_add`); the horizontal push is scaled by the ground's own slope 32 units ahead along it (`blast::slope_factor`): about ×1.06 flat, up to ×2 where the ground falls away, floored at 3 climbing a rise (pop3-rev-analysis.md "Blast push and flight"). The caster's own followers (not the shaman) are pushed only on the last of the 3 turns, at most `BLAST_FRIEND_AFFECT` 5 of them (which 5, when more are in range, is not traced: a guess, kept to iteration order). Every building the blast ever reaches takes `BLAST_DAMAGE_BLDG` 40 wear once (`WorldEvent::Wear`, the same accumulator hacking and firewarrior bolts feed, combat.md "Attacking a building"). Client-side only (`blast_fireball.rs`, `blast_impact.rs`): the fireball flies as a small flickering flame, and its landing gives a small flash (deliberately sized about a firewarrior bolt's own impact, not the blast's own multi-cell damage radius) fading into a burst of dark smoke (`effects::BLAST`); `blast::VISUAL_TICKS` keeps the effect alive a little past its 3 damage turns purely so this has room to play out. Not yet: angels of death and ghosts (neither modelled at all yet). Dev: `BLAST=x,z` (cells, cast by another tribe), `BLAST_TICKS=N` |
| Lightning | aimed on the ground like a blast (`Spell::Lightning`, world units), released 6 turns into the cast jump, walked into range first; free in the sandboxes. An effect (`lightning`, `EffectKind::Lightning`, 12 turns, a guess): on its first turn it kills outright up to `LIGHTNING_NUM_KILLS` + 1 = 7 people in the target cell outside buildings and inside the building it hits, in id order (`DamageKind::Lightning` through `GameMap::damage`: any tribe, the caster's own followers and enemy shamans too; spared: the caster's own shaman and the shielded), sets a building covering the point on fire (`WorldEvent::SetFire`, `fire`: its survivors thrown out, one piece of wood lost), burns the trees of its cell (`WorldEvent::BurnTrees`, `Tree::ignite`) or else lights a ground fire (`ground_fire`), and spawns a simple blast there. Drawn (`client lightning.rs`, our own look): a jagged white-blue bolt from 8 cells up, flickering between 3 zigzags seeded by the effect id and fading over the strike, and a glow on the ground. Victims carbonized while dying or dead (anim 97, brave only - `Pose::Carbonized`, `Unit::death_cause`; other kinds and CC0 art fall back to the ordinary fall). Not yet: the shot from the shaman. Dev: `LIGHTNING=x,z` (cells, another tribe's), `LIGHTNING_TICKS=N` |
| Swamp | aimed on the ground like a blast (`Spell::Swamp`, world units), released 6 turns into the cast jump, walked into range first; a normal charge in the sandboxes (not free, unlike Teleport, Flatten and Lightning: `SpellBook::sandbox`). An effect (`swamp`, `EffectKind::Swamp`, 32 000 turns): no terrain change; every 4 turns (`swamp::KILL_EVERY`) it kills outright everyone standing on its 3 × 3 cells (`swamp::cells`), any tribe, even the caster's own, shielded or inside a building (`DamageKind::Swamp`: `effect::hurts` never gates it, unlike blast, lightning, swarm or hypnotise). Several swamps, any player's, can stand on the same cell: `GameMap::tend_swamps` sees every swamp at once and settles a contested cell on the most recently cast one there (`Registry`'s ids only grow, so the highest id is the newest); that one's own tally (`Swamp::processed`) grows, and once it reaches `SWAMP_NUM_PEOPLE` (10) it ends there and then, short of its full lifetime. The owner on each kill's `WorldEvent::Damage` is the claiming swamp's caster, for whatever later counts kills per player (score, mana: not implemented yet). Drawn (`client swamp.rs`, our own look, the original has no world sprite at all for it, only an aerial-map icon): a pale, misty patch a few centimetres over the ground, shaped by a handful of overlapping round lobes seeded from the effect's id (`blob_alpha`) so it reads as an irregular, cloud-like blob rather than the 3 × 3 grid it is built on, with a scatter of flat reed-like leaves lying on it (also seeded, `leaf_offset`/`leaf_yaw`). Not yet: the 30-per-tribe/oldest-goes bookkeeping, ending once most of its cells are unsuitable (pop3-rev-analysis.md "Per spell" 11), and an aerial-map icon. Dev: `SWAMP=x,z` (cells, another tribe's), `SWAMP_TICKS=N` |
| Magical Shield | aimed on the ground like a blast (`hud::spells::ground_spell`), walked into range first, released `CAST_RELEASE` (6 turns) into the shaman's cast jump: at the cast point, not wherever the shaman herself ends up standing (confirmed). `SHIELD_NUM_PEOPLE` (6) nearest own followers in the 3 × 3 cells around the cast point (`nearest::in_block`) — not the shaman herself, not a wildman, not a Ghost Army copy (`UnitKind::FOLLOWERS`, `GameMap::cast_nearest_status`, the nearest-N selector shared with Bloodlust/Invisibility/Hypnotism, `nearest::nearest_n_fresh_first`) — are shielded (`status::Timed::steps(SHIELD_STEPS)`, 180 steps × 8 turns, 120 s). A recast reaches a not-yet-shielded follower before it reaches one that already has it, regardless of distance — the already-shielded one is only touched again (its timer reset to a fresh full duration, not stacked) once there is nobody unshielded left among the nearest 6 (confirmed). Already blocks Blast, Lightning and a firewarrior's bolt (`effect::hurts`); does not block a swamp's kill or a landscape-caused one (confirmed, docs/plans/spells.md). Ties at equal distance break on the lowest unit id (a guess, not traced, kept for lockstep replay safety). Drawn: the existing shield-ring marker (`units/markers.rs`), already wired to `Unit::statuses.shield`. |
| Bloodlust | aimed on the ground like Magical Shield (hidden spell, model 20): `BLOODLUST_NUM_PEOPLE` (6) nearest own followers in the 3 × 3 cells around the cast point are bloodlusted (`status::Timed::steps(BLOODLUST_STEPS)`, 180 steps, 120 s) `CAST_RELEASE` into the jump. Its damage math was already in place from the combat work (`combat::strength`'s ×3 attacker multiplier, `firebolt`'s ×3 bolt damage and its reload `>> BLOODLUST_SW_BLAST_X`, all reading `Unit::statuses.bloodlust` directly) — what was still missing, now added to `GameMap::damage`: a bloodlusted *defender* takes `>> BLOODLUST_HEALTH_SHIFT` (÷8) of an incoming blow or bolt, independently of the attacker's own multiplier; a blast, lightning bolt or swamp kill is untouched (bloodlust only softens a fight, not a spell). No early-end condition, unlike Invisibility: it just runs out. Drawn: the existing red-bubbles marker (`units/markers.rs`), already wired to `Unit::statuses.bloodlust`. |
| Invisibility | aimed on the ground like Magical Shield: `INVIS_NUM_PEOPLE` (6) nearest own followers in the 3 × 3 cells around the cast point turn invisible (`status::Timed::steps(INVIS_STEPS)`, 180 steps, 120 s) `CAST_RELEASE` into the jump. Enemy auto-targeting skips an invisible unit entirely (`spotting::first_seen`'s target filter); a player who already has its id can still order an attack on it directly (untraced either way, left open). Ends early (a guess, not traced, TODO.md): the instant it is ordered to attack anything — on its own via auto-spotting or by the player — its own invisibility ends (`attack::go_attack`); merely walking does not. Not yet: any client-side rendering of invisibility (own people translucent, enemies hidden entirely) — flagged in TODO.md, since it needs per-viewer visibility the renderer does not have yet. |
| Convert | aimed on the ground (`hud::spells::ground_spell`), released like a blast; free charge count from the demo loadout, a normal charge otherwise. An effect (`convert`, `EffectKind::Convert`, 18 turns, confirmed to expire even without a single success): every 8 turns, the nearest living wildman within 2 cells of the cast point joins the caster's tribe (`WorldEvent::ChangeOwner`, the same event preacher conversion uses), if its tribe has room (`headcount::MAX_POPULATION`, the hut-breeding/preacher-conversion cap). `Unit::change_owner` keeps a kind for an ordinary conversion (a warrior stays a warrior) but now turns a wildman into a brave specifically — a wildman has no tribe of its own, so there was nothing to preserve; the reincarnation site's free welcome at level start still spawns its braves directly instead of calling this. Dev: `CONVERT=x,z` (cells, the player's cast, with a wildman placed next to it), `CONVERT_TICKS=N` |
| Hypnotism | aimed on the ground like a blast (`hud::spells::ground_spell`, not self-cast). An effect (`hypnotism`, `EffectKind::Hypnotism`, one turn: the whole spell happens at once, nothing lingers): the `HYPNO_NUM_PEOPLE` (6) nearest people of other tribes around the cast point — not a wildman, not a shaman, not a shielded one — join the caster's tribe for `HYPNO_STEPS` (55) steps × 8 turns (440 turns, 36.7 s): `WorldEvent::Hypnotize`/`GameMap::hypnotize`, which calls `Unit::change_owner_keeping_body` (like an ordinary conversion it resets orders and other statuses, but — confirmed live, 0x50EB00, unlike an ordinary conversion — keeps the captured person's current life and carried wood, both across the capture and across the later revert) and then records `status::Statuses.hypnotized`/`hypnotized_from` on top. A not-yet-hypnotised person sorts before an already-hypnotised one regardless of distance (`nearest::nearest_n_fresh_first`, the selector shared with Magical Shield/Bloodlust/Invisibility, same recast-priority rule): a cast reaches fresh captives first, recapturing an already-hypnotised one (even the caster's own enemy's catch) only to fill what room is left. Recapturing changes who currently controls the unit and restarts its timer, but `hypnotized_from` always keeps the tribe from *before its very first capture* — chasing through as many recaptures by different tribes as happen in between, never overwritten with an intermediate captor. Once the timer runs out (`GameMap::revert_expired_hypnosis`, checked every tick right after `Statuses::tick`), it reverts to that one true original tribe — unless that tribe is wiped out by then (`GameMap::tribe_wiped_out`, the same check the win/lose Outcome model uses), in which case the change stays permanent (confirmed: "if their tribe is gone, they stay"); `hypnotized_from` is cleared either way, so this never fires twice for the same capture. Drawn: the existing hypnosis halo marker (`units/markers.rs`), already wired to `Unit::statuses.hypnotized`. Dev: `HYPNOTISM=x,z` (cells, the player's cast, with an enemy placed next to it), `HYPNOTISM_TICKS=N` |
| Volcano | aimed on the ground like a blast (`hud::spells::ground_spell`); 1 charge. A multi-phase controller effect (`volcano`, `EffectKind::Volcano`, `TOTAL_TURNS` 160 turns): `RUMBLE_TURNS` (80) of nothing but rumbling — drawn client-side as Ground Shake (`ground_shake.rs` [client], `effect::GroundShake`), a camera shake (a kick and a roll on the camera itself, not a trembling terrain mesh) within `SHAKE_RADIUS_CELLS` (6) building towards the rise; its magnitude and reach are our own guess, untraced, shared with Earthquake's own shake the same way `lava` is shared by their aftermaths — then `RISE_TURNS` (59) of the cone rising one `TerrainChange::Volcano` turn at a time (`volcano::apply`/`step`, the usual "ease towards the target, land on it the last turn" shape) — within `RIM_RADIUS_CELLS` (2.5) a real depression (not the trace's "plug": a crater, not a raised feature), down to `CRATER_FLOOR` (0, a guess: no number given, mirroring the rim's own absolute extreme) at the very centre, rising to the engine's own ceiling `MAX_HEIGHT` (1024) right at the rim, then sloping back down to the ground's own height at cast by `SLOPE_RADIUS_CELLS` (5). At `DESTROY_AT` (110, 30 turns into the rise), the `DESTROY_AREA_CELLS` square glows first (`Volcano::pre_glow`, drawn client-side by `lava.rs` [client]'s `VolcanoGlowView` reusing `LavaView`'s own molten look at partial, ramping strength — a guess, untraced: the trace gives no cue for the burst below, otherwise invisible until the crater finishes rising): every building in that area is destroyed outright (`WorldEvent::Destroy` → `GameMap::burn_down`, bypassing the normal wood/wear-down model) and everyone there, open or inside, catches fire (`WorldEvent::Ignite`/`Unit::catch_fire`, confirmed: never an outright kill) but the caster's own shaman (`effect::hurts`'s `DamageKind::Lava` eligibility rule, shared with `lava` itself). Once shaping ends (`SHAPING_TURNS` 139): a `lava::flowing` effect is spawned over the crater's own cells (pop3-rev-analysis.md "Volcano lava flow", confirmed in detail: a downhill-following spread, not a disc interpolated between two radii) — seeded over `INITIAL_RADIUS_CELLS` (1.5), each mature cell (`MATURE_BLEND`) either pushes into its real downhill neighbour (our own reading of the terrain: the original's own per-cell field source is "Open") or, flat, deepens itself; an accumulator stops growth at `GROWTH_TURNS` (192, 16.0 s) or sooner once it passes a cap, then `FADE_TURNS` (16, 1.3 s) of fading, then the whole footprint becomes a permanent ash scorch (`WorldEvent::Scorch`, `GameMap::ashes`, the same 6000-turn hold as a destroyed building's own). Every cell still hot (`IGNITE_BLEND`) sets alight whoever and whatever stands on it, every turn — never an outright kill (confirmed, the same correction as the turn-110 destruction above). Each cell's own drawn alpha fades with its own blend (`LavaView`'s own `blend_alpha`, client-side only) rather than popping to full strength the instant it's seeded or off the instant it's removed, so a growing or cooling edge reads as a gradient, not a hard cutoff; a cell an actual spread step just added additionally flashes a brief directional "pour-in" highlight (`CellArrival`/`pour_in_alpha`, client-side only) sweeping from the corner facing the neighbour it spread from, before settling into that same fade — without it, cells only ever lit up in place, one tile after another, instead of reading as a single flow pouring downhill from tile to tile. `FIREBALLS` (28, "about 22 small and 6 big": mechanically identical, the split is visual only) land one `panicking_blast` at a time at random points in the 6 × 6 area, spread evenly (ceiling division) over the controller's own remaining turns — both outlive the controller once it ends, same "pure scheduler for sub-effects" shape as Firestorm's own. A fireball's own damage is `DamageKind::Blast` (a shield blocks it, same as a plain blast); lava's own ignite is never blocked by a shield (like Swamp). Sparkles while it's still rising (`Effect::reshaping`, same convention as Land Bridge/Flatten). The 26 decorative rocks (pop3-rev-analysis.md "Volcano rocks") are drawn client-side only, no `EffectKind` data of their own (`client volcano_rocks.rs`, `views::WorldView`): one born each of turns 105-130 at a random angle and landing distance 1536-2558 units (3-5 cells) from the centre — the trace's own placement numbers. Drawn as a camera-facing sprite like wood (`wood.rs`): the original's rolling rock (`hfx0-0.dat`, `VOLCANO_ROCK_SPRITES`, frames 346-349 picked by position, `346 + ((x >> 6) + (z >> 6)) & 3`) when the original files are allowed, else our own small procedurally generated stand-in (4 seeded grey-brown blobs, no original art to trace). Motion is our own invented stand-in, not the original's (which has no launch at all — "no initial velocity", created already at rest on the slope at its landing point): ours instead visibly ejects each rock from the crater's own centre, arcing out to that landing point under gravity (`FLIGHT_SECS`, a `peak × 4t(1−t)` parabola scaled by throw distance, the same shape `units::mod.rs`'s boarding hops use), then rolls downhill from there (`downhill_at`, a finite difference on the real terrain under its landing point, same spirit as the trace's own "slides while the slope is above 24" without its exact per-triangle physics — steeper ground rolls it farther, flat ground barely at all, with a friction-like ease-out) before settling and fading after `REST_SECS` ("about 10.7 s", the trace's own rest-before-deletion timer). `Grounded` re-samples the real terrain height under it throughout, so it genuinely falls and rolls over the land's own shape. No damage, no collision, nothing for the simulation itself to do. Dev: `VOLCANO=x,z` (cells, the player's cast), `VOLCANO_TICKS=N` |
| Firestorm | aimed on the ground like a blast; a pure scheduler effect (`firestorm`, `EffectKind::Firestorm`, `DURATION_TURNS` 220 turns, 18.3 s, file-confirmed): every `FIREBALL_EVERY` (4) turns, one `panicking_blast` fireball lands at a random point of the 6 × 6-cell area around the target (`volcano::random_point_in_area`, the shared helper, same shape as Volcano's own second half) — "about 55" fireballs over its lifetime (220 / 4 exactly). Each impact, beyond the fireball's own simple-blast-that-panics (radius 5 cells, confirmed — pop3-rev-analysis.md "Fireball impact"): on land, a `ground_fire` effect at the landing point (confirmed this session: the original creates the exact same scenery object Lightning's own strike does, not Lightning's tree-burns-else-fire branch) and, if a building covers the point, it catches fire (`WorldEvent::SetFire`, same call as Lightning's). A water landing skips the fire entirely, matching the original (`firestorm::impact`'s own `is_sea` check): the splash effect itself is not modelled (sound 0x55, hfx 1152, never traced in detail) — a fireball landing in the sea today spawns no fire and no replacement visual either, rather than a silently wrong fire over water. Every fireball and its fire outlive the controller once it ends (`panicking_blast::VISUAL_TICKS` past its own 3-turn blast, purely so its fireball-shot view below has room to play out). Drawn: the landing flame (`ground_fire.rs` [client]) as before, plus each fireball's own flight (`fireball_shot.rs` [client], `views::WorldView` of `panicking_blast`'s effect, shared with Volcano's own fireballs): a bright ball falling from above, shrinking as it nears the ground (pop3-rev-analysis.md "Firestorm fireballs": the original's own sprite, hfx 1120-1127, shrinks the same way), a few dimmer embers trailing behind it (not `smoke.rs`'s grey puffs), landing in a flash that grows and fades into a last puff of smoke (hfx ~1180-1210, approximate bounds) — our own invented flight and timing throughout, not the original's own speed/fall-distance numbers. Dev: `FIRESTORM=x,z` (cells, another tribe's), `FIRESTORM_TICKS=N` |
| Earthquake | aimed on the ground like a blast; a timed controller effect (`earthquake`, `EffectKind::Earthquake`, `DURATION_TURNS` 120 turns, 10 s, file-confirmed) of shake — drawn client-side as Ground Shake (`ground_shake.rs` [client], `effect::GroundShake`, shared with Volcano's own pre-eruption rumble), a camera shake (a kick and a roll on the camera itself, not a trembling terrain mesh), a quick ramp up, a steady tremor within `SINK_RADIUS_CELLS` (8), a quick ramp down; its magnitude is our own guess, untraced. At `SINK_AT` (60, confirmed) turns in, once: the ground within `SINK_RADIUS_CELLS` (8) sinks by up to `SINK_AMOUNT` (600) — reusing the existing `raise` brush (`WorldEvent::Terrain(TerrainChange::Raise)`, `by: -600`) rather than the trace's own distinct `600 × (4096 − d) / 4096` falloff, an approximation like Erode's own reuse of the same brush — and a lava fissure opens at the centre (`lava::effect`, the "lava square" patch this module was named for, pop3-rev-analysis.md "Volcano lava flow": several short-lived single cells approximated as one patch with a shared blend clock) over `FISSURE_RADIUS_CELLS` (2, a guess: no size given); it decays and self-destructs on its own (confirmed: `MATURE_BLEND` 196 down `STATIC_DECAY` 2 a turn, gone below `GONE_BLEND` 127), no lifetime of our own to guess, scorching its cells into the same permanent ash a destroyed building leaves (`WorldEvent::Scorch`, `GameMap::ashes`) once they're gone, same as Volcano's own flow. The three building-shake checks at turns 24, 56 and 88 are done (`earthquake::Earthquake::shake`, `WorldEvent::Shake`, confirmed 20/128 chance per building within `SHAKE_HALF_WIDTH_CELLS` (8, "16 × 16 cells") of the cast point): a hit costs the building one piece of wood outright (not the ordinary wear-threshold model "Attacking a building" uses — the same direct-loss primitive fire's own burn tick already has), a large building's occupants out regardless (`wreck::large`, `GameMap::throw_out`), and only if that was its last piece, a partial explosion (`GameMap::burn_down`) — confirmed from the trace's own function-level row that the explosion is conditional on the wood running out, not unconditional on every hit. Not yet: the trace's "possibly two lava flows", not confirmed enough to add a second patch. Dev: `EARTHQUAKE=x,z` (cells, another tribe's), `EARTHQUAKE_TICKS=N` |
| Swarm | aimed on the ground like a blast (`Spell::Swarm`, world units). An effect (`swarm`, `EffectKind::Swarm`, `DURATION` 200 turns, 16.7 s): the cloud itself has no terrain change and no drawable body of its own (same convention as Swamp) and wanders within `LEASH_CELLS` (6) of the cast point — steering back once further out, otherwise a fresh random heading every 1-32 turns (`REAIM_TURNS`) — at `SPEED` 80 world units/turn (`crate::angle`). Every `STING_EVERY` (8) turns: every living, open, non-shaman enemy in the 3 × 3 cells around its *current* position (`swamp::cells`, reused) takes `PERSON_DAMAGE` 100 (`DamageKind::Swarm`: no friendly fire, no shaman of *any* tribe — broader than Lightning/Lava's "only the caster's own shaman" rule — and still blocked by Magical Shield, unlike Swamp) and panics unless shielded; an enemy vehicle with passengers in that same area gets `WorldEvent::PutOut` — `GameMap::unload` onto nearby land if close enough to shore, else `GameMap::put_out` where it is (falls from a balloon, drowns off a boat at sea), the TODO.md "Units" vehicle-eviction ask. Up to `MAX_RAIDS` (3) times, every `RAID_EVERY` (64) turns, it raids one enemy building with occupants in reach: `WorldEvent::Raid` throws its people out and panics them (`GameMap::throw_out`, the same primitive fire's own burn-tick uses) and each took the same sting damage. Drawn (`client swarm.rs`, our own look — the original draws up to 60 separately-spawned point-sprites scattered near the invisible cloud, re-centring on its current victim, pop3-rev-analysis.md "Insect plague"; `[ours]` simplification: `BEE_COUNT` (24) camera-facing dots trickling in over the effect's first turns rather than 60 individually-timed ones): a soft round dot each, black (the exact original byte was never decoded, `[ours]`) except `TRIBE_CHANCE` (25%, bumped well past the trace's own "roughly one in sixteen" — near-invisible at the size a dot actually renders at — `[ours]`) tinted the caster's own tribe colour instead (`sites::tribe_color`), each looping its own oval around the swarm's live position rather than a uniform circle (`buzz`: a random long-axis heading per bee, reads as a loose flock banking around the cloud rather than one spiral), hovering `HOVER_HEIGHT` above the ground with a small bob on top. Every bee of every swarm shares one of a handful of pre-built materials (`tribe_material`/`SwarmAssets`, one per tribe plus the neutral tint, built once at `Startup`) rather than a fresh `StandardMaterial` per bee, so same mesh + same material batches on the GPU. Dev: cast through the HUD (`SpellKind::Swarm`), or `SWARM=x,z` (cells, another tribe's), `SWARM_TICKS=N`. |

To do: the other original spells, as traced in pop3-rev-analysis.md "Per spell" (any randomness through `map::Lcg`,
never `rand`). Mana cost,
charges, cast range from shaman (original values
below, "Original balance"). Range: `cast_range` (`SP_W_RANGE` by the caster's altitude band, `GameMap::altitude`,
+1/3 in a finished drum tower), none on a god level. Cast beyond it (`GameMap::in_cast_range`), the shaman walks as close to the target as she can,
reachable or not (land on another island, the sea): to the point nearest it of the cell nearest it among those she
can walk to (`path::nearest_reachable`, `path::closest_point_in_cell`), and casts as soon as she is in range (`GameMap::walk_to_cast`, `cast_when_in_range`;
the original's walk-to-cast state): no charge is used before, another order on the way drops the cast, and she gives
up if she stops short; without a living shaman the spell takes effect at once; nothing is cast
while the shaman is off the ground.
After every cast: update normals, walkability and destroy buildings on uneven/flooded ground.
Spells must only be cast through `Command::Cast` to stay deterministic.

## Effects (`game_core::effect`)
In the original a cast flies a shot to the target, which spawns reusable components there (pop3-rev-analysis.md
"From cast to effect", "Components"); the components do the work. Ours: `GameMap::effects`, a `Registry<Effect>` (by
`Id<Effect>`) of `Effect` (caster's tribe, position, `left` turns, `EffectKind` with only its own data), ticked once per turn before the
units (`GameMap::tend_effects`, architecture.md "The tick"). An effect reads the map and returns `WorldEvent`s (`event.rs`):
`Damage` (with the caster's tribe as `from`, through `GameMap::damage`, the one place for immunities: shielded units, the caster's own tribe for a
blast), `DamageVehicle` (`GameMap::damage_vehicle`, enemies only), `Push` (`Unit::fling`, not the shielded), `SpawnEffect` (a new effect, ticking from the next turn), `Terrain`
(raise, flatten, land bridge). `SetFire` burns a building (`fire`, buildings.md "Damage and repair"). `BurnTrees` sets a cell's trees alight (`Tree::ignite`). `Panic` sets the person running (`Action::Panicking`: out by the door first when inside, then `PANIC_SPEED` 110 at a random heading for `PANIC_TICKS` 64 turns, a quarter turn left when the way is blocked, no orders meanwhile; a guess: the original's turning not traced). `Raid` is routed but does nothing yet. An effect never holds another
object's state: a unit it panics or sets on fire counts down its own state.

## Spell book (`game_core::spell_book`)
Per player (`GameMap::spell_books`, by owner): one `SpellSlot` per `SpellKind` with `Availability` (Hidden / Discoverable / Provided { shots } /
Known / Unlimited: cast at will, used by sandbox spells), `charges` (max 4, fewer for big spells: `max_charges()` from `cost()`, to replace with `SP_1_OFF_MAX`, see
"Original balance") and `recharge` mana.
`charge(mana)` shares mana evenly between the spells taking it (known, not full, not paused: `set_paused`), a
spell filled up passing the rest of its share on, and returns what is left; `cast(kind)` consumes a charge or a
provided shot, `discover(kind)` turns "?" into Known. Integer only, deterministic.
Provided shots (a totem's or a vault's one-shot gift, `worship::one_more_shot`) never recharge and cannot be paused;
each gift adds a shot, none to a spell already known. The last shot used leaves the slot empty (Hidden), even when it
was "?" before the gift: "?" means not discovered, and the gift showed the spell, so it does not come back. The engine owns it: casts
(`Command::Cast` for a spell aimed at a spot, through `Spell::kind`; `Command::CastSelf` for one not implemented yet,
just the cast animation with no effect) use a charge and are ignored without one, `Command::PauseSpell` pauses,
rewards go to their tribe's book. The one remaining terrain brush, `Raise` (`Spell::kind` None: editor and dev
only), uses no book. The view only reads it (ui-and-editor.md).

## Mana (`game_core::mana`)
Per player (`GameMap::mana`, by owner): `ManaPool { bank, incoming, received, used, advisor_eta }`, amounts as
`Mana`. As in the original [exe] (pop3-rev-analysis.md "Mana rules"), two entry points:
- `add(amount)`: mana gained during the turn (a reward) only goes to `incoming`;
- `tick(now, income, book, stores)`, once per turn from `GameMap::tick`: `income` (`GameMap::mana_income`,
  huts-and-training.md "Mana" rule 7) every 16 turns (`MANA_EVERY`), recomputing `advisor_eta` from `book` at the
  same time (below), then `distribute`: a training building making nothing refunds its store first
  (`Production::refund`); half of `incoming` offers the training buildings still making something a share each
  (`production::charge`, huts-and-training.md "Mana"); what they do not take, plus the other half, charges the
  spells first (`SpellBook::charge`); with every spell full, a second share tops the stores back up
  (`production::top_up`); what is left banks (capped at `MAX_MANA`). Only this changes the bank, the charges and
  the stores.

`received` is the last income, `used` what went into spells since then. Starts at `START_MANA`.

Shaman death (`GameMap::shaman_died`, pop3-rev-analysis.md "Shaman death"): caught at the end of whatever tick
kills her (melee, a spell, drowning, stranded, lava...) by comparing alive-before/alive-after, since `Unit::die`
sets `Action::Dying` synchronously wherever damage is dealt rather than through a tick-returned event. Her tribe
loses `SHAMEN_DEAD_MANA_PCT_LOST` (25 %, file and exe agree) of `base` from its bank - `base` is the bank alone
here; the file's own "needed + stored building mana" alternative applies only to a controller-type-2 ("human")
tribe, and no controller type is modelled yet (the same gap `training::train_cost` has). The killer's own share
(`SHAMEN_DEAD_MANA_PCT_GAIN`) is exe-confirmed **0 %, not the file's 25 %** - matching the exe faithfully means no
mana ever actually reaches the killer, so the delayed reservoir it would go through (1000 turns) is left
unimplemented rather than built to move nothing. Not done: a cast's own cost (`incoming -= cost` in the original,
on top of our prepaid charges?), a negative `incoming` taking mana back from the training stores, and the stores
counted into the shaman death loss's own `base` for a human tribe.

The HUD mana bar (`hud/mana.rs`, `ManaPool::advisor_fraction`) is not `bank` against `MAX_MANA`
(pop3-rev-analysis.md "The mana bar (HUD)"): always full with nothing charging (`advisor_eta` zero, from
`SpellBook::charging_candidate`, returning the first charging spell's `SpellStats`), otherwise `received`
against `advisor_eta` scaled ~1.78x (`456 / 256`, confirmed). `advisor_eta` itself is `[ours]`: the exe scales
its own estimate by an undecoded global byte (`0x89D161`); lacking it, `advisor_eta` spreads the candidate's
`advisor_seconds` (`SPELL_<X>_OPT_S`) over that many income periods against its own `cost`, not the exe's own
"share the picker returns" against its own mana scale. A fill/track bar: the tribe's own accent colour
(`sites::tribe_color`) filled, a fixed colour for the rest; no flash (two more undecoded flags, not confirmed
to even be the same signal as each other). In generated/no-original mode `cost` is still our own tiny
placeholder (`SpellStats::default_for`, "Original balance" above), so a cheap spell's own `cost / periods`
still rounds down to 1-2 mana and the bar reads full well before the spell is near its real charge count —
cosmetic only there, `advisor_fraction` doesn't drive the actual charging (`GameMap::mana_income` below does).
With a real install's `constant.dat` loaded (`SpellBook::apply_constants`), `cost` is the real number and this
mostly goes away.

`GameMap::mana_income(tribe)` (`headcount.rs`, huts-and-training.md "Mana" rule 7): sums each living follower's
busy or idle rate (`MANA_BUSY_BRAVES`/`_IDLE_BRAVES` 15/4, `MANA_BUSY_SPECIALS`/`_IDLE_SPECIALS` 5/4 for warrior,
spy, firewarrior, preacher; busy `[ours]` meaning housed or working, `headcount::state_of`) plus the shaman's own
flat `MANA_F_SHAMEN` 30 regardless, then scales by `HUMAN_MANA_ADJUST` (×1.25) — every tribe, human or not: no
AI/controller type exists yet to read `COMPUTER_MANA_ADJUST` instead (same gap as `training.rs`'s `train_cost`).
Replaces the old fixed `mana::INCOME` placeholder, which charged every spell in a single income tick regardless of
population.

`SpellStats` (`cost`, `max_charges`, `advisor_seconds`, `range`): a spell's balance, resolved once per
`SpellSlot` at construction (`SpellStats::default_for`, our own numbers — "Original balance" above except
`cost`, kept as a placeholder, constants.md "Use here"). `GameMap::load_original` then tries the sibling
`constant.dat` next to the level's own `.dat` (`pop3_format::constants::Constants::load`, silently keeping
the defaults on any failure — missing file, wrong format); when it parses, `SpellBook::apply_constants`
corrects every slot's `stats` from the real file (`SpellStats::from_constants`, a key-by-key override: a
partial or modded file only changes what it actually defines), then `fill()` runs again so starting charges
reflect any corrected `max_charges`. No other constructor touches this, so `--no-original` always gets the
defaults.

Two peers resolving `SpellStats` differently (a different install, or one with and one without) would silently
diverge mid-match: `SpellBook::constants_hash()` hashes every spell's resolved `cost`/`max_charges` (not
`advisor_seconds`/`range`, which can't desync anything - HUD pacing and a locally-checked cast range) for
comparison at game start (`game_net::Message::Handshake`, multiplayer.md "Wire format"). Not yet wired into an
actual join: there is no network driver or host/join flow to call it from yet (TODO.md "Network driver").

`SpellBook::from_level(header, level)`: the header's `SpellsAvailable` panel spells are Known with full charges
(how charged the original starts them is not checked), the spell discoveries (`DiscoveryType 11`) not already
known are Discoverable, each slot keeping its own thing's availability (`SpellSlot::discovery_availability`, see
"Discovery things" below for the byte) for `discover` to read once it is actually found: a `Once` discovery
becomes `Provided { shots: 1 }` (a single free use, same as Volcano's own `demo()` loadout), `Permanent` and
`ThisLevel` both become `Known` - there is no cross-level save/progression system yet to make them differ from
each other, so a discovery's `Known` state today never survives past the level it was found on regardless of
which of the two it is. Given to every
player of an original level (`GameMap::spell_books`); generated maps and levels without a header get
`SpellBook::demo` (every tile state), the sandboxes `SpellBook::sandbox` (plus Teleport, unlimited). Armageddon (bit 18) is special: the
campaign's last spell, discovered in levels 17 and 18 and in the masks from level 19 on, always with Convert (17).
Levels 1 and 2 also set bit 18, among leftover bits (1 burn, 20 bloodlust, 21 teleport) and without Convert:
`known_in_header` only takes Armageddon next to Convert. The panel keeps a tile for it.
Convert follows the same campaign path: absent from levels 1-3 (level 1 has no wildmen; levels 2 and 3 only have
the wildmen around the player's site, converted for free, see units.md), single-shot discoveries
in levels 4 and 5, a permanent one in level 6, in every mask from level 7 (except level 22, the shaman alone).

## Original spells (levels and scripts)
Spell model numbers, as used by discovery things, the level header masks and AI scripts (names from the ALACN
world editor, checked against the campaign's discoveries, see level-format.md):

| Model | Spell | Model | Spell | Model | Spell |
|---|---|---|---|---|---|
| 1 | burn | 8 | firestorm | 15 | flatten |
| 2 | blast | 9 | ghost army | 16 | volcano |
| 3 | lightning | 10 | erode | 17 | convert |
| 4 | tornado (whirlwind) | 11 | swamp | 18 | armageddon |
| 5 | swarm (insect plague) | 12 | land bridge | 19 | magical shield |
| 6 | invisibility | 13 | angel of death | 20 | bloodlust |
| 7 | hypnotism | 14 | earthquake | 21 | teleport |

`SpellKind::model()` / `SpellKind::from_model()` map them; `from_model` returns every spell but Teleport (model 21,
kept only for reference: our `Teleport` is a sandbox spell) and Burn (model 1, no `SpellKind` of its own): models
2-19 are the panel's 18 spells, plus Bloodlust (20) so a discovery thing can still grant it, even though
`known_in_header` keeps the header's own mask from ever giving it (TODO.md "Bloodlust"). Our costs
(`SpellStats::cost`, "Mana" below) are placeholders on another scale (blast 40) even when a real install's
numbers load, by design; the original ones are under "Original balance".

The editor also names models 23-30 (hill, rise, valley, dip, place tree, clear mapwho, place shaman, place
wild) but can't place them: unverified.

How a level grants spells:
- **Level header** `SpellsAvailable`: bit N = spell N, known from the start. `SpellsNotCharging` uses the same bits.
- **Discovery things** (general model 2, `DiscoveryType 11`, `DiscoveryModel` = spell): availability 1 permanent,
  2 this level, 3 once ("one shot"). A spell discovered in a level shows up in the start mask of later levels.
- **Mana discoveries**: `DiscoveryType 6`, model 3, 50 000 mana.
- **AI scripts** can give spells or one-shots and set spell costs (see ai-scripts.md).

Land bridge effects in the levels store their target cell (see level-format.md).

## Original balance (`constant.dat`) [files]
Decoded from `levels/constant.dat`, see constants.md for the file. Names are `P3CONST_` + the column's prefix and
the spell's suffix.

| Model | Spell | Suffix | Cost `SPELL_` | Charges `SP_1_OFF_MAX_` | `SPELL_x_OPT_S` | Range `SP_W_RANGE_` (cells) |
|---|---|---|---|---|---|---|
| 2 | blast | `BLAST` | 10 000 | 4 | 30 s | 3072 (6) |
| 3 | lightning | `BOLT` | 80 000 | 4 | 180 s | 6144 (12) |
| 4 | tornado | `WWIND` | 90 000 | 3 | 180 s | 4096 (8) |
| 5 | swarm | `PLAGUE` | 40 000 | 4 | 45 s | 6144 (12) |
| 6 | invisibility | `INVIS` | 50 000 | 4 | 120 s | 4096 (8) |
| 7 | hypnotism | `HYPNO` | 85 000 | 3 | 240 s | 4096 (8) |
| 8 | firestorm | `FIREST` | 400 000 | 2 | 180 s | 4096 (8) |
| 9 | ghost army | `GARMY` | 18 000 | 4 | 60 s | 4096 (8) |
| 10 | erode | `EROSION` | 210 000 | 2 | 240 s | 4096 (8) |
| 11 | swamp | `SWAMP` | 100 000 | 3 | 180 s | 4096 (8) |
| 12 | land bridge | `LBRIDGE` | 70 000 | 4 | 180 s | 5120 (10) |
| 13 | angel of death | `AOD` | 510 000 | 1 | 300 s | 3072 (6) |
| 14 | earthquake | `QUAKE` | 175 000 | 2 | 240 s | 4096 (8) |
| 15 | flatten | `FLATTEN` | 125 000 | 3 | 240 s | 4096 (8) |
| 16 | volcano | `VOLCANO` | 800 000 | 1 | 300 s | 3072 (6) |
| 17 | convert | `CONVERT_WILD` (`CONVERT` for charges) | 10 000 | 4 | 300 s | 8192 (16) |
| 18 | armageddon | `ARMAGEDDON` | - | 1 | 60 s | - |
| 19 | magical shield | `SHIELD` | 60 000 | 4 | 120 s | 4096 (8) |
| 20 | bloodlust | `BLOODLUST` | - | 4 | 120 s | 4096 (8) |
| 21 | teleport | `TELEPORT` | - | 4 | 120 s | 65 536 (128) |

Reading the columns:
- Cost is the mana of one cast ("Mana cost of firing spell" in the file's comments). The AI scripts read it as
  `INT_M_SPELL_x_COST`.
  - Armageddon, bloodlust and teleport have no cost entry.
  - Mana: start 30 000, cap 1 000 000, updated every `MANA_UPDATE_FREQ` + 1 = 16 turns. Income is scaled by
    `HUMAN_MANA_ADJUST` 125 % for humans and `COMPUTER_MANA_ADJUST` 50 % for computer players.
  - A shaman's death loses 25 % of her tribe's mana, and her killer gains 25 % (`SHAMEN_DEAD_MANA_%_LOST/GAIN`),
    after a 1000-turn delay [exe].
- Charges: `SP_1_OFF_MAX` ("one-off max") is how many charges a spell holds, the panel's 1-4 count [exe]. Fixed:
  `SpellStats::default_for`'s `max_charges` now reads this table directly (`game_core::spell_book`), no longer
  derived from the cost (which used to get tornado, hypnotism, swamp and flatten wrong, at 4/4/2/4 instead
  of the file's 3 each).
- `_OPT_S` is in seconds per the comments. It is not a recharge or cooldown time: the original only reads it in the
  advisor's mana bookkeeping, and no per-spell cooldown exists, only a 1 s cooldown after any human cast [exe].
- Range is the cast distance from the shaman in world units (512 per cell).
  - It is scaled by the caster's altitude: `ALT_BAND_0..7_SPELL_INCR` give 80, 90, ..., 150 % from the lowest
    band to the highest.
  - Firewarriors' shots use the same scaling (`ALT_BAND_x_SUPER_INCR`).
  - The caster's band is `clamp(z >> 7, 0, 7)`, 128 height units per band, interpolated linearly to the next
    band [exe]. A shaman in a finished drum tower gets +1/3.
  - Teleport's 65 536 (128 cells) is the farthest any point can be along one axis of the 256-cell torus, so it
    reads as "no limit". Every other range is 6-16 cells. The original checks the range in the interface and the
    shaman's walk-to-cast, not again when the command runs [exe]; our sandbox teleport has no range limit.
  - Convert has the longest real range (16 cells), and blast, angel of death and volcano the shortest (6).

Effect constants:
| Constant | Value | Meaning |
|---|---|---|
| `HYPNO_COUNT_X8`, `INVISIBLE_COUNT_X8`, `SHIELD_COUNT_X8`, `BLOODLUST_COUNT_X8` | 55, 180, 180, 180 | duration in turns / 8 (440, 1440 turns) |
| `HYPNO_NUM_PEOPLE`, `INVIS_NUM_PEOPLE`, `SHIELD_NUM_PEOPLE`, `BLOODLUST_NUM_PEOPLE` | 6 each | people affected per cast |
| `LIGHTNING_NUM_KILLS` | 6 | at most this + 1 people killed in the target cell [exe] |
| `SWARM_PERSON_DAMAGE` | 100 | |
| `FIRESTORM_DURATION` | 220 | turns (18.3 s): one fireball every 4 turns [exe] |
| `LAND_BRIDGE_MAX_CHANGE`, `LAND_BRIDGE_DURATION` | 256, 64 | height change limit, build time |
| `AOD_KILL_COUNT`, `AOD_DURATION`, `LIFE_AOD`, `BLAST_DAMAGE_AOD` | 40, 2500, 10 000, 10 | the angel: kills, lifetime (turns), hit points; the exe's matching constant is `SW_BLAST_DAMAGE_AOD` (firewarrior shots against an angel, % of `SW_BLAST_DAMAGE`), so this unprefixed file name may not apply (unverified) |
| `BLOODLUST_DAMAGE_X`, `BLOODLUST_HEALTH_X`, `BLOODLUST_SW_BLAST_X` | 3 each | multipliers |
| `DME_RESTORE_TIME` | 6000 | unknown |
| `LSME_DURATION_SECS` | 120 | length of the timed landscape-shaping mode, where erosion, land bridge and flatten get 5 charges [exe] |
