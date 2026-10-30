//! Map loading/switching and the curved terrain entity.

use crate::camera::{CameraRig, CurveParamsRes};
use crate::ground_material::GroundMaterial;
use crate::terrain_mesh;
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use game_core::map::GameMap;
use game_core::terrain::DirtyRect;
use pop3_format::Theme;
use std::path::{Path, PathBuf};

/// The GPU-side assets `rebuild_terrain` writes into, bundled so the system stays under the 16-param
/// `SystemParam` tuple limit now that it also takes `TerrainGlide`/`SimClock`.
#[derive(SystemParam)]
struct TerrainAssets<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    mats: ResMut<'w, Assets<GroundMaterial>>,
    images: ResMut<'w, Assets<Image>>,
}

#[derive(Resource)]
pub struct CurrentMap(pub GameMap);

/// Original `levl*.dat` files found on disk, PageUp/PageDown cycles through them.
#[derive(Resource, Default, Clone)]
pub struct LevelList {
    /// Original `data/` dir (themes, sprites), `$POP3_DATA` or the levels dir's sibling `data`.
    pub data_dir: PathBuf,
    /// Original `objects/` dir (3D models), `$POP3_OBJECTS` or the levels dir's sibling `objects`.
    pub objects_dir: PathBuf,
    /// false with `--no-original`: never read any original file.
    pub original: bool,
    pub files: Vec<PathBuf>,
    pub index: usize,
}

/// Set when the heightmap changed and the mesh must be rebuilt.
#[derive(Resource, Default)]
pub struct TerrainDirty(pub bool);

/// Regions whose heights the simulation changed since the last frame: the ground texture is baked again and the
/// terrain mesh patched (`terrain_mesh::patch`) there only (a land bridge changes a few cells every tick), unless
/// `TerrainDirty` asks for all of it.
#[derive(Resource, Default)]
pub struct TerrainPatches(pub Vec<DirtyRect>);

/// Heightmap state to glide the terrain mesh from, while `SimClock::alpha()` is still short of 1 for the tick
/// window that produced it (the sim ticks at 12 Hz, rendering at ~60 fps, so a terrain edit's cells would
/// otherwise jump straight to their new height once every ~5 frames instead of rising smoothly, unlike every
/// other world visual). Set once per tick-running frame by `units::run_ticks` (mirrors `SimClock::prev*`),
/// cleared by `rebuild_terrain` once the glide has caught up.
#[derive(Resource, Default)]
pub struct TerrainGlide {
    pub prev: Option<game_core::terrain::Heightmap>,
    pub rects: Vec<DirtyRect>,
}

/// Cell borders drawn on the ground: on in the sandboxes, G toggles it.
#[derive(Resource, Default)]
pub struct ShowGrid(pub bool);

#[derive(Component)]
struct TerrainMesh;

/// The terrain's one ground material (`ground_material.rs`): created once with placeholder textures
/// (`spawn_terrain`), filled with the real per-level data on `rebuild_terrain`'s first pass and every level
/// switch, and synced a dirty rect at a time after that — never swapped for a new handle, so the mesh's own
/// `MeshMaterial3d` component is set once and left alone.
#[derive(Resource)]
pub struct TerrainMaterial(pub Handle<GroundMaterial>);

impl LevelList {
    /// No original data at all: maps and themes are generated, PgUp/PgDn change the seed.
    pub fn generated_only() -> Self {
        LevelList::default()
    }

    /// `arg` may be a level file, a levels directory, a bare filename to find inside the install's own
    /// `levels/` (`just shot out.png levl2001.dat`, run from anywhere - not necessarily that directory), or
    /// nothing: then `$POP3_LEVELS`, else the `levels/` of the install (`$POP3_INSTALL` or the usual Wine /
    /// `C:\` locations). Nothing found: fully generated, like `--no-original`.
    pub fn discover(arg: Option<&str>) -> Self {
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        let fallback_dir = env("POP3_LEVELS").or_else(|| pop3_format::install::find_install().map(|i| pop3_format::install::subdir(&i, "levels")));
        let path = match arg {
            Some(a) => Some(resolve_arg_path(a, fallback_dir.as_deref(), &|p| p.exists())),
            None => fallback_dir,
        };
        let Some(path) = path else {
            info!("no original install found (set POP3_INSTALL): using generated maps and art");
            return LevelList::generated_only();
        };
        let (dir, selected) = if path.is_file() {
            (path.parent().map(Path::to_path_buf).unwrap_or_default(), Some(path.clone()))
        } else {
            (path, None)
        };
        let files = list_levels(&dir);
        if files.is_empty() {
            warn!("no level files found in {} - falling back to generated maps/art", dir.display());
        } else {
            info!("original levels: {} ({} found)", dir.display(), files.len());
        }
        let index = selected.and_then(|s| files.iter().position(|f| *f == s)).unwrap_or(0);
        let (data_sibling, objects_sibling) = sibling_dirs(&dir);
        let data_dir = env("POP3_DATA").unwrap_or(data_sibling);
        let objects_dir = env("POP3_OBJECTS").unwrap_or(objects_sibling);
        LevelList { data_dir, objects_dir, original: true, files, index }
    }

    /// The current level, as a new game (`new_game`): always logs plainly which one happened, so a silent
    /// fallback (an unresolved level file, a bad parse) never masquerades as the real thing (AH: a prior
    /// session lost real time to exactly this - a bare filename arg not found fell back to a generated map
    /// with no indication at all).
    pub fn load_current(&self) -> GameMap {
        let map = match self.files.get(self.index) {
            Some(p) => match GameMap::load_original(p) {
                Ok(map) => {
                    info!("loaded original level: {}", p.display());
                    map
                }
                Err(e) => {
                    warn!("{}: {e} - falling back to a generated map", p.display());
                    GameMap::generate(self.index as u32 + 1)
                }
            },
            None => {
                warn!("no original level at index {} ({} found) - falling back to a generated map", self.index, self.files.len());
                GameMap::generate(self.index as u32 + 1)
            }
        };
        new_game(map)
    }
}

/// `arg` resolved against the current directory first (a level file or levels directory of its own, found
/// as given - the flexible cases `discover`'s own doc comment already covers), falling back to looking for it
/// by filename inside `fallback_dir` (the auto-detected install's own `levels/`, or `$POP3_LEVELS`) when it
/// doesn't exist as given. Without this fallback, the common case - `just shot out.png levl2001.dat`, run from
/// the repository root, not the install's own `levels/` directory - silently treated the bare filename as a
/// (nonexistent) directory and fell all the way through to a fully generated map, with nothing in `discover`
/// itself to say so (`load_current`'s own logging now covers that half; this is the other half, resolving the
/// path correctly in the first place). `exists` abstracts the filesystem check so this stays pure/testable.
fn resolve_arg_path(arg: &str, fallback_dir: Option<&Path>, exists: &dyn Fn(&Path) -> bool) -> PathBuf {
    let p = PathBuf::from(arg);
    if exists(&p) {
        return p;
    }
    if let Some(dir) = fallback_dir {
        let joined = dir.join(arg);
        if exists(&joined) {
            return joined;
        }
    }
    p
}

/// `map` as a new game, with its own seed: `GAME_SEED` if set (to replay a game), else drawn from the clock.
pub fn new_game(map: GameMap) -> GameMap {
    let seed = std::env::var("GAME_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or_else(clock_seed);
    map.seeded(seed)
}

fn clock_seed() -> u32 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    ((t.as_nanos() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32) as u32
}

/// `data/` and `objects/` next to a levels directory (any case).
pub fn sibling_dirs(levels_dir: &Path) -> (PathBuf, PathBuf) {
    let install = levels_dir.parent().unwrap_or(levels_dir);
    (pop3_format::install::subdir(install, "data"), pop3_format::install::subdir(install, "objects"))
}

/// The original level files in a directory, in level order (`sort_levels`).
pub fn list_levels(dir: &Path) -> Vec<PathBuf> {
    let files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            name.starts_with("levl") && name.ends_with(".dat") && name[4..name.len() - 4].chars().all(|c| c.is_ascii_digit())
        })
        .collect();
    sort_levels(files)
}

/// Level order, the same on every machine whatever the directory listing order or the file name
/// case: by the number in `levlNNNN.dat`, then by lower-case name.
pub fn sort_levels(mut files: Vec<PathBuf>) -> Vec<PathBuf> {
    let key = |p: &PathBuf| {
        let name = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
        let number = name.trim_start_matches("levl").trim_end_matches(".dat").parse::<u32>().unwrap_or(u32::MAX);
        (number, name)
    };
    files.sort_by_cached_key(key);
    files
}

pub struct WorldPlugin {
    pub level_arg: Option<String>,
    /// false = never read the original game files (`--no-original`).
    pub use_original: bool,
}

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        let levels = if self.use_original {
            LevelList::discover(self.level_arg.as_deref())
        } else {
            LevelList::generated_only()
        };
        let map = levels.load_current();
        let objects = if levels.original {
            crate::original_models::OriginalObjects::load(&levels.objects_dir)
        } else {
            crate::original_models::OriginalObjects::default()
        };
        app.insert_resource(CurrentMap(map))
            .insert_resource(objects)
            .insert_resource(levels)
            .insert_resource(TerrainDirty(true))
            .init_resource::<TerrainPatches>()
            .init_resource::<TerrainGlide>()
            .init_resource::<ShowGrid>()
            .init_resource::<CurveParamsRes>()
            .add_systems(Startup, spawn_terrain)
            .add_systems(Update, ((switch_level, toggle_grid).in_set(crate::menu::Gameplay), rebuild_terrain.after(crate::units::SimStep)).chain())
            .add_systems(Update, animate_water.in_set(crate::menu::Gameplay));
    }
}

fn spawn_terrain(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<GroundMaterial>>, mut images: ResMut<Assets<Image>>) {
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    let material = mats.add(crate::ground_material::placeholder(&mut images));
    commands.insert_resource(TerrainMaterial(material.clone()));
    commands.spawn((TerrainMesh, Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::default()));
}

#[allow(clippy::too_many_arguments)]
fn switch_level(
    keys: crate::keymap::Shortcuts,
    mut levels: ResMut<LevelList>,
    mut map: ResMut<CurrentMap>,
    mut dirty: ResMut<TerrainDirty>,
    mut glide: ResMut<TerrainGlide>,
    mut selected: ResMut<crate::hud::spells::SelectedSpell>,
    mut blueprint: ResMut<crate::blueprint::Blueprint>,
    mut schedule: ResMut<crate::units::GameSchedule>,
    mut clock: ResMut<crate::units::SimClock>,
) {
    use crate::keymap::Shortcut;
    let forward = if keys.just_pressed(Shortcut::NextLevel) {
        true
    } else if keys.just_pressed(Shortcut::PreviousLevel) {
        false
    } else {
        return;
    };
    levels.index = next_index(levels.index, levels.files.len(), forward);
    map.0 = levels.load_current();
    *schedule = crate::units::GameSchedule::default();
    clock.new_game();
    selected.0 = None;
    blueprint.put_away();
    dirty.0 = true;
    // A glide in flight would otherwise keep blending towards/from the old level's heightmap once `clock`'s
    // alpha resets near 0 for the new game.
    glide.prev = None;
    glide.rects.clear();
}

fn toggle_grid(keys: crate::keymap::Shortcuts, mut grid: ResMut<ShowGrid>, mut dirty: ResMut<TerrainDirty>) {
    if keys.just_pressed(crate::keymap::Shortcut::Grid) {
        grid.0 = !grid.0;
        dirty.0 = true;
    }
}

/// Wraps through original files; with none, walks generated seeds (never below 0).
pub fn next_index(index: usize, files: usize, forward: bool) -> usize {
    match (files, forward) {
        (0, true) => index + 1,
        (0, false) => index.saturating_sub(1),
        (n, true) => (index + 1) % n,
        (n, false) => (index + n - 1) % n,
    }
}

/// The original theme to read, if any: never without original files, even if the map names one.
pub fn original_theme(theme: Option<u8>, original: bool) -> Option<u8> {
    theme.filter(|_| original)
}

/// Whether `rebuild_terrain` has anything to do this frame: a forced rebuild (`dirty`), new patches
/// (`!changed_is_empty`), the focus moved, or a still-in-flight glide (`gliding`) that needs re-drawing towards
/// its target even though nothing landed this frame.
fn terrain_needs_rebuild(dirty: bool, changed_is_empty: bool, focus_moved: bool, gliding: bool) -> bool {
    dirty || !changed_is_empty || focus_moved || gliding
}

#[allow(clippy::too_many_arguments)]
fn rebuild_terrain(
    rig: Res<CameraRig>,
    map: Res<CurrentMap>,
    params: Res<CurveParamsRes>,
    mut dirty: ResMut<TerrainDirty>,
    mut patches: ResMut<TerrainPatches>,
    mut glide: ResMut<TerrainGlide>,
    clock: Res<crate::units::SimClock>,
    mut last_focus: Local<Option<Vec2>>,
    mut baked: Local<Option<(Theme, crate::terrain_texture::GroundCells)>>,
    mut geometry: Local<Option<terrain_mesh::TerrainGeometry>>,
    levels: Res<LevelList>,
    material: Res<TerrainMaterial>,
    mut assets: TerrainAssets,
    mut walkable_cache: Local<Vec<bool>>,
    q: Query<&Mesh3d, With<TerrainMesh>>,
    grid: Res<ShowGrid>,
) {
    let changed = std::mem::take(&mut patches.0);
    let gliding = glide.prev.is_some() && !glide.rects.is_empty();
    if !terrain_needs_rebuild(dirty.0, changed.is_empty(), *last_focus != Some(rig.focus), gliding) {
        return;
    }
    // Merged once, shared below: an ongoing effect (Volcano's rise, its lava flow) can dirty a few hundred
    // adjacent single-cell rects a tick, each otherwise rebaking `REBAKE_MARGIN`'s neighbourhood over the same
    // ground its touching neighbours just did.
    let changed = crate::terrain_texture::merge_dirty_rects(&changed);
    // `changed` is already merged above; `glide.rects` was merged once at capture time in `run_ticks`, so neither
    // branch here needs to merge again. While nothing landed this frame, the last tick's rects keep getting
    // re-touched towards the same target as `clock.alpha()` climbs, until the glide catches up — shared by the
    // texture bake below and the geometry patch further down.
    let touch_set: &[DirtyRect] = if !changed.is_empty() { &changed } else { &glide.rects };
    let glide_src = glide.prev.as_ref().filter(|_| gliding).map(|prev| (prev, clock.alpha()));
    match baked.as_mut().filter(|_| !dirty.0) {
        Some((_, cells)) => {
            // Dirty-rect limited (`rewalkable_cells`), not a whole-map scan: refreshed only on a real tick,
            // reused as-is on a pure reglide frame (the coastline doesn't move while a rim rises above it) so it
            // never runs every reglide frame either.
            for &rect in &changed {
                crate::terrain_texture::rewalkable_cells(&map.0.terrain, rect, &mut walkable_cache);
            }
            if !changed.is_empty() || gliding {
                for &rect in touch_set {
                    crate::terrain_texture::rebake_cells(&map.0.terrain, &map.0.shading, &map.0.ashes, &walkable_cache, rect, &mut cells.channels, &mut cells.altitude, glide_src);
                }
                if let Some(m) = assets.mats.get(&material.0) {
                    crate::ground_material::sync(m, &mut assets.images, cells);
                }
            }
        }
        None => {
            let original = original_theme(map.0.theme, levels.original).and_then(|t| {
                Theme::load(&levels.data_dir, t).map_err(|e| warn!("theme {t}: {e}")).ok()
            });
            let theme = original.unwrap_or_else(|| crate::procedural_theme::generate(levels.index as u32 + 1));
            *walkable_cache = crate::terrain_texture::walkable_cells(&map.0.terrain);
            let cells = crate::terrain_texture::bake_cells(&map.0.terrain, &map.0.shading, &map.0.ashes, &walkable_cache);
            if let Some(mut m) = assets.mats.get_mut(&material.0) {
                crate::ground_material::fill(&mut m, &mut assets.images, &cells, &theme, grid.0);
            }
            *baked = Some((theme, cells));
        }
    }
    // The camera moving needs every vertex recomputed (its curved bend is relative to the live focus); otherwise
    // only `changed`'s own cells do, `terrain_mesh::patch` touching just those instead of the whole disc — the
    // difference that matters while an ongoing effect (Volcano's rise, its lava flow) dirties the ground almost
    // every tick with the camera standing still.
    let focus_moved = dirty.0 || *last_focus != Some(rig.focus);
    dirty.0 = false;
    *last_focus = Some(rig.focus);
    let focus = (rig.focus.x, rig.focus.y);
    let touched = if focus_moved || geometry.is_none() {
        *geometry = Some(terrain_mesh::build(&map.0.terrain, focus, &params.0));
        true
    } else {
        terrain_mesh::patch(geometry.as_mut().unwrap(), &map.0.terrain, focus, &params.0, touch_set, glide_src)
    };
    if gliding && clock.alpha() >= 1.0 {
        glide.prev = None;
        glide.rects.clear();
    }
    if !touched {
        return;
    }
    let g = geometry.as_ref().unwrap();
    for handle in &q {
        if let Some(mut mesh) = assets.meshes.get_mut(&handle.0) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, g.positions.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, g.normals.clone());
            if focus_moved {
                mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, g.uvs.clone());
                mesh.insert_indices(Indices::U32(g.indices.clone()));
            }
        }
    }
}

/// Drives the sea's own ripple (`ground_material::set_turn`/`set_height_scale`, `ground.wgsl`'s vertex/fragment
/// stages) every frame, independently of `rebuild_terrain`: that system skips most frames outright (nothing else
/// changed), but the sea must keep rippling regardless.
fn animate_water(map: Res<CurrentMap>, curve: Res<CurveParamsRes>, material: Res<TerrainMaterial>, mut mats: ResMut<Assets<GroundMaterial>>) {
    if let Some(mut m) = mats.get_mut(&material.0) {
        crate::ground_material::set_turn(&mut m, map.0.now.to_wire());
        crate::ground_material::set_height_scale(&mut m, curve.0.height_scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_theme_only_with_original_files() {
        assert_eq!(original_theme(Some(3), true), Some(3));
        assert_eq!(original_theme(Some(3), false), None);
        assert_eq!(original_theme(None, true), None);
    }

    #[test]
    fn terrain_rebuild_gate() {
        assert!(!terrain_needs_rebuild(false, true, false, false), "nothing changed, nothing to do");
        assert!(terrain_needs_rebuild(true, true, false, false), "forced dirty");
        assert!(terrain_needs_rebuild(false, false, false, false), "new patches");
        assert!(terrain_needs_rebuild(false, true, true, false), "focus moved");
        assert!(terrain_needs_rebuild(false, true, false, true), "still gliding");
    }

    #[test]
    fn levels_in_number_order_whatever_the_listing_and_case() {
        let files = ["levels/levl2080.dat", "levels/LEVL2002.DAT", "levels/levl2001.dat", "levels/Levl2025.dat"].map(PathBuf::from).to_vec();
        let sorted = sort_levels(files.iter().rev().cloned().collect());
        assert_eq!(sorted, sort_levels(files));
        let names: Vec<_> = sorted.iter().map(|p| p.file_name().unwrap().to_string_lossy().to_lowercase()).collect();
        assert_eq!(names, ["levl2001.dat", "levl2002.dat", "levl2025.dat", "levl2080.dat"]);
    }

    #[test]
    fn level_cycling() {
        assert_eq!(next_index(2, 3, true), 0);
        assert_eq!(next_index(0, 3, false), 2);
        assert_eq!(next_index(4, 0, true), 5, "generated seeds keep going");
        assert_eq!(next_index(0, 0, false), 0);
    }

    #[test]
    fn data_and_objects_sit_next_to_levels() {
        let (data, objects) = sibling_dirs(Path::new("/nowhere/Populous/levels"));
        assert_eq!((data, objects), (PathBuf::from("/nowhere/Populous/data"), PathBuf::from("/nowhere/Populous/objects")));
    }

    #[test]
    fn generated_only_lists_nothing() {
        let l = LevelList::generated_only();
        assert!(l.files.is_empty() && !l.original);
    }

    #[test]
    fn resolve_arg_path_uses_the_arg_as_given_when_it_already_exists() {
        let exists = |p: &Path| p == Path::new("/cwd/levl2001.dat");
        let p = resolve_arg_path("/cwd/levl2001.dat", Some(Path::new("/install/levels")), &exists);
        assert_eq!(p, PathBuf::from("/cwd/levl2001.dat"), "found as given: never even looks at the fallback dir");
    }

    #[test]
    fn resolve_arg_path_finds_a_bare_filename_inside_the_fallback_dir() {
        let exists = |p: &Path| p == Path::new("/install/levels/levl2001.dat");
        let p = resolve_arg_path("levl2001.dat", Some(Path::new("/install/levels")), &exists);
        assert_eq!(p, PathBuf::from("/install/levels/levl2001.dat"), "not found in cwd, found by filename in the install's levels dir");
    }

    #[test]
    fn resolve_arg_path_falls_back_to_the_bare_arg_when_nothing_resolves() {
        let p = resolve_arg_path("levl2001.dat", Some(Path::new("/install/levels")), &|_| false);
        assert_eq!(p, PathBuf::from("levl2001.dat"), "same degrade-to-empty-listing behaviour as before this fix, not a crash");
    }

    #[test]
    fn resolve_arg_path_without_a_fallback_dir_just_returns_the_arg() {
        let p = resolve_arg_path("levl2001.dat", None, &|_| false);
        assert_eq!(p, PathBuf::from("levl2001.dat"));
    }
}
