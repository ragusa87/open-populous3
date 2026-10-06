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
| 0 | u16 | sprite index x 6 (offset in `HSPR0-0.TAB`, 6-byte entries) |
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

93-96 (kneeling) have only 4 views per tribe (front, side facing right, back-diagonal, back) but the
direction table points at 5 consecutive frames: from direction 1 on it is one view off, and direction 4
lands on the next tribe's front (blue shows red, green a black silhouette). `ShamanAnim::view` remaps:
directions 0-1 front, 2 side, 3 back-diagonal, 4 back, 5-7 mirror 3-1. 81-84 look the same (unchecked).
No animated shaman prayer was found (90/91 look like braves bowing to the ground, unconfirmed).
