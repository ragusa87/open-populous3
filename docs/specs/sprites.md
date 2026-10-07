# Original sprite banks ("PSFB")

`pop3_format::sprites`, example `sprite_info <file>`. Read-only, never shipped. Cross-checked with
PopSpriteEditor 1.4.0 (Toksisitee, GPLv3), PopResourceEditor (MIT) and a scan of `HSPR0-0.DAT`,
`hfx0-0.dat` and `POINT0-0.DAT`.

| Off | Type | Content |
|---|---|---|
| 0 | 4 bytes | magic `PSFB` |
| 4 | u32 | sprite count |
| 8 | count x (u16 width, u16 height, u32 offset) | offset is absolute in the file |

- Little-endian. The pixel data usually starts right after the table (`8 + 8 * count`); `POINT0-0.DAT`
  has 2 extra bytes (`a3 00`) before it.
- Offsets do not always increase. Empty entries (0 x 0) all point to the first data offset:
  `HSPR0-0.DAT` 71, 167, 2130, 4484, 5182, 5184, 5186, 5188, 5190, 5192, 5767, 6320, 6876, 6878;
  `hfx0-0.dat` 1089, 1449, 1593. Treat them as empty sprites, not errors.

## Pixel data
`height` rows, each a list of commands read as signed bytes:
- `0`: end of the row, the rest of the row (up to `width`) is transparent.
- `n > 0`: `n` palette indices follow (opaque pixels).
- `n < 0`: skip `-n` transparent pixels.

Observed in the game files:
- Rows mostly stop early: the trailing transparent part is not encoded (87% of `HSPR0-0` rows).
  `HSPR0-0` and `POINT0-0` never end a row with a skip, `hfx0-0` does it 351 times.
- No row goes past `width` (decoders clip anyway). Runs and skips are at most 127 (an encoder splits
  longer ones). Longest seen: runs 39 / 104 / 40, skips 39 / 127 / 33 (HSPR / hfx / POINT).
- Each sprite is followed by one extra `0x7F` byte after its last row (every sprite in the three banks,
  the last one in the file too). Always seek to the table offset, never read sprites back to back.
  PopSpriteEditor does not write this byte and its banks still load in the game.

## Transparency and palette
- Index 255 is the colour key: skipped pixels are 255, and 255 never appears as a literal (literals seen:
  0..253).
- Palette: `pal0-0.dat` (UI, 1024 bytes, 256 x R, G, B, pad). Normal sprites only use index 0 (black,
  outlines) and 128..254: no literal in 1..127 in `HSPR0-0` or `POINT0-0`.

## Banks in `data/`

| File | Sprites | Content |
|---|---|---|
| `HSPR0-0.DAT` | 7953 | people and units |
| `hfx0-0.dat` | 1615 | effects, map things, HUD art (below) |
| `POINT0-0.DAT` | 162 | mouse pointers (below) |
| `plspanel.spr`, `plsspace.spr` | 29, 14 | panel artwork |
| `FONT0..2-0..2.DAT`, `font3..7-0.dat` | | fonts |
| `MSHAD0-0.DAT`, `EDIT0-0.DAT`, `f0{0,1,2}t{0..11}-0.dat` | | not analysed |

Not PSFB: `HSPR0-0.TAB`, `HSPR0-1.DAT`/`.TAB`, `HFX20-0.DAT`/`.TAB` (look like the older Bullfrog
`.DAT` + `.TAB` sprite pair), `vspr-0.inf`.

## `data/POINT0-0.DAT` (162 sprites)
13 arrow + "?", 14 arrow (the standard pointer, tip at 0,0), 15-21 arrow + 2..8, 27 arrow + up/down,
28 move cross, 29 rotate, 30 gold arrow, 31-33 gold arrow variants (spell targeting?),
38-66 spell / building icons, 80-155 action pointers (build, plant, guard...), 156-161 small icons.

## `data/hfx0-0.dat` (1615 sprites: effects, map things, HUD art)
Same palette. Identified: 2-3 dry plants, 22 a flat dark smear (shadow of 23?), 23 a pile of logs (17 x 11, a piece of
wood on the ground: `catalog::WOOD_PILE_SPRITE`), 90-93 fire, 107-111 spell names, 187-210 build/wood action icons,
1030-1089 spell and building glyphs, 1090-1600 spell effects (dust, explosions, swarm...).
The 3D objects have no wood pile: objects 0 and 12 are a camp fire (logs and a flame board), see objects.md.

Alpha sprites: 1090..1499 and 1538..1592 are blended (same ranges in PopResourceEditor,
`blend::is_alpha_sprite`). Their indices (including 1..127, 633k pixels) are rows of the alpha table, not colours.
`pop3_format::sprites` returns the raw indices; `blend::AlphaTable::rgba` turns them into RGBA (tint colour,
opacity strength / 15) for GPU blending (`art::alpha_picture_frame`, uploaded with `AlphaMode::Blend`).
Identified: 1225-1239 a cream dust cloud growing and fading (the teleport landing puff), 1210-1224 the same in
grey, 1181-1209 fire clouds, 1241-1247 and 1265-1279 flashes, 1330-1345 smoke columns.

## Blend tables (`data/`, one per theme, 65 536 bytes = 256 x 256 palette indices)
Layouts from PopResourceEditor (Toksisitee, MIT, `140e389`), checked against the files.

### Alpha, `al0-X.dat`
`al[row * 256 + background]`, `row = tint * 16 + strength`, so an alpha sprite pixel is
`tint << 4 | strength`:
- strength 0 = background unchanged (the first row of each block is 98% identity), 15 = about the tint.
  In between the blend is linear: fitted over all backgrounds, the mix is strength / 15 within a few percent
  (themes 0, 1, c), so an alpha pixel is the RGBA (tint, strength * 17).
- tints (theme 1, row 15): 0 red, 1 orange, 2 cream, 3 green, 4 blue, 5 yellow, 6 brown, 7 pale cyan,
  8..15 black (darkening, shadows).
- The table shape matches the guessed in-game lookup `al[idx * 256 + background]`; the game code itself was
  not checked. PopSpriteEditor's preview `al[idx * 256 + 0]` is the blend over black.
- `alpha.dat` (65 536), `alavaa.dat` and `awat.dat` (131 072) also exist, not analysed.

### Ghost, `ghost0-X.dat`
`ghost[a * 256 + b]` = nearest palette colour to `pal[a] + (pal[b] - pal[a]) * 0.66` (fitted on themes 0,
1, 5, c, z: 0.66-0.67 everywhere; PopResourceEditor's generator also defaults to 66%). Diagonal is the
identity, not symmetric. Which operand is the sprite and which the background is unknown. Used for
transparency (ghost / see-through units).
