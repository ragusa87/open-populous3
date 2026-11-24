# Fog of war

Level flag 0x01 (levels 9 and 18; level-format.md "Flags"). The original's rules are in pop3-rev-analysis.md
"Fog of war": a permanent shroud of *explored* land, not line of sight. Nothing is hidden again once seen, and
spotting, targeting, combat, spells and the AI ignore it.

## State (`game_core::fog`)
- `GameMap::fog: Option<Fog>`: `None` on a level without the flag. `Fog` holds one `Explored` grid (one bit per
  cell, wrapping) per **human tribe**: a present tribe with no computer script. The original keeps one grid, for
  the local player; we keep every human's, so lockstep peers agree on what each one may build and fetch.
- `GameMap::enable_fog` (level load, or `FOG=1` on any map): nothing explored but disc 14 around each human
  tribe's reincarnation site (the original's disc 7 after it lies inside).
- Discs: disc r = the cells closer than r + 1 to the centre (`fog::disc`): 109 cells for disc 5, 697 for disc 14,
  the original's `MWSEARCH.DAT` rings, computed rather than read.

## Revealing
| What | When | Reveals |
|---|---|---|
| a human tribe's person (not inside a building) | each turn its own cell or one of the 4 cells straight ahead (1 cell apart, along `facing`) is unexplored | disc 5 around its cell |
| its boat or balloon | same rule, along `heading` | disc 5 |
| its Angel of Death | each turn its own cell is unexplored | disc 5 |
| the site at level start | once | disc 14 |
| effect-81 things no trigger aims at | level load, for every human tribe | disc 5 around each cell of their 3x3 block |
| effect-81 things a trigger aims at | when the trigger gives them (`Reward::RevealFog`, a totem's gift), to the tribe given | same |
| Armageddon | when it starts, for every human tribe | every cell closer than 11 to the arena centre |

The original reveals on entering a new cell; we check every turn (`GameMap::tend_fog`, after the units moved): the
same, since a disc 5 holds the 4 cells ahead in every direction, so a unit standing still never reveals twice. A
person converted or hypnotised into a human tribe reveals on its next turn, as in the original.

## Gameplay
- No building over unexplored land: `placement::can_place` refuses a footprint point on a cell its owner has not
  explored (`placement::fogged`).
- A human tribe's braves fetch no wood they have not discovered: `fetch_wood` and the nearest-tree refind of
  `go_cut` skip trees and wood pieces on unexplored cells (the original's wood-search zones skip them the same way).

## Drawing (`game-client/src/fog.rs`, the local player only)
- Ground: `GroundMaterial::fog`, one byte per cell (255 explored), sampled grid-vertex-aligned and bilinear in
  `ground.wgsl`, so each corner takes its own cell's brightness and unexplored land fades to black over one cell.
  The texture is always map-sized and rewritten in place when the explored cells change.
- Objects: anything `Grounded` (units, buildings, trees, totems, wood, effects) on an unexplored cell is hidden by
  `grounded::place_grounded`, whoever owns it; a shot shows again once over explored land.
- Hover: nothing on unexplored land is hovered (units through their hidden view, the others by position).
- Blueprint: footprint points over unexplored land are red.
- Sky: `SkyDim`, k = min(16, explored cells in view * 16 / 50) sixteenths, scales the sky dome and the clear
  colour. "In view" is our guess: the cells within 12 of the camera focus (the original counts the cells it draws).

## Not done
See TODO.md "Things and level data": the fog-exempt special level, the portal scenery's reveal, the mouse-over
text (lang00 0x349) and building refusal message (0x24E), and the animated sea polygons left out under fog.
