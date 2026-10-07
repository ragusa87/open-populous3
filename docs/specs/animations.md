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
| 6 | u16 | flags: 0x1 mirrored, 0x4 shadow, 0x10 tribe layer (tribe 1-3 in bits 9-10), 0x20 outfit layer (see below) |
| 8 | u16 | next element in the frame, 0 = last |
Elements are drawn in chain order (back to front). `AnimBank::compose_as(.., tribe, outfit)` draws the elements
`anim::element_shown` keeps: no shadows, the tribe layer of that tribe only, the outfit layer of that outfit only.

Shadow elements (flags `0x204`, 3289 of them) only use 4 sprites of `HSPR0-0.DAT`, all plain black (index 0)
ellipses: 22 (21 x 4, 2767 elements), 68 (16 x 2), 69 (9 x 2), 70 (39 x 6), about 9 px left of the feet. The
client draws its own shadow instead (`units/shadow.rs`): a soft black ellipse 21 px wide (0.24 cells), 60% dark,
lying on the ground under every unit and leaning with the slope, hidden once dead or drowning.

## Identified animations
Persons other than the shaman are layered: one body drawn blue (tribe 0), a tribe layer (0x10) per other tribe
recolouring the cloth, and outfit layers (0x20) giving the unit type's gear over a shared body.

Tribesman body (braves, warriors, firewarriors, spies), outfits by `(flags & 0x30, bits 9-10)`: none = brave,
`0x20/1` horned skull helmet and fire in the hands = firewarrior, `0x30/1` grey pointed helmet and armour =
warrior, `0x20/3` long dark hair or hood = spy (by elimination, unsure), `0x20/2` headband and a grey tool =
unknown. Outfits exist on 5, 6, 8-19, 25, 37-40, 48-52.

| Anim | Frames | Action |
|---|---|---|
| 5 | 4 | walk (used) |
| 6 | 6 | standing, breathing (idle) |
| 8 | 6 | kneeling, arms raised (praying) |
| 9, 11 | 4 | walk variants |
| 10 | 1 | standing still |
| 12 | 4 | crouch, leap, arms spread (flung?) |
| 13-15 | 7 | punches; 16 (1) staggering |
| 17 | 12 | throwing fire (firewarrior) |
| 18 | 6 | sitting down |
| 19, 37 | 4 | tumbling in the air |
| 38 | 8 | struck down onto the back (death, last frame lying) |
| 48 | 8 | hands to the face; 49 (9) waving an arm; 50 (7) crouching; 51 (4) sitting |
| 52 | 4 | arms flailing (used for drowning) |
| 20-24 | 4-18 | warrior with built-in helmet and club: attacks |
| 89 | 12 | standing, gesturing; 90-91 (14) bowing to the ground (worship); 92 (17) working |

Preacher body (blue headdress, staff, tribe layers on the gear): 26 (5) and 27 (8) walk, 28 (24) preaching with
purple magic (praying), 29 (4) staff swing, 30 (4) flung, 32 (14) converting, 33-36 gestures, 39/44 (1) lying
dead, 40/45 dying (burning, spirit rising), 41 (3) standing with the staff (first frame = idle), 42 (5) kicking
(used for drowning, unsure), 43 (8) struck down onto the back, 46 (4) tumbling.
Wildmen (no layers): 0 (4) walk, 1 (1) stand, 2 (3) gesture, 3 (1) sitting, 4 (3) crouching, 31/47 (4) flung.

Shamans: 4 consecutive animations per action, one per tribe (blue, red, yellow, green), colours drawn in:

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
