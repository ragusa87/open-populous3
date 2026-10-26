//! The mana strip under the population strip, on every tab (docs/specs/ui-and-editor.md "Mana"): a
//! "Mana" label, its current amount and a bar out of `MAX_MANA` (`GameMap::mana`, spells.md "Mana"); no original
//! icon identified for it yet, so, like the population strip without one, a text label stands in.

use super::panel::{ManaStrip, DARK_BROWN, INK};
use crate::units::PLAYER;
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::mana::{Mana, ManaPool, MAX_MANA};

/// The fill's colour.
const FILL: Color = Color::srgb(0.25, 0.45, 0.85);

#[derive(Component)]
struct AmountText;

#[derive(Component)]
struct Fill;

/// What the strip shows: the bank, formatted, and its share of `MAX_MANA` in percent.
#[derive(Clone, Debug, PartialEq)]
pub struct ManaView {
    pub text: String,
    pub share: f32,
}

pub fn mana_view(pool: &ManaPool) -> ManaView {
    ManaView { text: format_amount(pool.bank), share: (pool.bank.0 as f32 * 100.0 / MAX_MANA.0 as f32).min(100.0) }
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
        s.spawn((Node { flex_grow: 1.0, height: px(10), border: UiRect::all(px(1)), ..default() }, BackgroundColor(Color::srgba(0.18, 0.10, 0.03, 0.35)), BorderColor::all(DARK_BROWN)))
            .with_child((Fill, Node { position_type: PositionType::Absolute, left: px(0), top: px(0), bottom: px(0), width: percent(0), ..default() }, BackgroundColor(FILL)));
    });
}

fn update_strip(map: Res<CurrentMap>, mut text: Query<&mut Text, With<AmountText>>, mut fill: Query<&mut Node, With<Fill>>) {
    let view = mana_view(&map.0.mana[PLAYER as usize]);
    for mut t in &mut text {
        if t.0 != view.text {
            t.0.clone_from(&view.text);
        }
    }
    for mut node in &mut fill {
        node.width = percent(view.share);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_shows_the_bank_grouped_by_thousands_against_the_cap() {
        let pool = ManaPool { bank: Mana(12_345), ..ManaPool::default() };
        let view = mana_view(&pool);
        assert_eq!(view.text, "12,345");
        assert!((view.share - 1.2345).abs() < 0.001);
        assert_eq!(mana_view(&ManaPool { bank: Mana(0), ..pool }).text, "0");
        assert_eq!(mana_view(&ManaPool { bank: MAX_MANA, ..pool }).share, 100.0);
        assert_eq!(mana_view(&ManaPool { bank: Mana(MAX_MANA.0 * 2), ..pool }).share, 100.0, "never past a full bar");
    }
}
