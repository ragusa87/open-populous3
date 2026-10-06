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
Fixed 204 px panel on the left, like the original: shaman preview at the top (live mini-view of the
main character, placeholder for now), 3 tabs
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
only consumes a charge). Mana: every 0.1 s each recharging spell gets 8 mana (`MANA_PER_TICK`).
Next: icons (Kenney game-icons), casting on the terrain, mana from followers, tooltips, Build/Stats tabs,
live shaman preview (render-to-texture camera following the shaman).

## World editor (`editor.rs`)
Tab toggles edit mode. Brushes at the camera focus: R raise, F lower (Erode), T flatten,
M mark + B land bridge from mark. Next: mouse picking on the curved surface, brush radius UI,
object placement, saving back to `.dat` (inverse of `pop3-format`).

## Window
Windowed 1280x720 for dev, F11 toggles borderless fullscreen, `FULLSCREEN=1` starts fullscreen.
`HEADLESS=1 SCREENSHOT=out.png [AERIAL=1]` renders offscreen and exits (UI text not captured yet).
