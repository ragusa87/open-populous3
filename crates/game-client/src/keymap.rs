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
    Editor,
    ViewPresets,
    CameraReadout,
    Pause,
    Faster,
    Slower,
}

/// A key on the keyboard, or a character wherever the layout puts it (`>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Code(KeyCode),
    Char(&'static str),
}

/// Modifiers held, Alt ignored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
}

const NONE: Mods = Mods { ctrl: false, shift: false };
const CTRL: Mods = Mods { ctrl: true, shift: false };
const SHIFT: Mods = Mods { ctrl: false, shift: true };

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
    pub fn matches(&self, down: impl Fn(Input) -> bool, mods: Mods) -> bool {
        let mods_ok = match self.input {
            Input::Char(_) => mods.ctrl == self.mods.ctrl,
            Input::Code(_) => mods == self.mods,
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
        Pause => &[key!(KeyP, NONE)],
        Faster => &[key!(Equal, SHIFT), key!(NumpadAdd, SHIFT)],
        Slower => &[key!(Minus, SHIFT), key!(NumpadSubtract, SHIFT)],
        TiltUp | TiltDown | FovWider | FovNarrower | Stop | CastSelf | NextLevel | PreviousLevel | Grid | Editor
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
        Stop => &[key!(KeyX, NONE), key!(KeyX, CTRL)],
        CastSelf => &[key!(KeyC, NONE)],
        NextLevel => &[key!(PageDown, NONE)],
        PreviousLevel => &[key!(PageUp, NONE)],
        Grid => &[key!(KeyG, NONE)],
        Editor => &[key!(Tab, NONE)],
        ViewPresets => &[key!(F2, NONE)],
        CameraReadout => &[key!(F3, NONE)],
        Pause => &[key!(KeyP, NONE)],
        Faster => &[key!(Equal, SHIFT), key!(NumpadAdd, SHIFT)],
        Slower => &[key!(Minus, SHIFT), key!(NumpadSubtract, SHIFT)],
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

    const ALL: [Shortcut; 27] = {
        use Shortcut::*;
        [
            Forward, Back, StrafeLeft, StrafeRight, RotateLeft, RotateRight, TiltUp, TiltDown, ZoomIn, ZoomOut, FovWider,
            FovNarrower, Aerial, LookAtShaman, LookAtSite, TurnPlan, Stop, CastSelf, NextLevel, PreviousLevel, Grid,
            Editor, ViewPresets, CameraReadout, Pause, Faster, Slower,
        ]
    };

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
        for s in [Stop, CastSelf, NextLevel, PreviousLevel, Grid, Editor, ViewPresets, CameraReadout, TiltUp, FovWider] {
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
        let shared = [(Shortcut::LookAtShaman, Shortcut::TurnPlan)];
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
    fn dev_mode_is_on_unless_turned_off() {
        assert_eq!(DevMode::from_env(None), DevMode(true));
        assert_eq!(DevMode::from_env(Some("1")), DevMode(true));
        assert_eq!(DevMode::from_env(Some("0")), DevMode(false));
    }
}
