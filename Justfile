levels := env_var_or_default("POP3_LEVELS", "")

# Run the sandbox (windowed). Optional: just run path/to/levl2005.dat
run *args:
    cargo run -p game-client -- {{args}}

# Compile the whole workspace without running: just build [--release]
build *args:
    cargo build --workspace {{args}}

# Bake the unit sprites (assets/units) from the CC0 characters in assets/3d/characters: just bake-units [kind]
bake-units *kind:
    cargo run --release -p unit-baker -- {{kind}}

# Rebuild the original CC0 building GLBs without Blender or original game data
generate-buildings:
    python3 tools/generate_buildings.py

# Blender review sheet of the generated kit only (optional authoring tool)
preview-buildings out:
    blender --background --factory-startup --python tools/preview_buildings.py -- "{{out}}"

# Never read the original game files: generated maps + generated theme
run-generated *args:
    cargo run -p game-client -- --no-original {{args}}

fullscreen *args:
    FULLSCREEN=1 cargo run -p game-client -- {{args}}

# Offscreen render, no window: just shot out.png [level]  (AERIAL=1 for planet view)
shot out *args:
    HEADLESS=1 SCREENSHOT={{out}} cargo run -p game-client -- {{args}}

# Offscreen video, no window: just video [level]  (VIDEO=secs, 5 by default; same setup vars as shot) -> target/video.mp4
video *args:
    HEADLESS=1 VIDEO=${VIDEO:-5} cargo run -p game-client -- {{args}}

# Headless video of the Volcano spell cast at x,z (cells, 64,64 by default), opened once done.
# VOLCANO_TICKS starts it mid-rumble (50 by default), well clear of the terrain-freeze bug
# (TODO.md) a late instant jump can trigger; VIDEO then runs long enough (26s by default) to
# play the rise, the turn-110 destruction and the crater's lava through in real time, when the
# terrain keeps updating correctly. FOCUS/DISTANCE/PITCH/YAW overridable, same as `just shot`.
volcano at="64,64":
    VOLCANO={{at}} VOLCANO_TICKS=${VOLCANO_TICKS:-50} FOCUS=${FOCUS:-{{at}}} DISTANCE=${DISTANCE:-10} PITCH=${PITCH:-40} YAW=${YAW:-20} HEADLESS=1 VIDEO=${VIDEO:-26} cargo run -p game-client --
    xdg-open target/video.mp4

# Per-system timings of a headless video run (same setup vars, VIDEO=secs, 2 by default): target/trace.json (open in
# ui.perfetto.dev), its most expensive spans printed (tools/trace_top.py target/trace.json 40 game_client: ours only)
profile *args:
    HEADLESS=1 VIDEO=${VIDEO:-2} TRACE_CHROME=target/trace.json cargo run -p game-client --features profile -- {{args}}
    python3 tools/trace_top.py target/trace.json 30

test:
    cargo test --workspace
    python3 -m unittest discover -s tools -q
    python3 tools/generate_buildings.py --check > /dev/null

level-info file:
    cargo run -p pop3-format --example level_info -- {{file}}

# Standalone page to map original 3D models to game items (embeds original art: stays in target/)
model-mapping:
    uvx --with pillow python tools/model_mapping.py

# Standalone page to pick each spell's icon among the original sprites (embeds original art: stays in target/)
spell-icons:
    uvx --with pillow python tools/spell_icons.py

# Standalone page of every identified original sprite, by name / file / index (embeds original art: stays in target/)
sprites:
    uvx --with pillow python tools/sprite_catalog.py

# Windowed run logging cursor confinement (focus, enter/leave, edge pushes)
cursor-debug *args:
    POP3_CURSOR_DEBUG=1 cargo run -q -p game-client -- {{args}} 2>&1 | grep --line-buffered cursor-debug
