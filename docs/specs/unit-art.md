# Unit artwork (open-source sprites)

Brief for drawing or generating free unit sprites to replace the generated figures (`units/procedural.rs`)
and to ship instead of the original art (copyrighted, never shipped). Loaded today: sheets rendered from a 3D
model (see "Rendered sheets"); hand-drawn sheets as described under "Delivery" are to be loaded once some exist.

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

## Rendered sheets (loaded today)
A rigged, animated 3D model (glTF) can be rendered into sheets by the `render_sprites` example:

  just render-sprites assets/models/witch.gltf assets/units/shaman [--head 3.1] [--tribe Clothes,Hat] [--skin d29a6e]
  just render-units    # every kind, with its model and tribe materials

One PNG per pose (`idle`, `walk`, `pray`, `cast`, `fall`, `drown`, `stranded`), 8 rows (directions 0-7, all rendered:
no mirroring) of 320 x 288 cells, 4 pixels per base pixel, feet at (160, 256) (bigger than a drawn cell: a
lying body and the cast jump must fit), orthographic, seen from 30 deg above, hard edges, 2 px dark outline.
The model's tribe materials (`--tribe`, default `Clothes,Hat`) are rendered in the magenta key, the material
named `Skin` in `--skin` (the Quaternius characters ship a near-black skin; default tan `d29a6e`). Pose to clip: idle Idle
(8 frames over the loop), walk Walk (12), pray SitDown (its last moment), cast Jump (12), fall Death (8, the
last two lying), drown RecieveHit (4, sunk 45% under a ripple line), stranded Victory (4, looping over 45-80% of the clip, see below). Scale: the head top (default 3.1 model
units) is 34 base px above the feet.

The game embeds the sheets (`units/sheets.rs`), crops every cell around the feet and swaps the magenta hue for
each tribe's. Every kind uses them whenever the original files are not (`--no-original`, no install): shaman =
witch (robe and hat in the tribe colour), brave = worker (shirt), warrior = soldier (top), preacher = wizard (robe;
his hat stays dark so he is not mistaken for the shaman), spy = ninja (details), firewarrior = cowboy (jacket).
A kind without sheets would fall back to its generated figure.

### Stranded sheet
The Quaternius characters have no "arms up" clip. Of the candidates, `Defeat` puts the hands on the head and
`Victory` raises both arms and pumps them, so `Victory` stands in. Sampled over its length (worker, back view,
where the stance reads best): 0-20% arms rising from the sides, 20-40% arms half up and spread, 45-80% both arms
fully above the head, 80-100% coming back down. The sheet loops 4 frames over 45-80% (`Timing::Span`), at 4 fps;
an earlier span (22-67%) showed the arms only shoulder high. Every model gets the sheet (the shaman's is unused:
she is never stranded). With the original files the followers hold a frame of anim 12 instead (animations.md).

## Generating with an image model
Models do not keep a fixed grid or anchor reliably: generate one direction or pose at a time, larger, then
clean up by hand. Downscale with nearest neighbour to the target size, redraw the outline, snap the
tribe-coloured parts to the key ramp, align the feet on (32, 58) in every cell, and check the loop in motion.
A prompt to start from: "pixel art game sprite, tiny tribal <kind> (<look>), full body, <direction>, <pose>,
3/4 top-down view, 1 px dark outline, flat shading, robe in pure magenta, transparent background".
