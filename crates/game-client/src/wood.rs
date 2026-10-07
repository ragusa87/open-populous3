//! Pieces of wood lying on the ground (`GameMap::wood`), drawn like units as a camera-facing sprite
//! standing on the terrain: the original pile of logs (`hfx0-0.dat`, see docs/specs/sprites.md) when
//! allowed, else the bundled one (`assets/sprites/wood_pile.png`, same pixel-art size).

use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::{render_pos, Grounded};
use crate::units::art::{picture_frame, Frame};
use crate::units::sheets::decode;
use crate::units::{toward_eye, upload_frame, FrameAsset};
use crate::world::{CurrentMap, LevelList};
use bevy::prelude::*;
use pop3_format::catalog::{EFFECT_SPRITE_FILE, WOOD_PILE_SPRITE};
use pop3_format::{Picture, SpriteBank, Theme, WORLD_UNITS_PER_CELL};
use std::path::Path;

/// Open-source pile of logs (see assets/CREDITS.md), pixel art in the original sprite's size.
const BUNDLED: &[u8] = include_bytes!("../../../assets/sprites/wood_pile.png");
/// Footprint half size (cells): the pile rests on the lowest ground under it.
const HALF: f32 = 0.1;
/// How far (cells) the sprite is pulled towards the camera so nearby ground does not cut it.
const PULL_TO_EYE: f32 = 0.3;

#[derive(Resource)]
struct WoodSprite(FrameAsset);

#[derive(Component)]
struct WoodView;

#[derive(Component)]
struct WoodBody;

pub struct WoodPlugin;

impl Plugin for WoodPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_sprite)
            .add_systems(Update, (respawn_views, face_camera).chain())
            .add_systems(PostUpdate, pull_to_eye.before(TransformSystems::Propagate));
    }
}

/// The original pile of logs, feet at its bottom centre.
fn original_frame(data_dir: &Path) -> Result<Frame, String> {
    let err = |e: pop3_format::LevelError| e.to_string();
    let bank = SpriteBank::load(data_dir, EFFECT_SPRITE_FILE).map_err(err)?;
    let sprite = bank.sprites.get(WOOD_PILE_SPRITE).ok_or(format!("no sprite {WOOD_PILE_SPRITE} in {EFFECT_SPRITE_FILE}"))?;
    let palette = Theme::load(data_dir, 0).map_err(err)?.palette;
    let origin = (sprite.width / 2, sprite.height.saturating_sub(1));
    Ok(picture_frame(&Picture { sprite: sprite.clone(), origin }, &palette))
}

/// The bundled pile, feet at its bottom centre; None if the PNG does not decode.
pub fn bundled_frame() -> Option<Frame> {
    let (width, height, rgba) = decode(BUNDLED)?;
    Some(Frame { width, height, origin: (width / 2, height.saturating_sub(1)), rgba, scale: 1 })
}

fn load_sprite(levels: Res<LevelList>, mut commands: Commands, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let original = levels.original.then(|| original_frame(&levels.data_dir).map_err(|e| warn!("original wood sprite: {e}")).ok()).flatten();
    let Some(frame) = original.or_else(bundled_frame) else {
        warn!("bundled wood sprite does not decode: no wood drawn");
        return;
    };
    commands.insert_resource(WoodSprite(upload_frame(&frame, &mut images, &mut meshes, &mut mats)));
}

/// One view per piece of a newly loaded map.
fn respawn_views(mut commands: Commands, map: Res<CurrentMap>, sprite: Option<Res<WoodSprite>>, existing: Query<Entity, With<WoodView>>) {
    let Some(sprite) = sprite else { return };
    if !map.is_changed() && !sprite.is_added() {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn();
    }
    let cell = WORLD_UNITS_PER_CELL as f32;
    for piece in &map.0.wood {
        let at = Vec2::new(piece.x as f32, piece.z as f32) / cell;
        commands
            .spawn((WoodView, Grounded { at, half: HALF }, Transform::default(), Visibility::Hidden))
            .with_child((WoodBody, Mesh3d(sprite.0.mesh.clone()), MeshMaterial3d(sprite.0.material.clone()), Transform::default()));
    }
}

fn face_camera(rig: Res<CameraRig>, mut views: Query<&mut Transform, With<WoodView>>) {
    let facing = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(-rig.pitch);
    for mut t in &mut views {
        t.rotation = facing;
    }
}

/// Pulls each sprite towards the camera, keeping its picture (`units::toward_eye`).
fn pull_to_eye(
    map: Res<CurrentMap>,
    rig: Res<CameraRig>,
    params: Res<CurveParamsRes>,
    eye: Query<&Transform, (With<GameCamera>, Without<WoodView>, Without<WoodBody>)>,
    views: Query<(&Grounded, &Transform, &Children), With<WoodView>>,
    mut bodies: Query<&mut Transform, (With<WoodBody>, Without<WoodView>)>,
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
    fn bundled_pile_stands_on_its_bottom_centre() {
        let f = bundled_frame().expect("assets/sprites/wood_pile.png decodes");
        assert_eq!((f.width, f.height, f.scale), (17, 11, 1), "the original sprite's size");
        assert_eq!(f.origin, (8, 10));
        let opaque = |x: usize, y: usize| f.rgba[(y * f.width + x) * 4 + 3] == 255;
        assert!((0..f.width).any(|x| opaque(x, f.height - 1)), "touches the ground");
        assert!((0..f.width).all(|x| !opaque(x, 0)), "room above the top log");
    }
}
