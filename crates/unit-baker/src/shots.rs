//! Which clip moment each pose's frames show.

/// A pose: which clip, how many frames, their timing, and how deep it is sunk.
pub struct PoseShot {
    pub pose: &'static str,
    pub clip: &'static str,
    pub frames: usize,
    pub timing: Timing,
    /// Fraction of the head height under the water line (drowning).
    pub sink: f32,
}

/// `Loop` spreads the frames over the clip, `Once` from start to end, `End` is the last moment only
/// (a held pose), `Span` loops over that fraction of the clip; a `fall` repeats its last frame (held
/// while dead).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Timing {
    Loop,
    Once,
    End,
    Span(f32, f32),
}

pub const SHOTS: &[PoseShot] = &[
    PoseShot { pose: "idle", clip: "Idle", frames: 8, timing: Timing::Loop, sink: 0.0 },
    PoseShot { pose: "walk", clip: "Walk", frames: 12, timing: Timing::Loop, sink: 0.0 },
    PoseShot { pose: "pray", clip: "SitDown", frames: 1, timing: Timing::End, sink: 0.0 },
    PoseShot { pose: "cast", clip: "Jump", frames: 12, timing: Timing::Once, sink: 0.0 },
    PoseShot { pose: "fall", clip: "Death", frames: 8, timing: Timing::Once, sink: 0.0 },
    PoseShot { pose: "drown", clip: "RecieveHit", frames: 4, timing: Timing::Loop, sink: 0.45 },
    PoseShot { pose: "stranded", clip: "Victory", frames: 4, timing: Timing::Span(0.45, 0.8), sink: 0.0 },
];

pub fn shot(pose: &str) -> &'static PoseShot {
    SHOTS.iter().find(|s| s.pose == pose).unwrap_or_else(|| panic!("no shot for pose {pose}"))
}

/// Time of frame `k` of `n` in a clip lasting `duration`.
pub fn frame_time(timing: Timing, k: usize, n: usize, duration: f32, pose: &str) -> f32 {
    match timing {
        Timing::Loop => duration * k as f32 / n as f32,
        Timing::End => duration,
        Timing::Span(from, to) => duration * (from + (to - from) * k as f32 / n as f32),
        Timing::Once if pose == "fall" => duration * (k as f32 / (n - 2) as f32).min(1.0),
        Timing::Once => duration * k as f32 / (n - 1).max(1) as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loops_never_reach_the_end() {
        let times: Vec<f32> = (0..4).map(|k| frame_time(Timing::Loop, k, 4, 2.0, "drown")).collect();
        assert_eq!(times, [0.0, 0.5, 1.0, 1.5]);
    }

    #[test]
    fn once_ends_on_the_last_moment() {
        assert_eq!(frame_time(Timing::Once, 11, 12, 1.1, "cast"), 1.1);
        assert_eq!(frame_time(Timing::Once, 0, 1, 1.0, "cast"), 0.0);
        assert_eq!(frame_time(Timing::End, 0, 1, 3.0, "pray"), 3.0);
    }

    #[test]
    fn the_fall_holds_its_last_two_frames() {
        assert_eq!(frame_time(Timing::Once, 6, 8, 1.2, "fall"), 1.2);
        assert_eq!(frame_time(Timing::Once, 7, 8, 1.2, "fall"), 1.2);
        assert!(frame_time(Timing::Once, 5, 8, 1.2, "fall") < 1.2);
    }

    #[test]
    fn span_loops_inside_its_window() {
        let t: Vec<f32> = (0..4).map(|k| frame_time(Timing::Span(0.5, 1.0), k, 4, 2.0, "stranded")).collect();
        assert_eq!(t, [1.0, 1.25, 1.5, 1.75]);
    }
}
