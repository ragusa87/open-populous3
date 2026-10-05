levels := env_var_or_default("POP3_LEVELS", "")

# Run the sandbox (windowed). Optional: just run path/to/levl2005.dat
run *args:
    cargo run -p game-client -- {{args}}

fullscreen *args:
    FULLSCREEN=1 cargo run -p game-client -- {{args}}

# Offscreen render, no window: just shot out.png [level]  (AERIAL=1 for planet view)
shot out *args:
    HEADLESS=1 SCREENSHOT={{out}} cargo run -p game-client -- {{args}}

test:
    cargo test --workspace

level-info file:
    cargo run -p pop3-format --example level_info -- {{file}}
