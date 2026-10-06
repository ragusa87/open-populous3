# Unit artwork (open-source sprites)

Brief for drawing or generating free unit sprites to replace the generated figures (`units/procedural.rs`)
and to ship instead of the original art (copyrighted, never shipped). Loaded today: a single still per kind
(see "Still"); the full sheets described under "Delivery" are to be loaded once the first ones exist.

## Style
- Pixel art, hard edges: every pixel is fully opaque or fully transparent (the engine cuts alpha at 50%:
  no soft or anti-aliased edges against the background). Soft shading inside the figure is fine.
- A 1 px dark outline all around (current figures use `#1E120A`), light from the top left.
- Small and readable: a standing unit is ~34 px tall and shows ~35-60 screen pixels tall at the default
  zoom (the engine upscales x4 with Scale2x, then filters). Favour a clear silhouette over detail.
- Seen from slightly above (about a 3/4 view, 20-30 deg down), like the original game. Sprites always face
  the camera (billboards); the view direction is chosen from the unit's heading.
- Original work only: do not trace, recolour or redraw Populous: The Beginning sprites.

## Size and anchor
- One frame = a 64 x 64 px cell. The feet (the point on the ground) are at pixel x = 32, y = 58, the same in
  every frame; the 5 rows below are for feet, dust and ripples. Things may stick out of the figure (staff,
  horns, flames) but must stay inside the cell.
- Scale: 1 px = 1/88 map cell. A standing adult is 32-36 px from feet to top of the head (about half a
  reincarnation stone); headgear and held items may go above. Braves may be a little shorter, warriors a
  little broader.

## Directions
Draw 5 directions; the engine mirrors the 3 left-hand ones (5 = mirrored 3, 6 = mirrored 2, 7 = mirrored 1):

| Row | Direction | Seen as |
|---|---|---|
| 0 | facing the viewer | front |
| 1 | facing down-right | front three-quarter |
| 2 | facing right | profile, walking to screen right |
| 3 | facing up-right | back three-quarter |
| 4 | facing away | back |

Held items switch hands when mirrored: that is expected.

## Poses
Frame counts are what the engine plays today (other counts work for looping poses).

| Pose | Frames | Playback | What it shows |
|---|---|---|---|
| idle | 4-6 | loop, 6 fps | standing, breathing; also used while landing after a teleport |
| walk | 8 | loop, 10 fps | one full stride (left and right foot), in place: the engine moves the unit |
| pray | 1-4 | loop, 4 fps | kneeling on one knee, head bowed, arms raised a little |
| cast | 12 | once, over 1.2 s | shaman only: crouch, jump (up to ~12 px), spell in raised hands, land |
| fall | 8 | once over 0.8 s, then holds frame 7 | hit and falling backwards; frame 7 (index 6) lies flat and stays shown while dead; frame 8 can repeat it |
| drown | 4 | loop, 8 fps | in deep water: only the upper body, arms flailing, ripples at the water line (y = 58) |
| stranded | 2-4 | loop, 4 fps | all kinds but the shaman: standing, both arms up, calling for help (cannot reach its target) |

Coming later (nice to have now): strike (4 frames, melee hit), hit (2, flinch), celebrate (4), and per kind:
build/chop for braves, preach for preachers, throw fire for firewarriors.

## Kinds
Each kind must be recognisable from its silhouette alone, at the size above and in every tribe colour.

| Kind | Role | Current look (keep or improve) |
|---|---|---|
| shaman | the player's leader, casts spells | feather headdress, long robe, tall staff |
| brave | worker, the most numerous | bare chest, loincloth, hair tuft, empty hands |
| warrior | melee fighter, the toughest | horned helmet, broad, club |
| preacher | converts enemies | pointed hood, long robe down to the ankles, book |
| spy | sneaks in disguise | dark cloak and cowl, small dagger |
| firewarrior | throws fireballs | red cone hat, flame in hand |

## Tribe colours
Four tribes share the art: blue `#2659F2`, red `#E62619`, yellow `#F2D926`, green `#33BF33`. Draw the
tribe-coloured parts (robe, loincloth, belt, hood...) with this 4-step key ramp only, which the engine swaps
for the tribe's own ramp when loading:

| Key | Meaning |
|---|---|
| `#FF80FF` | tribe colour, highlight |
| `#FF00FF` | tribe colour, base |
| `#B000B0` | tribe colour, shade |
| `#600060` | tribe colour, deep shade |

Use no other magenta anywhere in the art. Skin, hair, wood, metal and the spy's cloak keep their own colours.

## Delivery
- PNG, RGBA, 8 bits per channel, transparent background, no colour profile tricks.
- One sheet per kind and pose: `assets/units/<kind>/<pose>.png`, kinds and poses as named above (lower case).
  Rows = the 5 directions in the order above, columns = frames in play order, every cell 64 x 64.
  Example: `assets/units/warrior/walk.png` is 512 x 320 (8 frames x 5 directions).
- A missing sheet falls back to the generated figure for that kind and pose, so art can arrive one pose at a time.
- Licence: CC0 preferred (CC-BY 4.0 accepted). List author, licence and source of every sheet in
  `assets/CREDITS.md`; for generated art, the tool and model too.

## Still (minimum delivery, loaded today)
One detailed picture per kind, facing down-right (direction 1), at any resolution: `tools/normalize_still.py`
turns it into `assets/units/<kind>/still.png` (256 x 256, 4 pixels per base pixel, feet at (128, 232), hard
edges, tribe colour turned magenta), given the feet point and the source pixels per base pixel (head top to
feet = 34 base px). Keep the source next to it as `source.png`. The game embeds the still (`units/still.rs`),
swaps the magenta hue for each tribe's (shading kept) and builds every pose by moving it around the feet:
idle as is, walk bobbing and swaying, pray squashed and bowed, cast jumping 12 base px, fall turning onto its
back, drown sunk 10 base px under a ripple line. Directions 0-4 show it as is (no back view yet), 5-7 mirrored.
Detailed art (more than 1 pixel per base pixel) is drawn as is, without the pixel-art upscaling.

The shaman's still is used whenever the original files are not (`--no-original`, no install):

  uvx --with pillow python tools/normalize_still.py assets/units/shaman/source.png assets/units/shaman/still.png --feet 206,700 --px-per-base 17

## Generating with an image model
Models do not keep a fixed grid or anchor reliably: generate one direction or pose at a time, larger, then
clean up by hand. Downscale with nearest neighbour to the target size, redraw the outline, snap the
tribe-coloured parts to the key ramp, align the feet on (32, 58) in every cell, and check the loop in motion.
A prompt to start from: "pixel art game sprite, tiny tribal <kind> (<look>), full body, <direction>, <pose>,
3/4 top-down view, 1 px dark outline, flat shading, robe in pure magenta, transparent background".
