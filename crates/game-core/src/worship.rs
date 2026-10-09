//! The shaman at a vault of knowledge (docs/specs/worship.md): ordered there she walks to its door and
//! prays (`Action::Worshipping`); its gauge fills while she prays or is inside and drains otherwise
//! (`Vault::tick`). Full, she walks in to its middle, which grants its reward, then walks out; from
//! going in until she is out no order reaches her (`GameMap::locked`).

use crate::building::{BuildingKind, Reward};
use crate::build_book::BuildAvailability;
use crate::command::Command;
use crate::map::GameMap;
use crate::spell_book::Availability;
use crate::unit::{torus_delta, Action, Inside, Order, UnitKind};

/// Within this many world units of the vault's middle she has reached it.
const MIDDLE: i32 = 64;

impl GameMap {
    /// The vault of knowledge whose footprint (grown by `enter::ENTER_MARGIN`) holds `at`, if it still
    /// teaches (not spent).
    pub fn vault_at(&self, at: (u16, u16)) -> Option<usize> {
        self.buildings.iter().position(|b| b.vault.is_some_and(|v| !v.is_spent()) && b.covers(at, crate::enter::ENTER_MARGIN))
    }

    /// The order sending the player's shaman among `units` to pray at the vault `vault` (index), if
    /// she is among them.
    pub fn worship_orders(&self, player: u8, units: &[u32], vault: usize) -> Vec<Command> {
        let Some(b) = self.buildings.get(vault).filter(|b| b.kind == BuildingKind::Vault) else { return Vec::new() };
        let site = (b.x, b.z);
        self.units.iter().filter(|u| u.owner == player && u.kind == UnitKind::Shaman && units.contains(&u.id)).map(|u| Command::OrderUnit { player, unit: u.id, order: Order::Worship { site } }).collect()
    }

    /// Unit `i` takes `Order::Worship`: a living shaman walks to the vault's door to pray there.
    pub(crate) fn go_worship(&mut self, i: usize, site: (u16, u16)) {
        let Some(b) = self.building_at_corner(site).map(|k| &self.buildings[k]) else { return };
        if self.units[i].kind != UnitKind::Shaman || b.vault.is_none_or(|v| v.is_spent()) {
            return;
        }
        let door = b.door();
        self.units[i].go_worship(site, door);
    }

    /// Unit `i` is inside a vault (walking in, at its middle, walking out): no order reaches her.
    pub fn locked(&self, i: usize) -> bool {
        let Some(inside) = self.units[i].inside else { return false };
        self.building_at_corner(inside.site).is_some_and(|k| self.buildings[k].vault.is_some())
    }

    /// `player`'s shaman is inside a vault (`locked`).
    pub fn shaman_locked(&self, player: u8) -> bool {
        self.units.iter().position(|u| u.owner == player && u.kind == UnitKind::Shaman).is_some_and(|i| self.locked(i))
    }

    /// Each tick, each vault: its gauge rises while its shaman prays at the door or is inside, and
    /// drains otherwise; full, she walks in; at its middle she gets the reward (kept in `granted`,
    /// given to the level's books) and walks out by the door.
    pub(crate) fn tend_vaults(&mut self) {
        for k in 0..self.buildings.len() {
            let Some(vault) = self.buildings[k].vault else { continue };
            let b = &self.buildings[k];
            let (site, door) = ((b.x, b.z), b.door());
            let middle = b.centre();
            let praying = self.units.iter().position(|u| u.is_alive() && u.action == Action::Worshipping { site });
            let inside = self.units.iter().position(|u| u.is_alive() && u.inside.is_some_and(|ins| ins.site == site));
            let mut vault = vault;
            vault.tick((praying.is_some() || inside.is_some()) as u16);
            match (praying, inside) {
                (Some(i), None) if vault.is_full() => self.units[i].enter(Inside { site, door }, middle),
                (_, Some(i)) if self.units[i].action == Action::Idle && near(self.units[i].x, self.units[i].z, middle) => {
                    if let Some(reward) = vault.grant() {
                        let owner = self.units[i].owner;
                        self.give(owner, reward);
                    }
                    self.units[i].start(Order::MoveTo { x: door.0, z: door.1 });
                }
                _ => {}
            }
            self.buildings[k].vault = Some(vault);
        }
    }

    /// `reward` goes to tribe `owner`: kept in `granted` (the client updates its panels from it) and
    /// made available in the level's books (shared by the tribes for now).
    fn give(&mut self, owner: u8, reward: Reward) {
        self.granted.push((owner, reward));
        match reward {
            Reward::Spell(kind) => {
                if let Some(book) = self.spell_book.as_mut() {
                    book.set(kind, Availability::Known);
                }
            }
            Reward::Building(kind) => {
                if let Some(book) = self.build_book.as_mut() {
                    book.set(kind, BuildAvailability::Available);
                }
            }
        }
    }
}

fn near(x: u16, z: u16, to: (u16, u16)) -> bool {
    torus_delta(x, to.0).abs() <= MIDDLE && torus_delta(z, to.1).abs() <= MIDDLE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::{VaultPhase, CLOSE_TICKS, OPEN_TICKS};

    fn sandbox() -> (GameMap, usize, u32) {
        let map = GameMap::sandbox_worship();
        let vault = map.buildings.iter().position(|b| b.kind == BuildingKind::Vault).unwrap();
        let shaman = map.shaman_of(0).unwrap().id;
        (map, vault, shaman)
    }

    fn phase(map: &GameMap, vault: usize) -> VaultPhase {
        map.buildings[vault].vault.unwrap().phase
    }

    fn send(map: &mut GameMap, shaman: u32, vault: usize) {
        for c in map.worship_orders(0, &[shaman], vault) {
            map.apply(&c);
        }
    }

    #[test]
    fn the_shaman_prays_goes_in_gets_the_reward_and_comes_out() {
        let (mut map, vault, shaman) = sandbox();
        send(&mut map, shaman, vault);
        let s = map.units.iter().position(|u| u.id == shaman).unwrap();
        let praying = (0..600).find(|_| {
            map.tick();
            matches!(map.units[s].action, Action::Worshipping { .. })
        });
        assert!(praying.is_some(), "at the door, praying");
        let slot = map.people_slots(crate::occupancy::Holder::Building(vault)).unwrap();
        assert_eq!(slot.filled, vec![shaman], "her place in the tooltip while she prays");
        let granted = (0..400).find(|_| {
            map.tick();
            !map.granted.is_empty()
        });
        assert!(granted.is_some(), "the gauge filled, she walked in");
        assert_eq!(map.granted, vec![(0, Reward::Building(BuildingKind::Temple))]);
        let out = (0..(OPEN_TICKS + CLOSE_TICKS) as usize).find(|_| {
            map.tick();
            map.units[s].inside.is_none()
        });
        assert!(out.is_some_and(|t| t < OPEN_TICKS as usize), "out by the door before it starts closing: {out:?}");
        (0..(OPEN_TICKS + CLOSE_TICKS) as usize).for_each(|_| {
            map.tick();
        });
        assert_eq!(phase(&map, vault), VaultPhase::Spent);
        assert_eq!(map.granted.len(), 1, "once");
        send(&mut map, shaman, vault);
        map.tick();
        assert!(!matches!(map.units[s].action, Action::Walking { .. }), "spent: she does not go back");
    }

    #[test]
    fn leaving_drains_the_gauge() {
        let (mut map, vault, shaman) = sandbox();
        send(&mut map, shaman, vault);
        let s = map.units.iter().position(|u| u.id == shaman).unwrap();
        (0..600).find(|_| {
            map.tick();
            matches!(phase(&map, vault), VaultPhase::Praying { progress } if progress == 10 * crate::gauge::STEP)
        });
        let (x, z) = (map.units[s].x, map.units[s].z.wrapping_sub(1024));
        map.apply(&Command::OrderUnit { player: 0, unit: shaman, order: Order::MoveTo { x, z } });
        for _ in 0..4 {
            map.tick();
        }
        assert!(matches!(phase(&map, vault), VaultPhase::Praying { progress } if progress < 10 * crate::gauge::STEP), "drains once she left");
    }

    #[test]
    fn no_order_reaches_her_inside() {
        let (mut map, vault, shaman) = sandbox();
        send(&mut map, shaman, vault);
        let s = map.units.iter().position(|u| u.id == shaman).unwrap();
        (0..1000).find(|_| {
            map.tick();
            map.locked(s)
        });
        assert!(map.locked(s), "walking in");
        let before = map.units[s].action;
        map.apply(&Command::OrderUnit { player: 0, unit: shaman, order: Order::Stop });
        map.apply(&Command::Order { player: 0, order: Order::Pray });
        assert_eq!(map.units[s].action, before, "ignored");
    }

    #[test]
    fn only_the_shaman_worships() {
        let (map, vault, _) = sandbox();
        let brave = map.units.iter().find(|u| u.kind == UnitKind::Brave).unwrap().id;
        assert!(map.worship_orders(0, &[brave], vault).is_empty());
        assert_eq!(map.vault_at(map.buildings[vault].centre()), Some(vault));
    }
}
