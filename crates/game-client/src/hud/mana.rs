//! The mana strip under the population strip, on every tab (docs/specs/ui-and-editor.md "Mana"): a
//! "Mana" label, the bank's amount, and a bar (`GameMap::mana`, spells.md "Mana") that is not `bank` against
//! `MAX_MANA` (pop3-rev-analysis.md "The mana bar (HUD)"): `ManaPool::advisor_fraction`, always full with
//! nothing charging, otherwise how well last turn's income kept pace with the charging spell's own pacing
//! estimate. A plain fill/track bar: the tribe's own accent colour filled, a fixed colour for the rest. No
//! original icon identified for the label yet, so, like the population strip without one, text stands in.

use super::panel::{ManaStrip, DARK_BROWN, INK};
use crate::sites::tribe_color;
use crate::units::PLAYER;
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::mana::{Mana, ManaPool};

/// The track's colour (tribe-independent): the fill is the tribe's own accent colour.
const TRACK: Color = Color::srgba(0.18, 0.10, 0.03, 0.35);

#[derive(Component)]
struct AmountText;

#[derive(Component)]
struct Fill;

/// What the strip shows: the bank, formatted, and the advisor bar's own fill share in percent.
#[derive(Clone, Debug, PartialEq)]
pub struct ManaView {
    pub text: String,
    pub share: f32,
    pub fill_color: Color,
}

pub fn mana_view(pool: &ManaPool, owner: u8) -> ManaView {
    ManaView { text: format_amount(pool.bank), share: pool.advisor_fraction() * 100.0, fill_color: tribe_color(owner) }
}

/// Thousands-grouped, e.g. `12,345`.
fn format_amount(amount: Mana) -> String {
    let digits = amount.0.to_string();
    let grouped: Vec<&str> = digits.as_bytes().rchunks(3).rev().map(|c| std::str::from_utf8(c).unwrap()).collect();
    grouped.join(",")
}

pub struct ManaPlugin;

impl Plugin for ManaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, spawn_strip).add_systems(Update, update_strip);
    }
}

fn spawn_strip(mut commands: Commands, strip: Query<Entity, With<ManaStrip>>) {
    let Ok(strip) = strip.single() else { return };
    commands.entity(strip).with_children(|s| {
        s.spawn((Text::new("Mana"), TextFont { font_size: FontSize::Px(12.0), ..default() }, TextColor(INK)));
        s.spawn((AmountText, Text::new(""), TextFont { font_size: FontSize::Px(13.0), ..default() }, TextColor(INK)));
        s.spawn((Node { flex_grow: 1.0, height: px(10), border: UiRect::all(px(1)), ..default() }, BackgroundColor(TRACK), BorderColor::all(DARK_BROWN)))
            .with_child((Fill, Node { position_type: PositionType::Absolute, left: px(0), top: px(0), bottom: px(0), width: percent(0), ..default() }, BackgroundColor(tribe_color(PLAYER))));
    });
}

fn update_strip(
    map: Res<CurrentMap>,
    mut text: Query<&mut Text, With<AmountText>>,
    mut fill: Query<(&mut Node, &mut BackgroundColor), With<Fill>>,
) {
    let view = mana_view(&map.0.mana[PLAYER as usize], PLAYER);
    for mut t in &mut text {
        if t.0 != view.text {
            t.0.clone_from(&view.text);
        }
    }
    for (mut node, mut color) in &mut fill {
        node.width = percent(view.share);
        color.0 = view.fill_color;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_shows_the_bank_grouped_by_thousands() {
        let pool = ManaPool { bank: Mana(12_345), ..ManaPool::default() };
        assert_eq!(mana_view(&pool, 0).text, "12,345");
        assert_eq!(mana_view(&ManaPool { bank: Mana(0), ..pool }, 0).text, "0");
    }

    #[test]
    fn the_bar_is_full_with_nothing_charging_regardless_of_the_bank() {
        let pool = ManaPool { bank: Mana(1), ..ManaPool::default() };
        assert_eq!(mana_view(&pool, 0).share, 100.0);
    }

    #[test]
    fn the_bar_shows_the_advisor_fraction_not_the_bank() {
        let pool = ManaPool { advisor_eta: Mana(100), received: Mana(50), ..ManaPool::default() };
        assert!((mana_view(&pool, 0).share - 100.0 * 50.0 / 178.0).abs() < 0.001);
    }

    #[test]
    fn the_fill_colour_is_the_tribe_s_own_accent() {
        assert_eq!(mana_view(&ManaPool::default(), 0).fill_color, crate::sites::tribe_color(0));
        assert_eq!(mana_view(&ManaPool::default(), 1).fill_color, crate::sites::tribe_color(1));
    }
}
