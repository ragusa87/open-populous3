# Camera, UI and editor

## Camera (`camera.rs`)
`CameraRig { focus, yaw, pitch, distance, aerial }`. Focus is in cell units, wrapped in `[0,128)`.
Like the original: scrolling happens only with the cursor pressed against the window border; speed
comes from how hard the mouse keeps pushing outward (raw motion while stuck at the edge, `edge_push.rs`),
slows down while holding still, stops when leaving the edge. The cursor is confined to the window
(Esc toggles). The panel does not block it, Left/Right rotate, Up/Down or WASD move (A/D strafe)
forward/back like the mouse, middle-drag rotates, Home/End tilt, Ctrl+PgUp/PgDn zoom, Shift+PgUp/PgDn field of view (values shown in the HUD), Enter toggles
aerial (pitch 1.35, distance 115) and restores the previous ground view (default tilt 3 deg, distance 20,
fov 60 deg, tuned by eye against the original). Clear color fades from sky blue to space when zooming out. Changing level frames a
low inland cell (`Heightmap::lowland_cell`).
The eye orbits the ground point at the focus, but never goes lower than the map's highest ground plus 1.5 cells
(`eye_position`, `EYE_CLEARANCE`), like the original's fixed camera elevation: over low ground (by the sea) it
stays up there and looks down more steeply, so cliffs between it and the focus never hide it inside the land.
H flies the camera to the player's reincarnation site (nothing on maps without one), Space (or a click on her
preview) to the shaman: a quick 0.4 s flight (`CameraRig::fly_to`, eased, the short way around the torus) rather
than a jump; pushing the camera by hand cancels it.

## Left panel (`hud/`)
Fixed 204 px panel on the left, like the original: shaman preview at the top (`hud/shaman.rs`: her current
sprite x2, same pose and view as on the map, what she is doing, e.g. "Praying" / "Reincarnating in 2s", and a
health bar with `hp/max`; clicking it looks at her), 3 tabs
(Spells / Build / Stats, `ActiveTab`), one content node per tab (`TabContent(n)`), info line under
the grid.

### Spells tab (`hud/spells.rs`)
Mirrors `game_core::spell_book::SpellBook` (resource `PlayerSpells`). An original level brings its own loadout
(`level_book` = `GameMap::spell_book`, set on New game and PgUp/PgDn): the header's spells known and full, its
spell discoveries as "?" (spells.md). Generated maps get the demo loadout (`demo_book`, every tile state); the
sandboxes add Teleport, unlimited (`sandbox_book`). One tile per `SpellKind`, Hidden ones as empty slots.
Armageddon is special (only some levels give it): it always has its own full-width tile under the grid
(`is_special`, `tile_order`), empty when the level does not give it, so the panel never has to make room for it.
Pure view model `tile_view(slot) -> TileView` (unit-tested) drives the tiles:

| Availability | Tile |
|---|---|
| Hidden | empty faint slot (keeps the grid stable) |
| Discoverable | dark tile with "?", cannot be selected for casting until discovered |
| Provided { shots } | gray tile, badge `xN`, no recharge, disappears when used up |
| Known | gold tile, 1-4 charge pips (max depends on the spell), blue recharge bar while not full; dimmed at 0 charges |
| Unlimited | gold tile, badge `free` |

Hover shows the tile's description, click selects (white border). Right click on a known spell pauses its recharge
(`SpellBook::toggle_pause`, like the original's "toggle on/off"): it takes no mana, keeps its charges and can still
be cast; badge "paused", grey frozen bar. Right click again resumes. Spells cast on a spot (`ground_spell`: Teleport
for now) are aimed with the mouse while selected: over the map the cursor becomes the animated gold arrow with the spell's icon on its right, the icon grayed out
where it cannot apply (`GameMap::can_cast`: Teleport only onto walkable ground), left click casts it there and puts
the spell away (arrow cursor, clicks go back to the units' selection), right click puts it away without casting; meanwhile clicks do not select or move units. The others: `C`
casts the selected spell (demo: consumes a charge and the shaman does her cast jump). Mana: every 0.1 s each recharging spell gets 8 mana (`MANA_PER_TICK`).
Next: icons (Kenney game-icons), casting on the terrain, mana from followers, tooltips, Build tab icons, the Stats tab (see below).

### Build tab (`hud/build.rs`)
Mirrors `game_core::build_book::BuildBook` (resource `PlayerBuilds`, `level_builds`: the level's own, set with the
spells on New game and PgUp/PgDn; generated maps and sandboxes: every kind). Same grid as the spells: a named tile
per available kind (no icons yet), "?" for plans to discover, an empty slot for hidden ones; hover describes it.
A click on an available tile picks its blueprint (white border, `blueprint.rs`): it follows the mouse on the map,
red where it cannot stand, Space turns it, right click puts it away; left click places it and sends the selected
braves to build it. Details in buildings.md "Blueprint" and "Construction".

### Stats tab (design, not implemented)
The player's units at a glance, as a matrix in the panel's width (204 px), and a quick way to select them
[player; picked from local HTML mockups, not kept].
- Columns: the row's label; its total (the sum of the kind columns after it, headed "Σ"); then one per kind in
  this order: brave, warrior, firewarrior, preacher, spy (not the shaman, not wildmen), headed by the tooltip's
  unit icons (`tooltip::icon_pixels`, at the same 1.5× size, in the player's colour; hovering one names the kind).
- Rows, in three groups set a little apart:
  1. Selected: the player's units selected now.
  2. Idle, Housed (inside a hut), Working (on any task: building, fetching or carrying wood, cutting, training,
     praying at a totem or vault...) [ours: what counts as working].
  3. In boat, In balloon: aboard a vehicle (once vehicles exist).
  With the original files each label is the row's icon, `hfx0-0.dat` 1084 Selected, 1085 Idle, 1086 Housed,
  1087 Working, 1088 In balloon, 655 In boat (sprites.md); the text labels otherwise (`--no-original`).
- Look ("D1"): each number on a tile like the spell tiles (`PARCHMENT` panel, light tiles with a dark brown
  border), the totals on dark brown tiles with light figures; the Selected row on brighter gold tiles, its label
  in bold, its total darker; zeros dimmed. Labels never wrap.
- Clicks (the Selected row is display only):
  - a number: one more unit of that kind and row joins the selection (one not selected yet), so three clicks on a
    3 select all three; Shift + click: all of them at once;
  - a row's total: one more unit of that row, kinds taken in column order (braves first, then warriors...);
    Shift + click: the whole row (e.g. every idle unit).
  The tiles do not show what is picked; the Selected row does. Hovering a tile or a total outlines it.
- Counts read from the simulation each frame (pure functions over `GameMap` and the selection, tested); which unit
  a click adds is decided the same way, in unit id order within a cell [ours].

## Main menu (`menu.rs`)
Shown before the game over the first map: New game (the level from the command line, PgUp/PgDn list: the original
`levlNNNN.dat` files in level-number order, any file name case, `world::sort_levels`),
Sandbox > Walk, Units, Buildings, Worship (the ground of every sandbox shows a grid along the cell borders, `world::ShowGrid`,
`terrain_texture::draw_grid`; levels do not) (`GameMap::sandbox_walk`: small flat island, gentle ramp east, steep hill north, lake west, mesa ringed by cliffs south-east, each a few cells past the spawn; `GameMap::sandbox_worship`: a pyramid of knowledge south of the site, door
facing the shaman, and north one totem of each look with 8 braves, worship.md), Quit.
Up/Down (W/S) move, Enter/Space pick, Esc/Backspace go back a page; the mouse hovers and clicks.
Esc in the game (once an open view-presets menu is closed) pauses: the mouse is released (`VirtualCursor::request`)
and the pause menu shows over the frozen, dimmed game: Resume (or Esc), Main menu > "Leave this game?" No / Yes.
Resuming or starting a game captures the mouse again.
`AppState::Menu | Playing | Paused`: gameplay systems (input, simulation, HUD actions) are in the `Gameplay` set and only run
while playing. Behind the menu the game camera is off (no terrain, units or HUD drawn); the menu and the cursor
are on an overlay camera (`OverlayCamera`, order 1) that clears the window in the menu and draws over the game otherwise. `POP3_START=menu|game|sandbox-walk|sandbox-units|sandbox-buildings|sandbox-worship` picks the start; screenshots start in the game by default.

## Selection and orders
Left click on a vault of knowledge with the shaman selected sends her to pray at it (worship.md). Left click a unit to select it (Ctrl adds/removes), left drag for a whitish box selection, right click to deselect (Shift + right click puts out the player's camp fire under the cursor, or cancels the player's plan not flat yet);
the shaman is selected like any unit on the map, clicking her panel preview selects her alone. Left click on the ground sends the selection there (on a tree: braves cut it; on a wood pile: braves with empty hands take a piece, units.md "Wood"; on one of the player's buildings still to build: braves work on it, buildings.md "Construction"; on a totem: the units that may pray there go and pray, worship.md), X stop (with Ctrl held, these orders are chained after
the units' current ones, see units.md "Chained orders"), Space
looks at the shaman. Selected units show a health bar; the cursor shows the count when more than one (see units.md).

## Hover halo (`hover.rs`)
The thing under the mouse gets a warm white outline (`Hovered`, picked each frame: a unit first, then a wood pile,
a tree, a totem, a building; a totem when the ground under the cursor is within `TOTEM_MARGIN` of its centre). What can be hovered is a `Hoverable { health }` component put on the views when they are
spawned, not a rule in the picking: every living unit not already selected (`health`: the player's own also show their health bar),
wood pieces, trees, totems and every building, any tribe or neutral (the player's plans are hovered too, for their tooltip,
but have no outline). The reincarnation site gets
none. Not over the panel, nor while a spell or a blueprint is out.
- Sprites (units, wood pieces): a quad 2.5 px larger behind the sprite with `hover_outline.wgsl`, which draws only
  the pixels within 1.5 px outside the sprite's alpha (16 samples), both sides.
- 3D models (trees, buildings): an inverted hull, each mesh pushed out 0.03 cell along normals averaged per
  position (no cracks at hard edges), front faces culled. Smoke puffs are left out (`NoOutline`).
- Outline meshes are made once per source mesh and kept (`OutlineCache`).
- Dev: `HOVER=unit:3` (or `wood`, `tree`, `totem`, `building` and an index in its `GameMap` list) forces it for a shot.
The hovered unit also shows its health bar (units.md). A building rested on for `HOVER_SECS` shows its tooltip
(buildings.md "Tooltip"). Every tooltip, done and planned, is in [tooltips.md](tooltips.md).

## World editor (`editor.rs`)
Tab toggles edit mode. Brushes at the camera focus: R raise, F lower (Erode), T flatten,
M mark + B land bridge from mark. Next: mouse picking on the curved surface, brush radius UI,
object placement, saving back to `.dat` (inverse of `pop3-format`; what to write is in level-format.md).

### Reference: the ALACN world editor (PopRe), tools worth copying
Behaviour of the community editor (no licence, behaviour only, not checked against the game):
- **Brush**: a square of `size + 1` vertices per side, centred on the picked cell. Size 0-128 on the mouse wheel,
  speed 0-8. The height step is the elapsed time divided by a per-speed divisor (30000 down to 1000), so it is
  frame-rate independent.
- **Raise** (left button): when the area is uneven, only the vertices below its highest point rise, up to it.
  Once flat, everything rises, capped at 1792 (the files never exceed 1024). **Lower** (right button) mirrors it
  down to 0. A modifier skips the levelling step.
- **Flatten**: every vertex moves by at most one step towards the height under the cursor.
- **Smooth**: every vertex moves by at most one step towards the mean of itself and its 8 neighbours, counting
  only neighbours above height 7, so sea and shore don't drag coasts under.
- **Objects**: pick along the mouse ray, drag snaps to cells, the wheel rotates (buildings by quarter turns,
  scenery by eighths), Del deletes, N duplicates. Land bridges have a second handle for their target. Triggers
  link up to 10 things by clicking.
- **Markers**: 256 slots, Ctrl+click puts the first free one (free = at cell 0,0), drag moves, Del frees.
- **View**: 60 x 60 cells around the camera, curved by `-0.01 * d^2` (cells, only when the camera is low).
  Heights are drawn exaggerated (1024 = 5 cells).
- **Header dialogs**: name, players 1-4, AI script per tribe, allies matrix, flags (fog, god mode, no guest spells,
  no reincarnation time), spell / building / vehicle availability, spells not charging, object bank (with a tree
  preview), landscape type 0-35.

## Window
Windowed 1280x720 for dev, F11 toggles borderless fullscreen, `FULLSCREEN=1` starts fullscreen.
`HEADLESS=1 SCREENSHOT=out.png [AERIAL=1]` renders offscreen and exits (UI text not captured yet).


Cursor confinement debugging: `POP3_CURSOR_DEBUG=1 just run` logs the display backend, focus,
enter/leave and edge contact. On COSMIC, pushing the system cursor against the screen edge made the
compositor take focus (auto-hide panel), which drops any pointer constraint. Hence the in-game cursor
(`virtual_cursor.rs`): the system cursor is locked and hidden, ours moves from raw motion and stops at
the window border, is re-sent as `CursorMoved` (picking) and written to `Window::set_cursor_position` (bevy_ui
`Interaction` reads it; on Wayland it is also the unlock position hint). Esc releases it (Esc again recaptures); the window gaining focus always recaptures;
`POP3_CURSOR_SPEED` scales it (default 1.5; raw motion is unaccelerated).
Pointer image: original arrow (`POINT0-0.DAT` sprite 14, see sprites.md, drawn x2, click point at its tip)
when original files are allowed, else a generated black-and-white arrow. `CursorLook` picks it each frame: while
a spell is aimed, its gold icon (`spell_sprite`, sprites 39-57, sprites.md; while a blueprint is out, the arrow with its building's teal icon at the
same scale on its right, `building_sprite`, 58-65, grey when it cannot stand there), click
point at the centre, or a generated gold ring; grayed and see-through (`dimmed`) where the spell cannot apply.

View presets: F2 opens a menu (`hud/view_menu.rs`) of camera/terrain presets applied live (distance, tilt,
relief, curvature; the terrain is rebuilt); the chosen values are logged. Esc closes it when open (and is
consumed), otherwise Esc releases/captures the mouse. View tuning (dev, read at start): `POP3_RELIEF` (relief vs the original height ratio, default 1.5; F2 presets x1 original, x3 dramatic), `POP3_CURVATURE`
(planet bend, default 0.008), `POP3_VIEW_DISTANCE` (cells, default 14), `POP3_VIEW_PITCH` (degrees, default 6).
The previous default (relief x2, curvature 0.012, distance 20, tilt 3) is a menu preset.
