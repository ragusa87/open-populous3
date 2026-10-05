//! Units (shaman, braves, warriors...). Placeholder: only the data shape.
//! Positions are fixed-point (1 cell = 512 units, like the original), wrapping at 65536.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    Shaman,
    Brave,
    Warrior,
    Preacher,
    Spy,
    Firewarrior,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub id: u32,
    pub owner: u8,
    pub kind: UnitKind,
    pub x: u16,
    pub z: u16,
}
