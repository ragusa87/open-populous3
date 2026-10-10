# Keyboard and mouse shortcuts

The original game's shortcuts, from the PopRe wiki page [Keyboard Shortcuts](https://wiki.popre.net/Keyboard_Shortcuts)
(described in our words), next to what our client does with the same input today. "-" means unbound in ours.
Where ours differs, the original wins unless a row says why not; conflicts are listed at the end.

## Game and view
| Input | Original | Ours |
|---|---|---|
| Esc | in-game menu | pause menu (`menu.rs`) |
| P | pause | - |
| Shift + `+` / `-` | game speed up / down | - |
| `+` / `-` | zoom in / out | - (wheel, Ctrl+PgUp/PgDn) |
| F1 | encyclopedia | - |
| Enter | world view / normal view | toggles aerial view |
| Shift+Enter | quick toggle world / normal view | - |
| Space | skip the level fly-by | looks at the shaman |
| S | level status | camera back (WASD) |
| L | toggle the commanded unit ("commandee") | - |
| `>` | camera to the shaman | - (Space) |
| H | camera to the reincarnation site | same |
| F8 / F9 | quick load / quick save; F9 resynchronises in multiplayer | - |
| M | in-game chat | editor: marks the land bridge start |
| Alt+C | screenshot | - |
| Scroll Lock | redraws a corrupted palette | - |

## Camera
| Input | Original | Ours |
|---|---|---|
| Up / Down | scroll forward / back | same (also W / S) |
| Left / Right | rotate | same |
| Ctrl + Left / Right | scroll sideways | - (A / D) |
| Numpad 4 / 6 | scroll sideways | - |
| Numpad 7 / 9 (diagonals) | rotate | - |
| Mouse at the screen edge | scroll that way | same (`edge_push.rs`) |
| Shift + Z, X, C, V | store a camera point | - |
| Z, X, C, V | go to that camera point | X: stop order; C: casts a self spell |
| `\` | follow the follower commanded last | - |

## Groups and panels
| Input | Original | Ours |
|---|---|---|
| Shift + 1-6 | put the selection in group 1-6 | - |
| 1-6 | select group | - |
| Alt + 1-6 | camera to group | - |
| Shift+Backspace | select the previous group | - |
| Insert / Home / PgUp | buildings / spells / followers panel | - / camera tilt / zoom (Ctrl) and next level |
| Right click on anything | query it | deselects (hover shows tooltips, tooltips.md) |

## Followers
| Input | Original | Ours |
|---|---|---|
| Left click on a follower | select | same (drag for a box) |
| Ctrl + left click on a follower | add to the selection, even after orders | same |
| Left click (with followers) | order to a place or action | same |
| Right click (with followers) | deselect | same |
| Ctrl + left click (with followers) | queue orders, up to 8 | chained orders (units.md) |
| Shift + left click (with followers) | force the order | - |
| Alt + left click (with followers) | toggle auto-select for this order | - |
| Ctrl+Alt + left click (with followers) | patrol points, up to 8 | - |
| Ctrl + left click after patrol points | hold the orders | - |
| N | release the held followers | - |
| G (with followers) / G | guard the shaman / stop guarding | toggles the cell grid |
| Ctrl + left click on a follower icon | select 5 of that type | - |
| Shift + left click on a follower icon | select all of that type, spies included | - |
| Left click on an enemy tribe icon (spy selected) | the spy disguises as that tribe | - |
| Left click on a vehicle (with followers) | board it | - |
| Shift + left click on a vehicle | one follower gets out | - |

## Buildings
| Input | Original | Ours |
|---|---|---|
| Left click (plan held) | place the plan | same |
| Ctrl + left click (plan held) | place several plans | - |
| Space (plan held) | rotate the plan | same |
| Shift + right click on a plan, guard post or swamp | remove it | plan not flat yet, camp fire |
| Shift + left click on a building | one follower comes out | - |
| Shift + left click on a building icon (followers selected) | send them to the nearest building of that type | - |
| Space (tower alarm) | camera to the tower under attack | - |
| B (double tower alarm) | nearby followers help the tower | editor: land bridge |

## Spells
| Input | Original | Ours |
|---|---|---|
| Left click (spell held) | cast | same |
| Right click (spell held) | put the spell away | same |
| Ctrl + left click (spell held) | cast several times | - |
| Alt + left click (spell held) | toggle auto-cast for this order | - |
| Right click on a spell icon | toggle its charging | same (`SpellBook::toggle_pause`) |
| Ctrl + right click on a spell icon | toggle every other spell's charging | - |
| Shift + right click on a spell icon | toggle every spell's charging | - |

## Other
| Input | Original | Ours |
|---|---|---|
| Shift + left click on a tribe icon | toggle the alliance (multiplayer) | - |
| Left click on a message icon | read it and go to its target | - |
| Shift + right click on a message icon | delete it | - |

## Ours only (development)
F2 view presets, F3 camera readout, F11 fullscreen, Tab world editor (R/F/T/B/M brushes, ui-and-editor.md),
Home/End tilt, PgUp/PgDn next level, G cell grid, W/A/S/D move, C casts the selected self spell.

## Conflicts to settle
- Space: ours looks at the shaman, the original skips the fly-by and uses `>` for the shaman.
- S, A, D (camera), X, C (orders), G (grid), Home, PgUp/PgDn and M, B (editor) take keys the original uses.
- `+` / `-` zoom and Shift + `+` / `-` set the speed: the game speed control (P, 1x-8x) follows that.
