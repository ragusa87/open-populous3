//! Camp fires on the map (`GameMap::campfires`): the original object 0 when allowed (its dark
//! logs from the theme atlas, its flame boards animated by `flame`), else generated logs and
//! crossed boards with the generated flame. Views follow the simulation's fires by id: lit ones
//! appear, burnt-out ones go.

use crate::flame::{self, Flame, FlameFrames};
use crate::grounded::Grounded;
use crate::original_models::{atlas_image, object_mesh, solid_part, to_mesh, MeshData, OriginalObjects};
use crate::world::{CurrentMap, LevelList};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::campfire::Campfire;
use pop3_format::catalog::CAMP_FIRE;
use pop3_format::{Atlas, Theme, WORLD_UNITS_PER_CELL};

/// Theme whose atlas and alpha table draw camp fires on maps without one.
const DEFAULT_THEME: u8 = 0;
/// Footprint half size (cells): the fire rests on the lowest ground under it.
const HALF: f32 = 0.15;
/// The generated fire: its size (cells, like the original object), logs' thickness and colour.
const SIZE: f32 = 0.31;
const LOG: f32 = 0.05;
const LOG_COLOUR: Color = Color::srgb(0.28, 0.17, 0.09);

/// What every camp fire of the current map is drawn with, for the theme it was made for.
#[derive(Resource)]
struct FireArt {
    theme: u8,
    logs: Handle<Mesh>,
    logs_material: Handle<StandardMaterial>,
    flame: Handle<Mesh>,
}

/// The view of the camp fire with this id.
#[derive(Component)]
struct FireView(u32);

pub struct CampfirePlugin;

impl Plugin for CampfirePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_views);
    }
}

/// Four logs in a cross, leaning in a little: the generated fire's wood.
fn generated_logs() -> MeshData {
    let (r, top) = (SIZE / 2.0, LOG);
    let ends = [Vec3::X, Vec3::Z, -Vec3::X, -Vec3::Z].map(|d| (d * r + Vec3::Y * LOG / 2.0, d * r * 0.1 + Vec3::Y * top));
    crate::construction::beams(&ends, LOG)
}

/// The original fire for `theme`: object 0's logs and flame mesh, and its atlas. None without the
/// original files.
fn original_art(levels: &LevelList, objects: &OriginalObjects, theme: u8, meshes: &mut Assets<Mesh>, images: &mut Assets<Image>, mats: &mut Assets<StandardMaterial>) -> Option<FireArt> {
    if !levels.original {
        return None;
    }
    let obj = objects.0.as_ref()?.get(CAMP_FIRE)?;
    let load = || -> Result<_, pop3_format::LevelError> {
        Ok((Atlas::load(&levels.data_dir, theme)?, Theme::load(&levels.data_dir, theme)?))
    };
    let (atlas, palette) = load().map_err(|e| warn!("camp fire art for theme {theme}: {e}")).ok()?;
    let logs_material = mats.add(StandardMaterial {
        base_color_texture: Some(images.add(atlas_image(&atlas, &palette.palette))),
        perceptual_roughness: 0.95,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    Some(FireArt {
        theme,
        logs: meshes.add(to_mesh(object_mesh(&solid_part(obj), 0))),
        logs_material,
        flame: meshes.add(to_mesh(flame::flame_mesh(obj))),
    })
}

fn generated_art(theme: u8, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) -> FireArt {
    FireArt {
        theme,
        logs: meshes.add(to_mesh(generated_logs())),
        logs_material: mats.add(StandardMaterial { base_color: LOG_COLOUR, perceptual_roughness: 0.95, ..default() }),
        flame: meshes.add(to_mesh(flame::crossed_boards(SIZE))),
    }
}

/// Which fires need a view and which views have no fire left, by id.
pub fn view_changes(fires: &[Campfire], shown: &[u32]) -> (Vec<usize>, Vec<u32>) {
    let new = fires.iter().enumerate().filter(|(_, f)| !shown.contains(&f.id)).map(|(i, _)| i).collect();
    let gone = shown.iter().copied().filter(|id| !fires.iter().any(|f| f.id == *id)).collect();
    (new, gone)
}

/// A new map drops every view (and the art when its theme changed); then views are added for new
/// fires and removed for those gone out.
#[allow(clippy::too_many_arguments)]
fn sync_views(
    mut commands: Commands,
    map: Res<CurrentMap>,
    levels: Res<LevelList>,
    objects: Res<OriginalObjects>,
    art: Option<Res<FireArt>>,
    views: Query<(Entity, &FireView)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut flames: ResMut<FlameFrames>,
) {
    let theme = map.0.theme.unwrap_or(DEFAULT_THEME);
    let art = art.filter(|a| a.theme == theme);
    let mut shown: Vec<u32> = Vec::new();
    for (e, view) in &views {
        if map.is_changed() || art.is_none() {
            commands.entity(e).despawn();
        } else {
            shown.push(view.0);
        }
    }
    let Some(art) = art else {
        if !map.0.campfires.is_empty() {
            let made = original_art(&levels, &objects, theme, &mut meshes, &mut images, &mut mats).unwrap_or_else(|| generated_art(theme, &mut meshes, &mut mats));
            commands.insert_resource(made);
        }
        return;
    };
    let (new, gone) = view_changes(&map.0.campfires, &shown);
    for (e, view) in &views {
        if gone.contains(&view.0) {
            commands.entity(e).despawn();
        }
    }
    let frames = if new.is_empty() { &[][..] } else { flames.get(&levels, theme, &mut images, &mut mats) };
    let cell = WORLD_UNITS_PER_CELL as f32;
    for i in new {
        let fire = &map.0.campfires[i];
        let at = Vec2::new(fire.x as f32, fire.z as f32) / cell;
        commands.spawn((FireView(fire.id), Grounded { at, half: HALF }, Transform::default(), Visibility::Hidden)).with_children(|v| {
            v.spawn((Mesh3d(art.logs.clone()), MeshMaterial3d(art.logs_material.clone())));
            let offset = fire.id as usize * 3 % flame::FRAMES;
            v.spawn((Flame { offset }, Mesh3d(art.flame.clone()), MeshMaterial3d(frames[offset].clone()), NotShadowCaster));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_follow_the_fires_by_id() {
        let fires = [Campfire::new(1, 0, (1, 1)), Campfire::new(4, 0, (2, 2))];
        assert_eq!(view_changes(&fires, &[]), (vec![0, 1], vec![]));
        assert_eq!(view_changes(&fires, &[1, 4]), (vec![], vec![]));
        assert_eq!(view_changes(&fires[1..], &[1, 4]), (vec![], vec![1]), "fire 1 went out");
        assert_eq!(view_changes(&fires, &[4]), (vec![0], vec![]));
    }

    #[test]
    fn generated_logs_lie_within_the_fire() {
        let m = generated_logs();
        assert!(!m.indices.is_empty());
        assert!(m.positions.iter().all(|p| p[0].abs() <= SIZE / 2.0 + LOG && p[2].abs() <= SIZE / 2.0 + LOG && p[1] >= -LOG && p[1] <= 2.0 * LOG));
    }
}
