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
use game_core::unit::{Order, Target};
use std::io::{self, Read, Write};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Hello { player: u8 },
    /// From the host before the first turn: the game's seed (`GameMap::seeded`), the same for every peer.
    Start { seed: u32 },
    /// All commands a player issues for one simulation tick (possibly none): its turn.
    Tick { tick: Tick, commands: Vec<Command> },
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
        Message::Hello { player } => b.extend([0, *player]),
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
    }
    b
}

/// `kind u8, player u8`, then for a cast `spell tag u8 + i16 cells` (Teleport, Blast, Lightning, Armageddon, Swamp: `u16 x, z`), for an order `order tag u8 [+ u16 x, z]`,
/// for a unit order `u32 unit id` then the order, for lighting or putting out a camp fire `u16 x, z`,
/// for placing a building `model u8, u16 x, z, facing u8`, for cancelling one `u16 x, z`, for a self cast `spell u8`
/// (its place in `SpellKind::ALL`), for pausing one `spell u8, paused u8`, for moving a vehicle `u32 vehicle id, u16 x, z`,
/// for unloading one `u32 vehicle id`, for clearing a swamp `u32 effect id`.
fn encode_command(b: &mut Vec<u8>, command: &Command) {
    match command {
        Command::Cast { player, spell } => {
            b.extend([0, *player]);
            if let Spell::Teleport { to: at } | Spell::Blast { at } | Spell::Lightning { at } | Spell::Armageddon { at } | Spell::Swamp { at } = spell {
                b.push(match spell {
                    Spell::Teleport { .. } => 4,
                    Spell::Blast { .. } => 5,
                    Spell::Armageddon { .. } => 7,
                    Spell::Swamp { .. } => 8,
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
                Spell::Teleport { .. } | Spell::Blast { .. } | Spell::Lightning { .. } | Spell::Armageddon { .. } | Spell::Swamp { .. } => unreachable!("encoded above"),
                Spell::MagicalShield | Spell::Bloodlust | Spell::Invisibility => unreachable!("self-cast: never built from a Command::Cast, see Command::CastSelf"),
            };
            b.push(tag);
            for (x, z) in cells {
                b.extend((*x as i16).to_le_bytes());
                b.extend((*z as i16).to_le_bytes());
            }
        }
        Command::Order { player, order } => {
            b.extend([1, *player]);
            encode_order(b, order);
        }
        Command::OrderUnit { player, unit, order } => {
            b.extend([2, *player]);
            put_id(b, *unit);
            encode_order(b, order);
        }
        Command::PlaceCampfire { player, at } => {
            b.extend([3, *player]);
            b.extend(at.0.to_le_bytes());
            b.extend(at.1.to_le_bytes());
        }
        Command::RemoveCampfire { player, at } => {
            b.extend([4, *player]);
            b.extend(at.0.to_le_bytes());
            b.extend(at.1.to_le_bytes());
        }
        Command::QueueOrder { player, unit, order } => {
            b.extend([5, *player]);
            put_id(b, *unit);
            encode_order(b, order);
        }
        Command::PlaceBuilding { player, kind, at, facing } => {
            b.extend([6, *player, kind.model()]);
            b.extend(at.0.to_le_bytes());
            b.extend(at.1.to_le_bytes());
            b.push(*facing);
        }
        Command::CancelBuilding { player, at } => {
            b.extend([7, *player]);
            b.extend(at.0.to_le_bytes());
            b.extend(at.1.to_le_bytes());
        }
        Command::Dismantle { player, building, on } => {
            b.extend([8, *player]);
            put_id(b, *building);
            b.push(*on as u8);
        }
        Command::CastSelf { player, kind } => b.extend([9, *player, spell_index(*kind)]),
        Command::PauseSpell { player, kind, paused } => b.extend([10, *player, spell_index(*kind), *paused as u8]),
        Command::MoveVehicle { player, vehicle, to } => {
            b.extend([11, *player]);
            put_id(b, *vehicle);
            b.extend(to.0.to_le_bytes());
            b.extend(to.1.to_le_bytes());
        }
        Command::Unload { player, vehicle } => {
            b.extend([12, *player]);
            put_id(b, *vehicle);
        }
        Command::ClearSwamp { player, swamp } => {
            b.extend([13, *player]);
            put_id(b, *swamp);
        }
    }
}

/// An id on the wire: `u32` LE.
fn put_id<T>(b: &mut Vec<u8>, id: Id<T>) {
    b.extend(id.raw().to_le_bytes());
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
        Order::Worship { site } => {
            b.push(10);
            b.extend(site.0.to_le_bytes());
            b.extend(site.1.to_le_bytes());
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
    }
}

fn decode_command(c: &mut Cursor) -> Option<Command> {
    let (kind, player) = (c.u8()?, c.u8()?);
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
        10 => Order::Worship { site: (c.u16()?, c.u16()?) },
        11 => Order::Attack { target: Target::Unit(c.id()?) },
        12 => Order::Attack { target: Target::Building(c.id()?) },
        13 => Order::Board { vehicle: c.id()? },
        14 => Order::GetOff,
        _ => return None,
    })
}

pub fn decode(b: &[u8]) -> Option<Message> {
    let mut c = Cursor(b);
    match c.u8()? {
        0 => Some(Message::Hello { player: c.u8()? }),
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
                Command::Cast { player: 1, spell: Spell::LandBridge { from: (1, 2), to: (120, -3) } },
                Command::Cast { player: 0, spell: Spell::Flatten { at: (5, 6) } },
                Command::Cast { player: 0, spell: Spell::Teleport { to: (65535, 7) } },
                Command::Cast { player: 3, spell: Spell::Blast { at: (9, 65535) } },
                Command::Cast { player: 2, spell: Spell::Lightning { at: (512, 7) } },
                Command::Cast { player: 3, spell: Spell::Armageddon { at: (1, 65535) } },
                Command::Order { player: 2, order: Order::MoveTo { x: 65535, z: 300 } },
                Command::Order { player: 0, order: Order::Cast },
                Command::Order { player: 1, order: Order::Stop },
                Command::OrderUnit { player: 1, unit: Id::from_raw(70_000), order: Order::MoveTo { x: 3, z: 4 } },
                Command::OrderUnit { player: 0, unit: Id::from_raw(9), order: Order::Campfire { fire: (65535, 256), point: 15 } },
                Command::PlaceCampfire { player: 3, at: (40_000, 7) },
                Command::RemoveCampfire { player: 1, at: (8, 65535) },
                Command::QueueOrder { player: 2, unit: Id::from_raw(12), order: Order::Stop },
                Command::OrderUnit { player: 0, unit: Id::from_raw(3), order: Order::CutTree { tree: (65535, 1) } },
                Command::QueueOrder { player: 1, unit: Id::from_raw(4), order: Order::FetchWood },
                Command::OrderUnit { player: 2, unit: Id::from_raw(5), order: Order::PickUp { at: (7, 65535) } },
                Command::PlaceBuilding { player: 1, kind: BuildingKind::AirshipHut, at: (512, 65024), facing: 6 },
                Command::PlaceBuilding { player: 0, kind: BuildingKind::Hut { size: 1 }, at: (0, 1024), facing: 0 },
                Command::OrderUnit { player: 1, unit: Id::from_raw(9), order: Order::Build { site: (512, 65024) } },
                Command::CancelBuilding { player: 3, at: (1, 2) },
                Command::Dismantle { player: 2, building: Id::from_raw(u32::MAX), on: true },
                Command::Dismantle { player: 0, building: Id::from_raw(7), on: false },
                Command::QueueOrder { player: 0, unit: Id::from_raw(3), order: Order::Enter { site: (1024, 2048) } },
                Command::OrderUnit { player: 0, unit: Id::from_raw(1), order: Order::Worship { site: (32768, 36864) } },
                Command::OrderUnit { player: 0, unit: Id::from_raw(1), order: Order::Attack { target: Target::Unit(Id::from_raw(70_000)) } },
                Command::QueueOrder { player: 0, unit: Id::from_raw(2), order: Order::Attack { target: Target::Building(Id::from_raw(9)) } },
                Command::CastSelf { player: 1, kind: SpellKind::Teleport },
                Command::PauseSpell { player: 3, kind: SpellKind::Armageddon, paused: true },
                Command::OrderUnit { player: 1, unit: Id::from_raw(4), order: Order::Board { vehicle: Id::from_raw(70_001) } },
                Command::MoveVehicle { player: 2, vehicle: Id::from_raw(8), to: (65535, 3) },
                Command::Unload { player: 0, vehicle: Id::from_raw(u32::MAX) },
                Command::OrderUnit { player: 3, unit: Id::from_raw(6), order: Order::GetOff },
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
    }

    #[test]
    fn roundtrip_over_tcp() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut s = TcpStream::connect(addr).unwrap();
            send(&mut s, &Message::Hello { player: 1 }).unwrap();
            send(&mut s, &sample_turn()).unwrap();
        });
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(recv(&mut s).unwrap(), Message::Hello { player: 1 });
        assert_eq!(recv(&mut s).unwrap(), sample_turn());
        client.join().unwrap();
    }
}
