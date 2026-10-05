# Open Populous (POC)

Rust + Bevy 0.19 sandbox inspired by *Populous: The Beginning* (Bullfrog, 1998).
Loads original `levlXXXX.dat` maps and shows them as a small curved planet.

```
just run                      # default: original levels dir from the Wine install, else $POP3_LEVELS
just run path/to/levl2005.dat
just shot /tmp/x.png          # headless screenshot (no window, no focus steal)
just test
```

Controls: mouse at screen edge scrolls, Left/Right rotate, Up/Down move, middle-drag rotate, **Enter** aerial view,
PgUp/PgDn change level, Tab editor mode (R/F/T/M/B brushes), F11 fullscreen.

Without original data a deterministic island map is generated.
See [docs/](docs/README.md) for architecture and specs.
