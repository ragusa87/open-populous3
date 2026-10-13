How the spell icons are made ?

# ChatGPT

Create a table of 15 simple spell icons in a 5×3 grid. Use a hand-drawn, colored-pencil style with simple shapes, clean dark outlines, minimal shading, and a light background. Each icon should fit approximately 60×70 px and have a label.
Spells: Burn, Blast, Lightning, Whirlwind, Swarm, Invisibility, Hypnotism, Firestorm, GhostArmy, Erosion, Swamp, Land Bridge, Angel of Death, Earthquake, and Flatten.

# Claude
User asked to extract/redraw each sprite from a 15-icon sheet image (Burn, Blast, Lightning, Whirlwind, Swarm, Invisibility, Hypnotism, Firestorm, GhostArmy, Erosion, Swamp, Land Bridge, Angel of Death, Earthquake, Flatten) as SVG, not needing to be pixel-identical.

I generated 15 flat-style SVG icons via a Python script (using math for shapes like stars/spirals), saved them to /home/laurent/Downloads/pop/sprites/ as 01-burn.svg through 15-flatten.svg, each with a 100x100 viewBox, cream background square with dark border, and matching colors/shapes to the originals. Rendered a preview montage with rsvg-convert/montage to visually verify all 15 looked correct.

# Baked PNGs (assets/spell-hud/*.png)

`crates/game-client` shows these as the spell panel's icons in `--no-original` mode (no `hfx0-0.dat` to draw the
original's ink glyphs from). `01-burn.svg` has no matching `SpellKind` and is not baked; the other 14 are, each
with its own background/border card stripped so only the artwork remains, transparent, at 128x128:

```sh
cd assets/3d/spell-hud
for f in 02-blast.svg 03-lightning.svg 04-whirlwind.svg 05-swarm.svg 06-invisibility.svg 07-hypnotism.svg \
         08-firestorm.svg 09-ghostarmy.svg 10-erosion.svg 11-swamp.svg 12-land-bridge.svg 13-angel-of-death.svg \
         14-earthquake.svg 15-flatten.svg; do
  slug="${f%.svg}"; name="${slug#*-}"
  sed '/<rect /d' "$f" | rsvg-convert -w 128 -h 128 --background-color=none -o "../../spell-hud/${name}.png"
done
```
