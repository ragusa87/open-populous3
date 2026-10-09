//! The population strip under the shaman preview, on every tab (docs/specs/ui-and-editor.md
//! "Population"): people icon, "population / room" and a bar from `GameMap::housing`, red and blinking
//! once there is no room left.

use super::panel::{PopulationStrip, DARK_BROWN, INK};
use crate::tooltip::IconImages;
use crate::units::PLAYER;
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::headcount::Housing;
use pop3_format::catalog::PEOPLE_ICON;

/// The fill's colour, red when full.
const ROOM_LEFT: Color = Color::srgb(0.27, 0.59, 0.24);
const NO_ROOM: Color = Color::srgb(0.78, 0.24, 0.12);
/// The full bar blinks this many times a second.
const BLINK_HZ: f32 = 1.0;

#[derive(Component)]
struct CountText;

#[derive(Component)]
struct Fill;

/// What the strip shows: "population / room", the bar's share in percent, full, over the room.
#[derive(Clone, Debug, PartialEq)]
pub struct StripView {
    pub text: String,
    pub share: f32,
    pub full: bool,
    pub over: bool,
}

pub fn strip_view(h: &Housing) -> StripView {
    let room = h.room();
    let share = if room == 0 { 100.0 } else { (h.population as f32 * 100.0 / room as f32).min(100.0) };
    StripView { text: format!("{} / {}", h.population, room), share, full: h.full(), over: h.population > room }
}

pub struct PopulationPlugin;

impl Plugin for PopulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, spawn_strip).add_systems(Update, update_strip);
    }
}

fn spawn_strip(mut commands: Commands, strip: Query<Entity, With<PopulationStrip>>, mut icons: ResMut<IconImages>, mut images: ResMut<Assets<Image>>) {
    let Ok(strip) = strip.single() else { return };
    let icon = icons.sprite(PEOPLE_ICON, &mut images);
    commands.entity(strip).with_children(|s| {
        match icon {
            Some((image, size)) => {
                s.spawn((ImageNode::new(image), Node { width: px(size.x), height: px(size.y), ..default() }));
            }
            None => {
                s.spawn((Text::new("Pop"), TextFont { font_size: FontSize::Px(12.0), ..default() }, TextColor(INK)));
            }
        }
        s.spawn((CountText, Text::new(""), TextFont { font_size: FontSize::Px(13.0), ..default() }, TextColor(INK)));
        s.spawn((Node { flex_grow: 1.0, height: px(10), border: UiRect::all(px(1)), ..default() }, BackgroundColor(Color::srgba(0.18, 0.10, 0.03, 0.35)), BorderColor::all(DARK_BROWN)))
            .with_child((Fill, Node { position_type: PositionType::Absolute, left: px(0), top: px(0), bottom: px(0), width: percent(0), ..default() }, BackgroundColor(ROOM_LEFT)));
    });
}

fn update_strip(time: Res<Time>, map: Res<CurrentMap>, mut text: Query<(&mut Text, &mut TextColor), With<CountText>>, mut fill: Query<(&mut Node, &mut BackgroundColor), With<Fill>>) {
    let view = strip_view(&map.0.housing(PLAYER));
    for (mut t, mut color) in &mut text {
        if t.0 != view.text {
            t.0.clone_from(&view.text);
        }
        color.set_if_neq(TextColor(if view.over { NO_ROOM } else { INK }));
    }
    let lit = !view.full || (time.elapsed_secs() * BLINK_HZ).fract() < 0.5;
    let shown = match (view.full, lit) {
        (false, _) => ROOM_LEFT,
        (true, true) => NO_ROOM,
        (true, false) => NO_ROOM.with_alpha(0.45),
    };
    for (mut node, mut bg) in &mut fill {
        node.width = percent(view.share);
        bg.set_if_neq(BackgroundColor(shown));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_counts_against_the_room_and_turns_full() {
        let view = strip_view(&Housing { huts: [3, 2, 1], population: 23 });
        assert_eq!(view.text, "23 / 26");
        assert!((view.share - 88.46).abs() < 0.1);
        assert!(!view.full && !view.over);
        let at = strip_view(&Housing { huts: [1, 0, 0], population: 3 });
        assert!(at.full && !at.over);
        let over = strip_view(&Housing { huts: [0; 3], population: 4 });
        assert_eq!((over.text.as_str(), over.share, over.full, over.over), ("4 / 0", 100.0, true, true));
    }
}
