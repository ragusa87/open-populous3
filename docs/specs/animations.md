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
Numbers here are animation indices in `VSTART-0.ANI` (0-98, `AnimBank::start(anim, dir)`).
Persons other than the shaman are layered: one body drawn blue (tribe 0), a tribe layer (0x10) per other tribe
recolouring the cloth, and outfit layers (0x20) giving the unit type's gear over a shared body.

Tribesman body (every follower: braves, warriors, firewarriors, spies, preachers), outfits by
`(flags & 0x30, bits 9-10)`: none = brave, `0x20/1` horned skull helmet and fire in the hands = firewarrior,
`0x20/2` headband, grey vest and a grey item at the hip = warrior, `0x20/3` long dark hair = spy, `0x30/1` grey
pointed helmet, armour and pink shoulder pads = preacher (monk; it covers `0x20/2` when both are drawn). Outfits
exist on 5, 6, 8-19, 25, 37-40, 48-52 (17 only has the firewarrior's and the spy's). All identified in the game.
"Brave only" below: the other outfits have the frames but never play them.

| Anim | Frames | Action |
|---|---|---|
| 5 | 4 | walk (used) |
| 6 | 6 | standing, breathing (idle, used) |
| 7 | 5 | firewarrior throwing fire (attack), gear drawn in (no layers) |
| 8 | 6 | kneeling, raising the arms up and down (praying; worshipping the shaman nearby; used) |
| 9 | 4 | walking, carrying wood (brave only) |
| 10 | 1 | standing still, holding wood (brave only) |
| 11 | 4 | cutting wood (brave only) |
| 12 | 4 | flattening the ground: crouch, arms up, jump (brave only; maybe also wrecking a construction site in an attack); frame 1 stands in for stranded units today |
| 13 | 7 | punching (unchecked) |
| 14 | 7 | receiving a punch |
| 15 | 7 | punching, left foot forward |
| 16 | 1 | tilted, probably while carried by a tornado |
| 17 | 12 | spy setting fire to a building: disguised as a brave (plain) or a firewarrior (`0x20/1`), or discovered (`0x20/3`) |
| 18 | 6 | brave sitting down, being converted by a preacher |
| 19, 37 | 4 | tumbling in the air; also falling down a slope |
| 25 | 7 | fighting: kick |
| 38 | 8 | struck down onto the back (death, last frame lying) |
| 39 | 1 | lying dead |
| 40 | 5 | the body lying, its spirit rising: drowning in the water (used) |
| 48 | 8 | brave hiding its eyes (an idle gesture?) |
| 49 | 9 | waving an arm: crawling, maybe while running away (unsure) |
| 50 | 7 | fighting: crouching to dodge a hit, then punching |
| 51 | 4 | pedalling: moving a vehicle |
| 52 | 4 | running away (bitten by a swarm of bees) |
| 20-24 | 4-18 | attacks with a club, the monk's pointed helmet drawn in (whose attacks: to check) |
| 89 | 12 | scratching itself: a brave idle for too long |
| 90 | 14 | push-ups (warriors idle for too long), firewarrior gear drawn in; 91 (14) push-ups, warrior headband drawn in |
| 92 | 17 | spy idle: juggling |
| 97 | 4 | brave carbonized (struck by lightning); 98 (4) shaman carbonized (probably unused) |

### Stranded (arms up)
A unit that cannot reach its target stands with both arms up. The client holds frame 1 of anim 12
(`catalog::ARMS_UP_FRAME`, `PersonAnim::ArmsUp`): standing, both arms straight up, drawn for every outfit, tribe and
direction (the side views 2 and 6 overlap both arms into one). Anim 12 is the braves' flattening jump: the real
stranded anim is still to find. Ruled out: 48 (hiding the eyes), 49 (waving an arm), 89 (itching).

Old shaman body (anims 26-46, blue headdress, staff, tribe layers on the gear): unused by the game, which draws
the shamans with 53-96. 26 (5) standing, 27 (8) walk, 28 (24) casting a spell (purple magic), 29 (4) punch, 30 (4)
flying in a tornado, 32 (14) another spell, 33-36 and 41 staff gestures (fighting?), 42 (5) foot kick, 43 (8)
falling down (death), 44 (1) lying dead, 45 dying (burning, spirit rising), 46 (4) tumbling.
Wildmen (no layers): 0 (4) walk, 1 (1) stand, 2 (3) eating fruit at a tree, 3 (1) drinking water (still), 4 (3)
drinking water (animated),
31 (4) flung,
47 (4) flying or rolling down.

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
| 81 | moving the magic wand | 1 |
| 93 | kneeling on one knee: praying (at the pyramid; 95 is the yellow one) | 1 |

Reading the sprite as `field / 6` (no -1) looks almost right but shows the next view's sprite once per
loop, and the next tribe's in the back view: the -1 is checked on every shaman pose and tribe.
No animated shaman prayer was found; 90/91 are push-ups (not worship, checked in the game).
