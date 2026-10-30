# Roadmap

Status of each feature area, grouped by the same priority tiers as [TODO.md](../TODO.md) (which has the
full, current task breakdown — this page is the overview). `docs/specs/*.md` has the per-area design and
trace notes.

- **Done**: implemented and matching the original as far as traced, or not applicable (e.g. our own UX).
- **Partial**: the core mechanic works; gaps, placeholders or known disparities remain.
- **Todo**: not implemented, or only scaffolding/parsing exists.

## Priority 1 — core gameplay correctness

| Area | Status | Notes |
|---|---|---|
| Combat | Partial | Melee, Challenge, Sermon, blast push/flight and physics traced and implemented. Open: fight poses, some spotting/joining-a-fight edge cases, firebolts' remaining pieces, death/healing details. See [combat.md](specs/combat.md). |
| The tick / coordinators | Partial | Coordinator infrastructure (fights, sermons, formations) done; order model and several tick rhythms still guessed or unported. See [coordinators.md](specs/coordinators.md), [architecture.md](architecture.md) "The tick". |
| Units | Partial | Shaman and followers walk/pray/cast/drown/die. **Spies are not implemented** (disguise, unnoticed attacks). Braves/warriors/preachers/firewarriors still mostly stand-in behaviour. Wildmen idle loop, death/spirit variants, physics and vehicle boarding have open items. See [units.md](specs/units.md). |
| Poses / animations | Partial | Many actions still use a fallback pose (idle/fall) instead of their own. Missing or stand-in: fight poses (strike/trade/kick/reel/shoved/taunt), wood-carrying poses, tumbling (`flung`/`tumble`), hammering, jump-for-flattening, and several transitions. See [unit-art.md](specs/unit-art.md). |
| Spells | Partial | Most spells work at the core-mechanic level (cast, charge, mana); several still lack the shaman's shot flight and the fireball/building-damage pieces. Per-spell breakdown below. See [spells.md](specs/spells.md). |
| Mana | Partial | Pool, per-follower income, HUD bar shape all match the trace. **Not implemented: mana dispatched between tribes** (the delayed reservoir — gifts, and the shaman-death transfer to the killer's tribe). Also open: distribution-sharing nuances. See [spells.md](specs/spells.md) "Mana". |
| Buildings | Partial | Construction, foundation collapse (uneven/flooded ground), vehicle huts and training huts implemented. Jail/prison needs work (locking, what protects the shaman). Dismantling and explosion are still an approximate whole-model reveal, not the real per-part animation. Many smaller gaps (repair, add-ons, tower effects, construction pace). See [buildings.md](specs/buildings.md). |
| Wood / trees | Partial | Cutting, carrying and construction are done. Open: obstacle behaviour, regrowth rules, trees submerged by terrain changes. See [trees.md](specs/trees.md). |
| Praying (worship) | Partial | Pyramids and basic totem worship work for the player. Open: contested totems, totem gift handling, prisons, HUD feedback. See [worship.md](specs/worship.md). |

### Spells, per spell

| Spell | Status | Notes |
|---|---|---|
| Blast | Partial | Push/damage/tumble physics match the trace. Missing: building damage (buildings have no health model for it yet), the fireball shot flying from the shaman first, angels of death/ghosts. |
| Lightning | Partial | Kill, building fire and tree burn implemented. Open: whether the strike itself should panic nearby survivors (reported vs. trace mismatch, needs a live check), the carbonized-victim animation, the shaman's own shot. |
| Whirlwind (tornado) | Todo | Not implemented at all (only specced: puts a vehicle's crew out, spins it, explodes like a destroyed boat). |
| Swarm (insect plague) | Partial | Wandering cloud, stinging, raiding and vehicle eviction implemented. Open: the 30-per-tribe/oldest-first cap, an aerial-map icon. |
| Invisibility | Partial | Targeting exclusion and early-end-on-attack implemented. Missing: any client-side rendering (own units translucent, enemies hidden entirely). |
| Hypnotism | Partial | Capture, recast priority, revert-on-timeout and the "stays permanent if the original tribe is wiped out" rule are all implemented and fairly settled. |
| Firestorm | Partial | Fireball scheduling and ground-fire/building-fire impacts implemented. Missing: the original's water-landing splash variant (we always light a fire instead). |
| Ghost Army | Todo | Not modelled at all — no status variant wired to any spell yet. |
| Erode | Partial | Falloff-pass approximation implemented and judged good enough, not a full per-cell hydraulic simulation. |
| Swamp | Partial | Kill tick and contested-cell (newest-wins) handling implemented. Open: the 30-per-tribe/oldest-expires bookkeeping, ending when most cells go unsuitable, an aerial-map icon. |
| Land Bridge | Partial | Full build-over-time raise/lower effect implemented with sparkles. Missing: re-levelling or collapsing buildings caught on it. |
| Angel of Death | Todo | Not modelled at all. |
| Earthquake | Partial | Shake and the lava fissure implemented. Missing: the three building-shake/occupant-raid checks during the quake. |
| Flatten | Partial | Disc-levelling implemented. Open: the original's real per-turn curve (ours is a guess), buildings on it not re-levelled. |
| Volcano | Partial | Extensively implemented (rumble, rising crater, building destruction, lava flow, fireballs, decorative rocks); several numbers and the rocks' launch physics are our own invented stand-ins, not traced. |
| Convert | Partial | Wildman-to-brave conversion implemented. Open: giving a single-shot "once" discovery only one use when first granted. |
| Armageddon | Partial — v1 only | Today only pauses combat and decides the winner instantly by headcount. Still to build: the real map wipe and arena respawn, the two-phase gather/fight sequence, per-tribe shields, lightning-kill-without-reincarnation, and the client presentation. |
| Magical Shield | Partial | Targeting/recast priority and blocking blast, lightning and firewarrior bolts all implemented; close to complete. |
| Bloodlust | Partial | Damage/defence multipliers implemented (hidden spell). Open: which levels actually grant it. |
| Teleport | Todo — sandbox only | Exists only as a dev/sandbox command for the shaman; not wired up as a real panel spell, and which original spells (including this one) we expose is undecided. |

Burn (model 1) and Raise (the editor's own terrain brush) are internal mechanics, not player-cast panel spells, and aren't tracked above.

## Priority 2 — computer players (AI) and what it needs

| Area | Status | Notes |
|---|---|---|
| Tribes | Todo | No `TribeId` model yet (owners are still bare bytes); per-tribe state (colour, allies, human/computer, mana/books) lives ad hoc on `GameMap`. |
| Level scripting (triggers) | Todo | Trigger layout is parsed; semantics (what each type waits for, what activating one does) not implemented. |
| Computer players (AI) | Partial — a first opponent plays | `game-core/src/ai.rs`'s native engine runs a priority sweep for each computer tribe (construct essential buildings, train people, post defenders) and `DO ATTACK` sends fighters at a target; `ai_script.rs` wires `STATE_*`/`INT_ATTR_*`, `SET_DRUM_TOWER_POS`/`BUILD_DRUM_TOWER`, `TRAIN_PEOPLE_NOW` and `ATTACK` to it. Flybys and reincarnation on/off also wired. Still missing: shaman auto-cast, garrison/vehicle tasks, scripted patrol/guard groups (need the level markers table), the reactive counter-attack, and most other `DO` commands. See [ai-scripts.md](specs/ai-scripts.md) and TODO.md "Computer players (AI)". |

## Priority 3 — remaining fidelity and content

| Area | Status | Notes |
|---|---|---|
| Terrain / world engine | Partial | Original levels parse and render: wrapping heightmap, curved "planet" mesh, theme-accurate ground textures, dirty-rect re-bakes. Known disparities still to address: projection, cell/selection highlighting, beatbox and flyleaf effects. **Frog and war frog are not implemented.** See [terrain.md](specs/terrain.md), [terrain-textures.md](specs/terrain-textures.md). |
| Winning and losing | Done (core) | Automatic elimination/outcome detection, victory/defeat screen, end-of-match freeze and winner's shield all implemented. Level-script-triggered win/lose waits on the scripting interpreter (Priority 2). |
| Things and level data | Partial | Things and triggers partially parsed and placed. Open: level names/language texts, object banks other than 0, start camera from the header, sunlight block. See [level-format.md](specs/level-format.md), [objects.md](specs/objects.md). |

## Priority 4 — presentation and UX polish

| Area | Status | Notes |
|---|---|---|
| UI and editor | Partial | HUD tabs and UX flows (build/spell panels, editor brushes) are implemented. **Not implemented: the visual design does not yet vary between the proprietary (original-asset) and open-source (generated) look** — same layout/art treatment today regardless of mode. Also not implemented: the "?" indicator for a not-yet-unlocked discovery on the Build tab. Notifications, building icons/tooltips and several original shortcuts are also still open. See [ui-and-editor.md](specs/ui-and-editor.md). |
| Tooltips | Partial | Core hover tooltips exist; coverage against the original is incomplete. See [tooltips.md](specs/tooltips.md). |
| Menu and sandboxes | Partial | Main menu exists but does not stick to the original's own layout/flow; per-feature sandboxes (walk/units/buildings/worship/spells/vehicles/bridge) exist for dev use. **Not implemented: tutorial and credits screen.** |
| Camp fire | Todo | Not implemented. See [buildings.md](specs/buildings.md) "Camp fire". |

## Priority 5 — performance and engine infrastructure

| Area | Status | Notes |
|---|---|---|
| Effects (VFX) | Partial, mostly de-prioritized | Construction, vehicle and status visuals exist. Spell/combat effects — fire trail, smoke, explosion, lightning bolt look, whirlwind — are largely placeholder or missing; original alpha-sprite effects not yet mapped frame-by-frame. See [sprites.md](specs/sprites.md). |
| Performance (60 fps) | Partial, ongoing | Dirty-rect terrain texture/mesh/walkability rebakes landed. Broader scaling work (many units, flow fields) still open. |

## Priority 6 — `--no-original` asset completeness (copyright safety)

De-prioritized: lower priority than the gameplay/content/UX tiers above, since `--no-original` mode already
runs, just with some original-only art/audio left.

| Area | Status | Notes |
|---|---|---|
| Sound | Todo | Format reverse-engineered but not yet parsed/loaded; no playback and no CC0 sound assets sourced at all. See [sound.md](specs/sound.md). |
| Non-original (CC0) assets | Partial, de-prioritized | Generated building kit, textures, and CC0 trees/unit sheets ship. Still original-only: building icons, ritual scenery (reincarnation stones), vehicles, worker action clips. See [assets.md](specs/assets.md). |
| Original sprites / blending | Partial | Alpha sprite table decoded and blended on GPU; per-effect frame mapping and the ghost (see-through) table still open. See [sprites.md](specs/sprites.md). |

## Priority 7 — multiplayer

Lowest priority: the lockstep codec already works in tests with no UI or dependents yet, and multiplayer
only matters once the single-player game above it is in better shape.

| Area | Status | Notes |
|---|---|---|
| Multiplayer | Todo — skeleton only | Lockstep wire codec and a TCP round-trip test exist, single-player only. No network driver, lobby/reconnect UI, desync checksum, or synced pause/speed yet. See [multiplayer.md](specs/multiplayer.md). |

## Next

See [TODO.md](../TODO.md) for the detailed, line-by-line task list this table summarises.
