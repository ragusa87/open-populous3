//! In-game cursor. While captured, the system cursor is locked in place and hidden, and this
//! cursor moves from raw mouse motion, stopping at the window border. The real pointer never
//! reaches the screen edge, so compositor edge features (auto-hide panels on COSMIC) cannot steal
//! focus while the player pushes against the border to scroll. Its moves are re-sent as
//! `CursorMoved` window events so Bevy UI hover/click keeps working. Esc releases the capture.
//! Drawn with the original arrow pointer (`POINT0-0.DAT` sprite 14) when the original files are
//! allowed and present, else a generated arrow. In the menus it is the animated gold arrow
//! (sprites 30-33). While a spell is aimed (`CursorLook::Spell`) the gold arrow carries the spell's
//! gold icon on its right, while a blueprint is out the arrow carries the building's icon; the icon
//! (else a generated gold ring) is grayed out where the spell or building cannot apply.

use crate::world::LevelList;
use game_core::building::BuildingKind;
use game_core::spell_book::SpellKind;
use pop3_format::sprites::POINTER_GOLD_ARROW;
use std::collections::HashMap;
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
/// Seconds per frame of the animated gold arrow.
const GOLD_FRAME: f32 = 0.12;

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

/// What the cursor shows; set each frame by whoever owns the mouse (spell aiming).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CursorLook {
    #[default]
    Arrow,
    /// The menus' animated gold arrow.
    Menu,
    /// Aiming `kind`; `valid` when it can be cast under the cursor.
    Spell { kind: SpellKind, valid: bool },
    /// Placing a blueprint of `kind`; `valid` when it can stand under the cursor.
    Building { kind: BuildingKind, valid: bool },
}

/// Whether the mouse is captured by the game (Esc toggles outside the game), and the in-game cursor position.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct VirtualCursor {
    pub captured: bool,
    /// Capture (true) or release (false) on the next frame, e.g. when pausing or resuming.
    pub request: Option<bool>,
    /// Logical pixels inside the window, None until first placed.
    pub position: Option<Vec2>,
    pub speed: f32,
}

impl Default for VirtualCursor {
    fn default() -> Self {
        let speed = std::env::var("POP3_CURSOR_SPEED").ok().and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_SPEED);
        VirtualCursor { captured: true, request: None, position: None, speed }
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

/// Generated spell cursor (no original data): a gold ring with a dark outline around a cross,
/// click point at the centre.
pub fn fallback_spell_pointer() -> PointerImage {
    const SIZE: usize = 17;
    let c = (SIZE / 2) as i32;
    let rgba = (0..SIZE * SIZE)
        .flat_map(|i| {
            let (dx, dy) = ((i % SIZE) as i32 - c, (i / SIZE) as i32 - c);
            let d2 = dx * dx + dy * dy;
            let cross = (dx == 0 || dy == 0) && d2 <= 9;
            match d2 {
                _ if cross => [255, 214, 90, 255],
                42..=56 => [235, 175, 40, 255],
                30..=41 | 57..=72 => [40, 25, 0, 255],
                _ => [0, 0, 0, 0],
            }
        })
        .collect();
    PointerImage { width: SIZE, height: SIZE, rgba, tip: (c as usize, c as usize) }
}

/// Grayed-out, see-through copy: the action cannot apply here.
pub fn dimmed(p: &PointerImage) -> PointerImage {
    let rgba = p
        .rgba
        .chunks(4)
        .flat_map(|px| {
            let grey = ((px[0] as u32 * 3 + px[1] as u32 * 6 + px[2] as u32) / 10 * 3 / 5 + 40) as u8;
            if px[3] == 0 { [0; 4] } else { [grey, grey, grey, 150] }
        })
        .collect();
    PointerImage { rgba, ..*p }
}

/// Original gold icon shown while aiming a spell (`POINT0-0.DAT`, named by someone who knows the
/// game; Armageddon and Ghost Army are the likeliest left, docs/specs/sprites.md).
pub fn spell_sprite(kind: SpellKind) -> usize {
    match kind {
        SpellKind::MagicalShield => 39,
        SpellKind::Armageddon => 40,
        SpellKind::Blast => 41,
        SpellKind::Convert => 42,
        SpellKind::GhostArmy => 43,
        SpellKind::Whirlwind => 44,
        SpellKind::Invisibility => 45,
        SpellKind::Swarm => 46,
        SpellKind::Hypnotism => 47,
        SpellKind::LandBridge => 48,
        SpellKind::Lightning => 49,
        SpellKind::Erosion => 50,
        SpellKind::Flatten => 51,
        SpellKind::Earthquake => 52,
        SpellKind::Swamp => 53,
        SpellKind::Firestorm => 54,
        SpellKind::AngelOfDeath => 55,
        SpellKind::Volcano => 56,
        SpellKind::Teleport => 57,
    }
}

/// The pointer with `badge` beside it on the right, vertically centred on it: the click point stays
/// the pointer's tip.
pub fn with_badge(pointer: &PointerImage, badge: &PointerImage) -> PointerImage {
    let (bx, by) = (pointer.width, pointer.height.saturating_sub(badge.height) / 2);
    let shift = badge.height.saturating_sub(pointer.height) / 2;
    let (width, height) = (bx + badge.width, pointer.height.max(badge.height));
    let mut rgba = vec![0; width * height * 4];
    let mut paint = |img: &PointerImage, ox: usize, oy: usize| {
        for y in 0..img.height {
            for x in 0..img.width {
                let src = &img.rgba[(y * img.width + x) * 4..][..4];
                if src[3] > 0 {
                    rgba[((oy + y) * width + ox + x) * 4..][..4].copy_from_slice(src);
                }
            }
        }
    };
    paint(pointer, 0, shift);
    paint(badge, bx, by);
    PointerImage { width, height, rgba, tip: (pointer.tip.0, pointer.tip.1 + shift) }
}

/// Original teal icon shown while placing a blueprint (`POINT0-0.DAT` 58-65, sprites.md); None for
/// kinds that are never built.
pub fn building_sprite(kind: BuildingKind) -> Option<usize> {
    Some(match kind {
        BuildingKind::Hut { .. } => 58,
        BuildingKind::DrumTower => 59,
        BuildingKind::WarriorTraining => 60,
        BuildingKind::FirewarriorTraining => 61,
        BuildingKind::Temple => 62,
        BuildingKind::SpyTraining => 63,
        BuildingKind::BoatHut => 64,
        BuildingKind::AirshipHut => 65,
        _ => return None,
    })
}

/// The original pointer bank and its palette, when allowed and present.
#[derive(Resource, Default)]
struct PointerSource(Option<(pop3_format::SpriteBank, Vec<[u8; 3]>)>);

impl PointerSource {
    fn load(levels: &LevelList) -> Self {
        if !levels.original {
            return PointerSource(None);
        }
        let load = || -> Result<_, pop3_format::LevelError> {
            let bank = pop3_format::SpriteBank::load(&levels.data_dir, pop3_format::sprites::POINTER_FILE)?;
            Ok((bank, pop3_format::Theme::load(&levels.data_dir, 0)?.palette))
        };
        PointerSource(load().map_err(|e| warn!("original pointer: {e}")).ok())
    }

    fn sprite(&self, index: usize, centred: bool) -> Option<PointerImage> {
        let (bank, palette) = self.0.as_ref()?;
        let mut p = sprite_pointer(bank.sprites.get(index)?, palette);
        if centred {
            p.tip = (p.width / 2, p.height / 2);
        }
        Some(p)
    }

    /// Frame `frame` of the animated gold arrow, the plain arrow without the original files.
    fn gold(&self, frame: usize) -> PointerImage {
        self.sprite(POINTER_GOLD_ARROW[frame % POINTER_GOLD_ARROW.len()], false).unwrap_or_else(fallback_pointer)
    }

    /// An icon cursor, centred, grey and see-through when `valid` is false.
    fn icon(&self, sprite: Option<usize>, valid: bool) -> PointerImage {
        let p = sprite.and_then(|s| self.sprite(s, true)).unwrap_or_else(fallback_spell_pointer);
        if valid { p } else { dimmed(&p) }
    }

    fn image(&self, look: CursorLook, frame: usize) -> PointerImage {
        let arrow = || self.sprite(pop3_format::sprites::POINTER_ARROW, false).unwrap_or_else(fallback_pointer);
        match look {
            CursorLook::Arrow => arrow(),
            CursorLook::Menu => self.gold(frame),
            CursorLook::Spell { kind, valid } => with_badge(&self.gold(frame), &self.icon(Some(spell_sprite(kind)), valid)),
            CursorLook::Building { kind, valid } => with_badge(&arrow(), &self.icon(building_sprite(kind), valid)),
        }
    }
}

/// Frames a look cycles through: the gold arrow's, else one.
pub fn look_frames(look: CursorLook) -> usize {
    match look {
        CursorLook::Menu | CursorLook::Spell { .. } => POINTER_GOLD_ARROW.len(),
        CursorLook::Arrow | CursorLook::Building { .. } => 1,
    }
}

/// The frame shown `elapsed` seconds in, out of `frames`, looping.
pub fn frame_at(elapsed: f32, frames: usize) -> usize {
    (elapsed / GOLD_FRAME) as usize % frames.max(1)
}

/// Uploaded cursor images by look and frame: image, size and click point in screen pixels.
#[derive(Resource, Default)]
struct PointerImages(HashMap<(CursorLook, usize), (Handle<Image>, Vec2, Vec2)>);

fn upload(images: &mut Assets<Image>, pointer: PointerImage) -> (Handle<Image>, Vec2, Vec2) {
    let size = Vec2::new(pointer.width as f32, pointer.height as f32) * SCALE;
    let tip = Vec2::new(pointer.tip.0 as f32, pointer.tip.1 as f32) * SCALE;
    let mut image = Image::new(
        Extent3d { width: pointer.width as u32, height: pointer.height as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        pointer.rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::nearest();
    (images.add(image), size, tip)
}

#[derive(Component)]
struct CursorSprite {
    look: CursorLook,
    frame: usize,
    /// Click point offset in screen pixels.
    tip: Vec2,
}

pub struct VirtualCursorPlugin;

impl Plugin for VirtualCursorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VirtualCursor>()
            .init_resource::<CursorLook>()
            .init_resource::<PointerImages>()
            .add_systems(Startup, spawn_sprite)
            .add_systems(Update, (menu_cursor, toggle_capture, move_cursor, draw_sprite).chain());
    }
}

fn spawn_sprite(
    mut commands: Commands,
    levels: Res<LevelList>,
    mut images: ResMut<Assets<Image>>,
    overlay: Single<Entity, With<crate::camera::OverlayCamera>>,
) {
    let source = PointerSource::load(&levels);
    let (image, size, tip) = upload(&mut images, source.image(CursorLook::Arrow, 0));
    commands.insert_resource(source);
    commands.spawn((
        CursorSprite { look: CursorLook::Arrow, frame: 0, tip },
        ImageNode::new(image),
        Node { position_type: PositionType::Absolute, width: Val::Px(size.x), height: Val::Px(size.y), ..default() },
        GlobalZIndex(i32::MAX),
        UiTargetCamera(*overlay),
        Pickable::IGNORE,
        Visibility::Hidden,
    ));
}

/// Capture state after this frame's input: gaining focus always captures (even after Esc released
/// the mouse); then an explicit request; otherwise Esc toggles.
pub fn next_captured(captured: bool, escape: bool, refocused: bool, request: Option<bool>) -> bool {
    if refocused {
        true
    } else if let Some(r) = request {
        r
    } else if escape {
        !captured
    } else {
        captured
    }
}

/// Applies `next_captured`; also re-applies the lock on refocus (compositors drop it on focus loss).
pub(crate) fn toggle_capture(
    keys: Res<ButtonInput<KeyCode>>,
    mut focus: MessageReader<WindowFocused>,
    mut cursor: ResMut<VirtualCursor>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut started: Local<bool>,
) {
    let refocused = focus.read().any(|f| f.focused);
    let escape = keys.just_pressed(KeyCode::Escape);
    let request = cursor.request.take();
    if !escape && !refocused && request.is_none() && *started {
        return;
    }
    let Ok((window, mut options)) = windows.single_mut() else { return };
    *started = true;
    let was = cursor.captured;
    cursor.captured = next_captured(was, escape, refocused, request);
    if cursor.captured && (!was || cursor.position.is_none()) {
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

/// The gold arrow outside the game (menus, pause), the plain arrow back in it.
fn menu_cursor(state: Res<State<crate::menu::AppState>>, mut look: ResMut<CursorLook>) {
    let playing = *state.get() == crate::menu::AppState::Playing;
    if !playing {
        look.set_if_neq(CursorLook::Menu);
    } else if *look == CursorLook::Menu {
        look.set_if_neq(CursorLook::Arrow);
    }
}

fn draw_sprite(
    cursor: Res<VirtualCursor>,
    time: Res<Time>,
    look: Res<CursorLook>,
    source: Option<Res<PointerSource>>,
    mut uploaded: ResMut<PointerImages>,
    mut images: ResMut<Assets<Image>>,
    mut q: Query<(&mut CursorSprite, &mut ImageNode, &mut Node, &mut Visibility)>,
) {
    for (mut sprite, mut image, mut node, mut vis) in &mut q {
        let frame = frame_at(time.elapsed_secs(), look_frames(*look));
        if (sprite.look, sprite.frame) != (*look, frame) {
            if let Some(source) = source.as_deref() {
                let (handle, size, tip) = uploaded.0.entry((*look, frame)).or_insert_with(|| upload(&mut images, source.image(*look, frame))).clone();
                image.image = handle;
                (node.width, node.height) = (Val::Px(size.x), Val::Px(size.y));
                *sprite = CursorSprite { look: *look, frame, tip };
            }
        }
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
        VirtualCursor { captured: true, request: None, position: None, speed: 1.0 }
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
    fn spell_cursor_is_centred_and_dims_when_invalid() {
        let p = fallback_spell_pointer();
        assert_eq!((p.width, p.tip), (17, (8, 8)));
        assert_eq!(p.rgba[(8 * 17 + 8) * 4 + 3], 255, "centre is opaque");
        let d = dimmed(&p);
        let centre = &d.rgba[(8 * 17 + 8) * 4..][..4];
        assert!(centre[0] == centre[1] && centre[1] == centre[2] && centre[3] < 255, "grey, see-through: {centre:?}");
        assert_eq!(d.rgba[3], 0, "transparent stays transparent");
    }

    #[test]
    fn every_spell_has_its_own_icon() {
        let icons: std::collections::BTreeSet<usize> = SpellKind::ALL.iter().map(|&k| spell_sprite(k)).collect();
        assert_eq!(icons.len(), SpellKind::ALL.len());
        assert!(icons.iter().all(|i| pop3_format::sprites::POINTER_SPELL_ICONS.contains(i)));
        assert!(!icons.contains(&38), "bloodlust is not on the panel");
        assert_eq!((spell_sprite(SpellKind::Teleport), spell_sprite(SpellKind::Hypnotism)), (57, 47));
    }

    fn solid(width: usize, height: usize, c: [u8; 4], tip: (usize, usize)) -> PointerImage {
        PointerImage { width, height, rgba: c.repeat(width * height), tip }
    }

    #[test]
    fn gold_arrow_loops_through_its_frames() {
        assert_eq!(look_frames(CursorLook::Menu), 4);
        assert_eq!(look_frames(CursorLook::Spell { kind: SpellKind::Blast, valid: true }), 4);
        assert_eq!(look_frames(CursorLook::Arrow), 1);
        let frames: Vec<usize> = (0..10).map(|k| frame_at(k as f32 * GOLD_FRAME + 0.01, 4)).collect();
        assert_eq!(frames, [0, 1, 2, 3, 0, 1, 2, 3, 0, 1]);
        assert_eq!(frame_at(123.4, 1), 0);
    }

    #[test]
    fn badge_on_the_right_tip_unchanged() {
        let arrow = solid(10, 16, [255, 255, 255, 255], (0, 0));
        let badge = solid(8, 8, [0, 0, 255, 255], (4, 4));
        let c = with_badge(&arrow, &badge);
        let px = |x: usize, y: usize| &c.rgba[(y * c.width + x) * 4..][..4];
        assert_eq!((c.width, c.height, c.tip), (18, 16, (0, 0)));
        assert_eq!(px(0, 0), &[255, 255, 255, 255], "the arrow at the click point");
        assert_eq!(px(10, 4), &[0, 0, 255, 255], "badge right of it, centred");
        assert_eq!(px(10, 3)[3], 0);
        let tall = with_badge(&solid(4, 4, [255; 4], (1, 1)), &badge);
        assert_eq!((tall.height, tall.tip), (8, (1, 3)), "a taller badge centres the arrow");
    }

    #[test]
    fn every_buildable_kind_has_its_own_icon() {
        let icons: std::collections::BTreeSet<usize> = game_core::build_book::BUILDABLE.iter().filter_map(|&k| building_sprite(k)).collect();
        assert_eq!(icons.len(), game_core::build_book::BUILDABLE.len());
        assert!(icons.iter().all(|i| pop3_format::sprites::POINTER_BUILDING_ICONS.contains(i)));
        assert_eq!(building_sprite(BuildingKind::Hut { size: 3 }), Some(58));
        assert_eq!(building_sprite(BuildingKind::Vault), None);
    }

    #[test]
    fn sprite_pointer_uses_palette_and_transparency() {
        let sprite = pop3_format::Sprite { width: 2, height: 1, pixels: vec![None, Some(1)] };
        let p = sprite_pointer(&sprite, &[[0; 3], [10, 20, 30]]);
        assert_eq!(p.rgba, vec![0, 0, 0, 0, 10, 20, 30, 255]);
        assert_eq!(p.tip, (1, 0));
    }

    #[test]
    fn focus_recaptures_after_escape() {
        assert!(!next_captured(true, true, false, None), "Esc releases");
        assert!(next_captured(false, true, false, None), "Esc again captures");
        assert!(next_captured(false, false, true, None), "regaining focus captures");
        assert!(next_captured(false, true, true, None), "focus wins over a same-frame Esc");
        assert!(!next_captured(false, false, false, None));
        assert!(!next_captured(true, true, false, Some(false)), "pausing releases whatever Esc says");
        assert!(next_captured(false, false, false, Some(true)), "resuming captures");
    }

    #[test]
    fn released_uses_the_system_cursor() {
        let c = VirtualCursor { captured: false, position: Some(Vec2::ONE), ..cursor() };
        assert_eq!(c.effective(Some(Vec2::new(3.0, 4.0))), Some(Vec2::new(3.0, 4.0)));
        assert_eq!(cursor_options(false), (CursorGrabMode::None, true));
        assert_eq!(cursor_options(true), (CursorGrabMode::Locked, false));
    }
}
