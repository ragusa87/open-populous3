# Person animations (original files)

`pop3_format::anim` (`AnimBank`), shaman ids in `pop3_format::catalog::ShamanAnim`. Read-only, never shipped.
Persons are 2D sprites: an animation is a loop of frames, a frame is a chain of sprites ("elements")
from `data/HSPR0-0.DAT` (PSFB bank, see sprites.md, palette `pal0-0.dat`) placed around the feet.

## `data/VSTART-0.ANI`: animations
99 animations x 8 directions x 4 bytes: `u16` first frame, `u16` mirror source (0 = own frames,
else the index `anim * 8 + dir` it copies). Directions 0-4 have their own frames, 5-7 are 3-1 mirrored.
Direction 0 faces the viewer, 2 faces screen right, 4 shows the back.

## `data/VFRA-0.ANI`: frames, 8 bytes
| Off | Type | Content |
|---|---|---|
| 0 | u16 | first element |
| 2 | u8, u8 | width, height (bounding box, unused) |
| 4 | u16 | flags (0x100 on the first frame of a loop) |
| 6 | u16 | next frame; the loop ends when it comes back to the first |
Record 0 is empty.

## `data/VELE-0.ANI`: elements, 10 bytes
| Off | Type | Content |
|---|---|---|
| 0 | u16 | (sprite index + 1) x 6 (offset in `HSPR0-0.TAB`, 6-byte entries, entry 0 unused); 0 = no sprite |
| 2, 4 | i16 | x, y of the sprite's top-left corner relative to the feet |
| 6 | u16 | flags: 0x1 mirrored, 0x4 shadow, 0x10 tribe layer with tribe in bits 9-10 (mapping unverified) |
| 8 | u16 | next element in the frame, 0 = last |
Elements are drawn in chain order (back to front). `AnimBank::compose` skips shadows and tribe layers.

## Identified animations
0-52 braves, warriors, preachers, firewarriors... (not mapped yet). Shamans: 4 consecutive animations per
action, one per tribe (blue, red, yellow, green), colours drawn in:

| Blue | Action | Frames |
|---|---|---|
| 53 | idle | 5 |
| 57 | staff strike | 4 |
| 61 | flying horizontally, arms spread (blown by a whirlwind) | 4 |
| 65 | cast (jumps, lightning in the hands) | 12 |
| 69 | kick | 5 |
| 73 | tumbling in the air | 4 |
| 77 | walk | 8 |
| 85 | knocked down | 8 |
| 81 | standing still | 1 |
| 93 | kneeling on one knee (used for praying) | 1 |

Reading the sprite as `field / 6` (no -1) looks almost right but shows the next view's sprite once per
loop, and the next tribe's in the back view: the -1 is checked on every shaman pose and tribe.
No animated shaman prayer was found; 90/91 are braves bowing to the ground (worship).
