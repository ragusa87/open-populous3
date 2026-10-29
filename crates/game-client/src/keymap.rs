//! Shortcuts in two keymaps: the original's (docs/specs/shortcuts.md) and ours for development, picked by
//! `DevMode` (pause menu toggle, `POP3_DEV=0|1`, on by default). Dev mode also shows the info line and the
//! camera readout. Systems ask `Shortcuts` for a `Shortcut`, never for a key.

use bevy::ecs::system::SystemParam;
use bevy::input::keyboard::Key;
use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DevMode(pub bool);

impl DevMode {
    pub fn from_env(value: Option<&str>) -> Self {
        DevMode(value != Some("0"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shortcut {
    Forward,
    Back,
    StrafeLeft,
    StrafeRight,
    RotateLeft,
    RotateRight,
    TiltUp,
    TiltDown,
    ZoomIn,
    ZoomOut,
    FovWider,
    FovNarrower,
    Aerial,
    LookAtShaman,
    LookAtSite,
    TurnPlan,
    Stop,
    CastSelf,
    NextLevel,
    PreviousLevel,
    Grid,
    DebugOverlay,
    Editor,
    ViewPresets,
    CameraReadout,
    Pause,
    Faster,
    Slower,
    ReleasePatrol,
    CancelFlyby,
    Guard,
}

impl Shortcut {
    pub const ALL: [Shortcut; 31] = {
        use Shortcut::*;
        [
            Forward, Back, StrafeLeft, StrafeRight, RotateLeft, RotateRight, TiltUp, TiltDown, ZoomIn, ZoomOut, FovWider,
            FovNarrower, Aerial, LookAtShaman, LookAtSite, TurnPlan, Stop, CastSelf, NextLevel, PreviousLevel, Grid,
            DebugOverlay, Editor, ViewPresets, CameraReadout, Pause, Faster, Slower, ReleasePatrol, CancelFlyby, Guard,
        ]
    };

    /// What it does, for the shortcuts page.
    pub fn describe(self) -> &'static str {
        use Shortcut::*;
        match self {
            Forward => "Scroll forward",
            Back => "Scroll back",
            StrafeLeft => "Scroll left",
            StrafeRight => "Scroll right",
            RotateLeft => "Rotate left",
            RotateRight => "Rotate right",
            TiltUp => "Tilt up",
            TiltDown => "Tilt down",
            ZoomIn => "Zoom in",
            ZoomOut => "Zoom out",
            FovWider => "Wider field of view",
            FovNarrower => "Narrower field of view",
            Aerial => "Aerial view",
            LookAtShaman => "Look at the shaman",
            LookAtSite => "Look at the reincarnation site",
            TurnPlan => "Turn the building plan",
            Stop => "Stop the selection",
            CastSelf => "Cast the selected spell",
            NextLevel => "Next level",
            PreviousLevel => "Previous level",
            Grid => "Cell grid",
            DebugOverlay => "Debug overlay",
            Editor => "World editor",
            ViewPresets => "View presets",
            CameraReadout => "Camera readout",
            Pause => "Pause the game",
            Faster => "Game faster",
            Slower => "Game slower",
            ReleasePatrol => "Release the held followers' patrol",
            CancelFlyby => "Cancel the current flyby",
            Guard => "Guard the shaman / stop guarding",
        }
    }
}

/// Mouse and other inputs, the same in both keymaps, for the shortcuts page.
pub const MOUSE: [(&str, &str); 12] = [
    ("Select a unit", "Left click (Ctrl: add)"),
    ("Select in a box", "Left drag"),
    ("Order the selection", "Left click: ground, tree, building, vehicle, enemy"),
    ("Deselect", "Right click"),
    ("Put out a camp fire, cancel a plan", "Shift + right click"),
    ("Scroll", "Mouse against a window edge"),
    ("Rotate and tilt", "Middle drag"),
    ("Zoom (Ctrl: tilt, Shift: field of view)", "Wheel"),
    ("Select the shaman alone", "Left click on her preview"),
    ("Look at the shaman", "Right click on her preview"),
    ("Pause menu", "Esc"),
    ("Fullscreen", "F11"),
];

/// Dev mode only, in the world editor (`editor.rs`).
pub const EDITOR_KEYS: (&str, &str) = ("Editor brushes", "R raise, F lower, T flatten, M mark, B bridge");

/// A key's name as printed on the shortcuts page.
pub fn key_name(code: KeyCode) -> String {
    let name = format!("{code:?}");
    match code {
        KeyCode::ArrowUp => "Up".into(),
        KeyCode::ArrowDown => "Down".into(),
        KeyCode::ArrowLeft => "Left".into(),
        KeyCode::ArrowRight => "Right".into(),
        KeyCode::PageUp => "PgUp".into(),
        KeyCode::PageDown => "PgDn".into(),
        KeyCode::Equal => "+".into(),
        KeyCode::Minus => "-".into(),
        KeyCode::NumpadAdd => "Numpad +".into(),
        KeyCode::NumpadSubtract => "Numpad -".into(),
        KeyCode::NumpadEnter => "Numpad Enter".into(),
        _ => match (name.strip_prefix("Key"), name.strip_prefix("Numpad")) {
            (Some(letter), _) => letter.into(),
            (_, Some(digit)) => format!("Numpad {digit}"),
            _ => name,
        },
    }
}

/// A binding as printed on the shortcuts page: its modifiers, then its key.
pub fn binding_name(b: &Binding) -> String {
    let key = match b.input {
        Input::Code(code) => key_name(code),
        Input::Char(c) => c.to_string(),
    };
    let ctrl = if b.mods.ctrl { "Ctrl+" } else { "" };
    let shift = if b.mods.shift { "Shift+" } else { "" };
    format!("{ctrl}{shift}{key}")
}

/// The shortcuts page of a keymap: every keyboard shortcut it binds (what it does, its keys), then the editor's in
/// dev mode, then the mouse.
pub fn shortcut_lines(dev: bool) -> Vec<(&'static str, String)> {
    let keys = Shortcut::ALL.iter().filter_map(|&s| {
        let b = bindings(dev, s);
        (!b.is_empty()).then(|| (s.describe(), b.iter().map(binding_name).collect::<Vec<_>>().join(" / ")))
    });
    let editor = dev.then(|| (EDITOR_KEYS.0, EDITOR_KEYS.1.to_string()));
    keys.chain(editor).chain(MOUSE.iter().map(|&(what, how)| (what, how.to_string()))).collect()
}

/// A key on the keyboard, or a character wherever the layout puts it (`>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Code(KeyCode),
    Char(&'static str),
}

/// Modifiers held. `alt` is read only by `Shortcuts::mods()`'s caller (`Ctrl+Alt+click`, units.md
/// "Patrol orders"); a `Binding` itself still ignores it, as every original key combo does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

const NONE: Mods = Mods { ctrl: false, shift: false, alt: false };
const CTRL: Mods = Mods { ctrl: true, shift: false, alt: false };
const SHIFT: Mods = Mods { ctrl: false, shift: true, alt: false };

/// An input with exactly these modifiers; a `Char` ignores Shift, which the layout may need to type it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub input: Input,
    pub mods: Mods,
}

macro_rules! key {
    ($code:ident, $mods:ident) => {
        Binding { input: Input::Code(KeyCode::$code), mods: $mods }
    };
}

impl Binding {
    /// Alt is never part of a binding's own modifiers (shortcuts.md: "Alt ignored"): only `mods.ctrl`
    /// and `mods.shift` are compared, whatever `mods.alt` is.
    pub fn matches(&self, down: impl Fn(Input) -> bool, mods: Mods) -> bool {
        let mods_ok = match self.input {
            Input::Char(_) => mods.ctrl == self.mods.ctrl,
            Input::Code(_) => mods.ctrl == self.mods.ctrl && mods.shift == self.mods.shift,
        };
        mods_ok && down(self.input)
    }
}

/// The bindings of `shortcut` in the dev or the original keymap.
pub fn bindings(dev: bool, shortcut: Shortcut) -> &'static [Binding] {
    if dev { dev_bindings(shortcut) } else { original_bindings(shortcut) }
}

fn original_bindings(shortcut: Shortcut) -> &'static [Binding] {
    use Shortcut::*;
    match shortcut {
        Forward => &[key!(ArrowUp, NONE), key!(Numpad8, NONE)],
        Back => &[key!(ArrowDown, NONE), key!(Numpad2, NONE)],
        StrafeLeft => &[key!(ArrowLeft, CTRL), key!(Numpad4, NONE)],
        StrafeRight => &[key!(ArrowRight, CTRL), key!(Numpad6, NONE)],
        RotateLeft => &[key!(ArrowLeft, NONE), key!(Numpad7, NONE)],
        RotateRight => &[key!(ArrowRight, NONE), key!(Numpad9, NONE)],
        ZoomIn => &[key!(Equal, NONE), key!(NumpadAdd, NONE)],
        ZoomOut => &[key!(Minus, NONE), key!(NumpadSubtract, NONE)],
        Aerial => &[key!(Enter, NONE), key!(NumpadEnter, NONE)],
        LookAtShaman => &[Binding { input: Input::Char(">"), mods: NONE }],
        LookAtSite => &[key!(KeyH, NONE)],
        TurnPlan => &[key!(Space, NONE)],
        CancelFlyby => &[key!(Space, NONE)],
        Pause => &[key!(KeyP, NONE)],
        Faster => &[key!(Equal, SHIFT), key!(NumpadAdd, SHIFT)],
        Slower => &[key!(Minus, SHIFT), key!(NumpadSubtract, SHIFT)],
        ReleasePatrol => &[key!(KeyN, NONE)],
        // `G`: free here (dev mode's own grid toggle owns it there instead, `dev_bindings`).
        Guard => &[key!(KeyG, NONE)],
        TiltUp | TiltDown | FovWider | FovNarrower | Stop | CastSelf | NextLevel | PreviousLevel | Grid | DebugOverlay | Editor
        | ViewPresets | CameraReadout => &[],
    }
}

fn dev_bindings(shortcut: Shortcut) -> &'static [Binding] {
    use Shortcut::*;
    match shortcut {
        Forward => &[key!(ArrowUp, NONE), key!(KeyW, NONE)],
        Back => &[key!(ArrowDown, NONE), key!(KeyS, NONE)],
        StrafeLeft => &[key!(KeyA, NONE)],
        StrafeRight => &[key!(KeyD, NONE)],
        RotateLeft => &[key!(ArrowLeft, NONE)],
        RotateRight => &[key!(ArrowRight, NONE)],
        TiltUp => &[key!(Home, NONE)],
        TiltDown => &[key!(End, NONE)],
        ZoomIn => &[key!(PageUp, CTRL)],
        ZoomOut => &[key!(PageDown, CTRL)],
        FovWider => &[key!(PageUp, SHIFT)],
        FovNarrower => &[key!(PageDown, SHIFT)],
        Aerial => &[key!(Enter, NONE), key!(NumpadEnter, NONE)],
        LookAtShaman => &[key!(Space, NONE)],
        LookAtSite => &[key!(KeyH, NONE)],
        TurnPlan => &[key!(Space, NONE)],
        CancelFlyby => &[key!(Space, NONE)],
        Stop => &[key!(KeyX, NONE), key!(KeyX, CTRL)],
        CastSelf => &[key!(KeyC, NONE)],
        NextLevel => &[key!(PageDown, NONE)],
        PreviousLevel => &[key!(PageUp, NONE)],
        Grid => &[key!(KeyG, NONE)],
        DebugOverlay => &[key!(F4, NONE)],
        Editor => &[key!(Tab, NONE)],
        ViewPresets => &[key!(F2, NONE)],
        CameraReadout => &[key!(F3, NONE)],
        Pause => &[key!(KeyP, NONE)],
        Faster => &[key!(Equal, SHIFT), key!(NumpadAdd, SHIFT)],
        Slower => &[key!(Minus, SHIFT), key!(NumpadSubtract, SHIFT)],
        ReleasePatrol => &[key!(KeyN, NONE)],
        // `G` is already `Grid` here (a dev-only convenience): Shift+G instead, so dev mode can still reach it.
        Guard => &[key!(KeyG, SHIFT)],
    }
}

/// The keyboard read through the keymap of the current mode.
#[derive(SystemParam)]
pub struct Shortcuts<'w> {
    codes: Res<'w, ButtonInput<KeyCode>>,
    chars: Res<'w, ButtonInput<Key>>,
    dev: Res<'w, DevMode>,
}

impl Shortcuts<'_> {
    pub fn dev(&self) -> bool {
        self.dev.0
    }

    pub fn mods(&self) -> Mods {
        Mods {
            ctrl: self.codes.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
            shift: self.codes.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
            alt: self.codes.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]),
        }
    }

    pub fn pressed(&self, shortcut: Shortcut) -> bool {
        self.active(shortcut, |i| match i {
            Input::Code(c) => self.codes.pressed(c),
            Input::Char(s) => self.chars.pressed(Key::Character(s.into())),
        })
    }

    pub fn just_pressed(&self, shortcut: Shortcut) -> bool {
        self.active(shortcut, |i| match i {
            Input::Code(c) => self.codes.just_pressed(c),
            Input::Char(s) => self.chars.just_pressed(Key::Character(s.into())),
        })
    }

    /// 1, -1 or 0 for a pair of opposite shortcuts held.
    pub fn axis(&self, plus: Shortcut, minus: Shortcut) -> f32 {
        self.pressed(plus) as i32 as f32 - self.pressed(minus) as i32 as f32
    }

    fn active(&self, shortcut: Shortcut, down: impl Fn(Input) -> bool) -> bool {
        let mods = self.mods();
        bindings(self.dev.0, shortcut).iter().any(|b| b.matches(&down, mods))
    }
}

pub struct KeymapPlugin;

impl Plugin for KeymapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DevMode::from_env(std::env::var("POP3_DEV").ok().as_deref()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Shortcut; 31] = Shortcut::ALL;

    fn fires(dev: bool, shortcut: Shortcut, input: Input, mods: Mods) -> bool {
        bindings(dev, shortcut).iter().any(|b| b.matches(|i| i == input, mods))
    }

    #[test]
    fn modifiers_must_match_exactly() {
        let left = Input::Code(KeyCode::ArrowLeft);
        assert!(fires(false, Shortcut::RotateLeft, left, NONE));
        assert!(!fires(false, Shortcut::RotateLeft, left, CTRL), "Ctrl+Left scrolls in the original");
        assert!(fires(false, Shortcut::StrafeLeft, left, CTRL));
        let page_up = Input::Code(KeyCode::PageUp);
        assert!(fires(true, Shortcut::PreviousLevel, page_up, NONE));
        assert!(!fires(true, Shortcut::PreviousLevel, page_up, CTRL));
        assert!(fires(true, Shortcut::ZoomIn, page_up, CTRL));
    }

    #[test]
    fn shift_plus_minus_set_the_speed_plain_ones_zoom() {
        let plus = Input::Code(KeyCode::Equal);
        assert!(fires(false, Shortcut::ZoomIn, plus, NONE) && !fires(false, Shortcut::Faster, plus, NONE));
        assert!(fires(false, Shortcut::Faster, plus, SHIFT) && !fires(false, Shortcut::ZoomIn, plus, SHIFT));
        assert!(fires(true, Shortcut::Pause, Input::Code(KeyCode::KeyP), NONE), "in both keymaps");
    }

    #[test]
    fn a_character_ignores_the_shift_typing_it() {
        assert!(fires(false, Shortcut::LookAtShaman, Input::Char(">"), SHIFT));
        assert!(!fires(false, Shortcut::LookAtShaman, Input::Char(">"), CTRL));
    }

    #[test]
    fn original_keymap_has_no_dev_tools() {
        use Shortcut::*;
        for s in [Stop, CastSelf, NextLevel, PreviousLevel, Grid, DebugOverlay, Editor, ViewPresets, CameraReadout, TiltUp, FovWider] {
            assert!(bindings(false, s).is_empty(), "{s:?}");
        }
        assert!(!fires(false, Shortcut::LookAtShaman, Input::Code(KeyCode::Space), NONE), "Space skips the fly-by there");
    }

    #[test]
    fn every_dev_shortcut_has_a_key() {
        assert!(ALL.iter().all(|&s| !bindings(true, s).is_empty()));
    }

    #[test]
    fn no_key_does_two_things_in_one_keymap() {
        let shared = [
            (Shortcut::LookAtShaman, Shortcut::TurnPlan),
            (Shortcut::LookAtShaman, Shortcut::CancelFlyby),
            (Shortcut::TurnPlan, Shortcut::CancelFlyby),
        ];
        for dev in [false, true] {
            for (i, &a) in ALL.iter().enumerate() {
                for &b in &ALL[i + 1..] {
                    if shared.contains(&(a, b)) {
                        continue;
                    }
                    for x in bindings(dev, a) {
                        assert!(!bindings(dev, b).contains(x), "{a:?} and {b:?} share {x:?} (dev {dev})");
                    }
                }
            }
        }
    }

    #[test]
    fn the_shortcuts_page_lists_the_active_keymap() {
        assert_eq!(binding_name(&key!(PageUp, CTRL)), "Ctrl+PgUp");
        assert_eq!(binding_name(&key!(Equal, SHIFT)), "Shift++");
        assert_eq!(binding_name(&key!(KeyW, NONE)), "W");
        assert_eq!(binding_name(&key!(Numpad8, NONE)), "Numpad 8");
        assert_eq!(binding_name(&Binding { input: Input::Char(">"), mods: NONE }), ">");
        let line = |dev: bool, what: &str| shortcut_lines(dev).into_iter().find(|l| l.0 == what).map(|l| l.1);
        assert_eq!(line(true, "Look at the shaman"), Some("Space".into()));
        assert_eq!(line(false, "Look at the shaman"), Some(">".into()));
        assert_eq!(line(false, "World editor"), None, "not in the original's keymap");
        assert_eq!(line(false, "Editor brushes"), None);
        assert!(line(true, "Editor brushes").is_some());
        assert_eq!(line(false, "Deselect"), Some("Right click".into()), "the mouse in both");
        assert_eq!(shortcut_lines(true).len(), Shortcut::ALL.len() + 1 + MOUSE.len(), "every dev shortcut has a key");
    }

    #[test]
    fn dev_mode_is_on_unless_turned_off() {
        assert_eq!(DevMode::from_env(None), DevMode(true));
        assert_eq!(DevMode::from_env(Some("1")), DevMode(true));
        assert_eq!(DevMode::from_env(Some("0")), DevMode(false));
    }
}
