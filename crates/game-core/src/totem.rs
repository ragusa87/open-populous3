//! Totems: the places followers (or only the shaman) pray at for a reward (docs/specs/worship.md).
//! Only where they stand and which look they have for now; which original model the levels' totems
//! (scenery 9) use is not known, so the Worship sandbox shows every candidate.

/// The original objects that look like something to pray at (objects.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TotemKind {
    /// Totem with rotating rocks.
    Totem,
    /// Totem of the winged death, a bird perched on it.
    WingedDeath,
    /// Stone prayer totem.
    Prayer,
    StoneHead,
    /// Totem pole, three variants.
    Pole(u8),
}

impl TotemKind {
    pub const ALL: [TotemKind; 7] = [TotemKind::Totem, TotemKind::WingedDeath, TotemKind::Prayer, TotemKind::StoneHead, TotemKind::Pole(0), TotemKind::Pole(1), TotemKind::Pole(2)];

    pub fn name(self) -> &'static str {
        match self {
            TotemKind::Totem => "Totem",
            TotemKind::WingedDeath => "Winged death totem",
            TotemKind::Prayer => "Prayer totem",
            TotemKind::StoneHead => "Stone head",
            TotemKind::Pole(_) => "Totem pole",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Totem {
    pub kind: TotemKind,
    /// Centre, world units (512 per cell).
    pub x: u16,
    pub z: u16,
}
