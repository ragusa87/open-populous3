//! A progress bar widget, the original's training bar (pop3-rev-analysis.md "The panel's training bar"): a dark
//! track, its overall fill, and under it a finer layer that sweeps the whole width each time the fill moves a pixel
//! (`fine_fill`). Used by the tooltips (a training unit, a vehicle hut, prayers) and the spell tiles (a spell's
//! charge). Whoever owns a bar writes its `ProgressBar`; `size_bars` and `blink_bars` draw it.

use bevy::prelude::*;

/// The overall fill's colour (mana given, work done), and the finer layer's.
pub const FILL: Color = Color::srgb(1.0, 0.85, 0.2);
pub const FINE: Color = Color::srgb(0.25, 0.5, 1.0);
/// A blocked bar blinks this many times a second.
const BLINK_HZ: f32 = 2.0;

/// What a bar shows: `fill` from 0 to 1; `fine` (value, max), the finer layer; `blocked`, blinking.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ProgressBar {
    pub fill: f32,
    pub fine: Option<(u32, u32)>,
    pub blocked: bool,
    /// Filling to the right (else upwards).
    pub horizontal: bool,
    pub color: Color,
}

impl ProgressBar {
    /// An empty horizontal bar in `color`, with or without the finer layer.
    pub fn horizontal(color: Color, fine: bool) -> Self {
        ProgressBar { fill: 0.0, fine: fine.then_some((0, 0)), blocked: false, horizontal: true, color }
    }

    pub fn vertical(color: Color) -> Self {
        ProgressBar { fill: 0.0, fine: None, blocked: false, horizontal: false, color }
    }
}

/// A part of a bar: its fill, or its finer layer.
#[derive(Component)]
struct Part {
    fine: bool,
}

/// The finer layer of a bar `width` pixels wide filled `value / max`, 0 to 1: what the fill has beyond its last
/// whole pixel, so it sweeps the whole width each time the fill moves by a pixel.
pub fn fine_fill(value: u32, max: u32, width: u32) -> f32 {
    if max == 0 {
        return 0.0;
    }
    ((value as u64 * width as u64) % max as u64) as f32 / max as f32
}

/// Spawns `bar` as a track sized by `track` (its thickness, length...), with its parts.
pub fn spawn<'a>(parent: &'a mut ChildSpawnerCommands, bar: ProgressBar, track: Node) -> EntityCommands<'a> {
    let fill = if bar.horizontal {
        Node { position_type: PositionType::Absolute, left: px(0), top: px(0), bottom: px(0), width: px(0), ..default() }
    } else {
        Node { position_type: PositionType::Absolute, left: px(0), right: px(0), bottom: px(0), height: px(0), ..default() }
    };
    let mut entity = parent.spawn((track, bar, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)), Pickable::IGNORE));
    entity.with_children(|t| {
        if bar.fine.is_some() {
            t.spawn((fill.clone(), BackgroundColor(FINE), Part { fine: true }, Pickable::IGNORE));
        }
        t.spawn((fill, BackgroundColor(bar.color), Part { fine: false }, Pickable::IGNORE));
    });
    entity
}

/// Each bar's parts sized from its `ProgressBar`: the fill its share of the track, the finer layer its share of the
/// track's laid-out length (whole pixels, as the original's).
fn size_bars(bars: Query<(&ProgressBar, &ComputedNode)>, mut parts: Query<(&Part, &ChildOf, &mut Node)>) {
    for (part, parent, mut node) in &mut parts {
        let Ok((bar, track)) = bars.get(parent.parent()) else { continue };
        let share = match (part.fine, bar.fine) {
            (true, Some((value, max))) => {
                let length = if bar.horizontal { track.size().x } else { track.size().y };
                fine_fill(value, max, (length * track.inverse_scale_factor()).round() as u32) * 100.0
            }
            _ => bar.fill.clamp(0.0, 1.0) * 100.0,
        };
        let size = if bar.horizontal { &mut node.width } else { &mut node.height };
        if *size != percent(share) {
            *size = percent(share);
        }
    }
}

/// Blocked bars blink: their parts fade out and back in.
fn blink_bars(time: Res<Time>, bars: Query<&ProgressBar>, mut parts: Query<(&Part, &ChildOf, &mut BackgroundColor)>) {
    let on = (time.elapsed_secs() * BLINK_HZ).fract() < 0.5;
    for (part, parent, mut color) in &mut parts {
        let Ok(bar) = bars.get(parent.parent()) else { continue };
        let base = if part.fine { FINE } else { bar.color };
        let shown = if bar.blocked && !on { base.with_alpha(0.2) } else { base };
        color.set_if_neq(BackgroundColor(shown));
    }
}

/// Draws the bars after whoever sets them this frame.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DrawBars;

pub struct ProgressBarPlugin;

impl Plugin for ProgressBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (size_bars, blink_bars).in_set(DrawBars));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fine_layer_sweeps_once_a_pixel() {
        assert_eq!(fine_fill(35, 3500, 100), 0.0, "exactly one pixel");
        assert!((fine_fill(52, 3500, 100) - 0.4857).abs() < 1e-3);
        assert_eq!(fine_fill(0, 3500, 100), 0.0);
        assert_eq!(fine_fill(10, 0, 100), 0.0, "nothing to make");
        assert!(fine_fill(u16::MAX as u32, u16::MAX as u32 - 1, u32::MAX) < 1.0, "no overflow");
    }

    #[test]
    fn the_two_fills_apart() {
        assert_ne!(FILL, FINE);
    }
}
