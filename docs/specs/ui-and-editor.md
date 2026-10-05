# Camera, UI and editor

## Camera (`camera.rs`)
`CameraRig { focus, yaw, pitch, distance, aerial }`. Focus is in cell units, wrapped in `[0,128)`.
Like the original: mouse at a screen edge scrolls (4 directions), arrows rotate (left/right) and tilt
(up/down), middle-drag rotates, wheel zooms, Enter toggles
aerial (pitch 1.35, distance 115) and restores the previous ground view (default pitch 0.32, distance 11,
close to the original). Clear color fades from sky blue to space when zooming out. Changing level frames a
low inland cell (`Heightmap::lowland_cell`).

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
