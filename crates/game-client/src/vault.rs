//! Pyramids of knowledge on screen (docs/specs/worship.md): the door and the top follow the vault's
//! phase (`Vault::door_and_top`). With the original files, the three frames of the pyramid (one mesh,
//! moved points) are blended point by point: the door from closed (192) to open (191), the top from
//! open (192) to folded (193). The generated pyramid slides a door slab up its doorway.

use crate::flame;
use crate::original_models::{object_mesh, solid_part, to_mesh, OriginalObjects};
use crate::world::CurrentMap;
use bevy::prelude::*;
use crate::game_frame::GamePos;
use game_core::vault::FULL;
use pop3_format::catalog::{KNOWLEDGE_PYRAMID, KNOWLEDGE_PYRAMID_CLOSED, KNOWLEDGE_PYRAMID_FOLDED};
use pop3_format::Object;

/// Points moving less than this (world units) between frames are export noise, not the door or top.
const MOVE_MIN: i16 = 10;
/// The generated door slab (cells, building frame in the game's frame): its size, where it stands closed,
/// how far it slides up to open.
pub const SLAB: Vec3 = Vec3::new(0.4, 0.5, 0.06);
pub const SLAB_AT: Vec3 = Vec3::new(0.0, 0.37, -0.95);
const SLAB_RISE: f32 = 0.5;

/// The original pyramid's frames, when all three share one mesh.
pub struct Frames<'a> {
    closed: &'a Object,
    open: &'a Object,
    folded: &'a Object,
}

impl<'a> Frames<'a> {
    pub fn from_bank(objects: &'a OriginalObjects) -> Option<Self> {
        let bank = objects.0.as_ref()?;
        Frames::new(bank.get(KNOWLEDGE_PYRAMID_CLOSED)?, bank.get(KNOWLEDGE_PYRAMID)?, bank.get(KNOWLEDGE_PYRAMID_FOLDED)?)
    }

    pub fn new(closed: &'a Object, open: &'a Object, folded: &'a Object) -> Option<Self> {
        let same = |o: &Object| o.points.len() == closed.points.len() && o.faces.len() == closed.faces.len();
        (same(open) && same(folded)).then_some(Frames { closed, open, folded })
    }

    /// The pyramid with its door `door` and its top `top` open (thousandths): each point moved from
    /// the closed frame towards the open one (door) or the folded one (top, folding as it closes).
    pub fn posed(&self, door: u16, top: u16) -> Object {
        let far = |a: [i16; 3], b: [i16; 3]| (0..3).any(|k| (a[k] - b[k]).abs() > MOVE_MIN);
        let lerp = |a: [i16; 3], b: [i16; 3], t: u16| std::array::from_fn(|k| a[k] + ((b[k] - a[k]) as i32 * t as i32 / FULL as i32) as i16);
        let points = (0..self.closed.points.len())
            .map(|i| {
                let (c, o, f) = (self.closed.points[i], self.open.points[i], self.folded.points[i]);
                if far(c, o) {
                    lerp(c, o, door)
                } else if far(c, f) {
                    lerp(c, f, FULL - top)
                } else {
                    c
                }
            })
            .collect();
        Object { points, faces: self.closed.faces.clone() }
    }
}

/// A pyramid's model (and its flames) drawn at the door and top position `shown`, for the building
/// at this index of `GameMap::buildings`.
#[derive(Component)]
pub struct VaultModel {
    pub index: usize,
    pub tribe: u8,
    pub flame: Option<Entity>,
    pub shown: Option<(u16, u16)>,
}

/// The generated pyramid's door slab, for the building at this index.
#[derive(Component)]
pub struct VaultSlab(pub usize);

/// Where the generated door slab stands with the door `door` open (thousandths).
pub fn slab_at(door: u16) -> Vec3 {
    Vec3::from(GamePos(SLAB_AT)) + Vec3::Y * SLAB_RISE * door as f32 / FULL as f32
}

pub struct VaultPlugin;

impl Plugin for VaultPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (pose_models, slide_slabs).after(crate::buildings::RespawnBuildings));
    }
}

fn pose_models(map: Res<CurrentMap>, objects: Res<OriginalObjects>, mut models: Query<(&mut VaultModel, &Mesh3d)>, flames: Query<&Mesh3d, Without<VaultModel>>, mut meshes: ResMut<Assets<Mesh>>) {
    let Some(frames) = Frames::from_bank(&objects) else { return };
    for (mut model, mesh) in &mut models {
        let Some(pose) = map.0.buildings.get(model.index).and_then(|b| b.vault).map(|v| v.door_and_top()) else { continue };
        if model.shown == Some(pose) {
            continue;
        }
        model.shown = Some(pose);
        let posed = frames.posed(pose.0, pose.1);
        meshes.insert(&mesh.0, to_mesh(object_mesh(&solid_part(&posed), model.tribe))).ok();
        if let Some(flame) = model.flame.and_then(|e| flames.get(e).ok()) {
            meshes.insert(&flame.0, to_mesh(flame::flame_mesh(&posed))).ok();
        }
    }
}

fn slide_slabs(map: Res<CurrentMap>, mut slabs: Query<(&VaultSlab, &mut Transform)>) {
    for (slab, mut t) in &mut slabs {
        if let Some((door, _)) = map.0.buildings.get(slab.0).and_then(|b| b.vault).map(|v| v.door_and_top()) {
            t.translation = slab_at(door);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(door_y: i16, top_x: i16, noise: i16) -> Object {
        Object { points: vec![[0, noise, 0], [0, door_y, -500], [top_x, 1700, 0]], faces: Vec::new() }
    }

    #[test]
    fn the_door_and_the_top_move_alone_from_the_closed_frame() {
        let (closed, open, folded) = (frame(0, 150, 0), frame(480, 150, 2), frame(0, 0, -3));
        let frames = Frames::new(&closed, &open, &folded).unwrap();
        assert_eq!(frames.posed(0, FULL).points, closed.points, "fresh: the closed frame");
        assert_eq!(frames.posed(FULL, FULL).points[1], [0, 480, -500], "door open");
        assert_eq!(frames.posed(500, FULL).points[1], [0, 240, -500], "half way");
        assert_eq!(frames.posed(0, 0).points[2], [0, 1700, 0], "top folded");
        assert_eq!(frames.posed(FULL, 0).points[0], [0, 0, 0], "noise is not a move");
    }

    #[test]
    fn frames_must_share_their_mesh() {
        let (closed, open) = (frame(0, 150, 0), frame(480, 150, 0));
        let other = Object { points: vec![[0, 0, 0]], faces: Vec::new() };
        assert!(Frames::new(&closed, &open, &other).is_none());
    }

    #[test]
    fn the_slab_rises_as_the_door_opens() {
        assert_eq!(slab_at(0), Vec3::from(GamePos(SLAB_AT)), "drawn mirrored like the kit");
        assert!((slab_at(FULL).y - SLAB_AT.y - SLAB_RISE).abs() < 1e-6);
    }
}
