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
Without the original files, buildings use a generated low-poly kit: 16 models made by an AI-written script
(procedural surface textures, tribe accents and construction frames).
Preview them with `POP3_START=sandbox-buildings just run-generated`.
See the [missing-asset inventory](docs/specs/assets.md) and
[building generator / Blender workflow](assets/3d/buildings/README.md).
Original install: `$POP3_INSTALL` (the folder holding `levels/`, `data/`, `objects/`), else the first
`Bullfrog/<game>/` found under `$WINEPREFIX`, `~/.wine` (`drive_c/Program Files*`, `GOG Games`) or, on
Windows, `C:\`. Overrides: `$POP3_LEVELS`, `$POP3_DATA`, `$POP3_OBJECTS`.
See [docs/](docs/README.md) for architecture and specs.

## License
This project is currently licensed under the [MIT License](LICENSE).
- Bundled art in `assets/` is CC0 (see [assets/CREDITS.md](assets/CREDITS.md)).
- *Populous: The Beginning* and its files are Bullfrog / Electronic Arts property: they are never shipped
  here, only read from your own install (and `--no-original` runs without them).

## References
File formats are reverse-engineered from the original files; community tools are used to cross-check them
(read only, no code copied, the files win when they disagree):
- [Pop-World-Editor](https://github.com/PopRe/Pop-World-Editor) (PopRe, no licence stated; fork:
  [ALACNPopWorldEditor](https://github.com/Toksisitee/ALACNPopWorldEditor), checked at `3d02fa3`): origin of the
  v2 level structures (`pop.h`), the thing and header field meanings, the computer-player script format
  (`cpscr`/`cpatr`), spell and object model names, the object banks' tree styles and the editor tools worth
  copying, all cross-checked on the original files, see [level-format.md](docs/specs/level-format.md),
  [ai-scripts.md](docs/specs/ai-scripts.md), [objects.md](docs/specs/objects.md),
  [spells.md](docs/specs/spells.md), [ui-and-editor.md](docs/specs/ui-and-editor.md).
- [PopScript Tutorial](https://ts.popre.net/archive/Downloads/Docs/PopScript_Wiki_HTML_Help_File.htm)
- The [Toksisitee](https://github.com/Toksisitee) Populous tools:
  - [PopResourceEditor](https://github.com/Toksisitee/PopResourceEditor) (MIT, checked at `140e389`): layouts of
    the theme files (palette, bigfade, sky, disp, fade, cliff, ghost, alpha), the `bl320` object atlas and the
    v2 level format (`.dat`, `.hdr`, `.ver`), see [terrain-textures.md](docs/specs/terrain-textures.md),
    [objects.md](docs/specs/objects.md).
  - [PopSpriteEditor](https://github.com/Toksisitee/PopSpriteEditor) 1.4.0 (GPLv3): PSFB sprite banks, see
    [sprites.md](docs/specs/sprites.md).
  - [PopSoundEditor](https://github.com/Toksisitee/PopSoundEditor) (GPLv3): SDT sound banks, see
    [sound.md](docs/specs/sound.md).
  - [PopLanguageEditor](https://github.com/Toksisitee/PopLanguageEditor) (GPLv3): language string files; our
    reader comes from the files themselves, see [language.md](docs/specs/language.md).
  - [PopScript Upgrader](https://github.com/TylerTheFox/popscript-upgraderi) (MIT)
- The [PopScript Wiki](https://ts.popre.net/archive/Downloads/Docs/PopScript_Wiki_HTML_Help_File.htm) (Megafont,
  2006): the AI script language, its commands and attributes, see [ai-scripts.md](docs/specs/ai-scripts.md).
- [popscript-upgrader](https://github.com/TylerTheFox/popscript-upgrader) (MIT, checked at
  `f2bd401`): the mapping of PopScript to Populous: Reincarnated's Lua API, see
  [ai-scripts.md](docs/specs/ai-scripts.md).
- [pop3-rev](https://github.com/hrttf111/pop3-rev) (hrttf111): reverse engineering of `D3DPopTB.exe`, which
  renders with Direct3D: the game's world is left-handed (x right, y up, z away), so we mirror z to draw it in
  Bevy, see [architecture.md](docs/architecture.md) "Handedness".
- The PopRe wiki page [Constant](https://wiki.popre.net/Constant) and Brandan Lasley's decoded
  `New_Constants.dat` (2012): the names of the balance constants, cross-checked on the decoded file, see
  [constants.md](docs/specs/constants.md).

Only facts about the file formats are taken from the GPL and unlicensed tools, never their code, so the MIT
licence is kept.
