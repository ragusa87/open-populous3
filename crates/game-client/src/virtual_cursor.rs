//! In-game cursor. While captured, the system cursor is locked in place and hidden, and this
//! cursor moves from raw mouse motion, stopping at the window border. The real pointer never
//! reaches the screen edge, so compositor edge features (auto-hide panels on COSMIC) cannot steal
//! focus while the player pushes against the border to scroll. Its moves are re-sent as
//! `CursorMoved` window events so Bevy UI hover/click keeps working. Esc releases the capture.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorMoved, CursorOptions, PrimaryWindow, WindowEvent, WindowFocused};

/// Multiplier on raw mouse motion (raw motion is unaccelerated); `POP3_CURSOR_SPEED` overrides.
pub const DEFAULT_SPEED: f32 = 1.0;
const SIZE: f32 = 14.0;

/// Whether the mouse is captured by the game (Esc toggles), and the in-game cursor position.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct VirtualCursor {
    pub captured: bool,
    /// Logical pixels inside the window, None until first placed.
    pub position: Option<Vec2>,
    pub speed: f32,
}

impl Default for VirtualCursor {
    fn default() -> Self {
        let speed = std::env::var("POP3_CURSOR_SPEED").ok().and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_SPEED);
        VirtualCursor { captured: true, position: None, speed }
    }
}

impl VirtualCursor {
    /// Moves by raw motion, clamped to the window: pushing past the border keeps it on the edge.
    pub fn apply(&mut self, delta: Vec2, window: Vec2) -> Vec2 {
        let start = self.position.unwrap_or(window / 2.0);
        let max = (window - Vec2::splat(1.0)).max(Vec2::ZERO);
        let p = (start + delta * self.speed).clamp(Vec2::ZERO, max);
        self.position = Some(p);
        p
    }

    /// Cursor to use for edge scrolling: ours while captured, the system one otherwise.
    pub fn effective(&self, system: Option<Vec2>) -> Option<Vec2> {
        if self.captured { self.position } else { system }
    }
}

/// Grab mode and system cursor visibility for a capture state.
pub fn cursor_options(captured: bool) -> (CursorGrabMode, bool) {
    if captured { (CursorGrabMode::Locked, false) } else { (CursorGrabMode::None, true) }
}

#[derive(Component)]
struct CursorSprite;

pub struct VirtualCursorPlugin;

impl Plugin for VirtualCursorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VirtualCursor>()
            .add_systems(Startup, spawn_sprite)
            .add_systems(Update, (toggle_capture, move_cursor, draw_sprite).chain());
    }
}

fn spawn_sprite(mut commands: Commands) {
    commands.spawn((
        CursorSprite,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(SIZE),
            height: Val::Px(SIZE),
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(Color::WHITE),
        BorderColor::all(Color::BLACK),
        GlobalZIndex(i32::MAX),
        Pickable::IGNORE,
        Visibility::Hidden,
    ));
}

/// Esc toggles; regaining focus re-applies (compositors drop the lock on focus loss).
fn toggle_capture(
    keys: Res<ButtonInput<KeyCode>>,
    mut focus: MessageReader<WindowFocused>,
    mut cursor: ResMut<VirtualCursor>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut started: Local<bool>,
) {
    let refocused = focus.read().any(|f| f.focused);
    let toggled = keys.just_pressed(KeyCode::Escape);
    if !toggled && !refocused && *started {
        return;
    }
    let Ok((window, mut options)) = windows.single_mut() else { return };
    *started = true;
    if toggled {
        cursor.captured = !cursor.captured;
    }
    if cursor.captured && (toggled || cursor.position.is_none()) {
        cursor.position = window.cursor_position().or(cursor.position);
    }
    let (grab, visible) = cursor_options(cursor.captured);
    options.grab_mode = grab;
    options.visible = visible;
    options.set_changed();
}

fn move_cursor(
    motion: Res<AccumulatedMouseMotion>,
    mut cursor: ResMut<VirtualCursor>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut events: MessageWriter<WindowEvent>,
) {
    let Ok((entity, window)) = windows.single() else { return };
    if !cursor.captured || !window.focused {
        return;
    }
    let before = cursor.position;
    let after = cursor.apply(motion.delta, window.size());
    if before != Some(after) {
        let delta = before.map(|b| after - b);
        events.write(WindowEvent::CursorMoved(CursorMoved { window: entity, position: after, delta }));
    }
}

fn draw_sprite(cursor: Res<VirtualCursor>, mut q: Query<(&mut Node, &mut Visibility), With<CursorSprite>>) {
    for (mut node, mut vis) in &mut q {
        match cursor.position.filter(|_| cursor.captured) {
            Some(p) => {
                node.left = Val::Px(p.x - SIZE / 2.0);
                node.top = Val::Px(p.y - SIZE / 2.0);
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor() -> VirtualCursor {
        VirtualCursor { captured: true, position: None, speed: 1.0 }
    }

    #[test]
    fn starts_centred_and_follows_motion() {
        let mut c = cursor();
        assert_eq!(c.apply(Vec2::new(10.0, -5.0), Vec2::new(800.0, 600.0)), Vec2::new(410.0, 295.0));
    }

    #[test]
    fn stops_at_the_border_and_keeps_touching_it() {
        let mut c = VirtualCursor { position: Some(Vec2::new(5.0, 300.0)), ..cursor() };
        let win = Vec2::new(800.0, 600.0);
        assert_eq!(c.apply(Vec2::new(-50.0, 0.0), win), Vec2::new(0.0, 300.0));
        assert_eq!(c.apply(Vec2::new(-50.0, 0.0), win), Vec2::new(0.0, 300.0));
        assert!(crate::edge_push::on_edge(c.position.unwrap(), win));
        assert_eq!(c.apply(Vec2::new(5000.0, 5000.0), win), Vec2::new(799.0, 599.0));
    }

    #[test]
    fn speed_scales_motion() {
        let mut c = VirtualCursor { position: Some(Vec2::new(100.0, 100.0)), speed: 2.0, ..cursor() };
        assert_eq!(c.apply(Vec2::new(10.0, 0.0), Vec2::new(800.0, 600.0)), Vec2::new(120.0, 100.0));
    }

    #[test]
    fn released_uses_the_system_cursor() {
        let c = VirtualCursor { captured: false, position: Some(Vec2::ONE), ..cursor() };
        assert_eq!(c.effective(Some(Vec2::new(3.0, 4.0))), Some(Vec2::new(3.0, 4.0)));
        assert_eq!(cursor_options(false), (CursorGrabMode::None, true));
        assert_eq!(cursor_options(true), (CursorGrabMode::Locked, false));
    }
}
