//! Praying for rewards (docs/specs/worship.md). At a vault of knowledge, the shaman: ordered there she
//! walks to its door and prays (`Action::Worshipping`); its gauge fills while she prays or is inside
//! and drains otherwise (`Vault::tick`). Full, she walks in to its middle, which grants its reward,
//! then walks out; from going in until she is out no order reaches her (`GameMap::locked`).
//! At a totem, the tribe's units (or only the shaman): each walks to a spot around it and prays; each
//! tribe's first `Totem::prayers` count (`Totem::queue`), its gauge fills on the soft curve of how many
//! count and drains linearly (`gauge::step`); full, its gifts go to the tribe, as many times as the
//! totem allows. A vault is keyed by its stored corner, a totem by its centre: never the same point.

use crate::building::{BuildingKind, Reward};
use crate::build_book::BuildAvailability;
use crate::command::Command;
use crate::map::GameMap;
use crate::spell_book::{Availability, SpellBook, SpellKind};
use crate::totem::TRIBES;
use crate::unit::{torus_delta, Action, Inside, Order, UnitKind};

/// Within this many world units of the vault's middle she has reached it.
const MIDDLE: i32 = 64;
/// The ground this close to a totem's centre (world units, either axis) is on it.
pub const TOTEM_MARGIN: i32 = 320;

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

    /// The totem within `TOTEM_MARGIN` of world point `at` (on the torus).
    pub fn totem_at(&self, at: (u16, u16)) -> Option<usize> {
        self.totems.iter().position(|t| !t.is_gone() && torus_delta(t.x, at.0).abs() <= TOTEM_MARGIN && torus_delta(t.z, at.1).abs() <= TOTEM_MARGIN)
    }

    /// Orders sending the player's units among `units` that may pray at totem `totem` (index) there:
    /// the shaman only for a shaman-only totem, else the followers and the shaman; none at an
    /// exhausted totem.
    pub fn totem_orders(&self, player: u8, units: &[u32], totem: usize) -> Vec<Command> {
        let Some(t) = self.totems.get(totem).filter(|t| !t.is_exhausted()) else { return Vec::new() };
        let site = (t.x, t.z);
        let may = |kind: UnitKind| if t.shaman_only { kind == UnitKind::Shaman } else { kind != UnitKind::Wildman };
        self.units.iter().filter(|u| u.owner == player && u.is_alive() && units.contains(&u.id) && may(u.kind)).map(|u| Command::OrderUnit { player, unit: u.id, order: Order::Worship { site } }).collect()
    }

    /// Unit `i` takes `Order::Worship`: at a vault the living shaman walks to its door, at a totem a
    /// unit allowed to pray there walks to a free spot around it, to pray there.
    pub(crate) fn go_worship(&mut self, i: usize, site: (u16, u16)) {
        if let Some(t) = self.totems.iter().position(|t| (t.x, t.z) == site) {
            self.go_pray_at_totem(i, t);
            return;
        }
        let Some(b) = self.building_at_corner(site).map(|k| &self.buildings[k]) else { return };
        if self.units[i].kind != UnitKind::Shaman || b.vault.is_none_or(|v| v.is_spent()) {
            return;
        }
        let door = b.door();
        self.units[i].go_worship(site, door);
    }

    fn go_pray_at_totem(&mut self, i: usize, totem: usize) {
        let t = &self.totems[totem];
        let u = &self.units[i];
        let allowed = if t.shaman_only { u.kind == UnitKind::Shaman } else { u.kind != UnitKind::Wildman };
        if !allowed || !u.is_alive() || t.is_exhausted() {
            return;
        }
        let site = (t.x, t.z);
        let (id, me) = (u.id, (u.x, u.z));
        let taken = self.taken_spots(&[id]);
        let spots = crate::slots::free_spots_near(&self.ground(), crate::slots::spot_of(site), &taken, t.prayers as usize + 8);
        if let Some(stand) = spots.iter().map(|&s| crate::slots::spot_centre(s)).filter(|&p| p != site).min_by_key(|&p| crate::map::torus_dist2(me, p)) {
            self.units[i].go_worship(site, stand);
        }
    }

    /// Each tick, each totem: its queue keeps the units praying at it in the order they started;
    /// each tribe's gauge fills with its first `prayers` of them on the soft curve, drains linearly
    /// without any; full, the tribe gets the gifts, its gauge starts again, and an exhausted totem
    /// sends its prayers away.
    pub(crate) fn tend_totems(&mut self) {
        for k in 0..self.totems.len() {
            let site = (self.totems[k].x, self.totems[k].z);
            let praying: Vec<u32> = self.units.iter().filter(|u| u.is_alive() && u.action == Action::Worshipping { site }).map(|u| u.id).collect();
            let totem = &mut self.totems[k];
            if totem.given > 0 {
                totem.since_given = totem.since_given.saturating_add(1);
            }
            totem.queue.retain(|id| praying.contains(id));
            for id in praying {
                if !totem.queue.contains(&id) {
                    totem.queue.push(id);
                }
            }
            let mut full = Vec::new();
            for tribe in 0..TRIBES {
                let count = totem.queue.iter().filter(|id| self.units.iter().any(|u| u.id == **id && u.owner as usize == tribe)).count() as u16;
                totem.gauges[tribe] = crate::gauge::step(totem.gauges[tribe], count, totem.prayers, totem.full());
                if count > 0 && totem.gauges[tribe] == totem.full() {
                    full.push(tribe as u8);
                }
            }
            for tribe in full {
                if self.totems[k].is_exhausted() {
                    break;
                }
                let totem = &mut self.totems[k];
                totem.gauges[tribe as usize] = 0;
                totem.given += 1;
                totem.since_given = 0;
                for gift in totem.gifts.clone() {
                    self.give(tribe, gift);
                }
            }
            if self.totems[k].is_exhausted() {
                for id in std::mem::take(&mut self.totems[k].queue) {
                    if let Some(u) = self.units.iter_mut().find(|u| u.id == id) {
                        u.start(Order::Stop);
                    }
                }
            }
        }
    }

    /// Unit `i` is locked (`Unit::locked`: tumbling or lifted) or inside a vault (walking in, at its
    /// middle, walking out): no order reaches it.
    pub fn locked(&self, i: usize) -> bool {
        if self.units[i].locked() {
            return true;
        }
        let Some(inside) = self.units[i].inside else { return false };
        self.building_at_corner(inside.site).is_some_and(|k| self.buildings[k].vault.is_some())
    }

    /// `player`'s shaman is `locked`: inside a vault, tumbling or lifted.
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
            Reward::OneShot(kind) => {
                if let Some(book) = self.spell_book.as_mut() {
                    one_more_shot(book, kind);
                }
            }
            Reward::Mana(_) | Reward::Unhandled { .. } => {}
        }
    }
}

/// One more cast of `kind` in `book`, unless it is known already.
pub fn one_more_shot(book: &mut SpellBook, kind: SpellKind) {
    let shots = match book.slots.iter().find(|s| s.kind == kind).map(|s| s.availability) {
        Some(Availability::Known | Availability::Unlimited) => return,
        Some(Availability::Provided { shots }) => shots.saturating_add(1),
        _ => 1,
    };
    book.set(kind, Availability::Provided { shots });
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
        let out = (0..(OPEN_TICKS + CLOSE_TICKS).get() as usize).find(|_| {
            map.tick();
            map.units[s].inside.is_none()
        });
        assert!(out.is_some_and(|t| t < OPEN_TICKS.get() as usize), "out by the door before it starts closing: {out:?}");
        (0..(OPEN_TICKS + CLOSE_TICKS).get() as usize).for_each(|_| {
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
        map.apply(&Command::Order { player: 0, order: Order::Cast });
        assert_eq!(map.units[s].action, before, "ignored");
    }

    fn braves(map: &GameMap, n: usize) -> Vec<u32> {
        map.units.iter().filter(|u| u.kind == UnitKind::Brave && u.owner == 0).take(n).map(|u| u.id).collect()
    }

    fn pray_at(map: &mut GameMap, units: &[u32], totem: usize) {
        for c in map.totem_orders(0, units, totem) {
            map.apply(&c);
        }
    }

    fn totem_of(map: &GameMap, prayers: u16, shaman_only: bool) -> usize {
        map.totems.iter().position(|t| t.prayers == prayers && t.shaman_only == shaman_only).unwrap()
    }

    #[test]
    fn eight_braves_fill_a_totem_of_eight_and_get_its_gift() {
        let mut map = GameMap::sandbox_worship();
        let head = totem_of(&map, 8, false);
        let gift = map.totems[head].gifts[0];
        let eight = braves(&map, 8);
        pray_at(&mut map, &eight, head);
        let done = (0..1500).find(|_| {
            map.tick();
            !map.granted.is_empty()
        });
        assert!(done.is_some(), "filled");
        assert_eq!(map.granted, vec![(0, gift)]);
        assert_eq!((map.totems[head].given, map.totems[head].gauges[0]), (1, 0), "given once, its gauge starts again");
    }

    #[test]
    fn the_first_count_and_a_waiting_one_takes_a_leavers_place() {
        let mut map = GameMap::sandbox_worship();
        let pole = totem_of(&map, 2, false);
        let all = braves(&map, 5);
        pray_at(&mut map, &all, pole);
        (0..600).find(|_| {
            map.tick();
            map.totems[pole].queue.len() == 5
        });
        let counted = map.people_slots(crate::occupancy::Holder::Totem(pole)).unwrap().filled;
        assert_eq!(counted.len(), 2, "two of five count");
        assert_eq!(counted, map.totems[pole].queue[..2].to_vec(), "the first to start");
        let leaver = counted[0];
        let u = map.units.iter().find(|u| u.id == leaver).unwrap();
        let (x, z) = (u.x, u.z.wrapping_add(2048));
        map.apply(&Command::OrderUnit { player: 0, unit: leaver, order: Order::MoveTo { x, z } });
        map.tick();
        let now = map.people_slots(crate::occupancy::Holder::Totem(pole)).unwrap().filled;
        assert_eq!(now.len(), 2, "still two counted");
        assert!(!now.contains(&leaver));
    }

    #[test]
    fn a_lone_prayer_fills_slowly_and_nobody_drains_it() {
        let mut map = GameMap::sandbox_worship();
        let head = totem_of(&map, 8, false);
        let one = braves(&map, 1);
        pray_at(&mut map, &one, head);
        (0..600).find(|_| {
            map.tick();
            !map.totems[head].queue.is_empty()
        });
        let before = map.totems[head].gauges[0];
        map.tick();
        assert_eq!(map.totems[head].gauges[0] - before, crate::gauge::gain(1, 8), "1 of 8: the soft curve");
        let id = map.totems[head].queue[0];
        map.apply(&Command::OrderUnit { player: 0, unit: id, order: Order::Stop });
        let held = map.totems[head].gauges[0];
        map.tick();
        assert_eq!(map.totems[head].gauges[0], held.saturating_sub(crate::gauge::DRAIN));
    }

    #[test]
    fn a_shaman_only_totem_takes_only_her() {
        let map = GameMap::sandbox_worship();
        let pole = totem_of(&map, 1, true);
        let shaman = map.shaman_of(0).unwrap().id;
        assert!(map.totem_orders(0, &braves(&map, 3), pole).is_empty());
        assert_eq!(map.totem_orders(0, &[shaman], pole).len(), 1);
        assert_eq!(map.totem_at((map.totems[pole].x + 100, map.totems[pole].z)), Some(pole));
    }

    #[test]
    fn an_exhausted_totem_sends_its_prayers_away() {
        let mut map = GameMap::sandbox_worship();
        let first = totem_of(&map, 1, false);
        map.totems[first].occurrences = 1;
        let units = braves(&map, 3);
        pray_at(&mut map, &units, first);
        (0..1500).find(|_| {
            map.tick();
            !map.granted.is_empty()
        });
        assert!(map.totems[first].is_exhausted());
        map.tick();
        assert!(map.units.iter().filter(|u| units.contains(&u.id)).all(|u| !matches!(u.action, Action::Worshipping { .. })));
        assert!(map.totem_orders(0, &units, first).is_empty(), "no more");
        let (x, z) = (map.totems[first].x, map.totems[first].z);
        assert_eq!(map.totem_at((x, z)), Some(first), "still there while it turns and sinks");
        map.run(crate::totem::TURN_TICKS + crate::totem::HOLD_TICKS + crate::totem::SINK_TICKS);
        assert!(map.totems[first].is_gone());
        assert_eq!(map.totem_at((x, z)), None, "sunk: gone");
    }

    #[test]
    fn a_one_shot_adds_a_cast_unless_known() {
        let mut book = SpellBook::new();
        one_more_shot(&mut book, SpellKind::Swarm);
        one_more_shot(&mut book, SpellKind::Swarm);
        assert_eq!(book.slots.iter().find(|s| s.kind == SpellKind::Swarm).unwrap().availability, Availability::Provided { shots: 2 });
        book.set(SpellKind::Blast, Availability::Known);
        one_more_shot(&mut book, SpellKind::Blast);
        assert_eq!(book.slots.iter().find(|s| s.kind == SpellKind::Blast).unwrap().availability, Availability::Known);
    }

    #[test]
    fn only_the_shaman_worships() {
        let (map, vault, _) = sandbox();
        let brave = map.units.iter().find(|u| u.kind == UnitKind::Brave).unwrap().id;
        assert!(map.worship_orders(0, &[brave], vault).is_empty());
        assert_eq!(map.vault_at(map.buildings[vault].centre()), Some(vault));
    }
}
