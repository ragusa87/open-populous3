//! Reincarnation sites: a stone ring with a tribe-coloured totem, placed on the curved
//! terrain around the camera focus (same `drop_at` bend as the ground mesh).

use crate::camera::{CameraRig, CurveParamsRes};
use crate::terrain_mesh::{drop_at, CurveParams};
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::site::ReincarnationSite;
use game_core::terrain::Heightmap;
use pop3_format::WORLD_UNITS_PER_CELL;

const STONES: usize = 8;
const RING_RADIUS: f32 = 1.3;

#[derive(Component)]
struct SiteMarker(ReincarnationSite);

pub struct SitesPlugin;

impl Plugin for SitesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (respawn_markers, place_markers).chain());
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

/// Render-space position of a cell point for a camera-centred, curved terrain, taking the
/// shortest way around the torus. None when outside the drawn disc.
pub fn render_pos(map: &Heightmap, at: Vec2, focus: Vec2, params: &CurveParams) -> Option<Vec3> {
    let size = map.size() as f32;
    let wrap = |d: f32| (d + size / 2.0).rem_euclid(size) - size / 2.0;
    let (dx, dz) = (wrap(at.x - focus.x), wrap(at.y - focus.y));
    let r = params.radius as f32 - 1.0;
    if dx * dx + dz * dz > r * r {
        return None;
    }
    let y = map.sample(at.x, at.y) * params.height_scale - drop_at(params, dx, dz);
    Some(Vec3::new(dx, y, dz))
}

fn respawn_markers(
    mut commands: Commands,
    map: Res<CurrentMap>,
    existing: Query<Entity, With<SiteMarker>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    if !map.is_changed() {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn();
    }
    let stone_mesh = meshes.add(Cuboid::new(0.22, 0.6, 0.22));
    let totem_mesh = meshes.add(Cylinder::new(0.16, 1.4));
    let stone = mats.add(StandardMaterial { base_color: Color::srgb(0.55, 0.52, 0.48), perceptual_roughness: 0.9, ..default() });
    for site in &map.0.sites {
        let totem = mats.add(StandardMaterial { base_color: tribe_color(site.owner), ..default() });
        commands
            .spawn((SiteMarker(*site), Transform::default(), Visibility::Hidden))
            .with_children(|p| {
                p.spawn((Mesh3d(totem_mesh.clone()), MeshMaterial3d(totem), Transform::from_xyz(0.0, 0.7, 0.0)));
                for i in 0..STONES {
                    let a = i as f32 * std::f32::consts::TAU / STONES as f32;
                    p.spawn((
                        Mesh3d(stone_mesh.clone()),
                        MeshMaterial3d(stone.clone()),
                        Transform::from_xyz(a.cos() * RING_RADIUS, 0.3, a.sin() * RING_RADIUS)
                            .with_rotation(Quat::from_rotation_y(-a)),
                    ));
                }
            });
    }
}

fn place_markers(
    rig: Res<CameraRig>,
    map: Res<CurrentMap>,
    params: Res<CurveParamsRes>,
    mut q: Query<(&SiteMarker, &mut Transform, &mut Visibility)>,
) {
    for (marker, mut t, mut vis) in &mut q {
        match render_pos(&map.0.terrain, site_cell_pos(&marker.0), rig.focus, &params.0) {
            Some(p) => {
                t.translation = p;
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat() -> (Heightmap, CurveParams) {
        (Heightmap::new(128), CurveParams { radius: 10, curvature: 0.0, ..Default::default() })
    }

    #[test]
    fn site_centre_is_cell_centre() {
        let s = ReincarnationSite::at_cell(0, (8, 107));
        assert_eq!(site_cell_pos(&s), Vec2::new(8.5, 107.5));
    }

    #[test]
    fn render_pos_wraps_around_the_torus() {
        let (map, p) = flat();
        let pos = render_pos(&map, Vec2::new(1.0, 1.0), Vec2::new(126.0, 127.0), &p).unwrap();
        assert_eq!((pos.x, pos.z), (3.0, 2.0));
    }

    #[test]
    fn render_pos_hidden_outside_disc_and_follows_height() {
        let (mut map, p) = flat();
        assert!(render_pos(&map, Vec2::new(40.0, 40.0), Vec2::ZERO, &p).is_none());
        map.set(5, 5, 768);
        let pos = render_pos(&map, Vec2::new(5.0, 5.0), Vec2::new(3.0, 3.0), &p).unwrap();
        assert_eq!(pos.y, 2.0);
    }
}
