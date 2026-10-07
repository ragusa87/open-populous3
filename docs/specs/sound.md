# Original sound banks ("SDT")

Not implemented yet. Read-only, never shipped. Derived from PopSoundEditor (Toksisitee, GPLv3, Qt) and
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
- `0x10`: compressed with an unknown codec (only in `soundd2low.SDT`).
- `0x20`: MPEG audio (music).
- `0x80`: placeholder, do not play. In `soundd2low` its data size is 0. In each drum bank, entry 0 is `0x83`
  and its data is entry 1's header + data shifted by 40 bytes (its range overlaps entry 1).
- `0x02` and `0x04` are set on all PCM entries and on music respectively, meaning unknown.

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
- The index is the sound id. Names hint at the use: `convert`, `erode`, `fire`, `Shrap01`, `chop1..5`,
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
