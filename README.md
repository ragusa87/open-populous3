# Open Populous (POC)

An open-source, clean-room reimplementation of *Populous: The Beginning* (Bullfrog, 1998) in Rust
and Bevy 0.19. The goal is a faithful simulation of the original's rules, written entirely from
reverse-engineered documentation and our own open assets — never from the original's code, art or
data — so the result can be built, run and shared under the MIT licence.

## Clean room & original data

*Populous: The Beginning* and all its files (models, textures, sprites, sounds, levels, UI strings)
are Bullfrog / Electronic Arts property. This repo:
- **never ships them.** Nothing original is committed: no models, textures, sprites, sounds, level
  dumps, or renders/screenshots made with the original files.
- **never quotes original text** (level names, briefings, UI strings): those are referred to by
  their text number ([docs/specs/language.md](docs/specs/language.md)).
- **is written from documentation, not from the executable.** Reverse engineering (disassembly, live
  memory reads, running the original) happens only in separate, dedicated sessions whose sole output
  is [docs/specs/pop3-rev-analysis.md](docs/specs/pop3-rev-analysis.md) — our own write-up of the
  original engine's internals (unit model, tribes, constants, mana, combat...), itself built on prior
  community reverse engineering such as [pop3-rev](https://github.com/ragusa87/pop3-rev) (see
  [References](#references) below for the full list of sources). All implementation work reads that
  document; it never disassembles, traces or runs the original executable itself.

See [AGENTS.md](AGENTS.md#licence-and-original-data) for the full rules this project holds itself to.

## Assets: original today, open-source growing in

Right now the game **reads your own legitimately-owned install** of the original at runtime (never
bundled, never modified) for most of its 3D models, textures, sprites and sounds — there simply
isn't yet a complete open replacement for all of them.

In parallel, every asset family is also getting an open (CC0 / generated) equivalent, so the project
can eventually run and be fully playable **without the original files at all**:
- `just run-generated` (or `--no-original` / `POP3_NO_ORIGINAL=1`) starts with nothing original read:
  generated terrain, deterministic ground theme, generated objects and reincarnation markers.
- Some families are already fully open: all 16 building kinds (a reproducible, procedurally
  textured low-poly kit, see [assets/3d/buildings/README.md](assets/3d/buildings/README.md)), CC0
  character models baked into unit sheets, CC0 trees, and a generated wood pile sprite.
- Others still fall back to a placeholder or are missing outright (vehicles, scenery, spell VFX,
  audio, campaign content...).

The current state of every asset family — usable / placeholder / missing, and what's left to do for
each — is tracked in [docs/specs/assets.md](docs/specs/assets.md).

## Roadmap

[TODO.md](TODO.md) is the living, prioritised list of what's left to implement — gameplay, assets,
everything. [docs/roadmap.md](docs/roadmap.md) summarises what's already done. See
[docs/](docs/README.md) for the full architecture and specs.

## Compilation

The workspace is plain Cargo underneath; [`Justfile`](Justfile) just wraps the common recipes:

```
just build [--release]         # cargo build --workspace: compile everything, don't run it
just test                      # cargo test --workspace, plus the Python asset-generator checks
just run [levl2005.dat]        # cargo run -p game-client: windowed game
just run-generated             # --no-original: never read game files, all generated
just shot /tmp/x.png           # headless screenshot (no window, no focus steal)
```

Run `just` with no arguments (or open the `Justfile`) for the full recipe list, including
`just bake-units`, `just generate-buildings`, `just video`, `just profile` and `just level-info`.
Without `just` installed, any recipe's underlying `cargo` command works the same way directly.

Controls: push the mouse against the window edges to scroll (Esc frees the cursor), Left/Right rotate, Up/Down or WASD move (A/D strafe), middle-drag rotate, Home/End tilt, Ctrl+PgUp/PgDn zoom, Shift+PgUp/PgDn field of view, **Enter** aerial view,
PgUp/PgDn change level, Tab editor mode (R/F/T/M/B brushes), F11 fullscreen.

## Original assets bootstrap

By default the game looks for your own legitimately-owned install of the original and reads its
levels, textures, sprites and 3D models from it at runtime (see
[Assets](#assets-original-today-open-source-growing-in) above) — nothing is downloaded or bundled.

**Where it scans:** `pop3_format::install::find_install` ([crates/pop3-format/src/install.rs](crates/pop3-format/src/install.rs)).
If `$POP3_INSTALL` is set it's used as-is and nothing else is scanned. Otherwise, each base folder
below is checked (itself, then its immediate sub-folders in name order), and the first one
containing a `levels/` sub-directory (any case) wins — in practice that's the game's own install
folder, e.g. `Populous - The Beginning`, under one of these bases:

- Linux / macOS, if `$WINEPREFIX` is set:
  - `$WINEPREFIX/drive_c/Program Files (x86)/Bullfrog/Populous - The Beginning`
  - `$WINEPREFIX/drive_c/Program Files/Bullfrog/Populous - The Beginning`
  - `$WINEPREFIX/drive_c/GOG Games/Populous - The Beginning`
- Linux / macOS, always also:
  - `~/.wine/drive_c/Program Files (x86)/Bullfrog/Populous - The Beginning`
  - `~/.wine/drive_c/Program Files/Bullfrog/Populous - The Beginning`
  - `~/.wine/drive_c/GOG Games/Populous - The Beginning`
- Windows:
  - `C:\Program Files (x86)\Bullfrog\Populous - The Beginning`
  - `C:\Program Files\Bullfrog\Populous - The Beginning`
  - `C:\GOG Games\Populous - The Beginning`

The exact sub-folder name isn't assumed — any name works, and `Bullfrog`/`GOG Games` themselves
qualify too if they directly hold `levels/` — `Populous - The Beginning` above is just what a
typical install calls it.

**Per-folder overrides**, each independent of the others and of the auto-detected install:
`$POP3_LEVELS` (the `levels/` folder), `$POP3_DATA` (`data/`, themes and sprites) and
`$POP3_OBJECTS` (`objects/`, 3D models) — see [crates/game-client/src/world.rs](crates/game-client/src/world.rs).

**Skipping it entirely:** pass `--no-original`, or set `POP3_NO_ORIGINAL=1`, and no original file is
read at all: maps and the ground theme are generated deterministically, and objects (buildings,
trees, reincarnation site stones) fall back to their open/generated equivalents.
`just run-generated` does this for you. PgUp/PgDn then changes the generation seed instead of the
level. Preview the generated building kit with `POP3_START=sandbox-buildings just run-generated`.
See the [missing-asset inventory](docs/specs/assets.md) and
[building generator / Blender workflow](assets/3d/buildings/README.md) for what's open-source today.

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
- [peterderivaz/populous](https://github.com/peterderivaz/populous) (no licence checked): confirmed the sea's own
  rendering mechanism (`watdisp.dat` sampled twice, scrolling in opposite directions, driving both the sea
  vertices' altitude and a dynamic lighting response to it, instead of the land's static per-cell lighting) - see
  pop3-rev-analysis.md "Water rendering (watdisp.dat)", [terrain-textures.md](docs/specs/terrain-textures.md).
- [popscript-upgrader](https://github.com/TylerTheFox/popscript-upgraderi) (MIT, checked at
  `f2bd401`): the mapping of PopScript to Populous: Reincarnated's Lua API, see
  [ai-scripts.md](docs/specs/ai-scripts.md).
- [pop3-rev](https://github.com/ragusa87/pop3-rev) (hrttf111, forked here): reverse engineering of `D3DPopTB.exe`, which
  renders with Direct3D: the game's world is left-handed (x right, y up, z away), so we mirror z to draw it in
  Bevy, see [architecture.md](docs/architecture.md) "Handedness". Our own write-up of the engine internals this
  and other sources feed into is [docs/specs/pop3-rev-analysis.md](docs/specs/pop3-rev-analysis.md).
- The PopRe wiki page [Constant](https://wiki.popre.net/Constant) and Brandan Lasley's decoded
  `New_Constants.dat` (2012): the names of the balance constants, cross-checked on the decoded file, see
  [constants.md](docs/specs/constants.md).
- The PopRe wiki page [Keyboard Shortcuts](https://wiki.popre.net/Keyboard_Shortcuts): the original's keyboard and
  mouse shortcuts, see [shortcuts.md](docs/specs/shortcuts.md).
- The PopRe wiki page [Dat Format](https://wiki.popre.net/Dat_Format): corroborates the `.dat` layout (land
  heights, blocks, orients, no-access squares, sunlight, objects), see [level-format.md](docs/specs/level-format.md).

Only facts about the file formats are taken from the GPL and unlicensed tools, never their code, so the MIT
licence is kept.
