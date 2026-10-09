//! Reincarnation sites: a stone ring around the shaman's spawn point. Every stone is its own
//! `Grounded` part, so the ring follows slopes and the planet curve instead of staying flat.
//! With the original files the stones are the game's reincarnation stones in the tribe's colour,
//! textured from the level theme's atlas; otherwise (or with `--no-original`) plain generated
//! blocks tinted with the tribe colour.

use crate::camera::CameraRig;
use crate::game_frame::GameYaw;
use crate::grounded::Grounded;
use crate::original_models::{atlas_image, object_mesh, to_mesh, OriginalObjects};
use crate::world::{CurrentMap, LevelList};
use bevy::prelude::*;
use crate::units::PLAYER;
use game_core::map::GameMap;
use game_core::site::ReincarnationSite;
use pop3_format::catalog::REINCARNATION_STONE;
use pop3_format::{Atlas, Object, Theme, WORLD_UNITS_PER_CELL};

const STONES: usize = 8;
const RING_RADIUS: f32 = 1.3;
const STONE: Vec3 = Vec3::new(0.22, 0.6, 0.22);
/// Footprint half size of a ring stone, wide enough for the original pillar (~0.47 cell).
const STONE_HALF: f32 = 0.24;
/// Theme whose atlas textures the stones on maps without one (sandboxes, generated maps).
const DEFAULT_THEME: u8 = 0;

#[derive(Component)]
struct SiteMarker;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SitePart {
    pub ground: Grounded,
    pub yaw: GameYaw,
}

pub struct SitesPlugin;

impl Plugin for SitesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, respawn_markers).add_systems(Update, look_at_site.in_set(crate::menu::Gameplay));
    }
}

/// Where the player's reincarnation site is (cells), if the map has one.
pub fn player_site_cell(map: &GameMap) -> Option<Vec2> {
    map.site_of(PLAYER).map(site_cell_pos)
}

/// H flies the camera to the player's reincarnation site.
fn look_at_site(keys: Res<ButtonInput<KeyCode>>, mut rig: ResMut<CameraRig>, map: Res<CurrentMap>) {
    if keys.just_pressed(KeyCode::KeyH) && let Some(cell) = player_site_cell(&map.0) {
        rig.fly_to(cell);
    }
}

/// Original tribe colours: blue, red, yellow, green.
pub fn tribe_color(owner: u8) -> Color {
    match owner {
        0 => Color::srgb(0.15, 0.35, 0.95),
        1 => Color::srgb(0.9, 0.15, 0.1),
        2 => Color::srgb(0.95, 0.85, 0.15),
        3 => Color::srgb(0.2, 0.75, 0.2),
        _ => Color::srgb(0.6, 0.6, 0.6),
    }
}

/// Site centre in cell coordinates (floats are fine outside the simulation).
pub fn site_cell_pos(site: &ReincarnationSite) -> Vec2 {
    Vec2::new(site.x as f32, site.z as f32) / WORLD_UNITS_PER_CELL as f32
}

/// Turn (game frame) bringing the stone's front (-Z in its own game frame: the glyph side, which it also
/// leans towards) to the centre, for a stone placed at angle `a` on the ring (offset `(cos a, sin a)` in x/z).
pub fn face_centre_yaw(a: f32) -> GameYaw {
    GameYaw(std::f32::consts::FRAC_PI_2 - a)
}

/// A ring of stones facing the centre (where the shaman stands), each with its own ground position.
pub fn site_parts(site: &ReincarnationSite) -> Vec<SitePart> {
    let centre = site_cell_pos(site);
    (0..STONES)
        .map(|i| {
            let a = i as f32 * std::f32::consts::TAU / STONES as f32;
            let at = centre + Vec2::new(a.cos(), a.sin()) * RING_RADIUS;
            SitePart { ground: Grounded { at, half: STONE_HALF }, yaw: face_centre_yaw(a) }
        })
        .collect()
}

/// Original stone and its theme atlas, None without original files.
fn original_stone<'a>(levels: &LevelList, objects: &'a OriginalObjects, theme: u8) -> Option<(&'a Object, Image)> {
    if !levels.original {
        return None;
    }
    let stone = objects.0.as_ref()?.get(REINCARNATION_STONE)?;
    let load = || -> Result<_, pop3_format::LevelError> {
        Ok((Atlas::load(&levels.data_dir, theme)?, Theme::load(&levels.data_dir, theme)?))
    };
    let (atlas, palette) = load().map_err(|e| warn!("object atlas for theme {theme}: {e}")).ok()?;
    Some((stone, atlas_image(&atlas, &palette.palette)))
}

#[allow(clippy::too_many_arguments)]
fn respawn_markers(
    mut commands: Commands,
    map: Res<CurrentMap>,
    levels: Res<LevelList>,
    objects: Res<OriginalObjects>,
    existing: Query<Entity, With<SiteMarker>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    if !map.is_changed() {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn();
    }
    let original = original_stone(&levels, &objects, map.0.theme.unwrap_or(DEFAULT_THEME)).map(|(stone, image)| {
        let mat = StandardMaterial {
            base_color_texture: Some(images.add(image)),
            perceptual_roughness: 0.95,
            double_sided: true,
            cull_mode: None,
            ..default()
        };
        (stone, mats.add(mat))
    });
    let block_mesh = meshes.add(Cuboid::from_size(STONE));
    for site in &map.0.sites {
        let (mesh, mat, lift) = match &original {
            Some((obj, mat)) => (meshes.add(to_mesh(object_mesh(obj, site.owner))), mat.clone(), 0.0),
            None => {
                let tint = Color::srgb(0.55, 0.52, 0.48).mix(&tribe_color(site.owner), 0.35);
                (block_mesh.clone(), mats.add(StandardMaterial { base_color: tint, perceptual_roughness: 0.9, ..default() }), STONE.y / 2.0)
            }
        };
        for part in site_parts(site) {
            commands
                .spawn((
                    SiteMarker,
                    part.ground,
                    Transform::from_rotation(part.yaw.into()),
                    Visibility::Hidden,
                ))
                .with_child((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_xyz(0.0, lift, 0.0)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h_finds_the_players_site_if_any() {
        let mut map = GameMap::sandbox_walk();
        assert_eq!(player_site_cell(&map), Some(Vec2::new(64.5, 64.5)));
        map.sites.clear();
        assert_eq!(player_site_cell(&map), None, "no site: the camera stays");
    }

    #[test]
    fn site_centre_is_cell_centre() {
        let s = ReincarnationSite::at_cell(0, (8, 107));
        assert_eq!(site_cell_pos(&s), Vec2::new(8.5, 107.5));
    }

    #[test]
    fn stones_face_the_centre() {
        let s = ReincarnationSite::at_cell(0, (10, 20));
        let centre = site_cell_pos(&s);
        for p in &site_parts(&s) {
            let front = Quat::from_rotation_y(p.yaw.0) * Vec3::NEG_Z;
            let to_centre = (centre - p.ground.at).normalize();
            assert!(front.xz().distance(to_centre) < 1e-5, "{front:?} vs {to_centre:?}");
        }
    }

    #[test]
    fn stones_ring_the_centre_each_on_its_own_ground() {
        let s = ReincarnationSite::at_cell(1, (10, 20));
        let parts = site_parts(&s);
        assert_eq!(parts.len(), STONES);
        for p in &parts {
            let d = p.ground.at.distance(Vec2::new(10.5, 20.5));
            assert!((d - RING_RADIUS).abs() < 1e-5);
        }
    }
}
