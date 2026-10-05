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

pub const DEFAULT_LEVELS_DIR: &str =
    "/home/laurent/.wine/drive_c/Program Files (x86)/Bullfrog/Populous - A l'aube de la création/levels";

#[derive(Resource)]
pub struct CurrentMap(pub GameMap);

/// Original `levl*.dat` files found on disk, PageUp/PageDown cycles through them.
#[derive(Resource, Default)]
pub struct LevelList {
    /// Original `data/` dir (themes), `$POP3_DATA` or `<levels>/../data`.
    pub data_dir: PathBuf,
    pub files: Vec<PathBuf>,
    pub index: usize,
}

/// Set when the heightmap changed and the mesh must be rebuilt.
#[derive(Resource, Default)]
pub struct TerrainDirty(pub bool);

#[derive(Component)]
struct TerrainMesh;

#[derive(Resource)]
struct TerrainMaterial(Handle<StandardMaterial>);

impl LevelList {
    /// `arg` may be a level file, a levels directory, or nothing (default dir / $POP3_LEVELS).
    pub fn discover(arg: Option<&str>) -> Self {
        let env = std::env::var("POP3_LEVELS").ok();
        let path = PathBuf::from(arg.or(env.as_deref()).unwrap_or(DEFAULT_LEVELS_DIR));
        let (dir, selected) = if path.is_file() {
            (path.parent().map(Path::to_path_buf).unwrap_or_default(), Some(path.clone()))
        } else {
            (path, None)
        };
        let files = list_levels(&dir);
        let index = selected.and_then(|s| files.iter().position(|f| *f == s)).unwrap_or(0);
        let data_dir = std::env::var("POP3_DATA").map(PathBuf::from).unwrap_or_else(|_| dir.join("../data"));
        LevelList { data_dir, files, index }
    }

    pub fn load_current(&self) -> GameMap {
        self.files
            .get(self.index)
            .and_then(|p| GameMap::load_original(p).map_err(|e| warn!("{}: {e}", p.display())).ok())
            .unwrap_or_else(|| GameMap::generate(self.index as u32 + 1))
    }
}

pub fn list_levels(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            name.starts_with("levl") && name.ends_with(".dat") && name[4..name.len() - 4].chars().all(|c| c.is_ascii_digit())
        })
        .collect();
    files.sort();
    files
}

pub struct WorldPlugin {
    pub level_arg: Option<String>,
}

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        let levels = LevelList::discover(self.level_arg.as_deref());
        let map = levels.load_current();
        app.insert_resource(CurrentMap(map))
            .insert_resource(levels)
            .insert_resource(TerrainDirty(true))
            .init_resource::<CurveParamsRes>()
            .add_systems(Startup, spawn_terrain)
            .add_systems(Update, (switch_level, rebuild_terrain).chain());
    }
}

fn spawn_terrain(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    let material = mats.add(StandardMaterial { perceptual_roughness: 0.95, ..default() });
    commands.insert_resource(TerrainMaterial(material.clone()));
    commands.spawn((TerrainMesh, Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::default()));
}

fn switch_level(
    keys: Res<ButtonInput<KeyCode>>,
    mut levels: ResMut<LevelList>,
    mut map: ResMut<CurrentMap>,
    mut dirty: ResMut<TerrainDirty>,
) {
    let n = levels.files.len().max(1);
    let step = if keys.just_pressed(KeyCode::PageDown) {
        1
    } else if keys.just_pressed(KeyCode::PageUp) {
        n - 1
    } else {
        return;
    };
    levels.index = (levels.index + step) % n;
    map.0 = levels.load_current();
    dirty.0 = true;
}

fn rebuild_terrain(
    rig: Res<CameraRig>,
    map: Res<CurrentMap>,
    params: Res<CurveParamsRes>,
    mut dirty: ResMut<TerrainDirty>,
    mut last_focus: Local<Option<Vec2>>,
    levels: Res<LevelList>,
    material: Res<TerrainMaterial>,
    mut textured: Local<bool>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    q: Query<&Mesh3d, With<TerrainMesh>>,
) {
    if !dirty.0 && *last_focus == Some(rig.focus) {
        return;
    }
    if dirty.0 {
        let theme = map.0.theme.and_then(|t| {
            Theme::load(&levels.data_dir, t).map_err(|e| warn!("theme {t}: {e}")).ok()
        });
        let image = theme.map(|t| images.add(theme_image(&map.0, &t, params.0.height_scale)));
        *textured = image.is_some();
        if let Some(mut m) = mats.get_mut(&material.0) {
            m.unlit = image.is_some();
            m.base_color_texture = image;
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
            if *textured {
                mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR);
            } else {
                mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, g.colors.clone());
            }
            mesh.insert_indices(Indices::U32(g.indices.clone()));
        }
    }
}

fn theme_image(map: &GameMap, theme: &Theme, height_scale: f32) -> Image {
    let (side, pixels) = crate::terrain_texture::bake(&map.terrain, theme, height_scale);
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
