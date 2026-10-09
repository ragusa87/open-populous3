//! The game's frame: left-handed like the original's Direct3D (x right, y up, z away from the viewer, see
//! docs/architecture.md "Handedness"). The files, `game-core` and the original models use it; Bevy is
//! right-handed (z towards the viewer), so a value in the game's frame reaches the renderer only through
//! these types, which mirror z. Without the mirror the whole world is drawn as its mirror image.

use bevy::prelude::*;

/// A position or offset in the game's frame (cells), never rendered directly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GamePos(pub Vec3);

impl GamePos {
    /// A ground offset `(dx, dz)` at height `y`.
    pub fn ground(dx: f32, y: f32, dz: f32) -> Self {
        GamePos(Vec3::new(dx, y, dz))
    }

    /// The game position drawn at render point `v`.
    pub fn from_render(v: Vec3) -> Self {
        GamePos(Vec3::new(v.x, v.y, -v.z))
    }

    /// Its ground part `(x, z)`.
    pub fn xz(self) -> Vec2 {
        Vec2::new(self.0.x, self.0.z)
    }
}

impl From<GamePos> for Vec3 {
    fn from(p: GamePos) -> Vec3 {
        Vec3::new(p.0.x, p.0.y, -p.0.z)
    }
}

/// A turn about the vertical axis in the game's frame (radians, the sense of its angles), never applied
/// directly: the mirror turns it the other way.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameYaw(pub f32);

impl GameYaw {
    /// The same turn as a render yaw about +y.
    pub fn render(self) -> f32 {
        -self.0
    }
}

impl From<GameYaw> for Quat {
    fn from(y: GameYaw) -> Quat {
        Quat::from_rotation_y(y.render())
    }
}

/// A triangle's corners in the order that keeps its front face once mirrored.
pub fn mirrored([a, b, c]: [u32; 3]) -> [u32; 3] {
    [a, c, b]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn z_is_mirrored_both_ways() {
        let p = GamePos(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(Vec3::from(p), Vec3::new(1.0, 2.0, -3.0));
        assert_eq!(GamePos::from_render(Vec3::from(p)), p);
        assert_eq!(GamePos::ground(4.0, 0.5, -6.0).xz(), Vec2::new(4.0, -6.0));
    }

    #[test]
    fn a_game_turn_moves_a_point_as_the_mirror_of_its_game_turn() {
        let quarter = GameYaw(std::f32::consts::FRAC_PI_2);
        let game_turned = Quat::from_rotation_y(quarter.0) * Vec3::Z;
        let drawn = Quat::from(quarter) * Vec3::from(GamePos(Vec3::Z));
        assert!(drawn.distance(Vec3::from(GamePos(game_turned))) < 1e-6);
        assert_eq!(mirrored([0, 1, 2]), [0, 2, 1]);
    }
}
