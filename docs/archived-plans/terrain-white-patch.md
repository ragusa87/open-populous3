# Terrain white-patch bug: investigation plan (archived, resolved)

> **Archived 2026-10-21.** This was a `docs/plans/` working file (gitignored, scratch) for the investigation
> below. The bug is fixed and committed; this copy is kept for posterity per `AGENTS.md` "Plans" - read it as
> history, not as current state. It is standalone: don't follow `docs/plans/terrain-white-patch.md` from anywhere
> else, that path no longer exists (the live plan was deleted once its task was committed, per the normal
> convention, before this archiving rule existed).
>
> **Reconstruction note**: the live plan was deleted (per `AGENTS.md` "Plans") just before the decision to archive
> plans going forward was made. It was rebuilt from this session's own transcript (this file's own edits are
> exact; a few passages describe content a concurrent session wrote directly to the live file, recovered from
> this session's `Read` calls on it) - essentially complete, but not a byte-for-byte guarantee. The "Final
> resolution" section at the end is a clean addendum written after reconstruction, from the same conversation,
> covering the part of the investigation that happened after the last point this transcript captured.
>
> **Two stale things in the body below, left as the plan actually said them at the time, corrected here instead
> of silently rewritten:**
> - "Where to see it" names `AppData/Local/Bullfrog/Populous/` as "the" install. That `data/` directory turned out
>   to be a corrupted leftover from a badly-uninstalled texture-pack extension (every `disp0-X.dat` differed from
>   the genuine 1998 release; it has since been deleted). The real install used from partway through this
>   investigation onward is `Program Files (x86)/Bullfrog/Populous - A l'aube de la création/` (1998-dated files).
> - Several sections' own "not yet resolved" / "needs a spec session" framing was each individually settled later
>   in the investigation - see "Final resolution" at the end for the actual outcome, don't stop reading at the
>   open questions below.

Context reset point. Goal: bisect to pin down exactly which commit(s) introduce the visible
regression, since root-causing by reasoning/formula-checking has found two real bugs already but
not fully explained what's on screen.

## The bug

Compared to `origin/main`, this branch rewrote terrain ground rendering from a pre-baked CPU
texture (Lambert slope lighting) to a per-fragment WGSL shader following a newly exe-traced colour
formula (see `docs/specs/terrain-textures.md` "Our bake", `docs/specs/pop3-rev-analysis.md`
"Terrain land colour"). The new renderer shows **white/pale patches on slopes and shorelines**
that look wrong - too light, missing the darker "cracked/rocky" cliff look - most visible on
level 1's decorative hill (a small steep mound with a temple on top, near the player's start).

`origin/main` (pre-rewrite) does not show this patch on the same level/camera.

## Where to see it

Level: `levl2001.dat` (level 1, theme 12 = `c`), auto-detected install at
`/home/laurent/.wine/drive_c/users/laurent/AppData/Local/Bullfrog/Populous/levels/levl2001.dat`
(data files alongside it in `.../Populous/data/`).

Repro shot (regenerate fresh each time - old screenshots lived in this session's scratchpad, which
won't be reachable from a new session):
```
FOCUS=2.0,8.1 DISTANCE=5.9 PITCH=20.4 YAW=0.0 just shot out.png levl2001.dat
```
This frames the hill directly behind the player's starting village; the white band runs along the
slope between the green hilltop and the water. An aerial view (`AERIAL=1 just shot out.png
levl2001.dat`) also shows a white coastal halo around other islands on the same level, so it's not
unique to this one hill.

## Investigation done this session, in order

1. **sRGB colour-space bug (confirmed, fixed, committed `4e88bfb`)**: `ground_material.rs::palette_image`
   stored the palette LUT as `TextureFormat::Rgba8Unorm` instead of `Rgba8UnormSrgb` - the only
   palette-derived colour texture in the codebase with that mistake (every sibling, `sky.rs` etc.,
   uses the sRGB format). Caused a double gamma-encode, washing out contrast everywhere. Fixed by
   using `Rgba8UnormSrgb`. Verified with a `just shot` before/after on level 1: real improvement,
   but the hill's white patch remained.

2. **`cell_light` neighbour-distance bug (confirmed, fixed, committed `56eea02`)**:
   `game_core::shading::cell_light` (new this session) computes a brightness byte from a height
   gradient, then hard-clamps to 0..255. It was reading the neighbour **2 real cells** over
   (`height(x+2,z)`, `height(x,z+2)`). A parallel reverse-engineering ("spec") session traced the
   real `compute_cell_light` (0x4BE230) and proved, against the level-load loop's own address
   formula (`load_level2`'s height-table populate loop), that the exe's raw x/z argument is
   **double** our real file-cell coordinate - so its own "+2" step is really **one** real cell.
   Fixed to `height(x+1,z)`/`height(x,z+1)`. Confirmed on raw level data: at `(x=10,z=11)` height
   353, `(x=12,z=11)` height 0 - this pair clamped hard under the old 2-cell read. Unit tests
   updated, full `just test` passes.
   - Also added `terrain_texture.rs::soften_light` (clamps the stored light byte to 32..223,
     `LIGHT_SOFT_MIN`/`MAX`): a **deliberate, renderer-only, non-faithful** safety net against the
     hard 0/255 clamp, independent of whether the clamp itself is "correct". Kept per explicit
     product decision (not a correctness claim).
   - **After this fix, the repro screenshot still shows a broad pale band** - better, but not
     fixed. Raw heights at the hill's shore are genuinely steep even over one real cell
     (`z=11`: `x=9..12` is `412, 353, 191, 0`), so some clamping is legitimate there.

3. **Found the real remaining cause (confirmed exe-side by the parallel spec session, NOT yet
   acted on in code until this interrupted attempt)**: `pop3_format::theme::Theme::land_start_row()`
   - a renderer-side floor that forces any cell's altitude-index row up to "the first row that
   looks like dry land" (137 for theme c), invented to stop a perfectly flat low platform (height
   0, blend 0) from reading as sea-blue - **has no counterpart in the original at all**. The real
   exe row formula (traced this session, `0x4BE330`'s corner computation) only ever reads
   height/blend/light, clamped `0..0x3FE`, no per-theme or per-shape minimum row anywhere. Separately,
   dumping `bigf0-c.dat` rows 128-139 (exactly the band `land_start_row()` floors into) across
   realistic brightness values shows that whole band is **inherently pale/sandy-grey**, not just at
   the extreme clamped brightness - so any shore cell floored into it reads light almost
   regardless of its own (correct) brightness. This looks like the dominant remaining cause of the
   visible white/pale band, separate from and on top of bug #2.
   - TODO.md's own next-step suggestion (now already written into the committed `56eea02` docs):
     *"Try removing the floor entirely against a real level with a flattened low platform before
     re-adding any shore-specific special case."*

## In-flight, NOT committed, NOT yet visually verified

Started removing `land_start_row()`'s floor entirely when the session was interrupted:
- `crates/pop3-format/src/theme.rs`: deleted `land_start_row()`, `sea_like()`, `MAX_SHORE_ROWS`,
  their tests; added a small replacement test for `disp_mean` (which had been piggy-backing on one
  of the deleted tests).
- `crates/game-client/src/ground.wgsl`: removed the `land_start` uniform field and the
  `altitude_floored = max(altitude, land_start)` line; `params` is now `x: size, y: grid, z:
  PX_PER_CELL, w: unused` (was `x: size, y: land_start, z: grid, w: PX_PER_CELL`).
- `crates/game-client/src/ground_material.rs`: `params()`/`fill()` no longer take `land_start`;
  updated the `params_packs_...` unit test.
- `crates/game-client/src/world.rs`: updated the one `ground_material::fill(...)` call site to
  drop `theme.land_start_row()`.
- `crates/game-client/src/terrain_texture.rs`: updated `soften_light`'s doc comment (was stale,
  referencing the already-superseded "masking is a no-op" conclusion from bug #2's first, wrong,
  pass).
- **Build is clean, `just test` passes in full (0 failures across every crate)**. **Not yet
  re-shot** against the level-1 repro to see whether removing the floor actually fixes the
  remaining pale band, or changes it to something else (e.g. a real sea-blue leak on a flat low
  platform, which is exactly the case `land_start` was invented to prevent - needs checking
  against a level/spell that flattens land to height 0, e.g. a land bridge or flatten spell, not
  just this one hill).
- Not committed. `git status --porcelain` at interruption time:
  ```
   M TODO.md
   M crates/game-client/src/ground.wgsl
   M crates/game-client/src/ground_material.rs
   M crates/game-client/src/terrain_texture.rs
   M crates/game-client/src/world.rs
   M crates/pop3-format/src/theme.rs
  ```
  (A `docs/specs/ai-scripts.md` change also shows in `git status` - that is NOT this work, it's a
  concurrent/parallel session's own unrelated output; leave it alone.)

## Current best clues for a bisect

- Known-good baseline: `origin/main` (`3f48fe0`, "terrain: glide the baked ground colour too, not
  just the mesh") - old CPU-baked texture + Lambert lighting, no white patch.
- Known-bad: current `HEAD` (`56eea02`) plus the in-flight uncommitted land_start removal above.
- The terrain-rewrite commits between them, oldest to newest (`git log --oneline origin/main..HEAD`
  before this session's own two fix commits):
  ```
  9cd9296 spec for terrain height
  bd7d573 terrain: DirtyRect::bounding, shared by Ashes::Patch::dirty
  554b889 shading: per-cell blend/light/downhill, recomputed from the heightmap
  ca0ee15 map: wire CellShading into GameMap, recomputed on every terrain edit
  7ab07b0 theme: load cliff0/fade0, disp_sensitivity curve, synthetic cliff for --no-original
  be5318d terrain_texture: faithful height+blend row, blend-driven cliff remap, sunlight brightness
  9a7a5db docs: terrain height/blend/cliff fixes landed, follow-ups and open questions noted
  67380d9 spec for terrain
  c5dc13a shading: SunParams is the traced flat 147/147/147 constant, not a ShadeRange/ShadeStart guess
  dd1bd47 spec: terrain
  db48312 terrain: evaluate the ground colour formula per fragment in a shader, not a baked texture
  027e4ed docs: terrain shader rewrite - mip-chain item fixed, bake section rewritten
  d9c63aa terrain: bilinearly sample disp/water noise instead of an exact per-pixel lookup
  11b4859 terrain: keep the altitude row continuous into the bigfade index instead of truncating
  7a6d98d terrain: revert the row-fraction index trick, blend row/row+1 colours instead
  ```
  then this session's `4e88bfb` (sRGB fix) and `56eea02` (cell_light neighbour fix).
- Best guess if bisecting for "when does the white patch first appear": likely **`be5318d`**
  (first commit to wire the new traced "sunlight brightness" into the live bake, replacing the old
  baked Lambert lighting) or **`db48312`** (the CPU-bake-to-shader rewrite) - both are plausible
  first-appearance points since the CPU-bake version (`be5318d`..`db48312`) used the *same*
  `cell_light`/`land_start` logic as today, just pre-baked into a texture instead of per-fragment,
  so the patch may already reproduce there (just blurred by the old texture's own minification/
  magnification, which could make it less obviously "white"). Building each candidate commit is
  slow (full Bevy rebuild per fresh worktree; reuse one worktree and `git checkout` between
  commits to stay incremental) - a sibling worktree was used this session at
  `/tmp/.../scratchpad/origin-main` and removed afterward; recreate as needed.
- Note: the main working tree may have **foreign uncommitted changes** from a concurrent
  session (seen twice this session: an "Insect plague" write-up in `pop3-rev-analysis.md`, later
  committed separately as `01f2955`; and the live `ai-scripts.md` edit noted above). Check
  `git status`/`git stash list` and don't discard anything that isn't yours before checking out
  other commits - prefer a separate `git worktree add` over `git checkout` in the main tree if
  anything unrelated is dirty.

## Open question not yet resolved

Whether removing `land_start_row()`'s floor (in-flight above) actually fixes the remaining white
band, or whether there's still a third contributing factor (the row/row+1 blend fraction, the
`disp` row-jitter, or the `cliff` remap) not yet isolated. Re-shoot the repro after finishing the
floor removal before concluding either way.

**Resolved by re-shoot, 2026-10-21**: the `land_start_row()` removal (now committed as `1a14b58`)
does **not** fix the white band - re-shot the repro at `HEAD` (`1a14b58`) and the pale slope band
is still clearly visible, same as before the floor removal. So a bisect was needed after all.

## Bisect result, 2026-10-21

Repro shot (`FOCUS=2.0,8.1 DISTANCE=5.9 PITCH=20.4 YAW=0.0`, `levl2001.dat`), built per commit in a
sibling worktree (`git worktree add --detach <path> <commit>`) with `CARGO_TARGET_DIR` pointed at
the main tree's `target/` to reuse already-built dependencies instead of recompiling the whole
Bevy dependency graph per commit (this matters: the main tree's `target/` is ~35G, and a fresh one
per worktree would duplicate that - on this machine `/tmp` is tmpfs/RAM-backed with far less than
35G free, so a from-scratch `target/` there risks exhausting RAM before even finishing a build).
Each candidate's screenshot was eyeballed by the user directly (`xdg-open`), not by the agent,
since the original data it renders can't be looked at by the agent (AGENTS.md "Licence and
original data").

Results, oldest to newest:
- `origin/main` (`3f48fe0`): **good**, no white patch - confirms the known-good baseline.
- `be5318d` ("terrain_texture: faithful height+blend row, blend-driven cliff remap, sunlight
  brightness"): **bad** - the plan's prime suspect, confirmed.
- `ca0ee15` ("map: wire CellShading into GameMap"): **good**.
- `7ab03b0` ("theme: load cliff0/fade0, disp_sensitivity curve, synthetic cliff for
  --no-original"), `ca0ee15`'s immediate child and `be5318d`'s immediate parent: **good**.

`7ab03b0` (good) -> `be5318d` (bad) are adjacent commits in the terrain-rewrite series, so the
bisect is conclusive without needing to test `bd7d573`/`554b889` (the commits before `ca0ee15`):
**`be5318d` is the first commit that introduces the white/pale patch.** Its own commit message
covers three changes at once (the height+blend row formula, the blend-driven cliff remap, and the
sunlight brightness constant).

## Isolated within `be5318d`, 2026-10-21

Checked out `be5318d` in the sibling worktree and patched `pixel_rgb` (diagnostic only, not
committed) to compute `brightness` with the *old*, pre-`be5318d` Lambert mesh-gradient formula
(`gx`/`gz` from `map.sample`, divided by the gradient vector's own length `n_len`), while leaving
the commit's new altitude-row formula (height + blend) and cliff remap untouched. Re-shot the same
repro: **the white patch is gone.** So the row/cliff changes are not the cause - it is specifically
the new **brightness** term, i.e. `shading.light` (`game_core::shading::cell_light`) feeding
`pixel_rgb`'s `brightness`.

**Root cause**: `cell_light` (`crates/game-core/src/shading.rs`) computes a **raw, unscaled**
height delta between neighbour cells, dotted against the sun vector, divided by a flat constant
(`/350`), then hard-clamps to `0..255`:
```rust
fn cell_light(terrain: &Heightmap, sun: &SunParams, ambient_nibble: u8, x: i32, z: i32) -> u8 {
    let dx = terrain.get(x, z) as i32 - terrain.get(x + 2, z) as i32; // now +1 since 56eea02
    let dz = terrain.get(x, z + 2) as i32 - terrain.get(x, z) as i32; // now +1 since 56eea02
    let ambient = (ambient_nibble as i32 + 8) * 16;
    ((sun.base + dz * sun.z - dx * sun.x) / 350 + ambient).clamp(0, 255) as u8
}
```
The old Lambert formula it replaced divided its own slope term by `n_len` (`sqrt(gx² + 1 + gz²)`),
which shrinks the contribution of a steep gradient back down before it ever reaches the final
0..255 range - a built-in softening `cell_light` has no equivalent of. On a genuinely steep real
slope (this hill's shore drops ~400 raw height over a couple of cells, confirmed in bug #2 above:
`z=11`, `x=9..12` is `412, 353, 191, 0`), the raw, un-normalized delta blows straight through the
clamp ceiling/floor regardless of the 1-vs-2-cell neighbour-offset fix (`56eea02`) already landed -
that fix was real and correct, but addressed a different, smaller error (reading the wrong
neighbour) on top of this larger one (no slope normalization at all). This is also independent of
`land_start_row()`'s floor (removed in `1a14b58`): both are real, both still left the patch
visible, because neither touches `cell_light`'s own clamp.

**Checked (current `HEAD`, not `be5318d`): `soften_light` IS wired into the live per-fragment shader
path, not a dead leftover** - `terrain_texture.rs`'s per-cell prep writes `soften_light(shading.light(...))`
into the terrain data texture's G channel (`channels[i * 4 + 1]`), and `ground.wgsl` reads it back
(`let light = ch.g * 255.0;`) to build `brightness`. But it alone does not explain why the patch is
still visible at `HEAD`, confirmed by the user re-shooting after both landed fixes: its inset range
(`LIGHT_SOFT_MIN..LIGHT_SOFT_MAX`, 32..223) still leaves 223 as a legal value - already quite bright
- and `ground.wgsl`'s own `brightness` then adds a further `disp`-driven delta term on top
(`(d_below - d_here) / 4.0`, up to about ±32) *after* reading the softened value, which can push a
softened-but-still-high 223 straight back up to the hard 255 ceiling. Combined with the
independently-confirmed fact that the bigfade rows a steep shore cell's altitude lands in are
*already* pale/sandy-grey at most brightness columns (TODO.md's terrain entry, `bigf0-c.dat` rows
128-139), a brightness in the low-200s is enough to read as "white" even without technically hitting
255. So `soften_light` is a real, working mitigation, just not strong enough on its own against this
hill's genuinely steep slope.

**Not yet decided**: whether `cell_light`'s hard clamp-to-0/255-with-no-normalization (before
`soften_light` even runs) is itself a faithful reproduction of the original (a guess per its own doc
comment and `pop3-rev-analysis.md` "Open questions" - `SunParams`'s inputs are themselves marked
GUESS, not traced) that the original simply also exhibits on steep slopes, or a divergence to fix
(e.g. normalize by the local slope steepness the way the old Lambert formula did, rather than only
softening the output range after the fact). This needs a spec-session trace of the real
`compute_cell_light` (0x4BE230) output range on a known steep slope before picking a fix, not a
coding-session guess - flag as a question for a spec session, or implement a guessed normalization
and tag it as such.

Not yet done: clean up the sibling worktree and its diagnostic, uncommitted patch to
`terrain_texture.rs` (`/tmp/.../scratchpad/bisect-wt` as of this session, removed once this plan's
task is fully done - AGENTS.md "Plans"). (Done - the bisect-wt worktree was already removed before this
continuation session started; `git worktree list` shows only the main tree.)

## Continued, 2026-10-21 (new session, context reset)

Re-read this plan cold and re-checked the "not yet decided" question above before acting on it.

**First pass concluded the clamp was probably faithful** - `pop3-rev-analysis.md` "Terrain land colour" already
has `compute_cell_light`'s pseudocode (`dx`/`dz`/divide-by-350/ambient/clamp, committed `4e88bfb`, *before* this
plan's own bisect work) tagged `[exe]`, no normalization term anywhere in it, matching our implementation
bit-for-bit (modulo the already-fixed neighbour distance). Checked every other step of the pipeline
(`altitude_cell`, `disp_sensitivity`, `row_jitter`, the brightness formula, the row/row+1 blend in `ground.wgsl`)
against the same spec section: all match, or are already-documented deliberate simplifications settled in earlier
commits (`7a6d98d`'s row/row+1 blend replacing the original's flat-index trick; `round()` vs the exe's truncating
`>>` shifts - a rounding-direction nitpick, not a magnitude bug). So nothing left in our own code to fix without
new exe data.

**Overturned by the user's own recollection of the original game**: asked directly, the user confirmed the real
hill reads "normal, green, just a very very little bit darker" - nothing like the current pale/white band. So
this **is** a real divergence, not faithful behaviour, despite the formula matching the traced pseudocode.

**The one concrete, checkable-from-the-docs-alone lead found**: the spec's own "live-verified" claim for the
`/350` divisor doesn't actually prove the divisor is 350. Its only live numeric check is on a **flat** cell (`dx
= dz = 0`): measured `light = 128`, matching `clamp(147/350 + 128, 0, 255)` because `147/350` floors to 0 - but
`147` divided by *any* divisor larger than 147 (848, 1500, whatever) also floors to 0 and would give the exact
same measured result. A flat cell can never disambiguate the divisor's magnitude, only confirm it's bigger than
`sunBase` (147). If the real divisor is substantially larger than 350, that alone would soften steep-slope
saturation a lot without needing any "normalize by slope steepness" term at all - and would explain why the
formula's pseudocode has no normalization yet the real hill still doesn't blow out. This needs a spec session to
re-check the literal immediate operand in the `idiv`/`div` instruction at `compute_cell_light` (0x4BE230) against
the disassembly bytes directly (decimal vs. a hex immediate like `0x350` = 848 misread as decimal - a plausible
transcription slip, not yet ruled out), and ideally a live measurement on an actual steep slope (e.g. this same
hill's cells, heights already known: `z=11`, `x=9..12` = `412, 353, 191, 0`) to get a ground-truth `light` byte to
diff our formula's output against directly, rather than re-deriving from the static pseudocode alone.

**Also surfaced this session, unrelated to the divisor**: the user pasted the PopRe wiki's "Dat Format" page
(community-tool reference, AGENTS.md-allowed). It independently corroborates `ShadeStart`/`ShadeRange` being
unused (matching the exe trace), and documents a higher "walkable" height cap (1408) above the 1024 ceiling this
project's own level corpus ever reaches - filed into `level-format.md` and `README.md` "References", not related
to the colour bug.

**Next step for a spec session** (not this one - AGENTS.md "clean room"): re-verify the `/350` immediate against
the raw disassembly bytes at `compute_cell_light` (0x4BE230), and live-measure its output `light` byte on a
genuinely steep slope (this hill's own cells above) to compare directly against what our formula currently
computes for the same heights/sun vector - that single number will settle whether 350 is wrong, whether a term is
missing from the traced pseudocode entirely, or whether something else in the pipeline (not `cell_light` itself)
is the real cause.

## Resolved (exe side), 2026-10-21: `cell_light` is faithful, not the cause

A pop3-rev session re-disassembled `compute_cell_light` (0x4BE230) and its sibling (0x4BE0B0) fresh, byte-for-byte:
the divisor is loaded as `mov esi/ebx,0x15e` right before a true `idiv` - `0x15e` = **350 decimal**, not a hex
`0x350` (848) misread, and it's a real division (truncating toward zero), not a multiply-by-reciprocal with a
different effective magnitude. The dx/dz term structure re-derives to exactly what's already in
`pop3-rev-analysis.md`/our code, no missing normalization step anywhere in the instruction stream.

Then live-measured the real exe's output on this exact hill's known steep shore row (`levl2001.dat`, z=11,
x=9..12, heights 412/353/191/0, sun 147/147/147) and compared cell-by-cell against the formula computed from the
same live-read heights/ambient nibbles: **measured and predicted light matched exactly** for all four cells
(x=9: 109/109, x=10: 165/165, x=11: 54/54, x=12: 128/128). None of these is anywhere near the 0/255 clamp -
the biggest delta (dx=162) only contributes about -60 after `/350`.

**This disproves the "clamp-with-no-normalization blows through 0/255 on this slope" theory from the sections
above.** `cell_light`'s formula, divisor, and clamp are all faithful to the original and are not what's producing
the white/pale band on this row. The remaining divergence must be downstream of `cell_light` - the bigfade
row/disp/cliff remap, or the row/row+1 blend (`7a6d98d`) - or on a steeper row than this sample covered. Full
trace: `pop3-rev/docs/functions.tsv`, row `0x4be230`.

## Side finding: `ambient_nibble` is a real per-cell value, not always 0

The live measurement above incidentally caught the map-cell +0xE low nibble (`ambient_nibble`) reading a nonzero
6 at one cell (x=10, z=11, under/near this hill's temple) while every neighbour read 0 - reproducible across a
full game restart, so not stale/garbage memory. This contradicts `pop3-rev-analysis.md`'s prior "nothing writes
it, probably 0 is right" conclusion, which only checked terrain-specific functions. Traced the real writers:
`landscape_move_1` (0x403A00) and `landscape_set_unit_shadow` (0x403C10) both do
`cell[+0xE] = (cell[+0xE] & 0xF0) | clamp(calc_unit_shade_value(cell), 0, 0xF)`; `calc_unit_shade_value` (0x450D50)
sums a shadow contribution from buildings/trees/scenery placed on or near the cell. So this nibble is a dynamic
object-cast ground shadow (0-15), not a static ambient constant - open-populous's hardcoded `ambient_nibble = 0`
is a real gap near any building/tree, separate from and unrelated to the now-exonerated divisor/clamp question.
Not yet fully traced: `calc_unit_shade_value`'s exact per-object-type/size weighting, or what distinguishes its
two thing-index roles (cell +0x6 vs +0x8). See `pop3-rev/docs/functions.tsv` rows `0x403a00`/`0x403c10`/`0x450d50`,
and `pop3-rev-analysis.md` "Terrain land colour" (correction appended) / `TODO.md`.

## Ruled out, 2026-10-21: GPU perspective-correct interpolation vs. the original's screen-space-linear Gouraud

Hypothesis (era-reasoning, not exe-traced): the spec's own description of `set_texture_landscape_main`
("value << 16 at the corners, delta << 12 per step", pure CPU, no D3D call) is classic 1998-era Gouraud shading -
linear in **screen space**, not perspective-correct (perspective-correct attribute interpolation needs a per-pixel
divide, routinely skipped in that era for exactly this kind of cheap landscape pass). Our renderer samples
`channels_tex`/`altitude_tex` at a UV carried by `bevy_pbr::forward_io::VertexOutput`, which wgpu/Bevy interpolates
perspective-correctly by default - a structural mismatch that would show up worst exactly where a quad's own
screen-depth varies most within itself: steep slopes and shorelines, matching the bug's symptom location.

**Tested, disproved**: temporarily patched `terrain_mesh::build` (`TERRAIN_SUBDIV` env var, uncommitted, reverted
after) to subdivide each real cell into `subdiv²` sub-quads, height/UV continuously sampled via `Heightmap::sample`
(already-existing bilinear interpolation) - shrinking each quad's own screen-depth range should shrink any
perspective-correct-interpolation artifact if that were a real contributing cause. Re-shot the same repro at
`TERRAIN_SUBDIV=4` next to the `subdiv=1` baseline; user's own verdict on the pair: "It's horrible" - markedly
*worse*, not better. That rules this hypothesis out (a real contributing cause should have softened, not
worsened, under finer subdivision) - don't re-attempt mesh subdivision as a fix for this bug. (Not investigated:
*why* subdivision made it look worse - likely `soften_light`/bigfade-row banding becoming more finely/densely
visible rather than smoothing out, itself informative about how `bigfade`'s row bands read at this hill, but a
tangent from this bug's own root cause.) Change fully reverted (`crates/game-client/src/terrain_mesh.rs` back to
its committed state, `just test` passes); nothing left in the working tree from this experiment.

## Narrowed to one exact cell, 2026-10-21: the earlier exe verification didn't cover the actual hotspot

Per instruction (via the user, relaying the spec session's own ask: narrow visually first, only bring back a
specific formula/cell once isolated, not three stages to re-verify blind) - wrote a Rust-side line-for-line replica
of `ground.wgsl`'s full per-fragment formula (both branches: land altitude/cliff/row-jitter, and the separate sea
`watdisp` path), run directly against the real `levl2001.dat`/theme-`c` data via `GameMap::from_level` +
`Theme::load` (temporary `#[ignore]`d tests in `terrain_texture.rs`, reverted after, not committed), scanning a
wide grid (`x: -8..12, z: -2..18`) around the repro's own `FOCUS=2.0,8.1` for any pixel whose mixed RGB has every
channel above 170.

**Found the pale pixels are NOT at the `z=11, x=9..12` row** the "Resolved (exe side)" section above live-measured
and exonerated - they cluster at `x≈-1.4..3.0, z≈7..11.2`, right around the camera's own focus point, a different
part of the same hill. That earlier exe verification didn't actually cover where the bug shows on screen.

Dumped raw `(height/blend)` and `light` for the integer cells in that region directly from `GameMap::from_level`
(our own terrain/shading, no exe needed - this is just reading already-parsed real level data). The hill rises
from flat sea (height 0) to over 300 within a handful of cells, and at **map cell `(x=2, z=8)`** specifically:
`height(2,8)=0`, `height(3,8)=206` (the x+1 neighbour `cell_light` reads), `height(2,9)=212` (the z+1 neighbour) -
an inside corner of the slope where both axes jump steeply at once, not just one. Through `cell_light`'s own
(already divisor/clamp-confirmed) formula: `dx = 0-206 = -206`, `dz = 212-0 = 212`,
`(147 + 212*147 - (-206)*147)/350 + 128 = (147 + 147*418)/350 + 128 ≈ 304`, clamped to **255** - the hard ceiling,
exactly matching our own computed `light=255` there. Neighbouring cells along a single slope direction (e.g.
`(2,7)`: `height=0`, only the x+1 neighbour jumps to 106, not both axes at once) read much lower - this specific
corner cell compounds both axes at once, which the single previously-tested row never exercised.

**Not yet resolved, needs the spec session now that it's a single named cell, not three blind stages**: does the
real exe's `compute_cell_light` also hard-clamp to 255 at this exact cell (`levl2001.dat`, map x=2, z=8, raw
heights 0/206/212, theme c) - i.e. is this corner case *also* faithfully reproduced (an original engine quirk a
real player might just never have noticed, or that reads differently on compounded two-axis slopes the exe's own
corner math handles some other way not yet captured by the traced single-axis pseudocode) - or does it diverge
here specifically. This is the same formula already traced, just never live-measured at a two-axis inside-corner
cell before (only the straighter `z=11` row was). If it diverges, the "Open question" section above (normalize
cell_light by local slope steepness, as the old Lambert formula implicitly did) becomes live again, but scoped to
*this specific geometry* (compounded corners) rather than a blanket guess.

Live-read the real exe's map-cell table (0x8A03E4) directly at `(x=2,z=8)`/`(x=3,z=8)`/`(x=2,z=9)` on a running
`levl2001.dat`: `height` 0/206/212 (matches this plan's own dump exactly), `ambient_nibble` (+0xE low nibble) 0 at
`(2,8)`, and the map-cell's own precomputed `+0xD` brightness byte read live as **255**. Recomputing
`compute_cell_light`'s formula from those same live heights with the already-established flat sun vector
(147/147/147): `dx=0-206=-206`, `dz=212-0=212`, `(147 + 212*147 - (-206*147))/350 + (0+8)*16 = 61593/350 (trunc)
+ 128 = 175 + 128 = 303`, i.e. **48 over the 255 ceiling** before clamping - clamped value matches the live-read
255 exactly.

**Answer: yes, the real exe hard-clamps to 255 at this exact cell too.** This is not a divergence - our own
computed `light=255` at `(2,8)` is faithful to the original, corner case included. The white/pale band's root
cause is therefore still downstream of `cell_light` (the bigfade row's own pale band at the altitude index this
height lands in, or the row/row+1 blend) as the "Resolved (exe side)" section above already concluded for the
single-axis row - this two-axis corner cell doesn't change that conclusion, it just confirms the clamp itself
isn't where to keep looking. Full trace: `pop3-rev/docs/functions.tsv`, row `0x4be230`.

Caveat from the same live session: the sun-vector addresses (0x937AA8/AA/AC) read back implausible via this
particular process's address translation (not 147/147/147) - attributed to an unverified POPTB.EXE address-map
anchor for those three cells specifically (the map-cell table anchor, used for every height/nibble read above,
is well-voted and read correctly); the flat-147 value itself rests on an earlier, separately-confirmed
measurement, not this session's own (failed) direct read of those three addresses.

## Corrected, 2026-10-21: today's own "95% of rows go pale, disp always positive" finding used a corrupted install

Re-ran today's own wide pixel-scan diagnostic (per-fragment `ground.wgsl` replica in Rust against real level/theme
data) to narrow down *which* bigfade-table mechanism produces the pale pixels, having confirmed `cell_light`
itself is faithful even at the worst two-axis corner cell. While verifying a `LevelList::discover` CLI-arg bug
(see below), noticed TWO Populous installs exist on this machine:
- `AppData/Local/Bullfrog/Populous/` - the one this entire session's diagnostics used (hardcoded path, matching
  the path this plan's very first "Where to see it" section names). Its `data/` directory dated **2014**.
- `Program Files (x86)/Bullfrog/Populous - A l'aube de la création/` (French retail/GOG edition) - files dated
  **1998**, matching the original release window.

Diffed their `data/` folders: `levl2001.dat`, `bigf0-c.dat`, `pal0-c.dat` are byte-identical between the two, but
**every single `disp0-X.dat` file differs**, and the AppData install has a stray `textures/` folder the
1998-dated one doesn't. Theme c's `disp0-c.dat`: the AppData copy is a narrow, oddly-centred **73..106** (exactly
what's already written into `docs/specs/terrain-textures.md` "disp" for theme c - that doc was evidently also
sourced from this same corrupted install, a pre-existing documentation problem beyond just this plan), full
0..255 range in the 1998-dated copy. **User confirmed: the AppData `data/` folder was leftover from a badly
uninstalled texture-pack extension, not genuine - deleted (`rm -rf`) on their instruction; only the 1998-dated
Program Files install is to be used going forward.**

Impact on today's "disp is a steady, always-positive ~90ish byte for theme c... row_jitter is reliably large and
positive... 144/151 nearby rows have a pale entry near their own column 0" mechanism (the "Resolved (exe side)"
sections above, and the ruled-out subdivision/low-res screenshot tests, which *also* hit this same corrupted
install via the separate `discover()` bug below): **re-ran the same wide pixel scan against the genuine 1998
data. The pale-pixel count drops from 102 to 35 (out of the same 10 000-point grid, same `>170` threshold) - the
phenomenon is real and still present with genuine data, clustered in the same general area, just roughly a third
the extent the corrupted install showed.** So the earlier mechanism (steep terrain sweeping the Gouraud-
interpolated altitude index through many bigfade rows, several of which have a pale entry near their own low-
brightness column) isn't *purely* a corrupted-asset artifact - but its reported severity very much was. Not yet
re-verified: the exact rows/cells responsible for the smaller genuine-data cluster (today's session ran out of
scope for this; re-run `dump_hill_pixels`-style diagnostics against the 1998 install specifically, not the
deleted one, before drawing further conclusions).

## Also found and fixed today: `LevelList::discover` silently loaded a GENERATED map for a bare filename arg

Separately from the above: `just shot out.png levl2001.dat` (this plan's own documented repro command, run from
the repository root, not the install's `levels/` directory) was **silently rendering a fully generated/procedural
map, not `levl2001.dat` at all** - every screenshot taken today this session before this fix (the `TERRAIN_SUBDIV`
comparison, the `SHOT_RESOLUTION` low-res test, both inconclusive/misleading as a result) was affected.

Root cause: `arg.map(PathBuf::from).or_else(|| env("POP3_LEVELS")).or_else(|| find_install()...)` - since `arg`
is `Some("levl2001.dat")`, the `.map` always succeeds, so neither fallback ever ran, skipping install
auto-detection entirely. `path.is_file()` then checked the bare string against the current directory (not the
install), found nothing, and the code treated the literal string `"levl2001.dat"` as an empty "levels directory"
- zero files found, **silently** falling back to `GameMap::generate(...)` with no warning anywhere (`load_current`
only warned on a parse *failure*, not on an empty file list). The secondary "objects objects: io error" /
"original unit sprites: io error" warnings this whole session were a downstream symptom of the same bug (the
install's parent directory resolving to empty), not a genuinely missing `objects/` folder as first assumed.

**Fixed** (`crates/game-client/src/world.rs`, committed nothing yet - uncommitted in the working tree): a new
pure `resolve_arg_path` tries `arg` as given first (preserves the existing "a full path to a level file
elsewhere, or a different levels directory" flexibility), then falls back to looking for it **by filename inside
the auto-detected install's own `levels/` dir** (or `$POP3_LEVELS`) - the actual common case this project's own
`just shot <file>.dat` convention relies on everywhere in `AGENTS.md` and this plan. Four new unit tests. Also,
per explicit request: `load_current` and `discover` now **always log plainly** which happened - `"loaded original
level: ..."` / `"no level files found in ...: falling back to generated maps/art"` / `"... - falling back to a
generated map"` - so this exact silent-fallback class of mistake is loud, not silent, from now on. Verified
end-to-end: `just shot`'s own invocation style now resolves to the genuine 1998 install with zero warnings.

**Not yet done**: redo the `TERRAIN_SUBDIV`/`SHOT_RESOLUTION` screenshot comparisons properly (correct install,
correct invocation) if those hypotheses still matter now that the pixel-level diagnostic approach has mostly
superseded them; re-verify this plan's very first "Where to see it" repro path still shows a (smaller, per above)
pale band against the 1998 install specifically.

## Final resolution, 2026-10-21 (addendum, written after reconstruction - see note at top)

The remaining ~30-pixel genuine-data cluster above turned out to be two compounding implementation bugs in
`ground.wgsl`'s land branch, both confirmed at instruction level by the spec session (not guessed):

1. **The row/row+1 RGB blend (`7a6d98d`) had no basis in the exe.** It reads `bigf0-X.dat` exactly once per
   pixel, at a single flat index - never blending two rows. The exe also masks away the interpolated altitude's
   own sub-integer fraction (`and e?x, 0xFFFF00FF`) right before building the row term, so no fractional
   precision survives into the index either - ruling out both the blend and an earlier, separately-tried "fold
   the live fraction into the column axis" attempt (which produced a dense zigzag, since it kept exactly the
   per-pixel entropy the exe discards). Fix: a single hard-floored `row = floor(altitude)`, no blend, no
   fraction folding.
2. **`row_jitter` (disp's own row-moving term) was being summed directly alongside `brightness` as if it were
   column-scale**, but the exe's own `and ecx,0xFFFFFC03; sar ecx,0x2` masking-before-shift makes it always a
   multiple of 256 - i.e. it moves the picked **row**, not the column, by up to dozens of rows on steep/high
   terrain. Under-scaling it by 256x left the hill's whole altitude-driven row range far more static/exposed
   than the original, which is what kept the patch visible even after fix 1. Fix: `row = altitude_int +
   row_jitter_quotient`, the sum multiplied by `BIGFADE_WIDTH` once.

Verified both ways: a pixel-level Rust-side replica of the shader against the real `levl2001.dat`/theme-`c` data
(pale-pixel count in the repro's own hotspot: 32 -> 0, the few remaining hits relocated far away to an unrelated
part of the map) and visually by the user against the genuine 1998 install, who confirmed it fixed.

Also found along the way, deliberately left open (not blockers for this fix):
- `game_core::shading::CellShading`'s `ambient_nibble` is hardcoded `0`, but it's really a dynamic 0-15 shadow
  cast by buildings/trees/scenery on or near a cell (`landscape_move_1` 0x403A00, `landscape_set_unit_shadow`
  0x403C10, `calc_unit_shade_value` 0x450D50) - wrong on any cell near such an object.
- A runtime table at 0x96EAE0/0x970AE0, read once more after `cliff0-X.dat`, indexed by the cliff output plus a
  fourth, unidentified per-corner fixed-point attribute - untraced, role and whether our ground shader needs it
  both unknown.

Committed as `734d78e` (the `ground.wgsl` fix; an earlier, wrong attempt at fix 2 was `5b840c1`, fixing only
issue 1 and accepting a "hedgehog" noise trade-off that turned out to be unnecessary once issue 2 was also
fixed), `53b2abb` (spec corrections), `f3879ac` (`TODO.md` trimmed to the final summary) on the `terrain` branch.
Full spec trace: `docs/specs/pop3-rev-analysis.md` "Terrain land colour", `docs/specs/terrain-textures.md`
"bigfade"/"disp"/"Our bake".
