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

# Windowed run logging cursor confinement (focus, enter/leave, edge pushes)
cursor-debug *args:
    POP3_CURSOR_DEBUG=1 cargo run -q -p game-client -- {{args}} 2>&1 | grep --line-buffered cursor-debug
