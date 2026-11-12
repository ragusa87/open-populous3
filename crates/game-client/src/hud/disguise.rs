//! The other tribes' squares under the mana strip (docs/specs/units.md "Spies", shortcuts.md): shown while the
//! selection holds a spy, a click disguises every selected spy as that tribe.

use super::panel::{DisguiseStrip, DARK_BROWN};
use crate::sites::tribe_color;
use crate::units::spy::{disguise_orders, has_spy, other_tribes};
use crate::units::{selection::Selection, GameSchedule, PLAYER};
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::tribes::TribeId;

#[derive(Component)]
struct TribeSquare(TribeId);

pub struct DisguisePlugin;

impl Plugin for DisguisePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, spawn_squares).add_systems(Update, (show_squares, square_clicks));
    }
}

fn spawn_squares(mut commands: Commands, strip: Query<Entity, With<DisguiseStrip>>) {
    let Ok(strip) = strip.single() else { return };
    commands.entity(strip).with_children(|s| {
        for tribe in TribeId::ALL.into_iter().filter(|&t| t != PLAYER) {
            s.spawn((
                TribeSquare(tribe),
                Button,
                Node { width: px(26), height: px(18), border: UiRect::all(px(2)), ..default() },
                BackgroundColor(tribe_color(tribe)),
                BorderColor::all(DARK_BROWN),
            ));
        }
    });
}

fn show_squares(map: Res<CurrentMap>, selection: Res<Selection>, mut strip: Query<&mut Node, (With<DisguiseStrip>, Without<TribeSquare>)>, mut squares: Query<(&TribeSquare, &mut Node)>) {
    let on = has_spy(&map.0, &selection.units);
    for mut node in &mut strip {
        let display = if on { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
    if !on {
        return;
    }
    let tribes = other_tribes(&map.0, PLAYER);
    for (square, mut node) in &mut squares {
        let display = if tribes.contains(&square.0) { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
}

fn square_clicks(q: Query<(&Interaction, &TribeSquare), Changed<Interaction>>, map: Res<CurrentMap>, selection: Res<Selection>, mut schedule: ResMut<GameSchedule>) {
    for (i, square) in &q {
        if *i == Interaction::Pressed {
            for c in disguise_orders(&map.0, PLAYER, &selection.units, square.0) {
                schedule.issue(c);
            }
        }
    }
}
