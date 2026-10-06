levels := env_var_or_default("POP3_LEVELS", "")

# Run the sandbox (windowed). Optional: just run path/to/levl2005.dat
run *args:
    cargo run -p game-client -- {{args}}

# Never read the original game files: generated maps + generated theme
run-generated *args:
    cargo run -p game-client -- --no-original {{args}}

fullscreen *args:
    FULLSCREEN=1 cargo run -p game-client -- {{args}}

# Offscreen render, no window: just shot out.png [level]  (AERIAL=1 for planet view)
shot out *args:
    HEADLESS=1 SCREENSHOT={{out}} cargo run -p game-client -- {{args}}

test:
    cargo test --workspace

level-info file:
    cargo run -p pop3-format --example level_info -- {{file}}

# Standalone page to map original 3D models to game items (embeds original art: stays in target/)
model-mapping:
    uvx --with pillow python tools/model_mapping.py

# Windowed run logging cursor confinement (focus, enter/leave, edge pushes)
cursor-debug *args:
    POP3_CURSOR_DEBUG=1 cargo run -q -p game-client -- {{args}} 2>&1 | grep --line-buffered cursor-debug
