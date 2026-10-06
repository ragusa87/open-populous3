//! What the bank 0 objects are (identified by hand, see docs/specs/objects.md), and how the
//! tribe-coloured variants are laid out. Tribe order: 0 blue, 1 red, 2 yellow, 3 green.

pub const TRIBES: u8 = 4;

/// Camp fire (probably): wood at the base, a flat board the flames are drawn on.
/// 12 looks identical but smaller (another frame or size, unconfirmed).
pub const CAMP_FIRE: usize = 0;
/// Totem (textured; 2 and 4 are untextured copies, 3 the rock-rotation animation frame).
pub const TOTEM: usize = 1;
pub const TOTEM_ANIMATED: usize = 3;
/// French "mort ailée" (winged death, probably the Angel of Death): 7 front, 100 back,
/// 107 wings oriented differently. 8-11, 101-106 and 108-116 are untextured copies.
pub const WINGED_DEATH: usize = 7;
pub const WINGED_DEATH_BACK: usize = 100;
pub const WINGED_DEATH_ALT: usize = 107;
/// Totem of the winged death, and the bird perched on top of it.
pub const WINGED_DEATH_TOTEM: usize = 19;
pub const WINGED_DEATH_PERCHED: usize = 21;
/// Stone prayer totem.
pub const PRAYER_TOTEM: usize = 20;
/// Stone placed around a reincarnation site (tribe-coloured, blue in the file).
pub const REINCARNATION_STONE: usize = 30;
pub const BOOK: usize = 31;
pub const SHIELD: usize = 32;
/// Trees: 60..=71.
pub const TREES: std::ops::RangeInclusive<usize> = 60..=71;
/// Stone head (83, 84 are untextured copies).
pub const STONE_HEAD: usize = 82;
/// Prison holding the shaman until freed (some levels).
pub const PRISON: usize = 94;
/// Totem poles (three variants).
pub const TOTEM_POLES: [usize; 3] = [187, 188, 189];
/// Pyramid of knowledge: unlocks a spell or a building. 192-193 are door animation frames.
pub const KNOWLEDGE_PYRAMID: usize = 191;
pub const KNOWLEDGE_PYRAMID_DOOR: std::ops::RangeInclusive<usize> = 192..=193;
/// Boat (blue) and airship.
pub const BOAT: usize = 181;
pub const AIRSHIP: usize = 182;

/// Buildings stored as 4 consecutive objects, one per tribe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Building {
    DrumTower,
    BoatHut,
    AirshipHut,
    SpyHut,
    PrayerHut,
    FirewarriorTraining,
    WarriorTraining,
}

impl Building {
    fn blue(self) -> usize {
        match self {
            Building::DrumTower => 117,
            Building::BoatHut => 121,
            Building::AirshipHut => 125,
            Building::SpyHut => 129,
            Building::PrayerHut => 133,
            Building::FirewarriorTraining => 137,
            Building::WarriorTraining => 141,
        }
    }

    pub fn object(self, tribe: u8) -> usize {
        self.blue() + (tribe % TRIBES) as usize
    }
}

/// Villager hut: 3 styles x 4 tribes x 3 sizes (simple, double, triple), from object 145.
pub fn villager_hut(style: u8, tribe: u8, size: u8) -> usize {
    145 + (style % 3) as usize * 12 + (tribe % TRIBES) as usize * 3 + size.clamp(1, 3) as usize - 1
}

/// Atlas tiles drawn in blue; the red, yellow and green versions follow them (`tile + tribe`).
/// Derived by comparing the tribe series face by face, plus the site stone glyph (194).
pub const BLUE_TILES: [u16; 11] = [16, 24, 32, 40, 44, 104, 112, 154, 186, 194, 226];

/// The tile to draw for `tribe` (objects are stored in blue).
pub fn tribe_tile(tile: u16, tribe: u8) -> u16 {
    if BLUE_TILES.contains(&tile) { tile + (tribe % TRIBES) as u16 } else { tile }
}

/// Shaman animations in `VSTART-0.ANI`, identified by eye: 4 consecutive ones per action,
/// one per tribe (the shaman's colours are drawn in, not layered).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShamanAnim {
    /// Standing, breathing (5 frames).
    Idle,
    /// Staff swing (4 frames).
    Strike,
    /// Flying horizontally, arms spread, as when blown by a whirlwind (4 frames).
    Flying,
    /// Jumps up with lightning in the hands (12 frames).
    Cast,
    /// Kick (5 frames).
    Kick,
    /// Tumbling in the air (4 frames).
    Flung,
    /// Walking (8 frames).
    Walk,
    /// Knocked down flat on her back (8 frames).
    Fall,
    /// Kneeling on one knee (1 frame): used for praying.
    Kneel,
}

impl ShamanAnim {
    fn blue(self) -> usize {
        match self {
            ShamanAnim::Idle => 53,
            ShamanAnim::Strike => 57,
            ShamanAnim::Flying => 61,
            ShamanAnim::Cast => 65,
            ShamanAnim::Kick => 69,
            ShamanAnim::Flung => 73,
            ShamanAnim::Walk => 77,
            ShamanAnim::Fall => 85,
            ShamanAnim::Kneel => 93,
        }
    }

    pub fn anim(self, tribe: u8) -> usize {
        self.blue() + (tribe % TRIBES) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buildings_follow_tribe_order() {
        assert_eq!(Building::DrumTower.object(0), 117);
        assert_eq!(Building::DrumTower.object(3), 120);
        assert_eq!(Building::BoatHut.object(1), 122);
        assert_eq!(Building::WarriorTraining.object(0), 141);
    }

    #[test]
    fn villager_huts_match_the_identified_objects() {
        assert_eq!(villager_hut(0, 0, 1), 145);
        assert_eq!(villager_hut(0, 0, 3), 147);
        assert_eq!(villager_hut(0, 1, 1), 148);
        assert_eq!(villager_hut(0, 2, 1), 151);
        assert_eq!(villager_hut(0, 3, 1), 154);
        assert_eq!(villager_hut(1, 0, 1), 157);
        assert_eq!(villager_hut(1, 1, 1), 160);
        assert_eq!(villager_hut(1, 3, 1), 166);
        assert_eq!(villager_hut(2, 0, 1), 169);
        assert_eq!(villager_hut(2, 3, 3), 180);
    }

    #[test]
    fn shaman_anims_follow_tribe_order() {
        assert_eq!(ShamanAnim::Idle.anim(0), 53);
        assert_eq!(ShamanAnim::Idle.anim(3), 56);
        assert_eq!(ShamanAnim::Walk.anim(1), 78);
        assert_eq!(ShamanAnim::Fall.anim(2), 87);
        assert_eq!(ShamanAnim::Kneel.anim(3), 96);
    }

    #[test]
    fn only_blue_tiles_change_colour() {
        assert_eq!(tribe_tile(226, 1), 227);
        assert_eq!(tribe_tile(194, 3), 197);
        assert_eq!(tribe_tile(224, 2), 224, "plain stone");
        assert_eq!(tribe_tile(24, 0), 24);
    }
}
