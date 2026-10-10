//! Commands scheduled for the tick they apply on (multiplayer.md). Local commands wait in `pending` until the
//! local turn closes: before running tick `now`, the turn of tick `now + delay` closes with them (and goes to
//! the peers). A tick runs once every player closed its turn; the first `delay` ticks after `start` need none,
//! nobody could have sent anything for them. Commands apply players in order, each player's as issued.
//! Single player is the same with one player and no delay: what was issued before a tick applies on it.

use crate::command::Command;
use crate::time::{Tick, Ticks};
use std::collections::BTreeMap;

#[derive(Default, Debug, Clone)]
struct Turn {
    /// Players done with this tick, each with its commands.
    closed: Vec<(u8, Vec<Command>)>,
}

#[derive(Debug, Clone)]
pub struct Schedule {
    local: u8,
    /// Every player of the game, sorted.
    players: Vec<u8>,
    delay: Ticks,
    start: Tick,
    pending: Vec<Command>,
    /// By the tick's counter; ticks are looked up, never ordered, so the wrap does not matter.
    turns: BTreeMap<u32, Turn>,
    /// The last tick the local player closed.
    closed_to: Option<Tick>,
}

impl Schedule {
    /// One player, commands on the next tick, from a map at `start`.
    pub fn single(local: u8, start: Tick) -> Self {
        Schedule::lockstep(local, vec![local], Ticks::ZERO, start)
    }

    pub fn lockstep(local: u8, mut players: Vec<u8>, delay: Ticks, start: Tick) -> Self {
        players.sort_unstable();
        players.dedup();
        Schedule { local, players, delay, start, pending: Vec::new(), turns: BTreeMap::new(), closed_to: None }
    }

    /// A local command, sent with the next turn closed.
    pub fn issue(&mut self, command: Command) {
        self.pending.push(command);
    }

    /// Before running tick `now`: closes the local turn of `now + delay` with the commands issued since the
    /// last one, and returns it to send to the peers. None if that turn is already closed (waiting on peers).
    pub fn close_local(&mut self, now: Tick) -> Option<(Tick, Vec<Command>)> {
        let at = now + self.delay;
        if self.closed_to == Some(at) {
            return None;
        }
        self.closed_to = Some(at);
        let commands = std::mem::take(&mut self.pending);
        self.receive(self.local, at, commands.clone());
        Some((at, commands))
    }

    /// A player's turn for tick `at`, from the network or the local player; a second one for the same tick
    /// is ignored.
    pub fn receive(&mut self, player: u8, at: Tick, commands: Vec<Command>) {
        let turn = self.turns.entry(at.to_wire()).or_default();
        if !turn.closed.iter().any(|(p, _)| *p == player) {
            turn.closed.push((player, commands));
        }
    }

    /// Every player closed tick `at`'s turn (or it is one of the first `delay` ticks).
    pub fn ready(&self, at: Tick) -> bool {
        if at - self.start < self.delay {
            return true;
        }
        let Some(turn) = self.turns.get(&at.to_wire()) else { return false };
        self.players.iter().all(|p| turn.closed.iter().any(|(q, _)| q == p))
    }

    /// The commands of tick `at` in their order, forgetting the turn. Call once `ready`.
    pub fn take(&mut self, at: Tick) -> Vec<Command> {
        let mut closed = self.turns.remove(&at.to_wire()).map(|t| t.closed).unwrap_or_default();
        closed.sort_by_key(|(p, _)| *p);
        closed.into_iter().flat_map(|(_, c)| c).collect()
    }

    /// Commands issued and not sent yet.
    pub fn pending(&self) -> &[Command] {
        &self.pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::Order;

    fn order(player: u8, x: u16) -> Command {
        Command::Order { player, order: Order::MoveTo { x, z: 0 } }
    }

    fn tick(n: u32) -> Tick {
        Tick::ZERO + Ticks::new(n)
    }

    #[test]
    fn single_player_commands_apply_on_the_next_tick() {
        let mut s = Schedule::single(0, Tick::ZERO);
        s.issue(order(0, 1));
        s.issue(order(0, 2));
        assert_eq!(s.close_local(Tick::ZERO), Some((Tick::ZERO, vec![order(0, 1), order(0, 2)])));
        assert!(s.ready(Tick::ZERO));
        assert_eq!(s.take(Tick::ZERO), vec![order(0, 1), order(0, 2)], "in the order issued");
        assert!(s.close_local(tick(1)).is_some());
        assert_eq!(s.take(tick(1)), vec![]);
    }

    #[test]
    fn issued_while_paused_waits_for_the_next_tick() {
        let mut s = Schedule::single(0, tick(5));
        s.issue(order(0, 1));
        assert_eq!(s.pending(), &[order(0, 1)]);
        s.issue(order(0, 2));
        s.close_local(tick(5));
        assert_eq!(s.take(tick(5)).len(), 2);
    }

    #[test]
    fn lockstep_waits_for_every_player_and_orders_them() {
        let delay = Ticks::new(2);
        let mut s = Schedule::lockstep(1, vec![1, 0], delay, Tick::ZERO);
        assert!(s.ready(Tick::ZERO) && s.ready(tick(1)), "the first ticks need no turn");
        s.issue(order(1, 10));
        assert_eq!(s.close_local(Tick::ZERO), Some((tick(2), vec![order(1, 10)])));
        assert_eq!(s.close_local(Tick::ZERO), None, "already closed: waiting");
        assert!(!s.ready(tick(2)), "player 0 has not sent tick 2");
        s.receive(0, tick(2), vec![order(0, 20)]);
        s.receive(0, tick(2), vec![order(0, 99)]);
        assert!(s.ready(tick(2)));
        assert_eq!(s.take(tick(2)), vec![order(0, 20), order(1, 10)], "players in order, a resent turn ignored");
    }

    #[test]
    fn ticks_across_the_wrap() {
        let start = Tick::from_wire(u32::MAX - 1);
        let mut s = Schedule::lockstep(0, vec![0], Ticks::new(3), start);
        assert!(s.ready(start + Ticks::new(2)));
        assert!(!s.ready(start + Ticks::new(3)));
        s.issue(order(0, 1));
        s.close_local(start);
        assert_eq!(s.take(start + Ticks::new(3)), vec![order(0, 1)]);
    }
}
