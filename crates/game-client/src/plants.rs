//! A level's plants (`GameMap::plants`, pop3-rev-analysis.md "Scenery looks: plants and prayer heads"): each one
//! camera-facing sprite from `hfx0-0.dat` (`PLANT_SPRITES`), bottom-centred on it like the volcano's rocks, never
//! turning nor moving. Only with the original files: our own maps have no plants.

use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::{render_pos, Grounded};
use crate::units::art::{picture_frame, Frame};
use crate::units::{toward_eye, upload_frame, FrameAsset};
use crate::world::{CurrentMap, LevelList};
use bevy::prelude::*;
use game_core::plant::PlantKind;
use pop3_format::catalog::{EFFECT_SPRITE_FILE, PLANT_SPRITES};
use pop3_format::{Picture, SpriteBank, Theme, WORLD_UNITS_PER_CELL};
use std::path::Path;

/// Footprint half size (cells): it rests on the lowest ground under it.
const HALF: f32 = 0.1;
/// How far (cells) the sprite is pulled towards the camera so nearby ground does not cut it (`wood.rs`).
const PULL_TO_EYE: f32 = 0.3;

/// The reeds' and the bulrushes' pictures.
#[derive(Resource)]
struct PlantSprites([FrameAsset; 2]);

#[derive(Component)]
struct PlantView;

#[derive(Component)]
struct PlantBody;

pub struct PlantsPlugin;

impl Plugin for PlantsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_sprites)
            .add_systems(Update, (respawn_views, face_camera).chain())
            .add_systems(PostUpdate, pull_to_eye.before(TransformSystems::Propagate));
    }
}

/// Which of `PLANT_SPRITES` draws `kind`.
fn sprite_index(kind: PlantKind) -> usize {
    match kind {
        PlantKind::Reeds => 0,
        PlantKind::Bulrushes => 1,
    }
}

/// The original sprite `sprite`, feet at its bottom centre.
fn original_frame(data_dir: &Path, sprite: usize) -> Result<Frame, String> {
    let err = |e: pop3_format::LevelError| e.to_string();
    let bank = SpriteBank::load(data_dir, EFFECT_SPRITE_FILE).map_err(err)?;
    let s = bank.sprites.get(sprite).ok_or(format!("no sprite {sprite} in {EFFECT_SPRITE_FILE}"))?;
    let palette = Theme::load(data_dir, 0).map_err(err)?.palette;
    let origin = (s.width / 2, s.height.saturating_sub(1));
    Ok(picture_frame(&Picture { sprite: s.clone(), origin }, &palette))
}

fn load_sprites(levels: Res<LevelList>, mut commands: Commands, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    if !levels.original {
        return;
    }
    let frames: Result<Vec<Frame>, String> = PLANT_SPRITES.iter().map(|&s| original_frame(&levels.data_dir, s)).collect();
    match frames {
        Ok(f) => {
            let [a, b] = [&f[0], &f[1]].map(|f| upload_frame(f, &mut images, &mut meshes, &mut mats));
            commands.insert_resource(PlantSprites([a, b]));
        }
        Err(e) => warn!("plant sprites: {e}"),
    }
}

/// One view per plant, redone when a map loads.
fn respawn_views(mut commands: Commands, map: Res<CurrentMap>, sprites: Option<Res<PlantSprites>>, existing: Query<Entity, With<PlantView>>) {
    let Some(sprites) = sprites else { return };
    if !map.is_changed() && !sprites.is_added() {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn();
    }
    let cell = WORLD_UNITS_PER_CELL as f32;
    for p in &map.0.plants {
        let sprite = &sprites.0[sprite_index(p.kind)];
        let at = Vec2::new(p.x as f32, p.z as f32) / cell;
        commands
            .spawn((PlantView, crate::object_scale::Scaled(crate::object_scale::ObjectGroup::Scenery), Grounded { at, half: HALF }, Transform::default(), Visibility::Hidden))
            .with_child((PlantBody, Mesh3d(sprite.mesh.clone()), MeshMaterial3d(sprite.material.clone()), Transform::default()));
    }
}

fn face_camera(rig: Res<CameraRig>, mut views: Query<&mut Transform, With<PlantView>>) {
    let facing = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
    for mut t in &mut views {
        t.rotation = facing;
    }
}

/// Pulls each sprite towards the camera, keeping its picture (`units::toward_eye`).
#[allow(clippy::type_complexity)]
fn pull_to_eye(
    map: Res<CurrentMap>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    eye: Query<&Transform, (With<GameCamera>, Without<PlantView>, Without<PlantBody>)>,
    views: Query<(&Grounded, &Transform, &Children), With<PlantView>>,
    mut bodies: Query<&mut Transform, (With<PlantBody>, Without<PlantView>)>,
) {
    let Ok(eye) = eye.single() else { return };
    for (ground, view, children) in &views {
        let Some(feet) = render_pos(&map.0.terrain, ground, rig.focus, &params.0) else { continue };
        let (offset, scale) = toward_eye(feet, eye.translation, PULL_TO_EYE);
        for &child in children {
            if let Ok(mut t) = bodies.get_mut(child) {
                t.translation = view.rotation.inverse() * offset;
                t.scale = Vec3::splat(scale);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_plant_has_its_own_sprite() {
        assert_ne!(sprite_index(PlantKind::Reeds), sprite_index(PlantKind::Bulrushes));
    }
}
