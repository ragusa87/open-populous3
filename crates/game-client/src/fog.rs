//! Fog of war as the local player sees it (pop3-rev-analysis.md "Fog of war" "Drawing"): unexplored ground drawn
//! black (`ground.wgsl`'s fog texture, kept in sync here), nothing on it drawn (`grounded::place_grounded`) nor
//! hovered (`hover.rs`), and the sky dimmed while little explored land is in view (`SkyDim`).

use crate::camera::CameraRig;
use crate::ground_material::{fog_bytes, fog_image, GroundMaterial};
use crate::units::PLAYER;
use crate::world::{CurrentMap, TerrainMaterial};
use bevy::prelude::*;
use game_core::fog::Explored;
use game_core::map::GameMap;

/// Explored cells in view at and over which the sky is at full brightness (the original's 50).
pub const FULL_SKY_CELLS: usize = 50;
/// How far from the camera focus (cells) explored land counts as in view: our guess at the patch the original walks
/// when drawing, about what our default camera shows.
pub const VIEW_CELLS: i32 = 12;

/// The sky's brightness in sixteenths (16: full), from the explored land in view.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkyDim(pub u8);

impl Default for SkyDim {
    fn default() -> Self {
        SkyDim(16)
    }
}

impl SkyDim {
    /// `colour` dimmed, its alpha kept.
    pub fn apply(self, colour: Color) -> Color {
        let k = self.0 as f32 / 16.0;
        let c = colour.to_srgba();
        Color::srgba(c.red * k, c.green * k, c.blue * k, c.alpha)
    }
}

/// The sky's sixteenths with `explored` cells in view: `explored * 16 / FULL_SKY_CELLS`, at most 16.
pub fn sky_sixteenths(explored: usize) -> u8 {
    (explored * 16 / FULL_SKY_CELLS).min(16) as u8
}

/// Explored cells within `VIEW_CELLS` of `focus` (cells).
pub fn explored_in_view(explored: &Explored, focus: Vec2) -> usize {
    let (fx, fz) = (focus.x.floor() as i32, focus.y.floor() as i32);
    let r = VIEW_CELLS;
    (-r..=r).flat_map(|dz| (-r..=r).map(move |dx| (dx, dz))).filter(|&(dx, dz)| dx * dx + dz * dz <= r * r && explored.is_explored((fx + dx, fz + dz))).count()
}

/// Whether the local player has not explored the cell under `at` (cells, any value: wraps).
pub fn hidden_at(map: &GameMap, at: Vec2) -> bool {
    map.fogged(PLAYER, (at.x.floor() as i32, at.y.floor() as i32))
}

/// Whether the local player has not explored the cell holding world point `at`.
pub fn hidden_world(map: &GameMap, at: (u16, u16)) -> bool {
    map.fogged(PLAYER, ((at.0 as u32 / pop3_format::WORLD_UNITS_PER_CELL) as i32, (at.1 as u32 / pop3_format::WORLD_UNITS_PER_CELL) as i32))
}

/// `FOG=1`: fog of war on any map, for checking it (`world::new_game`).
pub fn forced() -> bool {
    std::env::var("FOG").is_ok_and(|v| v == "1")
}

pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SkyDim>().add_systems(Update, (sync_ground_fog, dim_sky).after(crate::units::SimStep));
    }
}

/// Uploads the player's explored cells to the ground material when they changed.
fn sync_ground_fog(map: Res<CurrentMap>, material: Res<TerrainMaterial>, mut mats: ResMut<Assets<GroundMaterial>>, mut images: ResMut<Assets<Image>>, mut shown: Local<Vec<u8>>) {
    let size = map.0.terrain.size();
    let bytes = fog_bytes(map.0.fog.as_ref().and_then(|f| f.of(PLAYER)), size);
    if *shown == bytes {
        return;
    }
    let side = size as u32;
    let Some(mut m) = mats.get_mut(&material.0) else { return };
    let fits = images.get(&m.fog).is_some_and(|image| image.width() == side);
    if fits && let Some(mut image) = images.get_mut(&m.fog) {
        image.data = Some(bytes.clone());
    } else {
        m.fog = images.add(fog_image(side, bytes.clone()));
    }
    *shown = bytes;
}

fn dim_sky(map: Res<CurrentMap>, rig: Res<CameraRig>, mut dim: ResMut<SkyDim>) {
    let k = map.0.fog.as_ref().and_then(|f| f.of(PLAYER)).map_or(16, |e| sky_sixteenths(explored_in_view(e, rig.focus)));
    dim.set_if_neq(SkyDim(k));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sky_brightens_in_sixteenths_up_to_fifty_explored_cells() {
        assert_eq!(sky_sixteenths(0), 0);
        assert_eq!(sky_sixteenths(25), 8);
        assert_eq!(sky_sixteenths(49), 15);
        assert_eq!(sky_sixteenths(50), 16);
        assert_eq!(sky_sixteenths(500), 16);
        assert_eq!(SkyDim(8).apply(Color::srgba(1.0, 0.5, 0.0, 0.25)), Color::srgba(0.5, 0.25, 0.0, 0.25));
    }

    #[test]
    fn explored_land_in_view_counts_round_the_focus_across_the_seam() {
        let mut e = Explored::new(128);
        e.reveal((0, 0), 2);
        assert_eq!(explored_in_view(&e, Vec2::new(127.5, 0.5)), game_core::fog::disc(2).count());
        assert_eq!(explored_in_view(&e, Vec2::new(64.0, 64.0)), 0);
    }

    #[test]
    fn only_fog_hides_a_cell() {
        let mut map = GameMap::sandbox_walk();
        let far = Vec2::new(10.5, 10.5);
        assert!(!hidden_at(&map, far));
        map.enable_fog();
        assert!(hidden_at(&map, far) && hidden_world(&map, (10 * 512, 10 * 512)));
        let site = map.sites[0].cell();
        assert!(!hidden_at(&map, Vec2::new(site.0 as f32 + 0.5, site.1 as f32 + 0.5)));
    }
}
