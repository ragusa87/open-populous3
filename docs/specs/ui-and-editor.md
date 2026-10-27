# Camera, UI and editor

All keys and clicks, original and ours: [shortcuts.md](shortcuts.md). Dev mode (pause menu, `POP3_DEV`) picks our
keymap over the original's and shows the info line and the camera readout; the keys below are dev mode's.

## Camera (`camera.rs`)
`CameraRig { focus, yaw, pitch, distance, aerial }`. Focus is in cell units, wrapped in `[0,128)`.
Like the original: scrolling happens only with the cursor pressed against the window border; speed
comes from how hard the mouse keeps pushing outward (raw motion while stuck at the edge, `edge_push.rs`),
slows down while holding still, stops when leaving the edge. The cursor is confined to the window
(Esc toggles). The panel does not block it, Left/Right rotate, Up/Down or WASD move (A/D strafe)
forward/back like the mouse, middle-drag rotates and tilts, Home/End tilt, the wheel or Ctrl+PgUp/PgDn zoom
(`WHEEL_ZOOM` per notch), Ctrl+wheel tilts, Shift+wheel or Shift+PgUp/PgDn change the field of view, Enter toggles
aerial (pitch 1.35, distance 115) and restores the previous ground view (default: the original's fixed
tilt 20.4 deg and eye 25.4 cells behind the focus, pop3-rev-analysis.md "Landscape projection"; fov 35 deg, not
traced, chosen to frame a view matched by eye on level 23 at 13.8 cells and 60 deg). Clear color fades from sky blue to space when zooming out. Changing level frames the
level's start camera when its header gives one (`GameMap::start_camera`, `.hdr` start cell and angle, only levels 3,
5, 21 and 79 do; the angle, read like the things' angles in the game's frame (`game_frame::GameYaw`), is where the camera
looks: checked on level 3, where from the stone circle the player's hut stands ahead and the totem on its right), else
the player's reincarnation site with angle 0 (looking towards the game's +z), else a low inland cell
(`Heightmap::lowland_cell`). Generated maps and sandboxes look north (`map::OUR_MAPS_CAMERA`, angle 1024), as they are
laid out.
The eye orbits the ground point at the focus, but never goes lower than the map's highest ground plus 1.5 cells
(`eye_position`, `EYE_CLEARANCE`), like the original's fixed camera elevation: over low ground (by the sea) it
stays up there and looks down more steeply, so cliffs between it and the focus never hide it inside the land.
H flies the camera to the player's reincarnation site (nothing on maps without one), Space (or a click on her
preview) to the shaman: a quick 0.4 s flight (`CameraRig::fly_to`, eased, the short way around the torus) rather
than a jump; pushing the camera by hand cancels it.
Camera readout (`camera_debug.rs`, bottom right, F3 hides it): frames per second and frame time (measured on the wall
clock and smoothed, `camera_debug::FrameRate`, so captures show the real rate too; on screen only), focus, the game angle it looks along (0-2047, as in a
level header's start camera), yaw, tilt, distance, fov, the eye's position (game frame, cells), and the
`FOCUS`/`DISTANCE`/`PITCH`/`YAW` variables that give the same view in `just shot`. The same readout is printed
to stdout on one line once the camera has stayed still 0.4 s after a change (`SettledLog`). G toggles the cell grid on the
ground (`ShowGrid`, on in the sandboxes, kept when changing level; the terrain texture is baked again).

## Left panel (`hud/`)
Fixed 204 px panel on the left, like the original: shaman preview at the top (`hud/shaman.rs`: her current
sprite x2, same pose and view as on the map, what she is doing, e.g. "Praying" / "Reincarnating in 2s", and a
health bar with `hp/max`; a left click on it selects her alone, the camera staying; a right click flies the camera
to her, the selection staying), the population strip (below), 3 tabs (Spells / Build / Stats, `ActiveTab`; a new
selection holding the shaman opens Spells, `panel::tab_for_selection`), one content node per tab
(`TabContent(n)`), info line under the grid.

### Population (`hud/population.rs`, the huts box in `hud/stats.rs`)
The tribe's population against the room its huts give (huts-and-training.md rule 4: every living follower but the
shaman, over the supply of its built huts, 3 / 5 / 7 by size, at most 200) [ours; picked from local HTML mockups]:
- a strip between the shaman preview and the tabs, on every tab: a people icon (`hfx0-0.dat` 680, else "Pop"), "population / room", and a bar
  filling with it; at the room or over it the bar turns red and blinks (like a hut's blocked growth bar) and the
  count turns red when over;
- on the Stats tab, a "Huts" box above the unit matrix (the strip stays above the tabs, its count and room not
  repeated): three tiles, small / medium / large, each with how many built huts of that size the tribe has and the
  room they give (e.g. 3 small, +9).
- Both read `GameMap::housing(tribe)` (`game_core::headcount::Housing`: built huts per size, population, `room()`,
  `full()`), which the computer players read too.

### Mana (`hud/mana.rs`) [ours]
A strip under the population strip, on every tab: a "Mana" label (no original icon identified for it, so text
stands in the way the population strip's own fallback does), the player's bank amount grouped by thousands, and a
bar filling with its share of `MAX_MANA` (`GameMap::mana[PLAYER]`, spells.md "Mana": `bank`, `received`, `used` -
only `bank` is shown; `received`/`used` are available to add to it later, e.g. as a tooltip, if that turns out to be
wanted). No blinking at the cap (unlike the population strip): running out of room is a problem a player should
notice, running out of mana to spend is not.

### Spells tab (`hud/spells.rs`)
Draws the player's `game_core::spell_book::SpellBook` from the map (`player_book` = `GameMap::spell_books[PLAYER]`),
no copy of its own. An original level brings its own loadout: the header's spells known and full, its spell
discoveries as "?" (spells.md). Generated maps get the demo loadout (`SpellBook::demo`, every tile state); the
sandboxes add Teleport, unlimited (`SpellBook::sandbox`). One tile per `SpellKind`, Hidden ones as empty slots,
in centred rows as on the original's panel (`ROWS`): Angel of Death, Volcano; Earthquake, Erosion, Firestorm;
Whirlwind, Swamp, Flatten; Land Bridge, Lightning, Hypnotism; Swarm, Invisibility, Magical Shield; Ghost Army,
Blast, Convert; on levels giving Bloodlust (a hidden spell, model 20, from discoveries only for now), its tile takes
Ghost Army's place (`STAND_INS`). Then Teleport's row, shown only
when the book has it (sandboxes). Armageddon is special (only some levels give it): it always has its own
full-width tile under the rows (`is_special`), empty when the level does not give it, so the panel never has to make
room for it.
A tile: on top a dot per charge the spell holds (`max_charges`: blast 4, volcano 1), blue loaded, red gifted, a
hollow (a dark pit) for room left; the spell's icon in the middle, most of the tile (the original's white glyph of `hfx0-0.dat`, drawn in the panel's dark ink,
`panel_glyph`: sprites.md; "?" 1055 for a spell to discover; its name without the original files or for Magical
Shield and Bloodlust, which have none); at the bottom the charge on its way, the training unit's bar (`progress_bar`: yellow,
the mana given over the cost; blue, the finer layer). Pure view model `tile_view(slot) -> TileView` (unit-tested):

| Availability | Tile |
|---|---|
| Hidden | empty faint slot (keeps the grid stable) |
| Discoverable | dark tile with "?", cannot be selected for casting until discovered |
| Provided { shots } | light grey tile (a gift found on the map), a red dot per use left, no recharge, no pause; an empty slot once used up, never back to "?" |
| Known | the panel's yellow, a blue dot per charge, hollows for the rest, the bar while a charge is on its way |
| Unlimited | the panel's yellow, every dot blue, badge `free` |

Hover shows the tile's description, click selects (white border). Right click on a known spell pauses its recharge
(`Command::PauseSpell`, like the original's "toggle on/off"): it takes no mana, keeps its charges and can still
be cast; badge "paused", its bar blinking, frozen. Right click again resumes. Spells cast on a spot (`ground_spell`: Teleport,
Blast, Land Bridge, Flatten) are aimed with the mouse while selected: over the map the cursor becomes the animated gold arrow with the spell's icon on its right, the icon grayed out
where it cannot apply (`GameMap::can_cast`: Teleport only onto walkable ground, a land bridge onto land). Out of
range (`GameMap::in_cast_range`) the click still casts: the shaman walks towards the target and casts once in range
(spells.md). Meanwhile a range ring stands round the shaman at the spell's range (`range_ring`, our own
look, isolated in that file to be redone: a gold curtain growing in with an overshoot in 0.75 s and shrinking back
when the spell is put away, a glowing line at its foot; when she starts casting a white line contracts into her and
the curtain jumps up; none for spells without a range nor on god levels), left click casts it there and puts
the spell away (arrow cursor, clicks go back to the units' selection), right click puts it away without casting; meanwhile clicks do not select or move units. The others: `C`
casts the selected spell (`Command::CastSelf`: the engine uses a charge and the shaman does her cast jump). Every
cast goes through a command; the engine uses the charge and recharges the spells from the player's mana (spells.md
"Mana"), shown in its own strip under the population strip ("Mana" above).
Next: icons (Kenney game-icons), casting on the terrain, mana from followers, tooltips, Build tab icons

### Build tab (`hud/build.rs`)
Draws the player's `game_core::build_book::BuildBook` from the map (`player_builds` = `GameMap::build_books[PLAYER]`:
the level's own; generated maps and sandboxes: every kind). A 2 x 5 grid as the original's panel (`LAYOUT`): hut,
drum tower; warrior hut, temple (preachers); spy hut, firewarrior hut; boat hut, airship hut; camp fire, an empty
cell. A tile is the building's icon (the original's highlight, `hfx0-0.dat` 1046-1054, in black, white on hover:
it reads better than its dark icons 1075-1083, sprites.md; else its name), "?" (1055) for plans to discover, an empty slot for hidden ones; hover describes it. Right click on a tile flies the camera to the player's
next built building of that kind (or camp fire), one after the other at each click (`own_places`, `next_step`).
Shift + left click with people selected sends them in (`GameMap::send_into`, huts-and-training.md): into the
player's buildings of that kind with room, spread evenly and nearest first (a training unit takes everyone, they
queue; braves only into vehicle huts), or round their camp fires.
A click on an available tile picks its blueprint (white border, `blueprint.rs`): it follows the mouse on the map,
red where it cannot stand, Space turns it, right click puts it away; left click places it and sends the selected
braves to build it. Details in buildings.md "Blueprint" and "Construction".

### Stats tab (`hud/stats.rs`)
The player's units at a glance, as a matrix in the panel's width (204 px), and a quick way to select them
[player; picked from local HTML mockups, not kept].
- Columns: the row's label; its total (the sum of the kind columns after it, headed "All": the panel font has no Σ); then one per kind in
  this order: brave, warrior, firewarrior, preacher, spy (not the shaman, not wildmen), headed by the tooltip's
  unit icons (`tooltip::IconImages`: the teal figures with the original files, else the generated ones in the
  player's colour; hovering one names the kind).
- Rows, in three groups set a little apart:
  1. Selected: the player's units selected now.
  2. Idle, Housed (inside a hut), Working (on any task: building, fetching or carrying wood, cutting, training or
     queueing at a training hut, praying at a totem or vault...) [ours: what counts as working].
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
- Counts read from the simulation each frame: `game_core::headcount` (`GameMap::headcount`, `state_of`, `kinds_of`
  for the Selected row), shared with the computer players; which unit a click adds is `GameMap::next_unit`, in unit
  id order within a cell [ours]. Working: an action on a task (cutting, holding wood, flattening, building, entering,
  hammering, praying, round a camp fire), or `Unit::busy` (a building to work on, wood carried, on the way to a tree,
  wood, a door, a place to pray or a fire), or inside a building that is not a built hut. A unit walking on a plain
  move, or out of a hut, is idle.

## Notifications (reported, not yet implemented)
A notification area to the right of the panels (ours: not started). New notices fall in as a small square icon
and pile up vertically; an older one is dismissed, probably by category, as new ones arrive. Two broad
behaviours on click:
- **Information**: opens a modal dialog with the message.
- **Event, jumps the camera to where it happened** instead of opening a dialog: a "preaching" notice (one of the
  player's own units has started listening to a sermon), an enemy spell cast (its own spell icon, e.g. Volcano
  or Angel of Death) - an enemy-spell notice additionally carries a small vertical bar tinted in the casting
  tribe's own colour.
- A separate **tower alert**: a sound (no icon reported) played when an occupied tower/building spots a nearby
  enemy.

pop3-rev-analysis.md "Notifications" has what the exe trace confirms so far: an advisor-message system
(`fire_advisor_message_once`, 0x499D90) that plausibly backs the plain "information" dialogs (debounced to
once-ever by default, with its own message-slot array that would back the "pile up vertically" stacking), and a
SEPARATE, now fully-read registration function (0x44D7F0, 10 categories) a handful of message ids route through
instead of the plain banner - one category draws a per-tribe-coloured bar, a strong match for the reported
enemy-spell indicator, but it is only confirmed reached from the level-script interpreter so far, not from an
automatic "an enemy just cast a spell" scan; another category chases a linked "head"/trigger scenery object, a
guess (not confirmed) for the preach notice rather than a worship-trigger message. Whoever actually DRAWS the
falling/stacking icon from the shared record these write was not located either; the preach/enemy-spell
automatic triggers themselves were not located this session.

The **tower alert** IS now traced (pop3-rev-analysis.md "Watchtower sentry scan and alarm", 0x40BD20): an
occupied tower garrisoned specifically by a warrior scans a radius (driven by the occupant's own named
`<TYPE>_DT_RADIUS` constant plus a building-height bonus) for one visible, hostile, living enemy; on finding one
it doesn't play a sound itself - it rallies nearby own units/buildings with an "attack (generic)" order, and
that order's handler is the exact code that edge-sets `0x89D167` (set once when a new fight/pre-fight starts
involving the local player, and continuously while one is ongoing - combat.md "Melee") - **so `0x89D167` is
confirmed as the tower alert's actual trigger path after all**, by virtue of the tower forcing a fight to start,
not because the byte is tower-specific. The scan itself is never run automatically for every tower every turn -
but (correcting a first-pass guess that this made it script/cheat-only) its trigger is the player's own **B**
keybind (shortcuts.md "Space (tower alarm) / B (double tower alarm)"), queued like any other player command and
drained every turn for every tribe regardless of game mode - so it works identically in singleplayer and
multiplayer, matching what was actually observed in play; a level-script opcode (`CALL_TO_ARMS`) can also invoke
the same primitive but is barely used (one level total). Still open: which UI code actually reads `0x89D167` to
play the sound/show whatever's shown (no icon was reported for this one); where the scan's own building-TYPE
argument (always 4/tower in practice) actually gets set, since the B-key command doesn't supply it; and whether
**Space**'s own "camera to the tower under attack" half reads the same state.

## Loading screen (`boot/`)
`AppState::Loading` is where the client starts, and where it goes when a game or sandbox starts: a title, a bar and
the current step (`boot::screen::screen_text`), on the menu's backdrop with the game camera off. At launch the boot
jobs run (`boot::BootJob`: today the unit sprites, read and upscaled on a worker thread, uploaded on the main thread;
"Unit sprites 7/15"), then the models Bevy loads in the background are waited for (`boot::TrackedAssets`: "Models
1/2"; a failed one counts as done). Starting a game draws "Preparing the map" for `boot::SHOW_FRAMES` frames, then
builds the map (`menu::begin_game` on `boot::BeginGame`), then plays. The end of a loading pass sends `boot::Loaded`
and enters a `ready` span: captures (`dev`) and `tools/trace_top.py` count from there.

## Main menu (`menu.rs`)
Shown before the game over the first map: New game (the level from the command line, PgUp/PgDn list: the original
`levlNNNN.dat` files in level-number order, any file name case, `world::sort_levels`),
Sandbox > Walk, Units, Buildings, Worship, Spells, Vehicles, Bridge (the ground of every sandbox shows a grid along the cell borders, `world::ShowGrid`,
`terrain_texture::draw_grid`; levels do not) (`GameMap::sandbox_walk`: small flat island, gentle ramp east, steep hill north, lake west, mesa ringed by cliffs south-east, each a few cells past the spawn; `GameMap::sandbox_worship`: a pyramid of knowledge south of the site, door
facing the shaman, and north one totem of each look with 8 braves, worship.md; `GameMap::sandbox_spells`: every spell
unlimited, Teleport too (`SpellBook::unlimited`), on islands a few cells of sea apart: the player's in the middle with 3 of each follower kind of the player south and of
tribe 1 north, kind by kind 5 cells apart, a red
village east with people in its huts, a hill north with a few people of tribe 2, an empty islet south-west;
`GameMap::sandbox_vehicles`: a boat hut and an airship hut on the south coast with 8 braves and wood to crew them,
boats on the water by the shore (one manned), one adrift at sea, a balloon by the site for the shaman, hills to fly
over (a tall one west, a lower one east, a mound on the south island), and an empty boat of tribe 1 by an island
south across a strait, vehicles.md; `GameMap::sandbox_bridge`: two large islands a realistic stretch of open sea
apart, the player's shaman alone near the home shore, a target cell at the far shore within Land Bridge's cast
range but with margin, for `BRIDGE=`/`BRIDGE_TICKS` screenshots and videos), Quit.
Up/Down (W/S) move, Enter/Space pick, Esc/Backspace go back a page; the mouse hovers and clicks.
Esc in the game (once an open view-presets menu is closed) pauses: the mouse is released (`VirtualCursor::request`)
and the pause menu shows over the frozen, dimmed game: Resume (or Esc), Game speed: x1/x2/x4/x8 (cycles), Dev mode: on/off (shortcuts.md), Shortcuts > (the
keys and mouse of the keymap in use, shortcuts.md; `MENU=shortcuts` opens it for shots), Main menu >
"Leave this game?" No / Yes.
Resuming or starting a game captures the mouse again.
`AppState::Loading | Menu | Playing | Paused`: gameplay systems (input, simulation, HUD actions) are in the `Gameplay` set and only run
while playing. Behind the menu the game camera is off (no terrain, units or HUD drawn); the menu and the cursor
are on an overlay camera (`OverlayCamera`, order 1) that clears the window in the menu and draws over the game otherwise. `POP3_START=menu|game|sandbox-walk|sandbox-units|sandbox-buildings|sandbox-worship|sandbox-spells|sandbox-vehicles|sandbox-bridge` picks the start; screenshots start in the game by default.

## Game speed (`game_speed.rs`)
P pauses the simulation only: the camera, hovering, selecting and the HUD keep working, units and their
animations stand still (the drawn moment `SimNow` stops, the glide between ticks holds). Shift + `+` / `-` (or the
numpad's) step the speed through x1, x2, x4, x8 and resume; the pause menu's Game speed entry cycles it;
`GAME_SPEED=0|1|2|4|8` sets the start. A label at the top of the view says "Paused" or "Speed xN" when not x1.
The clock (`SimClock::steps_due`) runs whole tick times (1/12 s), each running the speed's number of ticks, at
most 4 tick times a frame (a slow frame drops the rest rather than catching up); units glide from where they were
before the frame's ticks. Everything in the world (flames, smoke, dust, nods, shakes) follows simulation time and
stops too; UI and mouse feedback (blinking, tooltips, a hovered tree's sway, the cursor) keep real time
(architecture.md "Client: world visuals").

## Selection and orders
Left click on a vault of knowledge with the shaman selected sends her to pray at it (worship.md). Left click a unit to select it (Ctrl adds/removes), left drag for a whitish box selection, right click to deselect (Shift + right click puts out the player's camp fire under the cursor, or cancels the player's plan not flat yet);
the shaman is selected like any unit on the map, clicking her panel preview selects her alone. Left click on the ground sends the selection there (on a tree: braves cut it; on a wood pile: braves with empty hands take a piece, units.md "Wood"; on one of the player's buildings still to build: braves work on it, buildings.md "Construction"; on a totem: the units that may pray there go and pray, worship.md), X stop (with Ctrl held, these orders are chained after
the units' current ones, see units.md "Chained orders"), Space
looks at the shaman. Selected units show a health bar; the cursor shows the count when more than one (see units.md).

## Hover halo (`hover.rs`)
The thing under the mouse gets a warm white outline (`Hovered`, picked each frame: a unit first, then a wood pile,
a tree, a totem, a building; a totem when the ground under the cursor is within `TOTEM_MARGIN` of its centre; a
building anywhere in the box its model covers on screen, footprint up to its model's height, the nearest when several
overlap (`building_on_screen`): with the low camera the ground under a tower's top lies behind it; a plan by the
ground under the cursor). A left click on a building uses the hovered one the same way (`building_click`). What can be hovered is a `Hoverable { health }` component put on the views when they are
spawned, not a rule in the picking: every living unit not already selected (`health`: the player's own also show their health bar),
wood pieces, trees, totems and every building, any tribe or neutral (the player's plans are hovered too, for their tooltip,
but have no outline). The reincarnation site gets
none. Not over the panel, nor while a blueprint is out. While a spell is aimed the halo shows what it would
land on, and the spell is cast on a hovered building or totem's centre, like a click (`hovered_site`).
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
consumed), otherwise Esc releases/captures the mouse. View tuning (dev, read at start): `POP3_RELIEF` (relief vs the original height ratio, 1024 = 4 cells, default 1; F2 preset x2 dramatic), `POP3_CURVATURE`
(planet bend, default the original's 0.011), `POP3_VIEW_DISTANCE` (cells, default 25.4), `POP3_VIEW_PITCH` (degrees,
default 20.4). The previous defaults (tilt 6, distance 14; relief x1.33, curvature 0.012, distance 20, tilt 3) are menu
presets. Units are drawn 1.5 times their sprites' size (`object_scale::UNITS_SCALE`, `POP3_SCALE_UNITS`).
Under the presets, "Tuning" rows step values live with - / + (0.1 a click, logged): the relief (0.5-4, the terrain
is rebuilt) and how big objects are drawn against the cells (`object_scale.rs`, `ObjectScale`, 0.25-4, 1 = the
original model size): trees, units (with their shadows), buildings, scenery (sites, totems, wood, camp fires), or
all objects at once. Each object's grounded root carries `Scaled(group)` and scales about the point it stands on,
so buildings above 1 overflow their footprint cells. Start values: `POP3_SCALE_TREES`, `_UNITS`, `_BUILDINGS`,
`_SCENERY`.
