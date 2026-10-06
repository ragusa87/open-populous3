//! In-game cursor. While captured, the system cursor is locked in place and hidden, and this
//! cursor moves from raw mouse motion, stopping at the window border. The real pointer never
//! reaches the screen edge, so compositor edge features (auto-hide panels on COSMIC) cannot steal
//! focus while the player pushes against the border to scroll. Its moves are re-sent as
//! `CursorMoved` window events so Bevy UI hover/click keeps working. Esc releases the capture.
//! Drawn with the original arrow pointer (`POINT0-0.DAT` sprite 14) when the original files are
//! allowed and present, else a generated arrow.

use crate::world::LevelList;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::{CursorGrabMode, CursorMoved, CursorOptions, PrimaryWindow, WindowEvent, WindowFocused};

/// Multiplier on raw mouse motion (raw motion is unaccelerated); `POP3_CURSOR_SPEED` overrides.
pub const DEFAULT_SPEED: f32 = 1.5;
/// Screen pixels per sprite pixel.
const SCALE: f32 = 2.0;

/// Fallback arrow: `#` outline, `.` fill, tip at (0, 0).
const ARROW: [&str; 17] = [
    "#",
    "##",
    "#.#",
    "#..#",
    "#...#",
    "#....#",
    "#.....#",
    "#......#",
    "#.......#",
    "#........#",
    "#.....#####",
    "#..#..#",
    "#.# #..#",
    "##  #..#",
    "#    #..#",
    "     #..#",
    "      ##",
];

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

/// RGBA image of a pointer and the pixel that is the click point.
#[derive(Debug, PartialEq)]
pub struct PointerImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub tip: (usize, usize),
}

/// Original pointer sprite in the given palette (transparent where the sprite has no pixel).
pub fn sprite_pointer(sprite: &pop3_format::Sprite, palette: &[[u8; 3]]) -> PointerImage {
    let rgba = sprite
        .pixels
        .iter()
        .flat_map(|p| match p.and_then(|i| palette.get(i as usize)) {
            Some(c) => [c[0], c[1], c[2], 255],
            None => [0, 0, 0, 0],
        })
        .collect();
    PointerImage { width: sprite.width, height: sprite.height, rgba, tip: sprite.tip().unwrap_or((0, 0)) }
}

/// Generated black-and-white arrow (no original data).
pub fn fallback_pointer() -> PointerImage {
    let width = ARROW.iter().map(|r| r.len()).max().unwrap_or(1);
    let rgba = ARROW
        .iter()
        .flat_map(|row| (0..width).map(move |x| row.as_bytes().get(x).copied().unwrap_or(b' ')))
        .flat_map(|c| match c {
            b'#' => [0, 0, 0, 255],
            b'.' => [255, 255, 255, 255],
            _ => [0, 0, 0, 0],
        })
        .collect();
    PointerImage { width, height: ARROW.len(), rgba, tip: (0, 0) }
}

fn original_pointer(levels: &LevelList) -> Option<PointerImage> {
    if !levels.original {
        return None;
    }
    let load = || -> Result<_, pop3_format::LevelError> {
        let bank = pop3_format::SpriteBank::load(&levels.data_dir, pop3_format::sprites::POINTER_FILE)?;
        Ok((bank, pop3_format::Theme::load(&levels.data_dir, 0)?))
    };
    let (bank, theme) = load().map_err(|e| warn!("original pointer: {e}")).ok()?;
    let sprite = bank.sprites.get(pop3_format::sprites::POINTER_ARROW)?;
    Some(sprite_pointer(sprite, &theme.palette))
}

#[derive(Component)]
struct CursorSprite {
    /// Click point offset in screen pixels.
    tip: Vec2,
}

pub struct VirtualCursorPlugin;

impl Plugin for VirtualCursorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VirtualCursor>()
            .add_systems(Startup, spawn_sprite)
            .add_systems(Update, (toggle_capture, move_cursor, draw_sprite).chain());
    }
}

fn spawn_sprite(mut commands: Commands, levels: Res<LevelList>, mut images: ResMut<Assets<Image>>) {
    let pointer = original_pointer(&levels).unwrap_or_else(fallback_pointer);
    let mut image = Image::new(
        Extent3d { width: pointer.width as u32, height: pointer.height as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        pointer.rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::nearest();
    commands.spawn((
        CursorSprite { tip: Vec2::new(pointer.tip.0 as f32, pointer.tip.1 as f32) * SCALE },
        ImageNode::new(images.add(image)),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(pointer.width as f32 * SCALE),
            height: Val::Px(pointer.height as f32 * SCALE),
            ..default()
        },
        GlobalZIndex(i32::MAX),
        Pickable::IGNORE,
        Visibility::Hidden,
    ));
}

/// Esc toggles; regaining focus re-applies (compositors drop the lock on focus loss).
pub(crate) fn toggle_capture(
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
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut events: MessageWriter<WindowEvent>,
) {
    let Ok((entity, mut window)) = windows.single_mut() else { return };
    if !cursor.captured || !window.focused {
        return;
    }
    let before = cursor.position;
    let after = cursor.apply(motion.delta, window.size());
    if before != Some(after) {
        let delta = before.map(|b| after - b);
        events.write(WindowEvent::CursorMoved(CursorMoved { window: entity, position: after, delta }));
    }
    // bevy_ui `Interaction` reads the window's cursor position, frozen while the system cursor is
    // locked (and reset by winit on enter/focus): keep it on ours.
    if window.cursor_position() != Some(after) {
        window.set_cursor_position(Some(after));
    }
}

fn draw_sprite(cursor: Res<VirtualCursor>, mut q: Query<(&CursorSprite, &mut Node, &mut Visibility)>) {
    for (sprite, mut node, mut vis) in &mut q {
        match cursor.position.filter(|_| cursor.captured) {
            Some(p) => {
                node.left = Val::Px(p.x - sprite.tip.x);
                node.top = Val::Px(p.y - sprite.tip.y);
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
    fn fallback_is_an_arrow_with_tip_at_origin() {
        let p = fallback_pointer();
        assert_eq!(p.rgba.len(), p.width * p.height * 4);
        assert_eq!(&p.rgba[..4], &[0, 0, 0, 255], "tip pixel is opaque");
        assert_eq!(p.rgba[(p.width - 1) * 4 + 3], 0, "top right is transparent");
        assert_eq!(p.tip, (0, 0));
    }

    #[test]
    fn sprite_pointer_uses_palette_and_transparency() {
        let sprite = pop3_format::Sprite { width: 2, height: 1, pixels: vec![None, Some(1)] };
        let p = sprite_pointer(&sprite, &[[0; 3], [10, 20, 30]]);
        assert_eq!(p.rgba, vec![0, 0, 0, 0, 10, 20, 30, 255]);
        assert_eq!(p.tip, (1, 0));
    }

    #[test]
    fn released_uses_the_system_cursor() {
        let c = VirtualCursor { captured: false, position: Some(Vec2::ONE), ..cursor() };
        assert_eq!(c.effective(Some(Vec2::new(3.0, 4.0))), Some(Vec2::new(3.0, 4.0)));
        assert_eq!(cursor_options(false), (CursorGrabMode::None, true));
        assert_eq!(cursor_options(true), (CursorGrabMode::Locked, false));
    }
}
