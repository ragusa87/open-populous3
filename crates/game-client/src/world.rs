//! Map loading/switching and the curved terrain entity.

use crate::camera::{CameraRig, CurveParamsRes};
use crate::terrain_mesh;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use game_core::map::GameMap;
use pop3_format::Theme;
use std::path::{Path, PathBuf};

#[derive(Resource)]
pub struct CurrentMap(pub GameMap);

/// Original `levl*.dat` files found on disk, PageUp/PageDown cycles through them.
#[derive(Resource, Default)]
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

/// Cell borders drawn on the ground: on in the sandboxes, G toggles it.
#[derive(Resource, Default)]
pub struct ShowGrid(pub bool);

#[derive(Component)]
struct TerrainMesh;

#[derive(Resource)]
struct TerrainMaterial(Handle<StandardMaterial>);

impl LevelList {
    /// No original data at all: maps and themes are generated, PgUp/PgDn change the seed.
    pub fn generated_only() -> Self {
        LevelList::default()
    }

    /// `arg` may be a level file, a levels directory, or nothing: then `$POP3_LEVELS`, else the
    /// `levels/` of the install (`$POP3_INSTALL` or the usual Wine / `C:\` locations). Nothing
    /// found: fully generated, like `--no-original`.
    pub fn discover(arg: Option<&str>) -> Self {
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        let path = arg.map(PathBuf::from).or_else(|| env("POP3_LEVELS")).or_else(|| {
            pop3_format::install::find_install().map(|i| pop3_format::install::subdir(&i, "levels"))
        });
        let Some(path) = path else {
            info!("no original install found (set POP3_INSTALL): using generated maps and art");
            return LevelList::generated_only();
        };
        let (dir, selected) = if path.is_file() {
            (path.parent().map(Path::to_path_buf).unwrap_or_default(), Some(path.clone()))
        } else {
            (path, None)
        };
        info!("original levels: {}", dir.display());
        let files = list_levels(&dir);
        let index = selected.and_then(|s| files.iter().position(|f| *f == s)).unwrap_or(0);
        let (data_sibling, objects_sibling) = sibling_dirs(&dir);
        let data_dir = env("POP3_DATA").unwrap_or(data_sibling);
        let objects_dir = env("POP3_OBJECTS").unwrap_or(objects_sibling);
        LevelList { data_dir, objects_dir, original: true, files, index }
    }

    pub fn load_current(&self) -> GameMap {
        self.files
            .get(self.index)
            .and_then(|p| GameMap::load_original(p).map_err(|e| warn!("{}: {e}", p.display())).ok())
            .unwrap_or_else(|| GameMap::generate(self.index as u32 + 1))
    }
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
            .init_resource::<ShowGrid>()
            .init_resource::<CurveParamsRes>()
            .add_systems(Startup, spawn_terrain)
            .add_systems(Update, ((switch_level, toggle_grid).in_set(crate::menu::Gameplay), rebuild_terrain).chain());
    }
}

fn spawn_terrain(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    let material = mats.add(StandardMaterial { perceptual_roughness: 0.95, ..default() });
    commands.insert_resource(TerrainMaterial(material.clone()));
    commands.spawn((TerrainMesh, Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::default()));
}

fn switch_level(
    keys: crate::keymap::Shortcuts,
    mut levels: ResMut<LevelList>,
    mut map: ResMut<CurrentMap>,
    mut dirty: ResMut<TerrainDirty>,
    mut spells: ResMut<crate::hud::spells::PlayerSpells>,
    mut selected: ResMut<crate::hud::spells::SelectedSpell>,
    mut builds: ResMut<crate::hud::build::PlayerBuilds>,
    mut blueprint: ResMut<crate::blueprint::Blueprint>,
    mut schedule: ResMut<crate::units::GameSchedule>,
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
    spells.0 = crate::hud::spells::level_book(&map.0);
    selected.0 = None;
    builds.0 = crate::hud::build::level_builds(&map.0);
    blueprint.put_away();
    dirty.0 = true;
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

fn rebuild_terrain(
    rig: Res<CameraRig>,
    map: Res<CurrentMap>,
    params: Res<CurveParamsRes>,
    mut dirty: ResMut<TerrainDirty>,
    mut last_focus: Local<Option<Vec2>>,
    levels: Res<LevelList>,
    material: Res<TerrainMaterial>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    q: Query<&Mesh3d, With<TerrainMesh>>,
    grid: Res<ShowGrid>,
) {
    if !dirty.0 && *last_focus == Some(rig.focus) {
        return;
    }
    if dirty.0 {
        let original = original_theme(map.0.theme, levels.original).and_then(|t| {
            Theme::load(&levels.data_dir, t).map_err(|e| warn!("theme {t}: {e}")).ok()
        });
        let theme = original.unwrap_or_else(|| crate::procedural_theme::generate(levels.index as u32 + 1));
        let image = images.add(theme_image(&map.0, &theme, params.0.height_scale, grid.0));
        if let Some(mut m) = mats.get_mut(&material.0) {
            m.unlit = true;
            m.base_color_texture = Some(image);
        }
    }
    dirty.0 = false;
    *last_focus = Some(rig.focus);
    let g = terrain_mesh::build(&map.0.terrain, (rig.focus.x, rig.focus.y), &params.0);
    for handle in &q {
        if let Some(mut mesh) = meshes.get_mut(&handle.0) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, g.positions.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, g.normals.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, g.uvs.clone());
            mesh.insert_indices(Indices::U32(g.indices.clone()));
        }
    }
}

fn theme_image(map: &GameMap, theme: &Theme, height_scale: f32, grid: bool) -> Image {
    let row_scale = std::env::var("POP3_ROW_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
    let (side, mut pixels) = crate::terrain_texture::bake(&map.terrain, theme, height_scale, row_scale);
    if grid {
        crate::terrain_texture::draw_grid(&mut pixels, side);
    }
    let mut image = Image::new(
        Extent3d { width: side as u32, height: side as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
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
}
