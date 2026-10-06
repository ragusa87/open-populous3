//! `POP3_CURSOR_DEBUG=1`: logs what the cursor confinement actually does (display backend,
//! focus, enter/leave, grab mode, edge contact of the in-game cursor). Bevy silently falls back to no grab when the
//! compositor refuses `Confined`, so this is the only way to see it from the game.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorEntered, CursorLeft, CursorOptions, RawHandleWrapper, WindowFocused};

pub struct CursorDebugPlugin;

impl Plugin for CursorDebugPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var("POP3_CURSOR_DEBUG").is_ok_and(|v| v == "1") {
            app.add_systems(Update, log_cursor);
        }
    }
}

/// "Wayland" / "Xlib" / "Xcb" from the raw display handle's debug output.
pub fn backend_name(display_handle_debug: &str) -> &str {
    display_handle_debug.split(['(', ' ', '{']).next().unwrap_or(display_handle_debug)
}

#[allow(clippy::too_many_arguments)]
fn log_cursor(
    handles: Query<&RawHandleWrapper>,
    windows: Query<(&Window, Ref<CursorOptions>)>,
    mut focus: MessageReader<WindowFocused>,
    mut entered: MessageReader<CursorEntered>,
    mut left: MessageReader<CursorLeft>,
    motion: Res<AccumulatedMouseMotion>,
    cursor: Res<crate::virtual_cursor::VirtualCursor>,
    time: Res<Time>,
    mut backend_logged: Local<bool>,
    mut last_edge_log: Local<f32>,
) {
    if !*backend_logged {
        if let Some(h) = handles.iter().next() {
            info!("cursor-debug: display backend {}", backend_name(&format!("{:?}", h.get_display_handle())));
            *backend_logged = true;
        }
    }
    let Some((window, options)) = windows.iter().next() else { return };
    if options.is_changed() {
        info!("cursor-debug: grab mode requested {:?}, visible {}", options.grab_mode, options.visible);
    }
    for f in focus.read() {
        info!("cursor-debug: focused {}", f.focused);
    }
    for _ in entered.read() {
        info!("cursor-debug: cursor entered window");
    }
    for _ in left.read() {
        warn!("cursor-debug: system cursor LEFT the window (grab {:?}, focused {})", options.grab_mode, window.focused);
    }
    let now = time.elapsed_secs();
    if let Some(c) = cursor.effective(window.cursor_position()) {
        let size = window.size();
        if crate::edge_push::on_edge(c, size) && motion.delta != Vec2::ZERO && now - *last_edge_log > 0.5 {
            info!("cursor-debug: on edge at {c:?} of {size:?}, pushing {:?}", motion.delta);
            *last_edge_log = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_names() {
        assert_eq!(backend_name("Wayland(WaylandDisplayHandle { display: 0x1 })"), "Wayland");
        assert_eq!(backend_name("Xlib(XlibDisplayHandle { .. })"), "Xlib");
    }
}
