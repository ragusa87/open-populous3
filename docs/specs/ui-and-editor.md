# Camera, UI and editor

## Camera (`camera.rs`)
`CameraRig { focus, yaw, pitch, distance, aerial }`. Focus is in cell units, wrapped in `[0,128)`.
Arrows move along yaw, Q/E or right-drag rotate, left/middle-drag pan, wheel zoom, Enter toggles
aerial (pitch 1.35, distance 115) and restores the previous ground view. Changing level frames the
highest cell.

## Control tabs (`hud.rs`)
Bottom bar: Spells / Buildings / Followers (placeholders, `ActiveTab` resource). Next: icon grid per
tab, mana bar, minimap (render the heightmap to a texture).

## World editor (`editor.rs`)
Tab toggles edit mode. Brushes at the camera focus: R raise, F lower (Erode), T flatten,
M mark + B land bridge from mark. Next: mouse picking on the curved surface, brush radius UI,
object placement, saving back to `.dat` (inverse of `pop3-format`).

## Window
Windowed 1280x720 for dev, F11 toggles borderless fullscreen, `FULLSCREEN=1` starts fullscreen.
`HEADLESS=1 SCREENSHOT=out.png [AERIAL=1]` renders offscreen and exits (UI text not captured yet).
