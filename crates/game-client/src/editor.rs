//! World editor (embryo), dev mode only: Tab toggles edit mode, then brushes apply at the
//! camera focus. R raise, F lower, T flatten, B land bridge from last mark (M).

use crate::camera::CameraRig;
use crate::world::{CurrentMap, TerrainDirty};
use bevy::prelude::*;
use game_core::spell::Spell;

#[derive(Resource, Default)]
pub struct EditorState {
    pub active: bool,
    pub mark: Option<(i32, i32)>,
}

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EditorState>().add_systems(Update, editor_input.in_set(crate::menu::Gameplay));
    }
}

/// Map pressed key to a spell at `at`, pure so it can be tested.
pub fn spell_for_key(key: KeyCode, at: (i32, i32), mark: Option<(i32, i32)>) -> Option<Spell> {
    match key {
        KeyCode::KeyR => Some(Spell::Raise { at }),
        KeyCode::KeyF => Some(Spell::Erode { at }),
        KeyCode::KeyT => Some(Spell::Flatten { at }),
        KeyCode::KeyB => mark.map(|from| Spell::LandBridge { from, to: at }),
        _ => None,
    }
}

fn editor_input(
    keys: Res<ButtonInput<KeyCode>>,
    shortcuts: crate::keymap::Shortcuts,
    rig: Res<CameraRig>,
    mut state: ResMut<EditorState>,
    mut map: ResMut<CurrentMap>,
    mut dirty: ResMut<TerrainDirty>,
) {
    if !shortcuts.dev() && state.active {
        state.active = false;
    }
    if shortcuts.just_pressed(crate::keymap::Shortcut::Editor) {
        state.active = !state.active;
    }
    if !state.active {
        return;
    }
    let at = (rig.focus.x.round() as i32, rig.focus.y.round() as i32);
    if keys.just_pressed(KeyCode::KeyM) {
        state.mark = Some(at);
    }
    for key in keys.get_just_pressed() {
        if let Some(spell) = spell_for_key(*key, at, state.mark) {
            dirty.0 |= spell.cast(&mut map.0.terrain).is_some();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_needs_a_mark() {
        assert_eq!(spell_for_key(KeyCode::KeyB, (1, 1), None), None);
        assert!(matches!(spell_for_key(KeyCode::KeyB, (1, 1), Some((0, 0))), Some(Spell::LandBridge { .. })));
    }
}
