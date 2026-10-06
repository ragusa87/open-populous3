//! Reincarnation sites: a stone ring with a tribe-coloured totem. Every stone is its own
//! `Grounded` part, so the ring follows slopes and the planet curve instead of staying flat.
//! With the original files the stones are the game's capped pillars, textured from the level
//! theme's atlas; otherwise (or with `--no-original`) plain generated blocks.

use crate::grounded::Grounded;
use crate::original_models::{atlas_image, object_mesh, to_mesh, OriginalObjects, RS_PILLAR};
use crate::world::{CurrentMap, LevelList};
use bevy::prelude::*;
use game_core::site::ReincarnationSite;
use pop3_format::{Atlas, Theme, WORLD_UNITS_PER_CELL};

const STONES: usize = 8;
const RING_RADIUS: f32 = 1.3;
const STONE: Vec3 = Vec3::new(0.22, 0.6, 0.22);
const TOTEM: (f32, f32) = (0.16, 1.4);
/// Footprint half size of a ring stone, wide enough for the original pillar (~0.47 cell).
const STONE_HALF: f32 = 0.24;

#[derive(Component)]
struct SiteMarker;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PartKind {
    Totem,
    Stone,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SitePart {
    pub kind: PartKind,
    pub ground: Grounded,
    pub yaw: f32,
}

pub struct SitesPlugin;

impl Plugin for SitesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, respawn_markers);
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

/// Totem at the centre, then a ring of stones facing it, each with its own ground position.
pub fn site_parts(site: &ReincarnationSite) -> Vec<SitePart> {
    let centre = site_cell_pos(site);
    let totem = SitePart { kind: PartKind::Totem, ground: Grounded { at: centre, half: TOTEM.0 }, yaw: 0.0 };
    let stones = (0..STONES).map(|i| {
        let a = i as f32 * std::f32::consts::TAU / STONES as f32;
        let at = centre + Vec2::new(a.cos(), a.sin()) * RING_RADIUS;
        SitePart { kind: PartKind::Stone, ground: Grounded { at, half: STONE_HALF }, yaw: -a }
    });
    std::iter::once(totem).chain(stones).collect()
}

/// Original pillar mesh and its theme atlas, None without original files or theme.
fn original_pillar(levels: &LevelList, objects: &OriginalObjects, theme: Option<u8>) -> Option<(Mesh, Image)> {
    if !levels.original {
        return None;
    }
    let (theme, pillar) = (theme?, objects.0.as_ref()?.get(RS_PILLAR)?);
    let load = || -> Result<_, pop3_format::LevelError> {
        Ok((Atlas::load(&levels.data_dir, theme)?, Theme::load(&levels.data_dir, theme)?))
    };
    let (atlas, palette) = load().map_err(|e| warn!("object atlas for theme {theme}: {e}")).ok()?;
    Some((to_mesh(object_mesh(pillar)), atlas_image(&atlas, &palette.palette)))
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
    let (stone_mesh, stone, stone_lift) = match original_pillar(&levels, &objects, map.0.theme) {
        Some((mesh, image)) => {
            let mat = StandardMaterial {
                base_color_texture: Some(images.add(image)),
                perceptual_roughness: 0.95,
                double_sided: true,
                cull_mode: None,
                ..default()
            };
            (meshes.add(mesh), mats.add(mat), 0.0)
        }
        None => {
            let mat = StandardMaterial { base_color: Color::srgb(0.55, 0.52, 0.48), perceptual_roughness: 0.9, ..default() };
            (meshes.add(Cuboid::from_size(STONE)), mats.add(mat), STONE.y / 2.0)
        }
    };
    let totem_mesh = meshes.add(Cylinder::new(TOTEM.0, TOTEM.1));
    for site in &map.0.sites {
        let totem = mats.add(StandardMaterial { base_color: tribe_color(site.owner), ..default() });
        for part in site_parts(site) {
            let (mesh, mat, lift) = match part.kind {
                PartKind::Totem => (totem_mesh.clone(), totem.clone(), TOTEM.1 / 2.0),
                PartKind::Stone => (stone_mesh.clone(), stone.clone(), stone_lift),
            };
            commands
                .spawn((
                    SiteMarker,
                    part.ground,
                    Transform::from_rotation(Quat::from_rotation_y(part.yaw)),
                    Visibility::Hidden,
                ))
                .with_child((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_xyz(0.0, lift, 0.0)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_centre_is_cell_centre() {
        let s = ReincarnationSite::at_cell(0, (8, 107));
        assert_eq!(site_cell_pos(&s), Vec2::new(8.5, 107.5));
    }

    #[test]
    fn stones_ring_the_totem_each_on_its_own_ground() {
        let s = ReincarnationSite::at_cell(1, (10, 20));
        let parts = site_parts(&s);
        assert_eq!(parts.len(), 1 + STONES);
        assert_eq!(parts[0].kind, PartKind::Totem);
        assert_eq!(parts[0].ground.at, Vec2::new(10.5, 20.5));
        for p in &parts[1..] {
            let d = p.ground.at.distance(parts[0].ground.at);
            assert!((d - RING_RADIUS).abs() < 1e-5);
        }
    }
}
