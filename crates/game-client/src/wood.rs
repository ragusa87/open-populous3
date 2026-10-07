//! Pieces of wood lying on the ground (`GameMap::wood`), drawn like units as a camera-facing sprite
//! standing on the terrain: the original pile of logs (`hfx0-0.dat`, see docs/specs/sprites.md) when
//! allowed, else a generated one in the same pixel-art size.

use crate::camera::{CameraRig, CurveParamsRes, GameCamera};
use crate::grounded::{render_pos, Grounded};
use crate::units::art::{picture_frame, Frame};
use crate::units::{toward_eye, upload_frame, FrameAsset};
use crate::world::{CurrentMap, LevelList};
use bevy::prelude::*;
use pop3_format::catalog::{EFFECT_SPRITE_FILE, WOOD_PILE_SPRITE};
use pop3_format::{Picture, SpriteBank, Theme, WORLD_UNITS_PER_CELL};
use std::path::Path;

/// Generated pile size, as the original sprite (base pixels).
const PILE: (usize, usize) = (17, 11);
/// Footprint half size (cells): the pile rests on the lowest ground under it.
const HALF: f32 = 0.1;
/// How far (cells) the sprite is pulled towards the camera so nearby ground does not cut it.
const PULL_TO_EYE: f32 = 0.3;

const BARK: [u8; 4] = [86, 56, 30, 255];
const BARK_DARK: [u8; 4] = [58, 36, 18, 255];
const CUT: [u8; 4] = [196, 158, 104, 255];
const RING: [u8; 4] = [150, 112, 66, 255];

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

/// A generated pile in the original's size: two logs side by side and one on top, bark along
/// them and a light cut end (a round with a ring) facing the viewer.
pub fn generated_frame() -> Frame {
    let (w, h) = PILE;
    let mut rgba = vec![0u8; w * h * 4];
    let mut put = |x: usize, y: usize, c: [u8; 4]| {
        if x < w && y < h {
            rgba[(y * w + x) * 4..][..4].copy_from_slice(&c);
        }
    };
    // (left, top) of each log, 8 x 4: bark on the left, the round cut end on the right.
    for (lx, ty) in [(0, 6), (8, 7), (4, 2)] {
        for dy in 0..4 {
            for dx in 0..6 {
                put(lx + dx, ty + dy, if dy == 0 || dy == 3 { BARK_DARK } else { BARK });
            }
            let (from, to) = if dy == 0 || dy == 3 { (6, 8) } else { (5, 9) };
            for dx in from..to {
                let ring = (dx == 6 || dx == 7) && (dy == 1 || dy == 2);
                put(lx + dx, ty + dy, if ring { RING } else { CUT });
            }
        }
    }
    Frame { width: w, height: h, origin: (w / 2, h - 1), rgba, scale: 1 }
}

fn load_sprite(levels: Res<LevelList>, mut commands: Commands, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let original = levels.original.then(|| original_frame(&levels.data_dir).map_err(|e| warn!("original wood sprite: {e}")).ok()).flatten();
    let frame = original.unwrap_or_else(generated_frame);
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
    fn generated_pile_stands_on_its_bottom_centre() {
        let f = generated_frame();
        assert_eq!((f.width, f.height, f.scale), (PILE.0, PILE.1, 1));
        assert_eq!(f.origin, (PILE.0 / 2, PILE.1 - 1));
        let opaque = |x: usize, y: usize| f.rgba[(y * f.width + x) * 4 + 3] == 255;
        assert!((0..f.width).any(|x| opaque(x, f.height - 1)), "touches the ground");
        assert!((0..f.width).all(|x| !opaque(x, 0)), "room above the top log");
        assert!(f.rgba.chunks(4).any(|p| p == CUT), "cut ends show");
    }
}
