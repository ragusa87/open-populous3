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

## Left panel (`hud/`)
Fixed 204 px panel on the left, like the original: shaman preview at the top (`hud/shaman.rs`: her current
sprite x2, same pose and view as on the map, what she is doing, e.g. "Praying" / "Reincarnating in 2s", and a
health bar with `hp/max`; clicking it looks at her), 3 tabs
(Spells / Build / Stats, `ActiveTab`), one content node per tab (`TabContent(n)`), info line under
the grid.

### Spells tab (`hud/spells.rs`)
Mirrors `game_core::spell_book::SpellBook` (resource `PlayerSpells`, demo loadout for now).
Pure view model `tile_view(slot) -> TileView` (unit-tested) drives the tiles:

| Availability | Tile |
|---|---|
| Hidden | empty faint slot (keeps the grid stable) |
| Discoverable | dark tile with "?", cannot be selected for casting until discovered |
| Provided { shots } | gray tile, badge `xN`, no recharge, disappears when used up |
| Known | gold tile, 1-4 charge pips (max depends on the spell), blue recharge bar while not full; dimmed at 0 charges |

Hover shows the tile's description, click selects (white border), `C` casts the selected spell (demo:
consumes a charge and the shaman does her cast jump). Mana: every 0.1 s each recharging spell gets 8 mana (`MANA_PER_TICK`).
Next: icons (Kenney game-icons), casting on the terrain, mana from followers, tooltips, Build/Stats tabs.

## Main menu (`menu.rs`)
Shown before the game over the first map: New game (the level from the command line, PgUp/PgDn list),
Sandbox > Walk (`GameMap::sandbox_walk`: flat island, gentle ramp east, steep hill north, lake west), Quit.
Up/Down (W/S) move, Enter/Space pick, Esc/Backspace go back a page; the mouse hovers and clicks.
Esc in the game (once an open view-presets menu is closed) pauses: the mouse is released (`VirtualCursor::request`)
and the pause menu shows over the frozen, dimmed game: Resume (or Esc), Main menu > "Leave this game?" No / Yes.
Resuming or starting a game captures the mouse again.
`AppState::Menu | Playing | Paused`: gameplay systems (input, simulation, HUD actions) are in the `Gameplay` set and only run
while playing. Behind the menu the game camera is off (no terrain, units or HUD drawn); the menu and the cursor
are on an overlay camera (`OverlayCamera`, order 1) that clears the window in the menu and draws over the game otherwise. `POP3_START=menu|game|sandbox-walk` picks the start; screenshots start in the game by default.

## Selection and orders
Left click a unit to select it (Ctrl adds/removes), left drag for a whitish box selection, right click to deselect;
the shaman is selected like any unit on the map, clicking her panel preview selects her alone. Left click on the ground sends the selection there, P pray, X stop, Space
looks at the shaman. Selected units show a health bar; the cursor shows the count when more than one (see units.md).

## World editor (`editor.rs`)
Tab toggles edit mode. Brushes at the camera focus: R raise, F lower (Erode), T flatten,
M mark + B land bridge from mark. Next: mouse picking on the curved surface, brush radius UI,
object placement, saving back to `.dat` (inverse of `pop3-format`).

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
when original files are allowed, else a generated black-and-white arrow.

View presets: F2 opens a menu (`hud/view_menu.rs`) of camera/terrain presets applied live (distance, tilt,
relief, curvature; the terrain is rebuilt); the chosen values are logged. Esc closes it when open (and is
consumed), otherwise Esc releases/captures the mouse. View tuning (dev, read at start): `POP3_RELIEF` (relief vs the original height ratio, default 3), `POP3_CURVATURE`
(planet bend, default 0.008), `POP3_VIEW_DISTANCE` (cells, default 14), `POP3_VIEW_PITCH` (degrees, default 6).
The previous default (relief x2, curvature 0.012, distance 20, tilt 3) is a menu preset.
