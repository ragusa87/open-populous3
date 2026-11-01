//! Deterministic lockstep over TCP: peers only exchange per-tick command lists (`game_core::schedule`).
//!
//! Wire format: `u32 LE length` + payload. Payload starts with a tag byte.
//! See docs/specs/multiplayer.md.

use game_core::building::BuildingKind;
use game_core::command::Command;
use game_core::id::Id;
use game_core::spell::Spell;
use game_core::spell_book::SpellKind;
use game_core::time::Tick;
use game_core::tribes::TribeId;
use game_core::unit::{Order, Target, UnitId, WorshipAt};
use std::io::{self, Read, Write};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Hello { player: TribeId },
    /// From the host before the first turn: the game's seed (`GameMap::seeded`), the same for every peer.
    Start { seed: u32 },
    /// All commands a player issues for one simulation tick (possibly none): its turn.
    Tick { tick: Tick, commands: Vec<Command> },
    /// A peer's own resolved spell constants (`game_core::spell_book::SpellBook::constants_hash`), exchanged
    /// at game start so a mismatch (different installs, or one with and one without `constant.dat`) is caught
    /// before it silently desyncs mid-match instead of after (spells.md "Mana"). Not yet sent/checked by any
    /// driver (TODO.md "Network driver"): the codec exists so that plan can send it without inventing a tag.
    Handshake { constants_hash: u64 },
}

/// Whether two peers' `Message::Handshake` hashes agree: `false` is a fatal "cannot join: constants mismatch",
/// never something to continue past (multiplayer.md "Wire format"). A trivial comparison today - the actual
/// network driver that exchanges handshakes and refuses a mismatched join doesn't exist yet (TODO.md "Network
/// driver"); this function is the contract it will call.
pub fn constants_match(mine: u64, theirs: u64) -> bool {
    mine == theirs
}

pub fn send(w: &mut impl Write, msg: &Message) -> io::Result<()> {
    let body = encode(msg);
    w.write_all(&(body.len() as u32).to_le_bytes())?;
    w.write_all(&body)
}

pub fn recv(r: &mut impl Read) -> io::Result<Message> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let mut body = vec![0u8; u32::from_le_bytes(len) as usize];
    r.read_exact(&mut body)?;
    decode(&body).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad message"))
}

pub fn encode(msg: &Message) -> Vec<u8> {
    let mut b = Vec::new();
    match msg {
        Message::Hello { player } => b.extend([0, player.as_u8()]),
        Message::Start { seed } => {
            b.push(2);
            b.extend(seed.to_le_bytes());
        }
        Message::Tick { tick, commands } => {
            b.push(1);
            b.extend(tick.to_wire().to_le_bytes());
            b.push(commands.len() as u8);
            for command in commands {
                encode_command(&mut b, command);
            }
        }
        Message::Handshake { constants_hash } => {
            b.push(3);
            b.extend(constants_hash.to_le_bytes());
        }
    }
    b
}

/// `kind u8, player u8`, then for a cast `spell tag u8 + i16 cells` (Teleport, Blast, Lightning, Armageddon, Swamp,
/// Convert, Hypnotism, Volcano, Firestorm, Earthquake, MagicalShield, Bloodlust, Invisibility: `u16 x, z`), for an
/// order `order tag u8 [+ u16 x, z]`, for a unit order `u32 unit id` then the order, for lighting or putting out a
/// camp fire `u16 x, z`, for placing a building `model u8, u16 x, z, facing u8`, for cancelling one `u16 x, z`, for
/// a self cast (a spell not aimed on a spot yet) `spell u8` (its place in `SpellKind::ALL`), for pausing one
/// `spell u8, paused u8`, for moving a vehicle `u32 vehicle id, u16 x, z`, for unloading one `u32 vehicle id`, for
/// clearing a swamp `u32 effect id`, for a patrol command `u8 unit count` then that many `u32` unit ids, then
/// `u16 x, z` (`SetPatrol`/`AddPatrolPoint`) or a target tag `u8` (0 unit, 1 building) + `u32` id
/// (`SetPatrolTarget`/`AddPatrolTargetPoint`), or nothing more (`ReleasePatrol`); for a formation command, `u8` unit
/// count then that many `u32` unit ids, then `u16 x, z` (`Formation`); for cancelling a guard, nothing more
/// (`CancelGuard`); an order tag of `Order::Guard` takes no payload either.
fn encode_command(b: &mut Vec<u8>, command: &Command) {
    match command {
        Command::Cast { player, spell } => {
            b.extend([0, player.as_u8()]);
            if let Spell::Teleport { to: at }
            | Spell::Blast { at }
            | Spell::Lightning { at }
            | Spell::Armageddon { at }
            | Spell::Swamp { at }
            | Spell::Convert { at }
            | Spell::Hypnotism { at }
            | Spell::Volcano { at }
            | Spell::Firestorm { at }
            | Spell::Earthquake { at }
            | Spell::MagicalShield { at }
            | Spell::Bloodlust { at }
            | Spell::Invisibility { at }
            | Spell::Swarm { at }
            | Spell::AngelOfDeath { at } = spell
            {
                b.push(match spell {
                    Spell::Teleport { .. } => 4,
                    Spell::Blast { .. } => 5,
                    Spell::Armageddon { .. } => 7,
                    Spell::Swamp { .. } => 8,
                    Spell::Convert { .. } => 9,
                    Spell::Hypnotism { .. } => 10,
                    Spell::Volcano { .. } => 11,
                    Spell::Firestorm { .. } => 12,
                    Spell::Earthquake { .. } => 13,
                    Spell::MagicalShield { .. } => 14,
                    Spell::Bloodlust { .. } => 15,
                    Spell::Invisibility { .. } => 16,
                    Spell::Swarm { .. } => 17,
                    Spell::AngelOfDeath { .. } => 18,
                    _ => 6,
                });
                b.extend(at.0.to_le_bytes());
                b.extend(at.1.to_le_bytes());
                return;
            }
            let (tag, cells): (u8, &[(i32, i32)]) = match spell {
                Spell::LandBridge { from, to } => (0, &[*from, *to]),
                Spell::Flatten { at } => (1, std::slice::from_ref(at)),
                Spell::Erode { at } => (2, std::slice::from_ref(at)),
                Spell::Raise { at } => (3, std::slice::from_ref(at)),
                Spell::Teleport { .. }
                | Spell::Blast { .. }
                | Spell::Lightning { .. }
                | Spell::Armageddon { .. }
                | Spell::Swamp { .. }
                | Spell::Convert { .. }
                | Spell::Hypnotism { .. }
                | Spell::Volcano { .. }
                | Spell::Firestorm { .. }
                | Spell::Earthquake { .. }
                | Spell::MagicalShield { .. }
                | Spell::Bloodlust { .. }
                | Spell::Invisibility { .. }
                | Spell::Swarm { .. }
                | Spell::AngelOfDeath { .. } => unreachable!("encoded above"),
            };
            b.push(tag);
            for (x, z) in cells {
                b.extend((*x as i16).to_le_bytes());
                b.extend((*z as i16).to_le_bytes());
            }
        }
        Command::Order { player, order } => {
            b.extend([1, player.as_u8()]);
            encode_order(b, order);
        }
        Command::OrderUnit { player, unit, order } => {
            b.extend([2, player.as_u8()]);
            put_id(b, *unit);
            encode_order(b, order);
        }
        Command::PlaceCampfire { player, at } => {
            b.extend([3, player.as_u8()]);
            b.extend(at.0.to_le_bytes());
            b.extend(at.1.to_le_bytes());
        }
        Command::RemoveCampfire { player, at } => {
            b.extend([4, player.as_u8()]);
            b.extend(at.0.to_le_bytes());
            b.extend(at.1.to_le_bytes());
        }
        Command::QueueOrder { player, unit, order } => {
            b.extend([5, player.as_u8()]);
            put_id(b, *unit);
            encode_order(b, order);
        }
        Command::PlaceBuilding { player, kind, at, facing } => {
            b.extend([6, player.as_u8(), kind.model()]);
            b.extend(at.0.to_le_bytes());
            b.extend(at.1.to_le_bytes());
            b.push(*facing);
        }
        Command::CancelBuilding { player, at } => {
            b.extend([7, player.as_u8()]);
            b.extend(at.0.to_le_bytes());
            b.extend(at.1.to_le_bytes());
        }
        Command::Dismantle { player, building, on } => {
            b.extend([8, player.as_u8()]);
            put_id(b, *building);
            b.push(*on as u8);
        }
        Command::CastSelf { player, kind } => b.extend([9, player.as_u8(), spell_index(*kind)]),
        Command::PauseSpell { player, kind, paused } => b.extend([10, player.as_u8(), spell_index(*kind), *paused as u8]),
        Command::MoveVehicle { player, vehicle, to } => {
            b.extend([11, player.as_u8()]);
            put_id(b, *vehicle);
            b.extend(to.0.to_le_bytes());
            b.extend(to.1.to_le_bytes());
        }
        Command::Unload { player, vehicle } => {
            b.extend([12, player.as_u8()]);
            put_id(b, *vehicle);
        }
        Command::ClearSwamp { player, swamp } => {
            b.extend([13, player.as_u8()]);
            put_id(b, *swamp);
        }
        Command::SetPatrol { player, units, at } => {
            b.extend([14, player.as_u8()]);
            put_units(b, units);
            b.extend(at.0.to_le_bytes());
            b.extend(at.1.to_le_bytes());
        }
        Command::SetPatrolTarget { player, units, target } => {
            b.extend([15, player.as_u8()]);
            put_units(b, units);
            put_target(b, *target);
        }
        Command::AddPatrolPoint { player, units, at } => {
            b.extend([16, player.as_u8()]);
            put_units(b, units);
            b.extend(at.0.to_le_bytes());
            b.extend(at.1.to_le_bytes());
        }
        Command::AddPatrolTargetPoint { player, units, target } => {
            b.extend([17, player.as_u8()]);
            put_units(b, units);
            put_target(b, *target);
        }
        Command::ReleasePatrol { player, units } => {
            b.extend([18, player.as_u8()]);
            put_units(b, units);
        }
        Command::Formation { player, units, to } => {
            b.extend([19, player.as_u8()]);
            put_units(b, units);
            b.extend(to.0.to_le_bytes());
            b.extend(to.1.to_le_bytes());
        }
        Command::CancelFlyby { player } => {
            b.extend([20, player.as_u8()]);
        }
        Command::CancelGuard { player } => {
            b.extend([21, player.as_u8()]);
        }
        Command::PlaceTree { player, at } => {
            b.extend([22, player.as_u8()]);
            b.extend(at.0.to_le_bytes());
            b.extend(at.1.to_le_bytes());
        }
    }
}

/// An id on the wire: `u32` LE.
fn put_id<T>(b: &mut Vec<u8>, id: Id<T>) {
    b.extend(id.raw().to_le_bytes());
}

/// Where `Order::Worship` prays, on the wire: tag `u8` (0 a vault's stored corner, 1 a totem's id).
fn put_worship_at(b: &mut Vec<u8>, at: WorshipAt) {
    match at {
        WorshipAt::Vault { corner } => {
            b.push(0);
            b.extend(corner.0.to_le_bytes());
            b.extend(corner.1.to_le_bytes());
        }
        WorshipAt::Totem { id } => {
            b.push(1);
            put_id(b, id);
        }
    }
}

/// A unit list on the wire: `u8` count, then that many `u32` ids.
fn put_units(b: &mut Vec<u8>, units: &[UnitId]) {
    b.push(units.len() as u8);
    for &unit in units {
        put_id(b, unit);
    }
}

/// An attack target on the wire: tag `u8` (0 unit, 1 building) + `u32` id.
fn put_target(b: &mut Vec<u8>, target: Target) {
    match target {
        Target::Unit(id) => {
            b.push(0);
            put_id(b, id);
        }
        Target::Building(id) => {
            b.push(1);
            put_id(b, id);
        }
    }
}

/// A spell kind as its place in `SpellKind::ALL` (Teleport has no panel model).
fn spell_index(kind: SpellKind) -> u8 {
    SpellKind::ALL.iter().position(|&k| k == kind).unwrap() as u8
}

fn encode_order(b: &mut Vec<u8>, order: &Order) {
    match order {
        Order::MoveTo { x, z } => {
            b.push(0);
            b.extend(x.to_le_bytes());
            b.extend(z.to_le_bytes());
        }
        Order::Cast => b.push(2),
        Order::Stop => b.push(3),
        Order::Campfire { fire, point } => {
            b.push(4);
            b.extend(fire.0.to_le_bytes());
            b.extend(fire.1.to_le_bytes());
            b.push(*point);
        }
        Order::CutTree { tree } => {
            b.push(5);
            b.extend(tree.0.to_le_bytes());
            b.extend(tree.1.to_le_bytes());
        }
        Order::FetchWood => b.push(6),
        Order::PickUp { at } => {
            b.push(7);
            b.extend(at.0.to_le_bytes());
            b.extend(at.1.to_le_bytes());
        }
        Order::Build { site } => {
            b.push(8);
            b.extend(site.0.to_le_bytes());
            b.extend(site.1.to_le_bytes());
        }
        Order::Enter { site } => {
            b.push(9);
            b.extend(site.0.to_le_bytes());
            b.extend(site.1.to_le_bytes());
        }
        Order::Worship { at } => {
            b.push(10);
            put_worship_at(b, *at);
        }
        Order::Attack { target: Target::Unit(id) } => {
            b.push(11);
            put_id(b, *id);
        }
        Order::Attack { target: Target::Building(id) } => {
            b.push(12);
            put_id(b, *id);
        }
        Order::Board { vehicle } => {
            b.push(13);
            put_id(b, *vehicle);
        }
        Order::GetOff => b.push(14),
        Order::Guard => b.push(15),
    }
}

fn decode_command(c: &mut Cursor) -> Option<Command> {
    let (kind, player) = (c.u8()?, TribeId::from_u8(c.u8()?)?);
    match kind {
        0 => {
            let spell = match c.u8()? {
                0 => Spell::LandBridge { from: c.cell()?, to: c.cell()? },
                1 => Spell::Flatten { at: c.cell()? },
                2 => Spell::Erode { at: c.cell()? },
                3 => Spell::Raise { at: c.cell()? },
                4 => Spell::Teleport { to: (c.u16()?, c.u16()?) },
                5 => Spell::Blast { at: (c.u16()?, c.u16()?) },
                6 => Spell::Lightning { at: (c.u16()?, c.u16()?) },
                7 => Spell::Armageddon { at: (c.u16()?, c.u16()?) },
                8 => Spell::Swamp { at: (c.u16()?, c.u16()?) },
                9 => Spell::Convert { at: (c.u16()?, c.u16()?) },
                10 => Spell::Hypnotism { at: (c.u16()?, c.u16()?) },
                11 => Spell::Volcano { at: (c.u16()?, c.u16()?) },
                12 => Spell::Firestorm { at: (c.u16()?, c.u16()?) },
                13 => Spell::Earthquake { at: (c.u16()?, c.u16()?) },
                14 => Spell::MagicalShield { at: (c.u16()?, c.u16()?) },
                15 => Spell::Bloodlust { at: (c.u16()?, c.u16()?) },
                16 => Spell::Invisibility { at: (c.u16()?, c.u16()?) },
                17 => Spell::Swarm { at: (c.u16()?, c.u16()?) },
                18 => Spell::AngelOfDeath { at: (c.u16()?, c.u16()?) },
                _ => return None,
            };
            Some(Command::Cast { player, spell })
        }
        1 => Some(Command::Order { player, order: decode_order(c)? }),
        2 => Some(Command::OrderUnit { player, unit: c.id()?, order: decode_order(c)? }),
        3 => Some(Command::PlaceCampfire { player, at: (c.u16()?, c.u16()?) }),
        4 => Some(Command::RemoveCampfire { player, at: (c.u16()?, c.u16()?) }),
        5 => Some(Command::QueueOrder { player, unit: c.id()?, order: decode_order(c)? }),
        6 => Some(Command::PlaceBuilding { player, kind: BuildingKind::from_model(c.u8()?), at: (c.u16()?, c.u16()?), facing: c.u8()? }),
        7 => Some(Command::CancelBuilding { player, at: (c.u16()?, c.u16()?) }),
        8 => Some(Command::Dismantle { player, building: c.id()?, on: c.u8()? != 0 }),
        9 => Some(Command::CastSelf { player, kind: *SpellKind::ALL.get(c.u8()? as usize)? }),
        10 => Some(Command::PauseSpell { player, kind: *SpellKind::ALL.get(c.u8()? as usize)?, paused: c.u8()? != 0 }),
        11 => Some(Command::MoveVehicle { player, vehicle: c.id()?, to: (c.u16()?, c.u16()?) }),
        12 => Some(Command::Unload { player, vehicle: c.id()? }),
        13 => Some(Command::ClearSwamp { player, swamp: c.id()? }),
        14 => Some(Command::SetPatrol { player, units: get_units(c)?, at: (c.u16()?, c.u16()?) }),
        15 => Some(Command::SetPatrolTarget { player, units: get_units(c)?, target: get_target(c)? }),
        16 => Some(Command::AddPatrolPoint { player, units: get_units(c)?, at: (c.u16()?, c.u16()?) }),
        17 => Some(Command::AddPatrolTargetPoint { player, units: get_units(c)?, target: get_target(c)? }),
        18 => Some(Command::ReleasePatrol { player, units: get_units(c)? }),
        19 => Some(Command::Formation { player, units: get_units(c)?, to: (c.u16()?, c.u16()?) }),
        20 => Some(Command::CancelFlyby { player }),
        21 => Some(Command::CancelGuard { player }),
        22 => Some(Command::PlaceTree { player, at: (c.u16()?, c.u16()?) }),
        _ => None,
    }
}

/// A unit list from the wire: `u8` count, then that many `u32` ids.
fn get_units(c: &mut Cursor) -> Option<Vec<UnitId>> {
    let n = c.u8()?;
    (0..n).map(|_| c.id()).collect()
}

/// An attack target from the wire: tag `u8` (0 unit, 1 building) + `u32` id.
fn get_target(c: &mut Cursor) -> Option<Target> {
    match c.u8()? {
        0 => Some(Target::Unit(c.id()?)),
        1 => Some(Target::Building(c.id()?)),
        _ => None,
    }
}

/// Where `Order::Worship` prays, from the wire: tag `u8` (0 a vault's stored corner, 1 a totem's id).
fn get_worship_at(c: &mut Cursor) -> Option<WorshipAt> {
    match c.u8()? {
        0 => Some(WorshipAt::Vault { corner: (c.u16()?, c.u16()?) }),
        1 => Some(WorshipAt::Totem { id: c.id()? }),
        _ => None,
    }
}

fn decode_order(c: &mut Cursor) -> Option<Order> {
    Some(match c.u8()? {
        0 => Order::MoveTo { x: c.u16()?, z: c.u16()? },
        2 => Order::Cast,
        3 => Order::Stop,
        4 => Order::Campfire { fire: (c.u16()?, c.u16()?), point: c.u8()? },
        5 => Order::CutTree { tree: (c.u16()?, c.u16()?) },
        6 => Order::FetchWood,
        7 => Order::PickUp { at: (c.u16()?, c.u16()?) },
        8 => Order::Build { site: (c.u16()?, c.u16()?) },
        9 => Order::Enter { site: (c.u16()?, c.u16()?) },
        10 => Order::Worship { at: get_worship_at(c)? },
        11 => Order::Attack { target: Target::Unit(c.id()?) },
        12 => Order::Attack { target: Target::Building(c.id()?) },
        13 => Order::Board { vehicle: c.id()? },
        14 => Order::GetOff,
        15 => Order::Guard,
        _ => return None,
    })
}

pub fn decode(b: &[u8]) -> Option<Message> {
    let mut c = Cursor(b);
    match c.u8()? {
        0 => Some(Message::Hello { player: TribeId::from_u8(c.u8()?)? }),
        2 => Some(Message::Start { seed: c.u32()? }),
        1 => {
            let tick = Tick::from_wire(u32::from_le_bytes(c.take(4)?.try_into().ok()?));
            let n = c.u8()?;
            let mut commands = Vec::with_capacity(n as usize);
            for _ in 0..n {
                commands.push(decode_command(&mut c)?);
            }
            Some(Message::Tick { tick, commands })
        }
        3 => Some(Message::Handshake { constants_hash: c.u64()? }),
        _ => None,
    }
}

struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Some(head)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn id<T>(&mut self) -> Option<Id<T>> {
        self.u32().map(Id::from_raw)
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    fn i16(&mut self) -> Option<i32> {
        self.take(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as i32)
    }
    fn cell(&mut self) -> Option<(i32, i32)> {
        Some((self.i16()?, self.i16()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};

    fn sample_turn() -> Message {
        Message::Tick {
            tick: Tick::from_wire(7),
            commands: vec![
                Command::Cast { player: TribeId::Red, spell: Spell::LandBridge { from: (1, 2), to: (120, -3) } },
                Command::Cast { player: TribeId::Blue, spell: Spell::Flatten { at: (5, 6) } },
                Command::Cast { player: TribeId::Blue, spell: Spell::Teleport { to: (65535, 7) } },
                Command::Cast { player: TribeId::Green, spell: Spell::Blast { at: (9, 65535) } },
                Command::Cast { player: TribeId::Yellow, spell: Spell::Lightning { at: (512, 7) } },
                Command::Cast { player: TribeId::Green, spell: Spell::Armageddon { at: (1, 65535) } },
                Command::Cast { player: TribeId::Red, spell: Spell::Convert { at: (100, 65500) } },
                Command::Cast { player: TribeId::Yellow, spell: Spell::Hypnotism { at: (65500, 300) } },
                Command::Cast { player: TribeId::Red, spell: Spell::Volcano { at: (12, 34) } },
                Command::Cast { player: TribeId::Yellow, spell: Spell::Firestorm { at: (56, 78) } },
                Command::Cast { player: TribeId::Green, spell: Spell::Earthquake { at: (90, 11) } },
                Command::Cast { player: TribeId::Blue, spell: Spell::MagicalShield { at: (23, 45) } },
                Command::Cast { player: TribeId::Red, spell: Spell::Bloodlust { at: (65, 87) } },
                Command::Cast { player: TribeId::Yellow, spell: Spell::Invisibility { at: (109, 131) } },
                Command::Cast { player: TribeId::Blue, spell: Spell::AngelOfDeath { at: (321, 654) } },
                Command::Order { player: TribeId::Yellow, order: Order::MoveTo { x: 65535, z: 300 } },
                Command::Order { player: TribeId::Blue, order: Order::Cast },
                Command::Order { player: TribeId::Red, order: Order::Stop },
                Command::OrderUnit { player: TribeId::Red, unit: Id::from_raw(70_000), order: Order::MoveTo { x: 3, z: 4 } },
                Command::OrderUnit { player: TribeId::Blue, unit: Id::from_raw(9), order: Order::Campfire { fire: (65535, 256), point: 15 } },
                Command::PlaceCampfire { player: TribeId::Green, at: (40_000, 7) },
                Command::RemoveCampfire { player: TribeId::Red, at: (8, 65535) },
                Command::QueueOrder { player: TribeId::Yellow, unit: Id::from_raw(12), order: Order::Stop },
                Command::OrderUnit { player: TribeId::Blue, unit: Id::from_raw(3), order: Order::CutTree { tree: (65535, 1) } },
                Command::QueueOrder { player: TribeId::Red, unit: Id::from_raw(4), order: Order::FetchWood },
                Command::OrderUnit { player: TribeId::Yellow, unit: Id::from_raw(5), order: Order::PickUp { at: (7, 65535) } },
                Command::PlaceBuilding { player: TribeId::Red, kind: BuildingKind::AirshipHut, at: (512, 65024), facing: 6 },
                Command::PlaceBuilding { player: TribeId::Blue, kind: BuildingKind::Hut { size: 1 }, at: (0, 1024), facing: 0 },
                Command::OrderUnit { player: TribeId::Red, unit: Id::from_raw(9), order: Order::Build { site: (512, 65024) } },
                Command::CancelBuilding { player: TribeId::Green, at: (1, 2) },
                Command::Dismantle { player: TribeId::Yellow, building: Id::from_raw(u32::MAX), on: true },
                Command::Dismantle { player: TribeId::Blue, building: Id::from_raw(7), on: false },
                Command::QueueOrder { player: TribeId::Blue, unit: Id::from_raw(3), order: Order::Enter { site: (1024, 2048) } },
                Command::OrderUnit { player: TribeId::Blue, unit: Id::from_raw(1), order: Order::Worship { at: WorshipAt::Vault { corner: (32768, 36864) } } },
                Command::QueueOrder { player: TribeId::Red, unit: Id::from_raw(2), order: Order::Worship { at: WorshipAt::Totem { id: Id::from_raw(42) } } },
                Command::OrderUnit { player: TribeId::Blue, unit: Id::from_raw(1), order: Order::Attack { target: Target::Unit(Id::from_raw(70_000)) } },
                Command::QueueOrder { player: TribeId::Blue, unit: Id::from_raw(2), order: Order::Attack { target: Target::Building(Id::from_raw(9)) } },
                Command::CastSelf { player: TribeId::Red, kind: SpellKind::Teleport },
                Command::PauseSpell { player: TribeId::Green, kind: SpellKind::Armageddon, paused: true },
                Command::OrderUnit { player: TribeId::Red, unit: Id::from_raw(4), order: Order::Board { vehicle: Id::from_raw(70_001) } },
                Command::MoveVehicle { player: TribeId::Yellow, vehicle: Id::from_raw(8), to: (65535, 3) },
                Command::Unload { player: TribeId::Blue, vehicle: Id::from_raw(u32::MAX) },
                Command::OrderUnit { player: TribeId::Green, unit: Id::from_raw(6), order: Order::GetOff },
                Command::SetPatrol { player: TribeId::Blue, units: vec![Id::from_raw(1), Id::from_raw(2), Id::from_raw(3)], at: (65535, 7) },
                Command::SetPatrolTarget { player: TribeId::Red, units: vec![Id::from_raw(4)], target: Target::Unit(Id::from_raw(70_000)) },
                Command::AddPatrolPoint { player: TribeId::Yellow, units: Vec::new(), at: (0, 65535) },
                Command::AddPatrolTargetPoint { player: TribeId::Green, units: vec![Id::from_raw(5), Id::from_raw(6)], target: Target::Building(Id::from_raw(9)) },
                Command::ReleasePatrol { player: TribeId::Red, units: vec![Id::from_raw(1), Id::from_raw(2), Id::from_raw(3)] },
                Command::Formation { player: TribeId::Blue, units: vec![Id::from_raw(1), Id::from_raw(2)], to: (65535, 512) },
                Command::CancelFlyby { player: TribeId::Yellow },
                Command::OrderUnit { player: TribeId::Blue, unit: Id::from_raw(1), order: Order::Guard },
                Command::CancelGuard { player: TribeId::Red },
                Command::PlaceTree { player: TribeId::Blue, at: (40_000, 7) },
            ],
        }
    }

    #[test]
    fn roundtrip_encoding() {
        let m = sample_turn();
        assert_eq!(decode(&encode(&m)), Some(m));
        assert_eq!(decode(&[9]), None);
        let start = Message::Start { seed: 0xDEAD_BEEF };
        assert_eq!(decode(&encode(&start)), Some(start));
        let handshake = Message::Handshake { constants_hash: 0x1234_5678_9ABC_DEF0 };
        assert_eq!(decode(&encode(&handshake)), Some(handshake));
    }

    #[test]
    fn constants_match_is_a_plain_equality() {
        assert!(constants_match(42, 42));
        assert!(!constants_match(42, 43));
    }

    #[test]
    fn roundtrip_over_tcp() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut s = TcpStream::connect(addr).unwrap();
            send(&mut s, &Message::Hello { player: TribeId::Red }).unwrap();
            send(&mut s, &sample_turn()).unwrap();
        });
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(recv(&mut s).unwrap(), Message::Hello { player: TribeId::Red });
        assert_eq!(recv(&mut s).unwrap(), sample_turn());
        client.join().unwrap();
    }
}
