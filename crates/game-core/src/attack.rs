//! The attack order (docs/specs/combat.md "Attacking"): who may attack what, and the attacker kept on its target's
//! trail until it is close enough for a fight to start.

use crate::building::Building;
use crate::map::GameMap;
use crate::unit::{Action, Target, Unit, UnitKind};

/// Whether `attacker` may go for `target`: both fight, of other tribes, the target alive.
pub fn may_attack(attacker: &Unit, target: &Unit) -> bool {
    fights(attacker.kind) && target.kind.stats().fights && attacker.owner != target.owner && target.is_alive()
}

/// Whether `attacker` may go for the building `b`: another tribe's.
pub fn may_attack_building(attacker: &Unit, b: &Building) -> bool {
    fights(attacker.kind) && attacker.owner != b.owner
}

/// Kinds that take an attack order. Not the firewarrior yet: its own attack throws fire from afar (TODO).
fn fights(kind: UnitKind) -> bool {
    kind.stats().fights && kind != UnitKind::Firewarrior
}

impl GameMap {
    /// Where an attack on `target` leads, if `attacker` may make it: the person, or the building's door.
    pub fn attack_goal(&self, attacker: &Unit, target: Target) -> Option<(u16, u16)> {
        match target {
            Target::Unit(id) => self.unit(id).filter(|t| may_attack(attacker, t)).map(|t| (t.x, t.z)),
            Target::Building(id) => self.building(id).filter(|b| may_attack_building(attacker, b)).map(Building::door),
        }
    }

    /// Unit `i` sets off to attack `target`, if it may.
    pub(crate) fn go_attack(&mut self, i: usize, target: Target) {
        if let Some(to) = self.attack_goal(&self.units[i], target) {
            self.units[i].attack(target, to);
        }
    }

    /// Every attacker follows its target; one whose target is gone, dead or no longer an enemy stops.
    pub(crate) fn tend_attacks(&mut self) {
        for i in 0..self.units.len() {
            let Action::Attacking { target, .. } = self.units[i].action else { continue };
            match (self.attack_goal(&self.units[i], target), target) {
                (Some(to), Target::Unit(_)) => self.units[i].retarget(to),
                (Some(_), Target::Building(_)) => {}
                (None, _) => self.units[i].action = Action::Idle,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::effect::DamageKind;
    use crate::unit::{Order, UnitId};

    fn duel(attacker: UnitKind, target: UnitKind) -> (GameMap, UnitId, UnitId) {
        let mut map = GameMap::sandbox_units();
        let s = map.shaman_of(0).unwrap();
        let at = (s.x.wrapping_add(1024), s.z);
        let a = map.spawn(0, attacker, at).id;
        let t = map.spawn(1, target, (at.0.wrapping_add(6 * 512), at.1)).id;
        (map, a, t)
    }

    fn attack(map: &mut GameMap, a: UnitId, t: UnitId) {
        map.apply(&Command::OrderUnit { player: 0, unit: a, order: Order::Attack { target: Target::Unit(t) } });
    }

    #[test]
    fn the_attacker_closes_in_on_a_walking_target() {
        let (mut map, a, t) = duel(UnitKind::Brave, UnitKind::Brave);
        attack(&mut map, a, t);
        assert!(matches!(map.unit(a).unwrap().action, Action::Attacking { .. }));
        let start = map.unit(t).map(|u| (u.x, u.z)).unwrap();
        let to = (start.0.wrapping_add(4 * 512), start.1);
        map.unit_mut(t).unwrap().order(Order::MoveTo { x: to.0, z: to.1 });
        let caught = (1..400).find(|_| {
            map.tick();
            !matches!(map.unit(a).unwrap().action, Action::Attacking { .. })
        });
        assert!(caught.is_some(), "caught up");
        assert!(matches!(map.unit(a).unwrap().action, Action::Challenge { .. }));
        let moved = map.unit(t).map(|u| crate::map::torus_dist2(start, (u.x, u.z))).unwrap();
        assert!(moved > 2 * 512 * 512, "it had walked off");
    }

    #[test]
    fn the_attack_stops_when_the_target_dies() {
        let (mut map, a, t) = duel(UnitKind::Warrior, UnitKind::Brave);
        attack(&mut map, a, t);
        map.damage(t, u16::MAX, DamageKind::Lightning, None);
        map.tick();
        assert_eq!(map.unit(a).unwrap().action, Action::Idle);
    }

    #[test]
    fn wildmen_never_fight_and_nobody_attacks_its_own() {
        let (mut map, a, t) = duel(UnitKind::Brave, UnitKind::Wildman);
        attack(&mut map, a, t);
        assert_eq!(map.unit(a).unwrap().action, Action::Idle, "a wildman is no target");
        let (mut map, a, _) = duel(UnitKind::Brave, UnitKind::Brave);
        let own = map.spawn(0, UnitKind::Brave, (0, 0)).id;
        attack(&mut map, a, own);
        assert_eq!(map.unit(a).unwrap().action, Action::Idle);
    }

    #[test]
    fn an_enemy_building_is_attacked_at_its_door() {
        let (mut map, a, _) = duel(UnitKind::Brave, UnitKind::Brave);
        let id = map.add_building(Building::new(crate::building::BuildingKind::Hut { size: 1 }, 1, 70 * 512, 70 * 512, 0));
        let door = map.building(id).unwrap().door();
        map.go_attack(map.unit_index(a).unwrap(), Target::Building(id));
        assert_eq!(map.unit(a).unwrap().action, Action::Attacking { target: Target::Building(id), to: door });
    }
}
