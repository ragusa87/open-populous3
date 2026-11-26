# Original sound banks ("SDT")

Read-only, never shipped. Derived from PopSoundEditor (Toksisitee, GPLv3, Qt) and
cross-checked with a scan of every `.SDT` in `Sound/` of a retail install. Where the editor and the files
disagree, the files win (see "PopSoundEditor discrepancies").

## Files in `Sound/`

| File | Entries | Content |
|---|---|---|
| `Soundd2.sdt` | 495 | SFX, follower voices, ambiences: 16-bit PCM, 22050 Hz |
| `soundd2low.SDT` | 495 | low-quality copy of `Soundd2`: mostly compressed, 11025 Hz (below) |
| `POPfightnew.SDT` | 37 | fight sounds (`Punch31`, `Attk01_a`...): mono PCM, 22050 Hz |
| `PopDrum022.SDT` .. `PopDrum922.SDT` | 4 (`PopDrum222`: 3) | drum loops, stereo PCM, 22050 Hz |
| `PopDrones22.SDT` | 5 | music (`idea11`, `idea13`, `idea16`, `idea17`, `idea19`), MPEG audio Layer II |
| `POPFIGHT.SF2` | | a SoundFont (RIFF `sfbk`), not SDT, not analysed |

Note the file-name case varies (`Soundd2.sdt`, `PopDrum022.SDT`): open them case-insensitively.

## Container
Little-endian.

| Off | Type | Content |
|---|---|---|
| 0 | u32 | entry count `n` |
| 4 | n x u32 | absolute offset of each entry header |

The first header starts right after the table (`4 + 4 * n`). Each entry is a 40-byte header followed by
`data_size` bytes of data. In every bank except the drum banks, entries are back to back and the last one
ends at EOF. Always seek to the table offset (see drums).

## Entry header (40 bytes)

| Off | Type | Content |
|---|---|---|
| 0 | u32 | header size, always 40 |
| 4 | u32 | data size in bytes |
| 8 | char[16] | name, NUL-padded (music: char[8] + 2 unknown u32, below) |
| 24 | u16 | sample rate in Hz: 22050 (also 44100, 11025) |
| 26 | u8 | bits per sample, always 16 |
| 27 | u8 | flags (below) |
| 28 | u32 | always 0 |
| 32 | u32 | loop start (SFX) / decoded size (music) |
| 36 | u32 | loop end (SFX) / 0 (music) |

### Flags (byte 27)
Values seen: `0x02`, `0x03` (PCM), `0x12`, `0x13` (compressed, low bank), `0x25` (music), `0x83`, `0x92`.
- `0x01`: stereo. Clear = mono. Confirmed on the samples: interleaved channels on `0x03`, one channel on `0x02`.
- `0x10`: EA MicroTalk compressed (only in `soundd2low.SDT`, see "Low bank codec").
- `0x20`: MPEG audio (music).
- `0x80`: in `soundd2low` an empty entry (data size 0): that variant plays nothing in low-quality mode. In each
  drum bank, entry 0 is `0x83` and its data is entry 1's header + data shifted by 40 bytes (its range overlaps
  entry 1). The game does play drum entry 0 (it is the calm loop, see "Music and drums"), so it is not a
  do-not-play placeholder there.
- `0x04`: the header's offset-32 field holds the decoded length (music). Same meaning in Dungeon Keeper 2's
  SDT reader, which shares this 40-byte header (ufdada/dk2-tools `sdt_struct.bt`). `0x02` is set on every PCM
  entry, meaning unknown (DK2 uses type 2 for plain 16-bit PCM). The `0x10` compressed type has no DK2 counterpart.

### Sample data (flags without `0x10` / `0x20`)
Raw signed 16-bit little-endian PCM, interleaved L/R when stereo, at the header's sample rate. Wrap it as a
WAV with the same rate, channel count and bit depth.

### Loops (offsets 32 / 36)
In bytes of decoded PCM, from the start of the data. Only 4 entries of `Soundd2` loop, with the same indices
in `soundd2low` (values about halved: half the sample rate):

| Index | Name | Loop start | Loop end | Data size |
|---|---|---|---|---|
| 2 | `fire` | 24058 | 70386 | 95744 |
| 84 | `warloop` | 54922 | 315666 | 387072 |
| 392 | `Shield` | 15490 | 76546 | 91136 |
| 439 | `tweets` | 17210 | 91250 | 105728 |

Bytes, not samples: the loop end in samples would be past the end of the data. Play the start up to loop
end once, then repeat `[start, end)` while the sound is active (the game's stop/release behaviour is
unknown). `0, 0` means no loop.

## `Soundd2.sdt` and `soundd2low.SDT`
- The index is NOT the game's sound id: the exe maps each of its 229 sound ids to a range of entries
  (see "How the original plays them" below). Names hint at the use: `convert`, `erode`, `fire`, `Shrap01`, `chop1..5`,
  `eat*`, `drink*`, `build*`, `v_*` (follower voices: `v_land`, `v_die`, `v_bow`, `v_ok`), `Sv_*`
  (spell voices), `En_Sv_*`, `Wm_*`, `amb_*` / `amb*` (ambiences), `spacey01..10`, `bloonrid_*`.
- `Soundd2`: 443 mono (`0x02`), 52 stereo (`0x03`, all ambiences: `amb_m1*`, `amb_w1*`, `amb2*`,
  `amb_spc*`, `amb_sea*`, `amb_hell*`, `disco*`). 22050 Hz except 107 `blastexp2` and 108 `blastexp3` at
  44100 Hz.
- `soundd2low`: 11025 Hz (9 entries at 22050). 453 entries compressed (`0x10`): data starts with
  `e2 71` (mono) or `e3 71` (stereo), codec not analysed. 42 are plain PCM at 11025 Hz. 39 are `0x92`
  placeholders with no data. Use `Soundd2` and ignore the low bank.
- Same count in both, but ~70 indices carry a different variant of the same family (`chop1` vs `chop5`,
  `Shrap01` vs `shrap03`, name case changes). Variants of a family look interchangeable (the game likely
  picks one at random): do not rely on names matching across the two banks.

## Drum banks `PopDrum{0..9}22.SDT`
- Entries: `<n>a` (`0x83`, placeholder), `<n>a`, `<n>b`, `<n>c` (`PopDrum222`: `2a`, `2a`, `2b`).
  `PopDrum422` and `PopDrum822` order them `a, c, b, a`.
- Stereo 16-bit 22050 Hz loops of 338684 bytes (3.84 s), 677368 in `PopDrum222` (7.68 s). No loop points
  in the header: each is meant to loop whole.
- The bank number and the `22` suffix (22 kHz) come from the file name. How the game picks a bank and a
  pattern is unknown.

## Music `PopDrones22.SDT`
Entry header with a shorter name:

| Off | Type | Content |
|---|---|---|
| 8 | char[8] | name (`idea11`) |
| 16 | u32 | unknown, 4523672 in every entry |
| 20 | u32 | unknown, 8133110..8137734 (differs per entry) |
| 24 | u16 | 22050 |
| 26 | u8 / u8 | 16, flags `0x25` |
| 32 | u32 | decoded PCM size in bytes (16-bit stereo 22050 Hz) |

- The data starts right after the 40-byte header and is a raw MPEG-2 (LSF) Layer II stream: first frame
  header `ff f5 b0 54` = 22050 Hz, 112 kbit/s, joint stereo, no CRC. Save it as `.mp2` or feed it to a
  Layer II decoder.
- Check for `idea11`: 3224868 bytes x 8 / 112000 = 230.3 s, and 20321278 / (22050 x 4) = 230.4 s.
- Lengths: `idea11` 230 s, `idea13` 321 s, `idea16` 284 s, `idea17` 307 s, `idea19` 361 s.

## Playback in the client
docs/architecture.md "Client: audio" for the code. The low-quality bank is left out (always `Soundd2`), and with
it the quality option: what only low quality changes (no ambience bed, no stop fades) does not apply.
- Channels: Music and Fx under a Master volume, plus Mute all (saved). UI sounds go on Fx, flat: centred, full
  volume.
- A `SoundCue` stands for one of the original's sound ids: a family of variants with its own volume (0-127),
  priority and "plays while paused" flag (the id's, sound.md "Sound id table"). `Pick::Any` is the original's random pick; `Pick::Variant(n)` plays
  exactly variant `n` (`chop4`), never another one in its place.
- Mixed as the original mixes them: the listener 16 cells from the camera's look-at point along its heading, 2D
  distances on the wrapped map, a cut-off at 48 cells (not started past it, faded out once out of it), the volume
  times `1 - d²/48²`, an equal-power balance from the bearing folded into -90..90°. Our own stereo source
  (`audio_source.rs`) applies the gains, updated every frame.
- At most 10 effects at once: a full mix cuts its lowest-scored one for a new one scoring at least as much, by the
  original's score ("Sound id table": priority, halved by remoteness and by copies of the same sound). A loop only cuts on its first
  start; coming back (next variant, back in reach, after being cut) it waits for a free slot, else loops would
  keep cutting each other (ours). The same sound never restarts within 250 ms (one-shots) from the same cell; from another cell it does (ours: a
  site's stones land a turn apart, the original keys this by sound id alone), except a person's own sounds
  (eating, drinking, burning, work), keyed by id alone as the original.
- An owner (a unit) gets no new sound while one of its sounds plays.
- Loops: `PlayLoop` under a key, sent again to move it or change its volume (0..1: the ambience layers' weights,
  the battle crowd's loudness), and `StopLoop`. A repeat loop replays the same clip; a variety loop takes a fresh
  variant each time one ends, and when stopped lets the current one finish.
- While the game is paused only sounds flagged for it start or go on; behind a menu only flat sounds; music always.
- Stopped sounds fade out: effects in 0.1 s, music in 1.5 s (lengths ours, the original's not traced).
- Music: a Menu track behind the menus, a Game track in a game picked at random among its variants; each time a
  Game track starts a random drum bank is drawn (none behind the menus: the Menu track plays alone) and its loops chained by the fight intensity (`SetIntensity`, rules in
  "Music and drums"; which melee loop counts as every 4th is our reading). A new drum loop starts the frame
  after one ends: a small gap, not sample-exact.
- Drums, ours (guesses, not the original's rules): calm plays no drums at all (the original plays its calm loop,
  entry 0); the intensity comes from how many of the player's living people attack, challenge or are in a melee
  (`audio_drums::fight_intensity`): none calm, 1-4 attacking (entry 1), `MELEE_ARMY` (5) or more melee (entries
  2/3), instead of the original's attacking/in-a-melee flag.
- Loaded from the install at startup (`audio_load.rs`, nothing with `--no-original`): `PopDrones22` entry 0 as
  the Menu track and entries 1-4 as the Game variants, their raw Layer II streams decoded by symphonia; the ten
  drum banks, each entry as a WAV (entry 0 included, played as stored).
- `Soundd2` cues through the sound id table ("Sound id table", `audio_ids.rs`): each `SoundCue` names its id
  (`SoundCue::id`), plays a random entry of that id's range with the id's volume and priority; the info-window
  click (`confirm`) plays while paused. Ids with no sample stay silent, and the spells past armageddon (shield,
  teleport, bloodlust) have no shout. Not used: the fight bank (`POPfightnew.SDT`, its ids' ranges are not in
  the table), the sync frames.
- Interface and voices (`audio_cues.rs`): the selection's bow lines and the order answers by type and crowd size
  1/2/3/4+ (wildmen count as followers), the flyby's start (`0x6F`) and cancel (`0x6E`), the menu click
  (`0x6A`), a reincarnation site's stones rising (`0x9F`, every second stone, "Pillars" [exe]) and each stone
  head talking once per nod as it tips forward (`0xC1`; the original on its animation's key frame).
- Game events (`audio_world.rs`), each system comparing the map with what it saw last frame (nothing sounds for
  what is there when a map loads), by the triggers of the id table:
  - People: the death cry, the shaman's own by tribe, a ghost's poof (the Angel of Death none: its roar `0xDC`
    and wing flaps are not wired); the drowning cry; a scream when thrown (a shaman at any speed, by tribe; the
    others only rising faster than 90) or sucked into a whirlwind; a grunt for a throw ending standing after
    falling at -100 or faster (the shaman's own; a wildman none); the casting shaman's shout by tribe (ours: as her
    cast starts, the original's at the spell's launch); a wildman converted (`0x05`); turning invisible (once per
    sweep) and visible again.
  - Per turn, owned by the person (one at a time each, so it carries on): burning (`0x51`), eating and drinking
    (each half the time one of its two ids: back to back, as the original's state 15), a wildman's mutter (1 in
    256 turns idle). Work, every `WORK_EVERY` (8) turns of it, ours for the original's one per animation cycle on
    its sync frame: chopping and hacking at a building (`0x01`), picking up wood (`0x0A`), hammering (`0x14`),
    levelling a foundation (`0x02`).
  - Effects appearing: a splash, an explosion or a panicking blast (`0xB2`), a simple blast (`0xA1`), lightning,
    ghost army, land bridge (two ids), the angel's cast and meteor launch, its shockwave (`0xDA`), flatten,
    convert, hypnotism, a spy's firebrand, a ground fire catching (`0xB6`). Loops while they live: variety for the
    whirlwind (stopped under 12 turns left), the swarm, the swamp, the ground fire; the same sample for erosion,
    the earthquake, the volcano and the lava (the original stops the lava's once its flow stops growing: not
    modelled). The earthquake cracks (`0x15`) halfway, the volcano's cone rising (`0xB0`) halfway.
  - Shots launched whoosh (`0xA1`): Blast's fireball, a firewarrior's bolt.
  - Vehicles: the moving loop from setting off to parking; going down or up wrecked, or gone without that,
    `boatexp`/`bloonpop`.
  - Buildings: the burning crackle while its countdown is above 79; the bell when one is finished.
  - The battle crowd (flat repeat loop): the summed closeness (0..1) of the fights after the 6th, in the
    coordinators' order, capped at 1. The shield hum (flat repeat loop) while a shielded person is alive within
    48 cells of the listener (ours for "drawn near the camera").
- The ambience (`audio_ambience.rs`), every frame: the cells within `NEAR` (12, ours for "drawn near the
  camera") of the camera's look-at point counted as sea (at sea level), high land (over 0x200) or low land; the
  layers of the level's theme weighted by share (the shore by twice the lesser of low land and sea, as a share too:
  ours, the original counts its renderer's cells), the three heaviest non-zero ones playing as flat variety loops
  at weight/255. Seagulls when every near cell is sea (not in hell) and a wildlife call of the theme while a tree is
  near, at the original's 1 in 256 and 1 in 128 per frame counted at 30 frames a second (ours: its frame rate is
  not in the docs). No space layer (no special view here).
- Not done yet: the fight bank and the melee moves' sounds, CC0 sounds for `--no-original`, the clips' own loop
  points (`fire`, `warloop`...), work sounds on their sync frame, reverb, the original's fight flag for the drums, and the triggers with no client-side
  state yet (birth, a whirlwind's wood crack and branch snap, a building creaking, the fight alarm, the interface
  clicks of the panel, discoveries, the soul swirls).

## PopSoundEditor discrepancies
The editor works on its own exports but misreads parts of the format. Do not copy these:
- It reads byte 27 as the channel count. It assumes 2 for SFX and halves the sample rate (exports 11025 Hz
  "stereo"): it plays at the right byte rate, but the files are really mono 22050 Hz. A `0x03` entry is
  exported as a 3-channel WAV. Its WAV `ByteRate` uses the unhalved rate.
- It previews 16-bit PCM as unsigned. The data is signed.
- Loop points are dropped on export. When it builds a sound bank, it hardcodes the 4 loops above by index
  and writes `0` at offset 28.
- When it builds a drum bank, it writes the WAV channel count (2) at bytes 27 and 39: flags `0x02` instead
  of `0x03`, and no `0x83` first entry.
- Music export skips 80 bytes instead of 40: it drops the first 40 bytes of the stream and copies 40 bytes
  of the next entry's header (past EOF for the last entry). Decoders resync, so it still plays. Building a
  music bank is not implemented.
- It cannot read `soundd2low.SDT` (it would dump compressed data as PCM).

## How the original plays them
From the original exe (pop3-rev-analysis.md would hold the formal spec; the full per-id trigger list is in "Sound id
table" below).

### Sound ids
- The game plays a **sound id** (0x00-0xE4), never a bank index. A per-id record gives: first entry and variant
  count in `Soundd2` (one variant picked at random per play), an optional alternative range in
  `POPfightnew.SDT` used whenever that bank is loaded (the punch/sword/yell/grunt ids of melee and battle cries),
  priority, a pitch spread in percent (pitch = 100 +/- random(spread)) and a volume 0-127. Entry numbers in that
  record are 1-based (0 = no sound; 23 ids are silent that way).
- `soundd2low.SDT` is the same id table read against the low bank, chosen by the "high quality sound" option.
- Positional sounds are attached to a unit and attenuated by distance to the local camera, culled beyond a
  cut-off. UI sounds play flat. A unit with a sound attached is not given another ambient sound until it ends,
  which is how per-tick sounds (crackle, eating, drinking) sound continuous instead of stacking.
- Looping sounds (fire, wind, swarm, swamp, guard post, boat/balloon ride, burning building) are started with a
  loop flag and explicitly stopped by the code that owns them.
- While paused only sounds flagged for it play (info window clicks); over overlay screens only UI sounds play.

### Mixing (positional sounds, effects)
- Listener: a point 16 cells from the camera's look-at point along its heading. Distances are 2D on the wrapped
  map (height ignored).
- Hard cut-off at 48 cells: a farther sound is not started, and a playing one is stopped once out of range.
- Inside it, `closeness = 127 * (1 - d^2 / 48cells^2)`. The original does not scale volume itself: it hands the
  sound to a 3D mixer (QSound) at full table volume, as a source at azimuth -90..+90 deg (bearing relative to the
  camera, front and back folded together) and range `(127 - closeness) * 16`, and the mixer's default distance
  model does the fading. A reimplementation can approximate the fade as volume x (1 - d^2/D^2) with an equal-power
  pan from that azimuth.
- 2D/UI sounds: no position, centred, full volume.
- Updated every frame while the unit and camera move. At most 10 effect channels: when full, the quietest-ranked
  one (low priority, far, many copies of the same id) is cut if the new sound outranks it. The same id never
  restarts within 250 ms.
- Work sounds (chop, hammer, stomp, eat, drink) wait for the unit's animation to reach a given frame, so they
  land on the hit.
- Looping: true loops (same sample) for the battle crowd, shield hum and rumbles; "variety loops" (a fresh random
  variant each time one ends) for fire, wind, swarm, swamp, guard post, boat/balloon. Stopping a variety loop lets
  the current variant finish.
- Effects: reverb only (EAX "Plain" outdoors, a long stone-corridor reverb in the special view); short fades on
  stop in high-quality mode, longer fades on music; some cinematic swooshes circle the listener. No pitch
  variation (a per-id pitch spread exists in the data but is never applied), no doppler, no filters.

### Voices
- Selecting: one unit -> its type's "bow" line (preacher, spy, shaman, generic follower); a group layers up to
  four lines at once - spies, preachers, the shaman, other followers - each picking a 1/2/3/4+ crowd variant.
- Ordering a selection: the shaman's "yes", a generic "ok" if braves/warriors/firewarriors are in it, plus spy and
  preacher "ok" lines by count.
- Casting: the caster shaman shouts the spell's own `Sv_*` line (local player) or `En_Sv_*` line (any other tribe).
- Death cry for any person, `Sv_die` / `En_Sv_die` for shamans; screams on being blown up or sucked into a
  whirlwind, grunts on landing hard.

### Ambience
- A continuous bed of up to three looping-by-restart layers, re-weighted every frame from what is drawn near the
  camera: low land, shoreline, high ground (wind), open sea (plus a "space" layer in the special view). The level's
  theme swaps the layer set (a hell set for theme 30, a different sea layer for theme 16). Only with high quality.
- Random one-shots: seagulls when only water is in view, wildlife calls when trees are in view (theme-specific),
  and a magical-shield hum while shielded units are in view.
- A battle-crowd loop whose volume grows with the number and nearness of melee fights, silent below 7 fights.

### Music and drums
- `PopDrones22` track 1 (`idea11`) plays in the frontend; in a level one of tracks 2-5 is picked at random. It
  loops for the whole level at the music volume.
- Each time music starts, a random drum bank `PopDrum0..922` is loaded and drum loops are chained one after the
  other, chosen from a per-turn intensity: 0 when the local player is not fighting (entry 0, the calm loop; after
  a fight entry 1 once first), 1 while a local unit is attacking (entry 1), 2 while a local unit is in a challenge
  or melee (entry 2, and every 4th loop entry 3 when the bank has one).
- Alternative CD-audio mode: plays the selected CD track instead, no drums.

### Not played at all
Sound ids present but never played: a chop variant, `oh001`, the two/three-voice "ok" lines (`v_ok2_*`,
`v_ok3_*`), `Snapshot`, `selectfe`, several `fire`/`sitespel` duplicates. Entries no id points at: `Wdrop01/02`,
`En_Sv_grunt01-04`.

### Sound id table
From the pop3-rev catalog (`docs/sound.tsv`, `docs/audio.md` there; addresses in pop3-rev-analysis.md style are
left out here). One 12-byte record per id `0x00`-`0xE4`:
- first `Soundd2` entry (1-based in the exe, 0 = silent: 23 ids) and a variant count: one of `[first, first+count)`
  at random per play. The table below gives the **0-based** entry range (`chop1..chop5 [15-19]`: entries 15-19).
- the same for the fight bank (`POPfightnew.SDT`), used instead when the id's fight flag is set and that bank is
  loaded: ids `0x0D`, `0x0E` (punches), `0x27` (swords), `0x2B` (sword flashes), `0x2D`-`0x30` (attack yells and
  the crowd cheer), `0x32` (hit grunts). Their `Soundd2` range is unrelated junk: the fight bank is needed. The
  fight-bank range equals the `Soundd2` one (same indices in `POPfightnew.SDT`).
- an animation-sync frame: a positional sound waits until its unit's animation frame reaches it (1: chop, eat,
  drink, hammer, wildman eat/drink; 3: stomp), and is dropped if the unit's animation changes first.
- a priority (0x20 faint ambient ... 0xB4 spells, 0xE6 volcano rise, 0xFF UI and voices), a volume 0-127.
- a pitch spread that nothing reads: every sample plays at its own rate.
- No "plays while paused" in the record: that is a flag of the play call; only the info-window clicks (`0x6A`
  `confirm`) use it.

Play call flags: positional or 2D (UI: full volume, centred); `0x2` true loop (the same sample, `warloop`, the
shield hum, the erosion/lava/quake/volcano rumbles); `0x40` chained variety loop (fire, wind, swarm, swamp, guard
post, boat/balloon, burning building), stopped by its owner, the current variant finishing; `0x4` an ambience-bed
slot. The local shaman's shouts (`0x74`-`0x86`) are not replayed while one plays on her.

Voice stealing, exactly: with 10 effects playing, each one's score is priority x 256, halved once per 16 steps of
remoteness (positional only: `127 - closeness`), x 1024, then halved once per other live copy of the same id (twice
for loops); the lowest is cut (no fade) if the new sound scores at least as much, else the new one is dropped.

Selection voices: one person alone, by type: preacher `0x57`, spy `0x56`, shaman `0x18`, anyone else `0x58`. A
group layers one line per type by count 1/2/3/4+: spies `0x56`/`0x46`/`0x47`/`0x48`, preachers
`0x57`/`0x49`/`0x4A`/`0x4B`, the shaman `0x18`, the others `0x58`/`0x43`/`0x44`/`0x45` (the 2/3/4+ ids share their
samples). Orders: `0x19` shaman, `0x37` followers, spies `0x3B`/`0x3C`/`0x3D`/`0x3E`, preachers `0x3F`-`0x42`.

Cast shouts: `0x74 + spell type` for the local tribe, `0x8A + spell type` for the others, on the shaman (2D if she
is dead). By spell: burn and blast `Sv_blast`, lightning `Sv_bolt`, whirlwind `Sv_wind`, swarm `Sv_insect`,
invisibility `Sv_invis`, hypnotism `Sv_hypno`, firestorm `Sv_fstm`, ghost army `Sv_ghost`, erosion `Sv_erode`,
swamp `Sv_swamp`, land bridge `Sv_bridge`, angel of death `Sv_angel`, earthquake `Sv_quake`, flatten `Sv_flat`,
volcano `Sv_volc`, convert `Sv_dance`, armageddon `Sv_blast` (`En_Sv_*` likewise). Spell types past armageddon
(bloodlust, shield, teleport) land on other ids: they have no shout of their own.

Ambience bed, every frame, high quality only: five slots, each an id and a weight from what was just drawn near
the camera; the three heaviest non-zero ones play as 2D loops at weight x volume:
| Slot | Normal theme | Theme 16 | Theme 30 (hell) | Weight |
|---|---|---|---|---|
| low land | `0x1D` | `0x1D` | `0xC7` | low-land cells x 256 / near cells |
| shore | `0x1E` | `0x1E` | `0xC8` | 2 x min(low land, water) |
| high land | `0x1F` | `0x1F` | `0xC9` | cells higher than 0x200 x 256 / near cells |
| space | `0x21` | `0x21` | `0x21` | 0, or 255 in the special view |
| sea | `0x20` | `0xD5` | `0xCA` | water cells x 256 / near cells |
Random, not in the special view: seagulls `0x50` 1/256 per frame when every near cell drawn is water (not theme
30); wildlife 1/128 per frame while trees are in view (`0xC2` theme 22, `0xC6` theme 30, else `0x1C`); the shield
hum `0xC3`, 2D, while any shielded person is drawn near the camera. In the special view: `0x54` 1/128 per frame.

Battle loop `0x6C` (`warloop`, 2D true loop), every turn: the sum, over every melee fight after the 6th, of a
contribution growing as the fight gets closer to the camera; played at min(sum, 127) when above 0, else stopped.

Correction: pop3-rev-analysis.md "Statuses" reads sound `0xCF` as a person's low-health loop; the id table gives
`0xCF` as `tweets`, birds on a building flagged with the same bit (`named`, not traced): which is right is open
(TODO.md "Sound"); nothing plays it here.

| Id | Entries | Prio | Vol | Sync | Use |
|---|---|---|---|---|---|
| 0x01 | chop1..chop5 [15-19] | 0x80 | 127 | 0x1 | Axe chop (flags 0x10, attached to the unit). Brave chopping a tree: one per completed chop-animation cycle. Brave taking wood from a tree on the way to deliver it. Construction brave entering 'take wood from target'. Unit hacking at an enemy building: one per blow cycle of 8-23 ticks. |
| 0x02 | Stomp4..stomp [273-276] | 0x80 | 63 | 0x3 | Stomp. Brave levelling a building foundation: each time one foundation cell is relevelled one step. Also the one-tick 'purify land' effect, whose spawner is not found. |
| 0x03 | Eat1..eat5 [20-24] | 0x60 | 63 | 0x1 | Eating, 50/50 with 0xBE: person state 15, every tick no sound is attached to the unit. |
| 0x04 | drink1..drink7 [26-32] | 0x60 | 127 | 0x1 | Drinking, 50/50 with 0xBF: wildman drinking at the shore, every tick while its timer runs, it is not busy and no sound is attached. |
| 0x05 | convert [0] | 0x80 | 127 | - | Conversion 'convert' chime, with a class-7 type-0x3A flash on kill-and-replace. A wildman is turned into a brave by the Convert spell or the Reincarnation Centre ground flatten, both via. A preacher's conversion roll succeeds. Person state 32 runs out and the unit becomes a brave of another tribe; no writer of state 32 found. |
| 0x06 | Fire_04..Fire_01 [364-367] | 0x50 | 127 | - | Ground fire crackle loop (flags 0x40) on a burning ground-fire scenery object. |
| 0x09 | v_land01..v_land07 [122-128] | 0x80 | 80 | - | Landing grunt: a BLOWN person lands on non-water ground with vel.y <= -100 and survives the fall damage (applied at <= -200). Not shaman (0x1B), not wildman (silent). |
| 0x0A | chop1..chop5 [15-19] | 0x80 | 127 | - | Chop while picking up wood: construction brave in 'take wood from target', every tick of the 3-tick pickup cooldown when the target is a wood pile (class 5 type 0xB). |
| 0x0B | chop1..chop5 [15-19] | 0x80 | 127 | - | Wood drop: one per 100 carried wood dumped as a class-5 type-0xB wood pile. Via (called from 8 order handlers), person state 19 recovery, a person being converted (state 23), forced eviction (state 41), and a dead person's rising spirit once 512 above ground. |
| 0x0C | chop1..chop5 [15-19] | 0x80 | 127 | - | ORPHAN: chop samples, no call site. |
| 0x0D | fight:Punch31..Punch1 | 0x80 | 102 | - | Punch (fight bank Punch*). Challenge shove: the attacker's windup timer (pre_fight_turn sub-state 6) reaches 0. Melee: the stance 2 or 4 timer expires on a fighter (fight_turn). |
| 0x0E | fight:Punch31..Punch1 | 0x80 | 102 | - | Punch (fight bank Punch*): melee stance 3 blow struck by a NON-warrior, whatever the opponent's type. |
| 0x0F | Sv_die [227] | 0x80 | 63 | - | Local player's shaman dies ('Sv_die'): death-enter. |
| 0x10 | v_die015..v_die001 [129-143] | 0x80 | 80 | - | Death cry of any non-shaman person on entering person state 3. Ghosts are deleted silently instead. |
| 0x11 | V_drwn01..V_drwn03 [10-12] | 0x80 | 80 | - | Drowning: once when a person enters state 2, together with the type-0x41 splash. |
| 0x12 | Wexp2..Wexp1 [93-94] | 0x80 | 127 | - | Wood-structure crack. A whirlwind pulls wood from a building and it loses a construction stage. Earthquake effect type 0xE knocks 100 wood off a building and its stage drops. Both play with the mesh shatter. |
| 0x13 | Shrap01..Shrap03 [4-6] | 0x80 | 63 | - | Flying building-debris piece lands on non-water ground, with its dust puff (controller state 7). |
| 0x14 | build1..build5 [68-72] | 0x80 | 16 | 0x1 | Hammering (flags 0x10): a brave building a structure, on each completed hammer cycle while the plan is still under construction. The next swing comes 16-31 ticks later. |
| 0x15 | quake [25] | 0x80 | 127 | - | Earthquake: the big 'quake' crack, once at the midpoint (tick 60) of the 120-tick quake effect. |
| 0x16 | oh001..oh014 [255-268] | 0x60 | 45 | - | Wildman idle mutter ('oh'): 1/256 per tick in state 8. |
| 0x17 | oh001 [255] | 0x20 | 10 | - | ORPHAN: single quiet 'oh001' (volume 10), no call site. |
| 0x18 | Sv_sel02..Sv_sel01 [41-42] | 0x80 | 100 | - | Shaman selection voice ('Sv_sel'): the shaman is selected alone or is part of a selected group. |
| 0x19 | Sv_yes2..Sv_yes3 [491-492] | 0x80 | 100 | - | Shaman 'yes' ('Sv_yes'): the player orders a selection that contains the shaman. Played on the tribe's shaman and attenuated only for the local shaman. |
| 0x1A | Sv_scrm01..Sv_scrm02 [37-38] | 0x80 | 100 | - | Local player's shaman scream ('Sv_scrm'): first tick it is blown (any speed), or sucked into a whirlwind. |
| 0x1B | Sv_grnt01..Sv_grnt04 [33-36] | 0x80 | 100 | - | Shaman landing grunt ('Sv_grnt'): a blown shaman lands with vel.y <= -100. |
| 0x1C | amb1001..amb1020 [45-64] | 0x40 | 32 | - | Ambient wildlife one-shot ('amb10xx', 20 variants), 1/128 per frame while trees are in view; default theme. Suppressed over overlay screens. |
| 0x1D | amb_m1005..amb_m1001 [150-154] | 0x40 | 32 | - | Ambience bed: low land ('amb_m1'), weighted by low-land cells near the camera. See audio.md 'Ambience bed'. |
| 0x1E | amb2006..amb2001 [161-166] | 0x40 | 32 | - | Ambience bed: shoreline ('amb2'), weighted by min(low land, water) in view. |
| 0x1F | amb_w1006..amb_w1001 [155-160] | 0x40 | 32 | - | Ambience bed: high ground ('amb_w1', wind), weighted by cells higher than 0x200. |
| 0x20 | amb_sea01..amb_sea04 [332-335] | 0x40 | 32 | - | Ambience bed: open sea ('amb_sea'), weighted by water cells in view. Replaced by 0xD5 in theme 16 and 0xCA in theme 30. |
| 0x21 | spacey10..spacey01 [441-450] | 0x40 | 32 | - | Ambience bed: 'spacey', weight 255 only in the special/overlay view, otherwise 0. |
| 0x22 | ghostdi3..ghostdie [87-89] | 0x80 | 127 | - | Ghost poof ('ghostdie'), with a class-7 type-0x20 spirit puff. A Ghost Army ghost is destroyed. Armageddon's start sweep removes every wildman and Angel of Death from the map. |
| 0x23 | fgtalarm [73] | 0xB4 | 127 | - | Fight alarm (UI, 'fgtalarm') while a combat-alert record is valid. Cell visible to the local player: 4 pulses per 32 turns (turn%32 = 0, 20, 24, 28). Not visible: once per 32 turns, only if not already playing. |
| 0x24 | bldgstmp [74] | 0x80 | 63 | - | Place-building click (UI, 'bldgstmp'): valid placement of the selected building plan (ui_cmd 0x77/0x78 -> tribe_cmd 0xE, or 0x62 for a guard post). |
| 0x25 | wrong [75] | 0x80 | 63 | - | 'Wrong' buzz (UI): building placement rejected (ui_cmd 0x77/0x78). Also raise/lower-land commands (ui_cmd 0x68/0x69, drag mode 0xE) while the land-edit flag is 0. That flag is never set non-zero, so those commands always buzz. |
| 0x26 | rotate [110] | 0x80 | 63 | - | Rotate the building plan (UI, ui_cmd 0x7B): the selected type's orientation cycles 0-3. |
| 0x27 | fight:swords8..swords1 | 0x80 | 50 | - | Sword clash (fight bank swords*): melee stance 3 blow, warrior striking a WARRIOR. 0x32 does not play with it. |
| 0x28 | birth_temp1 [351] | 0x80 | 30 | - | Birth ('birth_temp1'): a hut of the LOCAL player produces a new brave. |
| 0x29 | l_bridge [76] | 0x80 | 70 | - | Land Bridge cast ('l_bridge'), layered with 0xAB on the same tick. |
| 0x2A | hypno [77] | 0x80 | 70 | - | Hypnotism cast, once on effect init. |
| 0x2B | fight:sw_flsh6..sw_flsh1 | 0x80 | 70 | - | Sword flash (fight bank sw_flsh*): melee stance 3 blow, warrior striking a NON-warrior. 0x32 plays on the opponent with it. |
| 0x2C | w_wtr03..w_wtr01 [80-82] | 0x80 | 127 | - | Water splash ('w_wtr'). Splash type 0x41 init (drowning, debris or a spirit hitting water). Splash type 0x2B init over water: fireball into water, whirlwind, lava-flow gloop. |
| 0x2D | fight:Attk01_c..Attk01_a | 0x80 | 96 | - | Attack yell, 1 attacker (fight bank Attk01). Group battle cry: an attack group closes from >0xA00 to <0x600 of its target, queued at and flushed by. Armageddon fight: random crowd yell for a tribe with few fighters left (n<6). |
| 0x2E | fight:Attk02_b,Attk01_c | 0x80 | 96 | - | Attack yell, 2 attackers (Attk02). Same battle-cry queue (group of 2). Armageddon crowd yell, n<18. |
| 0x2F | fight:Attk03_c..Attk03_a | 0x80 | 96 | - | Attack yell, 3 attackers (Attk03). Same queue (group of 3). Armageddon crowd yell, n>=18. |
| 0x30 | fight:Attk99_a | 0x80 | 96 | - | Crowd cheer (Attk99). Battle cry for a group of 4 or more. Victory: 50% per turn while bit 2 is set (local tribe won; UI). Armageddon: 50% per tick in sub-state 2, and once per tribe when it is ordered to march into the arena. |
| 0x31 | Invis [83] | 0x80 | 127 | - | Invisibility takes effect: the radius sweep made at least one unit invisible (effect state 14). |
| 0x32 | fight:v_hit10..v_hit01 | 0x80 | 80 | - | Hit grunt (fight bank v_hit*) on the OPPONENT: only together with 0x2B, i.e. a warrior hitting a non-warrior. |
| 0x33 | v_pre_pre001..v_pre_pre004 [215-218] | 0x80 | 43 | - | Local preacher's sermon voice ('v_pre_pre'): in the preach wait, every 16th tick with 50% odds a gesture starts. The voice plays only if the preacher has hit a listener. |
| 0x34 | Wstress3..Wstress1 [90-92] | 0x80 | 63 | - | Building creak ('Wstress', wood stress, not a siren). Alerted building (+0x9C bits 0x10 and 0x2): every 4th tick with a 1-in-4 roll. Wear/decay tick: a 100-wood chunk is lost and the building survives. |
| 0x35 | Invisoff [117] | 0x80 | 96 | - | Invisibility off. The invisibility counter (+0xA3) runs out. Or an invisible unit reveals itself by acting. |
| 0x36 | Alight04..Alight01 [118-121] | 0x80 | 96 | - | Spy arson: the spy throws the firebrand at the end of its windup (live-confirmed 2026-11-12). |
| 0x37 | v_ok005..v_ok001 [191-195] | 0xFF | 63 | - | Order acknowledgement 'ok' ('v_ok'): the player orders a selection containing any brave, warrior or firewarrior, on the first ordered unit. |
| 0x38 | v_ok2_001..v_ok2_002 [183-184] | 0xFF | 63 | - | ORPHAN: 'v_ok2' (two-voice ok). Jump-table slot at is unreachable: the index is only ever 0 or 1. |
| 0x39 | v_ok3_001..v_ok3_002 [185-186] | 0xFF | 63 | - | ORPHAN: 'v_ok3', same unreachable slot. |
| 0x3A | v_ok3_001..v_ok3_002 [185-186] | 0xFF | 63 | - | ORPHAN: 'v_ok3', same unreachable slot (also the default case). |
| 0x3B | v_spy_ok001..v_spy_ok004 [201-204] | 0xFF | 63 | - | Spy 'ok', 1 spy in the ordered selection. |
| 0x3C | v_spy_ok2_001..v_spy_ok2_002 [205-206] | 0xFF | 63 | - | Spy 'ok', 2 spies. |
| 0x3D | v_spy_ok2_001..v_spy_ok2_002 [205-206] | 0xFF | 63 | - | Spy 'ok', 3 spies. |
| 0x3E | v_spy_ok2_001..v_spy_ok2_002 [205-206] | 0xFF | 63 | - | Spy 'ok', 4+ spies. |
| 0x3F | v_pre_ok001..v_pre_ok004 [211-214] | 0xFF | 63 | - | Preacher 'ok', 1 preacher in the ordered selection. |
| 0x40 | v_pre_ok001..v_pre_ok004 [211-214] | 0xFF | 63 | - | Preacher 'ok', 2 preachers. |
| 0x41 | v_pre_ok001..v_pre_ok004 [211-214] | 0xFF | 63 | - | Preacher 'ok', 3 preachers. |
| 0x42 | v_pre_ok001..v_pre_ok004 [211-214] | 0xFF | 63 | - | Preacher 'ok', 4+ preachers. |
| 0x43 | v_bow2_001..v_bow2_003 [177-179] | 0x80 | 127 | - | Selection voice, 2 followers selected (not spy/preacher/shaman; 'v_bow2'). |
| 0x44 | v_bow3_001..v_bow3_003 [180-182] | 0x80 | 127 | - | Selection voice, 3 followers ('v_bow3'). |
| 0x45 | v_bow3_001..v_bow3_003 [180-182] | 0x80 | 127 | - | Selection voice, 4+ followers ('v_bow3'). |
| 0x46 | v_spy_bow2_001..v_spy_bow2_002 [199-200] | 0x80 | 127 | - | Selection voice, 2 spies. |
| 0x47 | v_spy_bow2_001..v_spy_bow2_002 [199-200] | 0x80 | 127 | - | Selection voice, 3 spies. |
| 0x48 | v_spy_bow2_001..v_spy_bow2_002 [199-200] | 0x80 | 127 | - | Selection voice, 4+ spies. |
| 0x49 | v_pre_bow001..v_pre_bow004 [207-210] | 0x80 | 127 | - | Selection voice, 2 preachers. |
| 0x4A | v_pre_bow001..v_pre_bow004 [207-210] | 0x80 | 127 | - | Selection voice, 3 preachers. |
| 0x4B | v_pre_bow001..v_pre_bow004 [207-210] | 0x80 | 127 | - | Selection voice, 4+ preachers. |
| 0x4C | bloonpop [189] | 0x80 | 127 | - | Balloon burst ('bloonpop'), with a simple blast. Balloon going down over land (vehicle state 5), carried above 1024 (state 6), or tumbling with its countdown out (state 9). |
| 0x4D | boatexp [190] | 0x80 | 127 | - | Boat explodes ('boatexp'): same three cases as 0x4C for a boat; in state 9 also when it hits the ground. |
| 0x4E | bloonrid_4..bloonrid_1 [487-490] | 0x80 | 70 | - | Balloon flight loop (flags 0x40) while flying (vehicle state 4). Stopped on landing, or when it enters any other state. |
| 0x4F | boatrid_1..boatrid_4 [483-486] | 0x80 | 70 | - | Boat rowing loop (flags 0x40) while under way (vehicle state 3). Stopped on beaching, or when it enters any other state. |
| 0x50 | amb_gull001..amb_gull007 [279-285] | 0x40 | 32 | - | Seagulls (UI): 1/256 per frame when every near cell drawn is water. Not in theme 30. |
| 0x51 | v_onfire01..v_onfire03 [252-254] | 0x80 | 63 | - | On-fire scream ('v_onfire'), every tick no sound is attached: scorched/panicking person (state 26) and burning person (state 31). |
| 0x52 | v_hdprey01..v_hdprey04 [269-272] | 0x80 | 40 | - | Worship prayer chant ('v_hdprey') while praying at a head/totem, any non-shaman with no sound attached. |
| 0x53 | Bldg_fire04..Bldg_fire01 [368-371] | 0x50 | 90 | - | Burning building crackle loop (flags 0x40): every turn while the burn countdown is above 79 (the first ~48 turns). STOPPED on the damage turn at countdown 79, so the remaining ~79 turns are silent. Live-confirmed timeline 2026-11-12. |
| 0x54 | amb_spc007..amb_spc001 [325-331] | 0x40 | 32 | - | Space ambience ('amb_spc'), 1/128 per frame, only in the special/overlay view. |
| 0x55 | Bouldersplash01..Bouldersplash03 [336-338] | 0x80 | 127 | - | Firestorm/volcano fireball hits a WATER cell ('Bouldersplash'), with the type-0x2B splash (and its own 0x2C). |
| 0x56 | v_spy_bow001..v_spy_bow003 [196-198] | 0x80 | 127 | - | Spy selection voice: a spy selected alone, or 1 spy in a group. |
| 0x57 | v_pre_bow001..v_pre_bow004 [207-210] | 0x80 | 127 | - | Preacher selection voice: a preacher selected alone, or 1 preacher in a group. |
| 0x58 | v_bow001..v_bow007 [170-176] | 0x80 | 127 | - | Follower selection voice ('v_bow', 7 variants): brave/warrior/firewarrior/wildman selected alone, or 1 such follower in a group. |
| 0x60 | Sel_bldg [146] | 0xFF | 63 | - | GUI click 'Sel_bldg': an in-game GUI-dialog widget of kind 2-6 activated (building-panel buttons; meaning named only). |
| 0x62 | (no sample) | 0xFF | 127 | - | SILENT (no sample): pointer mode reset to 0xC. |
| 0x63 | Snapshot [9] | 0xFF | 63 | - | ORPHAN: 'Snapshot' (camera shutter), no call site. |
| 0x64 | (no sample) | 0xFF | 127 | - | SILENT (no sample): minimap click. The camera jump it queues plays 0xDE. |
| 0x66 | selectfe [149] | 0xFF | 63 | - | ORPHAN: 'selectfe' (frontend select), no call site in this exe. |
| 0x6A | confirm [79] | 0xFF | 63 | - | Generic UI 'confirm' click. Cast the armed spell at a target (ui_cmd 0x6A/6B, 0xC4). Send the shaman to a point (0x6D). Select own person / group move order (0x6E/6F). Track/select commands (0x74-0x76, 0x7D). Building toggle (0xB2, 0xB3). Raise/lower land in the Reincarnation Centre intro (0xC2/0xC3). Commit a queued order target (0x7E/7F). Hover-popup row/header clicks. Message-icon clicks. GUI widget clicks. Info-window list/button/query clicks (flags 0x801, also while paused). Modal dialog buttons. |
| 0x6B | sham_rtn [85] | 0xFF | 127 | - | Local player's shaman reincarnates (UI, 'sham_rtn'). |
| 0x6C | warloop [84] | 0xFF | 127 | - | Battle crowd loop ('warloop', 2D): only with 7+ melee fights in the world; volume = summed proximity of the fights past the 6th to the local camera. Stopped when the total is 0. |
| 0x6D | Pageturn [144] | 0xFF | 63 | - | Page turn: a GUI page/dialog opens in game mode==2. |
| 0x6E | Zoom_out [147] | 0xFF | 63 | - | 'Zoom_out' click: UI toggle button when tribe+0x93D bit 0x80 is clear; queues tribe_cmd 0x5F(1). |
| 0x6F | Zoom_in [148] | 0xFF | 63 | - | 'Zoom_in' click: same toggle with the bit set; queues tribe_cmd 0x5F(0). |
| 0x70 | discosky [169] | 0xFF | 127 | - | Discovery claimed (UI, 'discosky'): a person reaches a discovery, or a trigger hands a discovery straight to a tribe. |
| 0x71 | discopop [168] | 0xFF | 127 | - | Discovery cinematic starts ('discopop'): spell discovery, or discovery kind 0xB. |
| 0x72 | discoend [167] | 0xFF | 127 | - | Spell/swirl cinematic reaches its arrival stage ('discoend'); stopped when the cinematic ends. |
| 0x73 | Sel_spel [145] | 0xFF | 63 | - | GUI click 'Sel_spel': a GUI-dialog widget of kind 9-0x21 activated (spell-panel buttons). |
| 0x74 | Sv_dance..Sv_dance2 [225-226] | 0x80 | 100 | - | ORPHAN: shout slot for spell type 0 (none) of the local shaman table; never reached. |
| 0x75 | Sv_blast..Sv_blast2 [219-220] | 0x80 | 100 | - | Local player shaman cast shout for spell type 1 (burn): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x76 | Sv_blast..Sv_blast2 [219-220] | 0x80 | 100 | - | Local player shaman cast shout for spell type 2 (blast): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x77 | Sv_bolt..Sv_bolt2 [221-222] | 0x80 | 100 | - | Local player shaman cast shout for spell type 3 (lightning): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x78 | Sv_wind..Sv_wind2 [246-247] | 0x80 | 100 | - | Local player shaman cast shout for spell type 4 (whirlwind): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x79 | Sv_insect..Sv_insect2 [238-239] | 0x80 | 100 | - | Local player shaman cast shout for spell type 5 (swarm): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x7A | Sv_invis..Sv_invis2 [240-241] | 0x80 | 100 | - | Local player shaman cast shout for spell type 6 (invisibility): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x7B | Sv_hypno..Sv_hypno2 [236-237] | 0x80 | 100 | - | Local player shaman cast shout for spell type 7 (hypnotism): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x7C | Sv_fstm..Sv_fstm2 [232-233] | 0x80 | 100 | - | Local player shaman cast shout for spell type 8 (firestorm): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x7D | Sv_ghost..Sv_ghost2 [234-235] | 0x80 | 100 | - | Local player shaman cast shout for spell type 9 (ghost army): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x7E | Sv_erode..Sv_erode2 [228-229] | 0x80 | 100 | - | Local player shaman cast shout for spell type 10 (erosion): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x7F | Sv_swamp..Sv_swamp2 [244-245] | 0x80 | 100 | - | Local player shaman cast shout for spell type 11 (swamp): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x80 | Sv_bridge..Sv_bridge2 [223-224] | 0x80 | 100 | - | Local player shaman cast shout for spell type 12 (land bridge): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x81 | Sv_angel2..Sv_angel [250-251] | 0x80 | 100 | - | Local player shaman cast shout for spell type 13 (angel of death): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x82 | Sv_quake..Sv_quake2 [242-243] | 0x80 | 100 | - | Local player shaman cast shout for spell type 14 (earthquake): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x83 | Sv_flat..Sv_flat2 [230-231] | 0x80 | 100 | - | Local player shaman cast shout for spell type 15 (flatten): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x84 | Sv_volc2..Sv_volc [248-249] | 0x80 | 100 | - | Local player shaman cast shout for spell type 16 (volcano): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x85 | Sv_dance..Sv_dance2 [225-226] | 0x80 | 100 | - | Local player shaman cast shout for spell type 17 (convert): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x86 | Sv_blast..Sv_blast2 [219-220] | 0x80 | 100 | - | Local player shaman cast shout for spell type 18 (armageddon): 0x74 + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. Not replayed while another 0x74-0x86 shout is still playing on the same shaman. |
| 0x88 | En_Sv_die [296] | 0x80 | 63 | - | Death of another tribe's shaman ('En_Sv_die'). Also the overflow id of the local shaman's cast shout for spell type 20 (bloodlust) if that spell launches through (not live-checked). |
| 0x89 | En_Sv_Scrm01..En_Sv_Scrm02 [317-318] | 0x80 | 100 | - | Another tribe's shaman scream ('En_Sv_Scrm'): first tick blown, or sucked into a whirlwind. Also the overflow shout id for local spell type 21 (teleport). |
| 0x8A | En_Sv_dance..En_Sv_dance2 [294-295] | 0x80 | 100 | - | ORPHAN: shout slot for spell type 0 (none) of the enemy shaman table; never reached. |
| 0x8B | En_Sv_blast..En_Sv_blast2 [286-287] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 1 (burn): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x8C | En_Sv_blast..En_Sv_blast2 [286-287] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 2 (blast): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x8D | En_Sv_bolt..En_Sv_bolt2 [290-291] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 3 (lightning): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x8E | En_Sv_wind..En_Sv_wind2 [323-324] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 4 (whirlwind): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x8F | En_Sv_insect..En_Sv_insect2 [311-312] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 5 (swarm): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x90 | En_Sv_invis..En_Sv_invis2 [313-314] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 6 (invisibility): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x91 | En_Sv_hypno..En_Sv_hypno2 [309-310] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 7 (hypnotism): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x92 | En_Sv_fstm..En_Sv_fstm2 [301-302] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 8 (firestorm): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x93 | En_Sv_ghost..En_Sv_ghost2 [303-304] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 9 (ghost army): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x94 | En_Sv_erode..En_Sv_erode2 [297-298] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 10 (erosion): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x95 | En_Sv_swamp..En_Sv_swamp2 [319-320] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 11 (swamp): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x96 | En_Sv_bridge..En_Sv_bridge2 [292-293] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 12 (land bridge): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x97 | En_Sv_angel2..En_Sv_angel [288-289] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 13 (angel of death): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x98 | En_Sv_quake..En_Sv_quake2 [315-316] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 14 (earthquake): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x99 | En_Sv_flat..En_Sv_flat2 [299-300] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 15 (flatten): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x9A | En_Sv_volc..En_Sv_volc2 [321-322] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 16 (volcano): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x9B | En_Sv_dance..En_Sv_dance2 [294-295] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 17 (convert): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x9C | En_Sv_blast..En_Sv_blast2 [286-287] | 0x80 | 100 | - | Enemy shaman cast shout for spell type 18 (armageddon): 0x8A + spell type, played when the spell is launched, on the shaman, or as a 2D sound if the tribe has no shaman and flag +0x93F bit 8 is set. |
| 0x9E | sitespel [7] | 0xB4 | 127 | - | 'sitespel': Reincarnation Centre ground prep. Also the overflow shout id for enemy spell type 20. |
| 0x9F | rokrise..rokrise3 [65-67] | 0xB4 | 96 | - | Rock rise ('rokrise'). A building sinking into the ground, every 32 ticks (building state 5). Order kind 33 entering sub-states 3 and 8, with a mesh morph. The Reincarnation Centre pillar rising. A burnt-out ground fire (scenery 11/12), every 32 ticks. Also the overflow shout id for enemy spell type 21. |
| 0xA1 | blast5..blast [101-105] | 0xB4 | 127 | - | Blast whoosh. Default blast effect init. A spell launches a class-8 type-4 shot (only Blast). Firewarrior fire-bolt launched (shot type 6 init). |
| 0xA2 | Lghtng..Lghtng2 [187-188] | 0xB4 | 127 | - | Thunder ('Lghtng'). Lightning strike effect created. Also a tribe is eliminated (after turn 32): plays twice (2D) during the 20-turn sky flash. |
| 0xA3 | Wind_01..Wind_04 [352-355] | 0xB4 | 127 | - | Whirlwind wind loop (flags 0x40), restarted every tick; stopped when fewer than 12 ticks remain. |
| 0xA4 | Swarm_04..Swarm_01 [356-359] | 0xB4 | 127 | - | Insect Plague swarm loop, every tick while the effect lives. |
| 0xA5 | fire [2] | 0xB4 | 127 | - | ORPHAN: 'fire', no call site. |
| 0xA6 | fire [2] | 0xB4 | 127 | - | ORPHAN: 'fire', no call site. |
| 0xA7 | fire [2] | 0xB4 | 127 | - | ORPHAN: 'fire', no call site. |
| 0xA8 | ghost [43] | 0xB4 | 127 | - | Ghost Army cast, once on effect init. |
| 0xA9 | erode [1] | 0xB4 | 127 | - | Erosion rumble (flags 2), restarted for its 64-tick life. |
| 0xAA | Swamp06..swamp01 [111-116] | 0x60 | 32 | - | Swamp bubbling loop (flags 0x40) while the swamp lives. |
| 0xAB | Landb2 [480] | 0xB4 | 127 | - | Land Bridge cast ('Landb2'), layered with 0x29. |
| 0xAC | angel [44] | 0xB4 | 127 | - | Angel of Death cast: the controller effect is created. |
| 0xAD | volstart [14] | 0xB4 | 127 | - | Earthquake rumble ('volstart', flags 2) for the whole 120 ticks, with a screen-shake effect. |
| 0xAE | sitespel [7] | 0xB4 | 127 | - | Flatten cast ('sitespel'), once on effect init. |
| 0xAF | volstart [14] | 0xB4 | 127 | - | Deep rumble ('volstart', flags 2, restarted when it ends): the volcano for its whole 160-tick life, Armageddon at init, and the terraforming class-7 effect on list (likely the Armageddon arena shaper). |
| 0xB0 | volrise [13] | 0xE6 | 127 | - | 'volrise'. Volcano cone starts rising (tick 80 of 160). Armageddon arena phase starts. The terraforming effect first moves terrain. |
| 0xB1 | fire [2] | 0xB4 | 127 | - | ORPHAN: 'fire', no call site. |
| 0xB2 | blastexp..blastexp4 [106-109] | 0xB4 | 127 | - | Explosion ('blastexp'). Explosion bursts 1/2/3 and the rock explosion at creation (Blast's visual, volcano rocks). Firewarrior fire-bolt impact. Angel of Death appears (sub-state 1). Angel removed (sub-state 8, 2D, heard map-wide). Statue-to-AoD apparition dismissed. |
| 0xB3 | skyfire [8] | 0xB4 | 127 | - | Firestorm: 'skyfire' on each falling fireball, one spawned every 4th tick. |
| 0xB4 | Conv_bg [78] | 0x80 | 70 | - | Convert cast, once on effect init ('Conv_bg'). |
| 0xB5 | Volcfire [86] | 0xB4 | 127 | - | Lava flow ('Volcfire', flags 2), restarted through the growing phase; stopped in sub-state 1. |
| 0xB6 | Fireexp [3] | 0xB4 | 127 | - | Firestorm/volcano fireball hits LAND: the ground fire ignites ('Fireexp'). |
| 0xB7 | sitespel [7] | 0xB4 | 127 | - | ORPHAN: 'sitespel', no call site. |
| 0xB8 | sitespel [7] | 0xB4 | 127 | - | ORPHAN: 'sitespel', no call site. |
| 0xB9 | sitespel [7] | 0xB4 | 127 | - | ORPHAN: 'sitespel', no call site. |
| 0xBA | sitespel [7] | 0xB4 | 127 | - | ORPHAN: 'sitespel', no call site. |
| 0xBB | sitespel [7] | 0xB4 | 127 | - | ORPHAN: 'sitespel', no call site. |
| 0xBC | Volbrst6..volbrst1 [95-100] | 0xB4 | 127 | - | Volcano fireball launch ('volbrst'), small or big, at init. |
| 0xBD | En_v_pre_pre001..En_v_pre_pre004 [347-350] | 0x80 | 43 | - | Sermon voice of a preacher of another tribe ('En_v_pre_pre'); same condition as 0x33. |
| 0xBE | Wm_eat01..Wm_eat04 [343-346] | 0x60 | 40 | 0x1 | Wildman eating ('Wm_eat'), 50/50 with 0x03 (person state 15). |
| 0xBF | Wm_drink02..Wm_drink01 [339-342] | 0x60 | 40 | 0x1 | Wildman drinking ('Wm_drink'), 50/50 with 0x04 (person state 16). |
| 0xC0 | Guard_04..Guard_01 [360-363] | 0x50 | 80 | - | Guard-post marker loop ('Guard', flags 0x40) while a class-10 type-0x10 guard-post marker exists (controller state 15). Stopped when the marker is deleted, or evicted past the 64-marker cap. |
| 0xC1 | Stonehead..Stonehead5 [398-402] | 0x80 | 127 | - | Stone Head talking ('Stonehead'): scenery type 9 while animating, on the key frame of each loop. |
| 0xC2 | amb4020..amb4001 [372-391] | 0x40 | 32 | - | Ambient wildlife one-shot ('amb40xx'), theme 22, 1/128 per frame while trees are in view. |
| 0xC3 | Shield [392] | 0xB4 | 127 | - | Magical Shield hum ('Shield', looping sample, 2D): while any shielded unit (+0x15 bit 0x80) is drawn near the camera; stopped when none is. |
| 0xC4 | v_intoair01..v_intoair02 [393-394] | 0x80 | 100 | - | 'Into the air' scream: the first tick a non-shaman person is blown, only if vel.y > 90. |
| 0xC5 | roksink..roksink3 [395-397] | 0xB4 | 96 | - | Rock sink ('roksink'): the Reincarnation Centre pillar sinking; the statue-to-AoD sequence claiming its statue. |
| 0xC6 | amb5001..amb5010 [403-412] | 0x40 | 32 | - | Ambient one-shot ('amb50xx'), theme 30 (hell), 1/128 per frame while trees are in view. |
| 0xC7 | amb_hellm1001..amb_hellm1005 [430-434] | 0x40 | 32 | - | Ambience bed, hell theme 30: low land ('amb_hellm1'). |
| 0xC8 | ambhell2001..ambhell2006 [418-423] | 0x40 | 32 | - | Ambience bed, hell theme: shoreline ('ambhell2'). |
| 0xC9 | amb_hellw1006..amb_hellw1001 [424-429] | 0x40 | 32 | - | Ambience bed, hell theme: high ground ('amb_hellw1'). |
| 0xCA | amb_hellsea01..amb_hellsea05 [413-417] | 0x40 | 32 | - | Ambience bed, hell theme: sea ('amb_hellsea'). |
| 0xCB | disco_bldg_path [435] | 0xFF | 127 | - | Building-discovery cinematic starts ('disco_bldg_path'), layered with 0xCC. |
| 0xCC | disco_bldg_pop [438] | 0xFF | 127 | - | Building-discovery cinematic starts ('disco_bldg_pop'). |
| 0xCD | disco_bldg_end [436] | 0xFF | 127 | - | Building-discovery cinematic final stage ('disco_bldg_end'). |
| 0xCE | disco_bldg_circ [437] | 0xFF | 127 | - | Building-discovery cinematic circling stage ('disco_bldg_circ'); stopped after 12 stages. |
| 0xCF | tweets [439] | 0x80 | 40 | - | Bird tweets on a building drawn on screen with flags_3 +0x14 bit 0x1000. The bit is set by hut growth and mana distribution, so plausibly a 'contented hut'. Stopped when the bit clears. |
| 0xD0 | swirl [440] | 0xFF | 127 | - | Soul swirl ('swirl'): the local player killed an enemy shaman. |
| 0xD1 | badswirl [451] | 0xFF | 127 | - | Bad swirl ('badswirl'): the local player's own shaman died. |
| 0xD2 | v_intowind01..v_intowind08 [452-459] | 0x80 | 100 | - | A non-shaman person is sucked into a whirlwind ('v_intowind'). The shaman uses 0x1A/0x89. |
| 0xD4 | branch_off01..Branch_off04 [466-469] | 0x80 | 100 | - | Branch snapping off ('branch_off'): a whirlwind over a tree holding 100+ wood, 50% per hit, spawning a tumbling branch. |
| 0xD5 | amb_w2001..amb_w2006 [460-465] | 0x40 | 32 | - | Ambience bed, theme 16: replaces the sea slot ('amb_w2'). |
| 0xD7 | bell1..bell3 [475-476] | 0x80 | 96 | - | Construction finished: the completion bell when a building enters state 2. |
| 0xD8 | AngelD1..AngelD2 [470-471] | 0x80 | 70 | - | Angel of Death person created (spawn voice 'AngelD'). |
| 0xD9 | Angelfall [472] | 0x80 | 127 | - | Angel of Death meteor launched ('Angelfall'), tick 0 of the AoD controller. |
| 0xDA | Angelland [473] | 0x80 | 127 | - | Angel of Death meteor impact ('Angelland'): shockwave init. |
| 0xDB | AngelWing2 [474] | 0x80 | 96 | - | Angel of Death wing flap ('AngelWing2'): every tick the angel's frame timer +0x37 is 0, i.e. once per animation loop. Not a one-shot at shadow allocation. |
| 0xDC | Angelblast [477] | 0x80 | 96 | - | Angel of Death roar ('Angelblast'), every tick while counter +0x95 > 0. Armed by random rolls in hunting/engagement, failed engagements, and the carry/release countdown. |
| 0xDD | bookmark_set2 [478] | 0x80 | 96 | - | Camera bookmark stored (ui_cmd 0x1E-0x21). |
| 0xDE | bookmark_ret2 [479] | 0x80 | 96 | - | Camera jump: bookmark recall (ui_cmd 0x22-0x25) or minimap click. |
| 0xE0 | newplayer2 [481] | 0x80 | 96 | - | Multiplayer lobby: an empty player slot becomes occupied ('newplayer2'). |
| 0xE1 | noentry [482] | 0x80 | 96 | - | 'noentry' (UI): a local non-shaman unit gets blocked (person state 33) within 2 command frames of the player's move click, with advisor message 0x25B and its 'no entry' label. |
| 0xE2 | Sv_aff01..Sv_aff02 [39-40] | 0x80 | 100 | - | Same block, the unit is the local shaman ('Sv_aff'). |
| 0xE3 | infobnc [493] | 0x80 | 128 | - | A new message icon drops into the panel's message column ('infobnc'). |
| 0xE4 | infodrp [494] | 0x80 | 128 | - | A falling message icon bounces at the bottom ('infodrp'). |

### Low bank codec: EA MicroTalk
`soundd2low.SDT`'s `0x10` entries are **EA MicroTalk** (also UTalk/UTK), a multi-pulse LPC speech codec: 432-sample
frames of four 108-sample subframes, about 32 kbit/s. Identified from the exe's decoder (pop3-rev-analysis.md
"Low-quality sound bank codec") and confirmed by decoding every entry: lengths and waveforms match `Soundd2`.

**Reference implementation**: [daddesio/utkencode](https://github.com/daddesio/utkencode) (`utk.h`, public
domain via the UNLICENSE; docs: [wiki.niotso.org/UTK](http://wiki.niotso.org/UTK)). Its frame decoder is the
original's, bit for bit: the 64-entry reflection-coefficient table, both excitation codebooks, the pulse command
table and every constant (header gains, 1/15 pitch gain, the reduced-bandwidth interpolation taps) are identical
to the copies in the exe. Use it as is for the frames; only the stream around them differs.

Differences from the formats utkencode handles (Maxis `.utk`, Beasts & Bumpkins, FIFA):
- **No container**: the entry's data (after the 40-byte SDT header) is the raw bit stream. Channels, rate and
  length come from the SDT header, not from a `UTM0`/WAVEFORMATEX header.
- **One extra leading bit**: the first bit of the stream (LSB-first, same bit reader) is a "second channel" flag,
  read before the standard MicroTalk header: 0 = mono (data starts `e2 71`), 1 = stereo (`e3 71`). It is read only
  when the SDT flags lack `0x04`, which is the case for every low-bank entry. To decode with utkencode as is,
  drop that bit (shift the stream right by one) and start a normal decode.
- **Stereo**, absent from standard MicroTalk (mono only): both channels share the one header and the one bit
  stream and alternate per frame - a full left frame, then a full right frame, and so on. Each channel keeps its
  own decoder state (reflection coefficients, synthesis history, adaptive codebook); the header-derived values
  (reduced bandwidth, multipulse threshold, gain table) are shared. With utkencode: two contexts, the second
  copying the first's parsed header, handing the bit-reader position back and forth after each frame.
  The 21 stereo entries are ambiences (`amb2*`, `amb_spc*`, `ambhell2*`) and `discoend`, `discosky`.
- **Sample rate**: played at the SDT header's rate, 11025 Hz for most entries, not MicroTalk's nominal 22050 Hz
  (the decoder itself does not depend on the rate).
- **Length**: no sample count is stored; the game decodes whole frames until the data runs out, so the output is
  padded to a multiple of 432 samples per channel (a few ms more than the `Soundd2` entry).
- **Output**: float samples clamped to signed 16 bits.
- Oddity: the six `ambhell2*` entries decode ~1.41x longer than their `Soundd2` counterparts at their header rate
  and do not match them; either different recordings or a header rate that is not the encoded one (not chased).

Still not needed as long as `Soundd2` is used.
