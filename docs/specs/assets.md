# Open asset inventory

Audit: 2026-10-07, starting at `4e1533e`. This inventories the shippable, original-data-free
path, including artwork blocked by unimplemented gameplay. It is not an inventory of EA's files.
Original textures, sprites, models and installed game data were not used for this work.

The original game's art is copyrighted: never ship it, only read user-owned level data.
When the user's install is present, original 3D objects and their atlas are read at runtime
(`original_models.rs`, see objects.md); every use must keep a generated fallback for `--no-original`.

## Original-data-free entry point

`just run-generated` passes `--no-original`; `POP3_NO_ORIGINAL=1` does the same.
`main.rs::Options` sets `use_original=false`. `world.rs::WorldPlugin` then uses
`LevelList::generated_only()` instead of install discovery: empty paths/list, `original=false`,
generated map/terrain theme and no `OriginalObjects` bank. Explicit level arguments are ignored.
Buildings, trees, sites, unit sheets, wood and cursors each have independent fallback paths.
Sky uses a plain clear colour without an original theme. Terrain now explicitly checks
`levels.original` too, even if a future generated map supplies a theme number.

Generated maps contain no village; use **Sandbox > Buildings** to see buildings and their stages:

```sh
POP3_START=sandbox-buildings just run-generated
POP3_START=sandbox-buildings FOCUS=64,76 DISTANCE=26 PITCH=45 YAW=180 \
  just shot buildings.png --no-original
```

## Inventory and remaining deliverables

Status: **usable**, **placeholder**, **missing**. Source paths are
relative to `crates/game-client/src/` unless prefixed with `assets/`.

| Family | Current generated-mode coverage | Remaining art / integration | Priority |
|---|---|---|---|
| Buildings | **Usable**: generated kit for every named kind (`generated_buildings.rs`, `assets/3d/buildings/`); unknown IDs stay labelled boxes | Icons, animation, coherent assembly chunks, damage/rubble, style variants, add-ons (see below). | P1 |
| Construction | **Usable**: blueprint grid, authored timber scaffold per kit model (unknown IDs keep the box scaffold), height-sorted build bands, shake, smoke (`construction.rs`) | Coherent part-by-part assembly, rubble, fire/damage and production animation. Actual construction simulation remains planned. | P0 / P1 |
| Building UI | **Missing**: 8 named build tiles (`hud/build.rs`) | 8 clear icons, upgrade/dismantle/repair and occupancy indicators; building picking/tooltips are separate gameplay work. | P1 |
| Followers | **Usable temporary style**: 6 CC0 Quaternius characters, 6 poses each, 8 directions, baked into one atlas per kind by `unit-baker` (`units/sheets.rs`, `assets/units/`) | Original project-specific clothing/silhouettes; real prayer, swimming/drowning, stranded, chop/carry/build, melee, fire throw, preaching, disguise, boarding clips. See [unit-art.md](unit-art.md). | P1 |
| Wildmen | **Placeholder**: brave atlas recoloured neutral (`units/sheets.rs`), procedural figure as backup | Distinct neutral character, idle/gesture, wandering/food/drink/conversion art and behaviours. | P2 |
| Vehicles | **Missing** at this revision: no client vehicle renderer | Model/sprite sets for boat and balloon, passengers, paddling, launch/landing and destruction; vehicle entity/boarding integration. A boat-hut jetty or balloon-hut gantry is scenery, not a vehicle. | P1 |
| Reincarnation site | **Placeholder**: 8 tinted cuboids (`sites.rs`) | Original carved-stone set, dormant/active/rebirth treatment. | P1 |
| Ritual scenery | **Missing** | Totem, stone head, discovery marker, worship gauges and effects; trigger/worship integration. Vault building is covered by first kit. | P1 |
| Trees | **Usable**: 10 CC0 tree models, scaled for 4 wood sizes (`nature.rs`, `assets/3d/nature/`) | Palms/tropical/other biome variants; cut stump, cutting/falling/burning states, optional low-detail models. | P2 |
| Wood | **Usable**: CC0 log-pile sprite (`wood.rs`, `assets/sprites/wood_pile.png`) | Carried log and chopping chips, attachment to worker animation. | P1 |
| Other scenery | **Missing** | Plants, shrubs, rocks, pillars, decorative fire and shoreline props; placement/rendering for these categories. | P2 |
| Building add-ons / wildlife | **Missing**, roles not implemented | Hut extension props; optional eagle/fish/other wildlife models and animation if these unsupported creature kinds are adopted. | P2 |
| Terrain / water | **Usable**: deterministic palette/noise theme (`procedural_theme.rs`, `terrain_texture.rs`) | More biome palettes, cliffs, lava/swamp surfaces, coast foam/waves and water motion; terrain deformation already works. | P2 |
| Sky | **Placeholder**: flat sky-to-space colour (`sky.rs`, `camera.rs`) | Generated cloud dome and weather. | P2 |
| Spell VFX | **Missing** except cast jump and basic unit effects | Blast, lightning, swarm, whirlwind, invisibility, hypnotism, firestorm, ghost army, erosion, swamp, land bridge, angel of death, earthquake, flatten, volcano, convert, armageddon, shield, teleport. Meshes/particles/sprite loops plus timing/hooks. Angel needs a creature and animation. | P1 |
| Unit feedback | **Usable basics**: selection rings, shadows, dust, health bars | Low-health stars, conversion/prayer feedback, hit/death effects, footprints, ripples. | P2 |
| Spell UI / cursors | **Placeholder**: named tiles; generated arrow and generic gold spell ring (`virtual_cursor.rs`) | Distinct spell icons/cursors, disabled variants and ability badges. | P1 |
| Menus / HUD | **Usable basic Bevy UI** | Consistent original panel skin, action icons, portraits, stats graphics, loading/title art; stats functionality is separate. | P2 |
| Audio | **Missing**, no playback implementation | CC0 or newly authored chopping/building, footsteps, water, combat/spells, nonverbal follower acknowledgements, ambience, drums and music; mixer/events/loops. | P1 |
| Campaign / presentation | **Missing** as shippable authored content | Original maps, tutorials, scenario scripts, text, discovery messages, victory/defeat presentation. Procedural islands and sandboxes already work. | P2 |

P0 = first building deliverable; P1 = next gameplay readability work; P2 = world/presentation breadth.
Do not treat an asset as missing just because it is generated: procedural terrain and CC0 trees already ship.
Do not mark gameplay implemented merely because its art exists.

## Building kit design and delivery (implemented)

The first pass replaced the tribe-coloured placeholder boxes: **16 generated GLBs now ship**,
covering every named `BuildingKind`, with separate scaffolds and runtime tribe recolouring.
Unknown IDs retain diagnostic boxes. Remaining building work is icons, animation, coherent assembly
chunks, damage/rubble, style variants and add-ons. Construction gameplay is still planned.

[Blender contact sheet](../images/building-kit.png) ·
[Textured in-engine close-up](../images/building-textures.png) ·
[In-engine construction stages](../images/building-stages.png)

Original stylised timber, clay, thatch and stone architecture; no traced game silhouettes or textures.
Use a limited warm palette, dark door recesses, substantial eaves and saturated tribe banners/bands.
Distinct function cues: domestic round huts; tall lookout with drum; stepped ceremonial temple;
compact dark spy lodge; crossed training poles; fire braziers; open boat shed with rear jetty;
wide balloon workshop with gantry; stone vault; barred prison. Reconversion/wall/gate/guard-post
designs are project interpretations of known enum names, not claims about unused original content.

A reproducible, procedurally textured glTF kit lives under `assets/3d/buildings/`, generated from
`tools/generate_buildings.py` and `tools/building_textures.py` (Python standard library). A shared 512 x 512
grayscale atlas adds timber grain, straw, clay, masonry and woven cloth; pigment/tribe colours stay separate.
Every GLB links the atlas next to it (`surfaces.png`); UVs and texture apply to construction frames/parts too.
glTF imports into Blender for hand editing;
Blender is useful for reviewing silhouettes but is not required for regeneration or runtime.
One cell = one model unit, Y up, origin on ground at the engine's building centre, entry local -Z,
boat jetty +Z. Respect `BuildingKind::footprint` dimensions/offsets. Neutral materials retain
their colour; only the material named `Tribe` is recoloured. Include a separate timber scaffold.
Models must keep the existing blueprint, construction/dismantling, shaking and smoke paths.

Acceptance: every named kind has a distinct nonempty model; finite outward-wound geometry;
footprint alignment; all 4 tribes plus neutral; empty/partial/full construction; reproducible
generation; headless screenshots in generated mode; workspace tests with install discovery
redirected to an empty directory. Add provenance to `assets/CREDITS.md`.

### Verification of this first pass

- `python3 tools/generate_buildings.py --check` (part of `just test`): atlas pixels and all 16 GLBs
  reproducible. GLBs are compared by content, floats within 1e-5, since another platform's `math`
  library may round them differently (`tools/test_generate_buildings.py`).
- `POP3_INSTALL=<empty dir> cargo test --workspace`: workspace tests,
  including geometry/footprint checks, tribe/neutral recolouring, every construction wood count,
  non-collapsed UVs inside the correct atlas gutters, GLBs linking the shared atlas and mip data.
  The optional installed-level test takes its no-install path; no original game files are required.
- Blender 5.2.2: imported all 16 GLBs and rendered the contact sheet; optional review `.blend` saved.
- Bevy headless `--no-original`: inspected the complete-kind gallery and construction rows,
  including blueprint, bare scaffold, partial/full, dismantling, attacked and occupied states.
  Stages screenshot: `FOCUS=66,40 DISTANCE=44 PITCH=65 YAW=180 POP3_START=sandbox-buildings`.
  Texture close-up: `FOCUS=63,71 DISTANCE=13 PITCH=40 YAW=180 POP3_START=sandbox-buildings`.
- Known baseline issue: Bevy 0.19.1 / Mesa 26.2.4 on Intel RPL-P logs
  `Use-after-free: attempted to copy element data for an unallocated key` during headless startup.
  Reproduced with the unchanged `4e1533e` snapshot and its box buildings as well as this kit;
  screenshots complete. Tracked separately in TODO, not attributed to the GLBs.

## Text-only research

Consulted 2026-10-07 for building *roles*, not visual copying (no linked images downloaded):
- [Community building overview](https://ts.popre.net/websites/poptb.com/guide/getting-started/buildings-structures/index.html):
  eight major constructible types, wooden construction, three domestic hut sizes, training/transport roles.
- [Community building guide](https://ts.popre.net/websites/poptb.com/guide/basic-gameplay/building/index.html):
  visible plans, flattening, wood delivery, upgrading and repair motivate readable stages/doors.
- [Warrior hut](https://wiki.popre.net/Warrior_Training_Hut) and
  [boat hut](https://wiki.popre.net/Boat_Hut): training and shoreline production roles.

These descriptions inform the brief only; simulation constants continue to come from existing specs.
Existing CC0 Quaternius models remain credited in `assets/CREDITS.md`. Possible future sourcing:
[Kenney](https://kenney.nl/assets), [Quaternius](https://quaternius.com/); check each pack's licence.
