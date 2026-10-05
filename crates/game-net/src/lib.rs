//! Deterministic lockstep over TCP: peers only exchange per-turn command lists.
//!
//! Wire format: `u32 LE length` + payload. Payload starts with a tag byte.
//! See docs/specs/multiplayer.md.

use game_core::command::Command;
use game_core::spell::Spell;
use std::io::{self, Read, Write};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Hello { player: u8 },
    /// All commands a player issues for one simulation turn (possibly none).
    Turn { turn: u32, commands: Vec<Command> },
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
        Message::Turn { turn, commands } => {
            b.push(1);
            b.extend(turn.to_le_bytes());
            b.push(commands.len() as u8);
            for Command::Cast { player, spell } in commands {
                b.push(*player);
                let (tag, cells): (u8, &[(i32, i32)]) = match spell {
                    Spell::LandBridge { from, to } => (0, &[*from, *to]),
                    Spell::Flatten { at } => (1, std::slice::from_ref(at)),
                    Spell::Erode { at } => (2, std::slice::from_ref(at)),
                    Spell::Raise { at } => (3, std::slice::from_ref(at)),
                };
                b.push(tag);
                for (x, z) in cells {
                    b.extend((*x as i16).to_le_bytes());
                    b.extend((*z as i16).to_le_bytes());
                }
            }
        }
    }
    b
}

pub fn decode(b: &[u8]) -> Option<Message> {
    let mut c = Cursor(b);
    match c.u8()? {
        0 => Some(Message::Hello { player: c.u8()? }),
        1 => {
            let turn = u32::from_le_bytes(c.take(4)?.try_into().ok()?);
            let n = c.u8()?;
            let mut commands = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let player = c.u8()?;
                let spell = match c.u8()? {
                    0 => Spell::LandBridge { from: c.cell()?, to: c.cell()? },
                    1 => Spell::Flatten { at: c.cell()? },
                    2 => Spell::Erode { at: c.cell()? },
                    3 => Spell::Raise { at: c.cell()? },
                    _ => return None,
                };
                commands.push(Command::Cast { player, spell });
            }
            Some(Message::Turn { turn, commands })
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
        Message::Turn {
            turn: 7,
            commands: vec![
                Command::Cast { player: 1, spell: Spell::LandBridge { from: (1, 2), to: (120, -3) } },
                Command::Cast { player: 0, spell: Spell::Flatten { at: (5, 6) } },
            ],
        }
    }

    #[test]
    fn roundtrip_encoding() {
        let m = sample_turn();
        assert_eq!(decode(&encode(&m)), Some(m));
        assert_eq!(decode(&[9]), None);
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
