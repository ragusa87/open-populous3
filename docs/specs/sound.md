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
  priority and "plays while paused" flag. `Pick::Any` is the original's random pick; `Pick::Variant(n)` plays
  exactly variant `n` (`chop4`), never another one in its place.
- Mixed as the original mixes them: the listener 16 cells from the camera's look-at point along its heading, 2D
  distances on the wrapped map, a cut-off at 48 cells (not started past it, faded out once out of it), the volume
  times `1 - d²/48²`, an equal-power balance from the bearing folded into -90..90°. Our own stereo source
  (`audio_source.rs`) applies the gains, updated every frame.
- At most 10 effects at once: a full mix cuts its lowest-ranked one for a new one that outranks it (priority, then
  fewer copies of the same sound, then closeness: that order is our reading). A loop only cuts on its first
  start; coming back (next variant, back in reach, after being cut) it waits for a free slot, else loops would
  keep cutting each other (ours). The same sound never restarts within 250 ms (one-shots).
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
- Loaded from the install at startup (`audio_load.rs`, nothing with `--no-original`): `PopDrones22` entry 0 as
  the Menu track and entries 1-4 as the Game variants, their raw Layer II streams decoded by symphonia; the ten
  drum banks, each entry as a WAV (entry 0 included, played as stored).
- Not done yet: the SFX (`Soundd2` PCM as WAV through the sound id table, the fight bank's alternative ranges, CC0
  sounds for `--no-original`), the clips' own loop points (`fire`, `warloop`...), work sounds waiting for an animation
  frame, ambience layers, reverb, game events turned into cues (the fight flag for the intensity).

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
From the original exe (pop3-rev-analysis.md would hold the formal spec; the full per-id trigger list lives in the
pop3-rev catalog). Not implemented yet.

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
