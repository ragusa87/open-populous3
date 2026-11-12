//! Spies on the player's side (docs/specs/units.md "Spies"): disguising the selected spies as another tribe, and
//! spotting other tribes' spies passing as the player's own people - the reveal cursor over one, a click unmasking it.

use crate::hover::{HoverTarget, Hovered};
use crate::virtual_cursor::CursorLook;
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::command::Command;
use game_core::map::GameMap;
use game_core::tribes::TribeId;
use game_core::unit::{Order, UnitId, UnitKind};

/// The other tribes the player may disguise as, by tribe index (theirs skipped): those in the game, or with anyone
/// or anything on the map.
pub fn other_tribes(map: &GameMap, player: TribeId) -> Vec<TribeId> {
    let present = |t: TribeId| map.tribes.all().any(|x| x == t) || map.units.iter().any(|u| u.owner == t) || map.buildings.iter().any(|b| b.owner == t);
    TribeId::ALL.into_iter().filter(|&t| t != player && present(t)).collect()
}

/// Whether `selected` holds one of the player's spies: only then can it disguise.
pub fn has_spy(map: &GameMap, selected: &[UnitId]) -> bool {
    selected.iter().filter_map(|&id| map.unit(id)).any(|u| u.kind == UnitKind::Spy && u.is_alive())
}

/// A click on `tribe`'s square: one disguise order per selected spy.
pub fn disguise_orders(map: &GameMap, player: TribeId, selected: &[UnitId], tribe: TribeId) -> Vec<Command> {
    selected
        .iter()
        .filter_map(|&id| map.unit(id))
        .filter(|u| u.kind == UnitKind::Spy && u.owner == player)
        .map(|u| Command::OrderUnit { player, unit: u.id, order: Order::Disguise { tribe } })
        .collect()
}

/// The spy under the cursor that a click would unmask: another tribe's, settled in the player's own look.
pub fn hinted_spy(map: &GameMap, hovered: Option<HoverTarget>, player: TribeId) -> Option<UnitId> {
    let Some(HoverTarget::Unit(i)) = hovered else { return None };
    map.units.get(i).filter(|u| u.fools(player) && u.is_alive()).map(|u| u.id)
}

/// A click over a hinted spy that gave no attack order: it is unmasked all the same.
pub fn reveal_click(map: &GameMap, hovered: Option<HoverTarget>, player: TribeId) -> Option<Command> {
    hinted_spy(map, hovered, player).map(|unit| Command::RevealSpy { player, unit })
}

pub struct SpyPlugin;

impl Plugin for SpyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, reveal_cursor.in_set(crate::menu::Gameplay));
    }
}

/// The reveal cursor while over a hinted spy, back to the arrow off it; never over another look (a spell, a plan).
fn reveal_cursor(map: Res<CurrentMap>, hovered: Res<Hovered>, mut look: ResMut<CursorLook>) {
    let hint = hinted_spy(&map.0, hovered.0, super::PLAYER).is_some();
    match *look {
        CursorLook::Arrow if hint => look.set_if_neq(CursorLook::RevealSpy),
        CursorLook::RevealSpy if !hint => look.set_if_neq(CursorLook::Arrow),
        _ => false,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::time::Ticks;

    fn map() -> (GameMap, UnitId, usize) {
        let map = GameMap::sandbox_units();
        let spy = map.units.iter().find(|u| u.owner == TribeId::Blue && u.kind == UnitKind::Spy).unwrap().id;
        let enemy = map.units.iter().position(|u| u.owner == TribeId::Red && u.kind == UnitKind::Spy).unwrap();
        (map, spy, enemy)
    }

    #[test]
    fn only_a_settled_enemy_disguise_gets_the_hint_and_a_click_reveals_it() {
        let (mut map, own, enemy) = map();
        let hover = Some(HoverTarget::Unit(enemy));
        assert_eq!(hinted_spy(&map, hover, TribeId::Blue), None, "undisguised");
        map.units[enemy].disguise_as(TribeId::Blue, game_core::disguise::DISGUISE_DELAY);
        assert_eq!(hinted_spy(&map, hover, TribeId::Blue), None, "still settling: an enemy for all to see");
        map.units[enemy].disguise_as(TribeId::Blue, Ticks::ZERO);
        let id = map.units[enemy].id;
        assert_eq!(reveal_click(&map, hover, TribeId::Blue), Some(Command::RevealSpy { player: TribeId::Blue, unit: id }));
        let own = map.units.iter().position(|u| u.id == own).unwrap();
        map.units[own].disguise_as(TribeId::Red, Ticks::ZERO);
        assert_eq!(hinted_spy(&map, Some(HoverTarget::Unit(own)), TribeId::Blue), None, "its own");
        map.units[enemy].disguise_as(TribeId::Yellow, Ticks::ZERO);
        assert_eq!(hinted_spy(&map, hover, TribeId::Blue), None, "passing as a third tribe");
        let brave = map.units.iter().position(|u| u.owner == TribeId::Red && u.kind == UnitKind::Brave).unwrap();
        assert_eq!(hinted_spy(&map, Some(HoverTarget::Unit(brave)), TribeId::Blue), None);
    }

    #[test]
    fn a_square_click_disguises_every_selected_spy_of_the_player_only() {
        let (map, spy, _) = map();
        let brave = map.units.iter().find(|u| u.owner == TribeId::Blue && u.kind == UnitKind::Brave).unwrap().id;
        assert!(has_spy(&map, &[brave, spy]) && !has_spy(&map, &[brave]));
        assert_eq!(disguise_orders(&map, TribeId::Blue, &[brave, spy], TribeId::Red), [Command::OrderUnit { player: TribeId::Blue, unit: spy, order: Order::Disguise { tribe: TribeId::Red } }]);
    }

    #[test]
    fn the_squares_skip_the_player_and_absent_tribes() {
        let (map, _, _) = map();
        let tribes = other_tribes(&map, TribeId::Blue);
        assert!(!tribes.contains(&TribeId::Blue) && tribes.contains(&TribeId::Red));
    }
}
