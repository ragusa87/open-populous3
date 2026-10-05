//! Original-style edge scrolling: only when the cursor is pressed against the
//! window border, and the speed comes from how hard the mouse keeps pushing
//! outward (raw motion while the cursor is stuck at the edge), not from distance.

use bevy::math::Vec2;

/// Pixels from the border that count as "on the edge".
pub const EDGE_PX: f32 = 2.0;
/// Velocity gained per pixel of outward push (velocity is in -1..1 per axis).
pub const PUSH_GAIN: f32 = 0.03;
/// Decay rates (per second) when on the edge without pushing, and after leaving it.
pub const HOLD_DECAY: f32 = 2.5;
pub const RELEASE_DECAY: f32 = 14.0;

/// Outward push this frame as (forward, right) in pixels: positive forward = top edge.
/// Only the motion component going into a border the cursor touches counts.
pub fn outward_push(cursor: Vec2, window: Vec2, delta: Vec2) -> Vec2 {
    let right = if cursor.x >= window.x - EDGE_PX {
        delta.x.max(0.0)
    } else if cursor.x <= EDGE_PX {
        delta.x.min(0.0)
    } else {
        0.0
    };
    let forward = if cursor.y <= EDGE_PX {
        (-delta.y).max(0.0)
    } else if cursor.y >= window.y - EDGE_PX {
        (-delta.y).min(0.0)
    } else {
        0.0
    };
    Vec2::new(forward, right)
}

pub fn on_edge(cursor: Vec2, window: Vec2) -> bool {
    cursor.x <= EDGE_PX || cursor.y <= EDGE_PX || cursor.x >= window.x - EDGE_PX || cursor.y >= window.y - EDGE_PX
}

/// Scroll velocity driven by pushes. Returns (forward, right) in -1..1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EdgePush {
    pub velocity: Vec2,
}

impl EdgePush {
    pub fn update(&mut self, cursor: Option<Vec2>, window: Vec2, delta: Vec2, dt: f32) -> Vec2 {
        let touching = cursor.is_some_and(|c| on_edge(c, window));
        let push = cursor.map_or(Vec2::ZERO, |c| outward_push(c, window, delta));
        let decay = if touching { HOLD_DECAY } else { RELEASE_DECAY };
        let kept = self.velocity * (-decay * dt).exp();
        // Pushing against an axis replaces a weaker or opposite velocity on that axis.
        let pushed = (push * PUSH_GAIN + kept * push.signum().abs()).clamp(Vec2::splat(-1.0), Vec2::splat(1.0));
        self.velocity = Vec2::new(
            if push.x != 0.0 { pushed.x } else { kept.x },
            if push.y != 0.0 { pushed.y } else { kept.y },
        );
        if self.velocity.length_squared() < 1e-6 {
            self.velocity = Vec2::ZERO;
        }
        self.velocity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: Vec2 = Vec2::new(800.0, 600.0);

    #[test]
    fn no_push_away_from_the_border() {
        assert_eq!(outward_push(Vec2::new(400.0, 300.0), WIN, Vec2::new(50.0, 50.0)), Vec2::ZERO);
        assert_eq!(outward_push(Vec2::new(0.0, 300.0), WIN, Vec2::new(30.0, 0.0)), Vec2::ZERO, "moving back inside");
    }

    #[test]
    fn push_directions() {
        assert_eq!(outward_push(Vec2::new(0.0, 300.0), WIN, Vec2::new(-10.0, 0.0)), Vec2::new(0.0, -10.0));
        assert_eq!(outward_push(Vec2::new(400.0, 0.0), WIN, Vec2::new(0.0, -5.0)), Vec2::new(5.0, 0.0));
        assert_eq!(outward_push(Vec2::new(799.0, 599.0), WIN, Vec2::new(4.0, 6.0)), Vec2::new(-6.0, 4.0));
    }

    #[test]
    fn harder_push_scrolls_faster_and_caps() {
        let at_left = Some(Vec2::new(0.0, 300.0));
        let (mut soft, mut hard) = (EdgePush::default(), EdgePush::default());
        let s = soft.update(at_left, WIN, Vec2::new(-5.0, 0.0), 0.016);
        let h = hard.update(at_left, WIN, Vec2::new(-20.0, 0.0), 0.016);
        assert!(h.y < s.y && s.y < 0.0);
        for _ in 0..50 {
            hard.update(at_left, WIN, Vec2::new(-100.0, 0.0), 0.016);
        }
        assert_eq!(hard.velocity.y, -1.0);
    }

    #[test]
    fn slows_when_holding_and_stops_when_leaving() {
        let at_left = Some(Vec2::new(0.0, 300.0));
        let mut p = EdgePush::default();
        let start = p.update(at_left, WIN, Vec2::new(-30.0, 0.0), 0.016).y;
        let held = p.update(at_left, WIN, Vec2::ZERO, 0.1).y;
        assert!(held < 0.0 && held > start, "keeps scrolling, slower");
        for _ in 0..30 {
            p.update(Some(Vec2::new(400.0, 300.0)), WIN, Vec2::ZERO, 0.016);
        }
        assert_eq!(p.velocity, Vec2::ZERO);
    }
}
