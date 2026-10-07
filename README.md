# Open Populous (POC)

Rust + Bevy 0.19 sandbox inspired by *Populous: The Beginning* (Bullfrog, 1998).
Loads original `levlXXXX.dat` maps and shows them as a small curved planet.

```
just run                      # original install auto-detected (Wine / C:\Program Files), or $POP3_INSTALL
just run path/to/levl2005.dat
just run-generated             # --no-original: never read game files, all generated
just shot /tmp/x.png          # headless screenshot (no window, no focus steal)
just test
```

Controls: push the mouse against the window edges to scroll (Esc frees the cursor), Left/Right rotate, Up/Down or WASD move (A/D strafe), middle-drag rotate, Home/End tilt, Ctrl+PgUp/PgDn zoom, Shift+PgUp/PgDn field of view, **Enter** aerial view,
PgUp/PgDn change level, Tab editor mode (R/F/T/M/B brushes), F11 fullscreen.

Without original data (or with `--no-original` / `POP3_NO_ORIGINAL=1`) maps and the ground
theme are generated deterministically and objects (reincarnation site stones) use generated
shapes; PgUp/PgDn then changes the seed.
Original install: `$POP3_INSTALL` (the folder holding `levels/`, `data/`, `objects/`), else the first
`Bullfrog/<game>/` found under `$WINEPREFIX`, `~/.wine` (`drive_c/Program Files*`, `GOG Games`) or, on
Windows, `C:\`. Overrides: `$POP3_LEVELS`, `$POP3_DATA`, `$POP3_OBJECTS`.
See [docs/](docs/README.md) for architecture and specs.

## References
File formats are reverse-engineered from the original files; community tools are used to cross-check them
(read only, no code copied, the files win when they disagree):
- [PopResourceEditor](https://github.com/Toksisitee/PopResourceEditor) (Toksisitee, MIT, checked at `140e389`):
  layouts of the theme files (palette, bigfade, sky, disp, fade, cliff, ghost, alpha), the `bl320` object atlas
  and the v2 level format (`.dat`, `.hdr`, `.ver` structures).
- PopSpriteEditor 1.4.0 (Toksisitee, GPLv3): PSFB sprite banks, see [sprites.md](docs/specs/sprites.md).
- PopSoundEditor (Toksisitee, GPLv3): SDT sound banks, see [sound.md](docs/specs/sound.md).
