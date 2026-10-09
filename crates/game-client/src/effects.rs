//! Puffs of smoke or dust: a few blobs, each born at its own time, moving out from an origin while
//! they rise, grow and fade. A pattern (`Puffs`) tells where puff `k` is `secs` after it started
//! (`Puffs::pose`); `move_puffs` poses the 3D ones (smoke balls: a busy hut's chimney, a sinking
//! totem). The landing dust (`units::dust`) poses its camera-facing motes with the same patterns.

use bevy::prelude::*;
use std::f32::consts::TAU;

/// When each puff is out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Emit {
    /// All at once, for one life.
    Burst,
    /// Forever, the puffs spread evenly over a life.
    Loop,
    /// One every `gap` seconds, each starting again when it ends, until `until` seconds.
    Run { gap: f32, until: f32 },
}

/// How a puff's age (0 to 1 of its life) shapes a motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    Linear,
    /// Fast at first, then slowing down.
    EaseOut,
    /// Slow at first, then faster.
    EaseIn,
}

impl Curve {
    pub fn at(self, p: f32) -> f32 {
        match self {
            Curve::Linear => p,
            Curve::EaseOut => 1.0 - (1.0 - p) * (1.0 - p),
            Curve::EaseIn => p * p,
        }
    }
}

/// How a set of puffs moves (cells, seconds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Puffs {
    pub count: usize,
    pub life: f32,
    pub emit: Emit,
    /// Distance from the origin, flat, at birth and at the end, along `spread_curve`.
    pub spread: (f32, f32),
    pub spread_curve: Curve,
    /// Height above the origin, size and opacity at birth and at the end, along `curve`.
    pub rise: (f32, f32),
    pub size: (f32, f32),
    pub opacity: (f32, f32),
    pub curve: Curve,
    /// The last part of its life (0 to 1) over which a puff shrinks away to nothing.
    pub shrink: f32,
    /// Direction of puff `k` (radians): `angle.0 + k * angle.1`.
    pub angle: (f32, f32),
}

/// Where a puff is: offset from the origin (cells), size (cells) and opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PuffPose {
    pub offset: Vec3,
    pub size: f32,
    pub opacity: f32,
}

/// A busy hut's smoke: grey puffs out of the chimney forever, rising, drifting aside and growing,
/// then shrinking away.
pub const CHIMNEY: Puffs = Puffs {
    count: 5,
    life: 2.5,
    emit: Emit::Loop,
    spread: (0.0, 0.377),
    spread_curve: Curve::EaseIn,
    rise: (0.0, 1.2),
    size: (0.088, 0.308),
    opacity: (0.55, 0.55),
    curve: Curve::Linear,
    shrink: 0.25,
    angle: (0.38, 0.0),
};

/// The dust where a unit touches down: motes spreading evenly around the feet at once, fast then
/// slowing, rising a little, growing and fading.
pub const DUST: Puffs = Puffs {
    count: 10,
    life: 0.8,
    emit: Emit::Burst,
    spread: (0.05, 0.4),
    spread_curve: Curve::EaseOut,
    rise: (0.02, 0.12),
    size: (0.12, 0.3),
    opacity: (0.9, 0.0),
    curve: Curve::EaseOut,
    shrink: 0.0,
    angle: (0.3, TAU / 10.0),
};

/// A sinking totem's smoke: puffs round its base, one every quarter second while it sinks.
pub const SINKING: Puffs = Puffs {
    count: 14,
    life: 1.4,
    emit: Emit::Run { gap: 0.25, until: 4.7 },
    spread: (0.45, 0.57),
    spread_curve: Curve::Linear,
    rise: (0.08, 0.88),
    size: (0.12, 0.36),
    opacity: (0.55, 0.0),
    curve: Curve::Linear,
    shrink: 0.0,
    angle: (0.0, 2.4),
};

impl Puffs {
    /// How far through its life (0 to 1) puff `k` is `secs` after the puffs started; None when it is
    /// not out.
    pub fn age(&self, k: usize, secs: f32) -> Option<f32> {
        match self.emit {
            Emit::Burst => (0.0..=self.life).contains(&secs).then_some(secs / self.life),
            Emit::Loop => Some((secs / self.life + k as f32 / self.count as f32).rem_euclid(1.0)),
            Emit::Run { gap, until } => {
                let since = secs - k as f32 * gap;
                (since >= 0.0 && secs <= until).then(|| since.rem_euclid(self.life) / self.life)
            }
        }
    }

    /// Puff `k`, `secs` after the puffs started; None when it is not out.
    pub fn pose(&self, k: usize, secs: f32) -> Option<PuffPose> {
        let p = self.age(k, secs)?;
        let lerp = |(a, b): (f32, f32), t: f32| a + (b - a) * t;
        let (out, c) = (self.spread_curve.at(p), self.curve.at(p));
        let angle = self.angle.0 + k as f32 * self.angle.1;
        let reach = lerp(self.spread, out);
        let shrink = if self.shrink > 0.0 && p > 1.0 - self.shrink { (1.0 - p) / self.shrink } else { 1.0 };
        Some(PuffPose { offset: Vec3::new(angle.cos() * reach, lerp(self.rise, c), angle.sin() * reach), size: lerp(self.size, c) * shrink, opacity: lerp(self.opacity, p) })
    }
}

/// A 3D puff (a ball of smoke): `pattern`'s puff `k` about `origin` (its parent's frame), `secs`
/// into its run. `driven`: its owner sets `secs` (and hides it when off); otherwise it runs on the
/// real-time clock.
#[derive(Component)]
pub struct Puff {
    pub pattern: Puffs,
    pub k: usize,
    pub origin: Vec3,
    pub secs: f32,
    pub driven: bool,
}

impl Puff {
    pub fn new(pattern: Puffs, k: usize, origin: Vec3) -> Self {
        Puff { pattern, k, origin, secs: 0.0, driven: false }
    }
}

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, move_puffs.before(TransformSystems::Propagate));
    }
}

/// Moves, grows and fades every 3D puff; one that is not out is hidden.
fn move_puffs(time: Res<Time>, mut puffs: Query<(&mut Puff, &mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>)>, mut mats: ResMut<Assets<StandardMaterial>>) {
    for (mut puff, mut t, mut vis, mat) in &mut puffs {
        if !puff.driven {
            puff.secs += time.delta_secs();
        }
        let Some(pose) = puff.pattern.pose(puff.k, puff.secs) else {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        };
        (t.translation, t.scale) = (puff.origin + pose.offset, Vec3::splat(pose.size));
        if *vis == Visibility::Hidden && !puff.driven {
            vis.set_if_neq(Visibility::Inherited);
        }
        if let Some(mut m) = mats.get_mut(&mat.0).filter(|m| (m.base_color.alpha() - pose.opacity).abs() > 1e-3) {
            m.base_color.set_alpha(pose.opacity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chimney_smoke_rises_drifts_grows_then_shrinks_away() {
        let at = |p: f32| CHIMNEY.pose(0, p * CHIMNEY.life).unwrap();
        let (start, mid, end) = (at(0.0), at(0.6), at(0.999));
        assert!(start.offset.y == 0.0 && mid.offset.y > start.offset.y && mid.offset.xz().length() > 0.0);
        assert!(mid.size > start.size && end.size < 0.01, "grows, then shrinks away before starting again");
        assert!(CHIMNEY.pose(0, 1000.0).is_some(), "forever");
        let phases: Vec<f32> = (0..CHIMNEY.count).map(|k| CHIMNEY.age(k, 0.0).unwrap()).collect();
        assert_eq!(phases, vec![0.0, 0.2, 0.4, 0.6, 0.8], "spread over a life");
    }

    #[test]
    fn dust_spreads_fast_then_slows_rises_grows_and_fades() {
        let at = |t: f32| DUST.pose(0, t * DUST.life).unwrap();
        let (start, mid, end) = (at(0.0), at(0.5), at(1.0));
        let flat = |p: PuffPose| p.offset.xz().length();
        assert!(flat(start) < flat(mid) && flat(mid) < flat(end) && (flat(end) - DUST.spread.1).abs() < 1e-5);
        assert!(flat(mid) - flat(start) > flat(end) - flat(mid), "fast first, then slowing");
        assert!(start.size < mid.size && mid.size < end.size && start.opacity > mid.opacity && end.opacity == 0.0);
        let around: Vec3 = (0..DUST.count).map(|k| DUST.pose(k, DUST.life).unwrap().offset * Vec3::new(1.0, 0.0, 1.0)).sum();
        assert!(around.length() < 1e-4, "evenly spread around the feet");
        assert!(DUST.pose(0, DUST.life + 0.01).is_none(), "once");
    }

    #[test]
    fn sinking_smoke_comes_out_one_after_another_rises_and_fades() {
        assert!(SINKING.pose(0, -0.1).is_none(), "not before the sinking");
        assert!(SINKING.pose(3, 0.5).is_none(), "puff 3 comes out at 0.75 s");
        let (low, high) = (SINKING.pose(0, 0.1).unwrap(), SINKING.pose(0, 1.2).unwrap());
        assert!(high.offset.y > low.offset.y && high.opacity < low.opacity);
        assert!(SINKING.pose(0, 10.0).is_none(), "over once sunk");
    }
}
