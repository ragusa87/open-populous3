//! What the bank 0 objects are (identified by hand, see docs/specs/objects.md), and how the
//! tribe-coloured variants are laid out. Tribe order: 0 blue, 1 red, 2 yellow, 3 green.

use crate::anim::Outfit;

pub const TRIBES: u8 = 4;

/// Effects and map sprites ("PSFB", palette `pal0-0.dat`): plants, wood, fire, spell effects, icons.
pub const EFFECT_SPRITE_FILE: &str = "hfx0-0.dat";
/// A pile of logs (17 x 11) in `EFFECT_SPRITE_FILE`: a piece of wood on the ground. 22 is a flat
/// dark smear under it (its shadow, probably).
pub const WOOD_PILE_SPRITE: usize = 23;
/// Teal unit figures (16 x 23) in `EFFECT_SPRITE_FILE`, in unit model order: brave, warrior, preacher,
/// spy, firewarrior, shaman. Identified by eye.
pub const UNIT_FIGURES: std::ops::RangeInclusive<usize> = 75..=80;

/// Camp fire: a cross of four dark logs (tile 137) and two crossed flame boards (blended faces,
/// tile 92). Object 12 is the same with logs of tile 136, a third of it see-through (unknown use).
pub const CAMP_FIRE: usize = 0;
/// Atlas tile of the flame boards: alpha pixels (`blend::AlphaTable`), drawn by `Face::is_alpha` faces.
pub const FLAME_TILE: u16 = 92;
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
/// Pyramid of knowledge: unlocks a spell or a building. Three frames of one mesh (same faces, moved
/// points, objects.md): door open, door closed, door closed with the top folded (spent).
pub const KNOWLEDGE_PYRAMID: usize = 191;
pub const KNOWLEDGE_PYRAMID_CLOSED: usize = 192;
pub const KNOWLEDGE_PYRAMID_FOLDED: usize = 193;
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

/// The 3D object a tree is drawn with: tree types 0-5 (scenery models 1-6) are objects 13-18
/// (cone pine, weeping tree, big weeping tree, pine, then copies of 14 and 15), confirmed on level 19;
/// the other tree objects, 60-71 (twisted bonsai-like trees, round, cone, palm), follow as types 6-17,
/// probably other landscapes' trees (which theme uses which is unknown).
pub fn tree_object(tree_type: u8) -> usize {
    match tree_type {
        0..=5 => 13 + tree_type as usize,
        t => 60 + (t as usize - 6) % 12,
    }
}

/// Tree types: 6 scenery models plus the 12 other tree objects.
pub const TREE_TYPES: u8 = 18;

/// Braves, warriors, firewarriors and spies share one tribesman body, drawn blue (tribe 0) with
/// tribe colour layers and an outfit layer per unit type (`anim::Outfit`), identified by eye.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersonAnim {
    /// Standing, breathing (6 frames).
    Stand,
    /// Walking (4 frames).
    Walk,
    /// Kneeling, arms raised (6 frames): used for praying.
    Kneel,
    /// Struck down onto its back (8 frames, the last lying).
    Fall,
    /// Lying, its spirit rising (5 frames): drowning.
    Drown,
    /// Crouch, both arms straight up (frame `ARMS_UP_FRAME`), leap, land (4 frames): its arms-up
    /// frame is held for stranded units.
    ArmsUp,
    /// Brave cutting wood with an axe (4 frames).
    Chop,
    /// Brave walking with a piece of wood (4 frames).
    CarryWalk,
    /// Brave standing with a piece of wood (1 frame).
    CarryStand,
}

/// The frame of `PersonAnim::ArmsUp` standing with both arms up.
pub const ARMS_UP_FRAME: usize = 1;

impl PersonAnim {
    pub fn anim(self) -> usize {
        match self {
            PersonAnim::Stand => 6,
            PersonAnim::Walk => 5,
            PersonAnim::Kneel => 8,
            PersonAnim::Fall => 38,
            PersonAnim::Drown => 40,
            PersonAnim::ArmsUp => 12,
            PersonAnim::Chop => 11,
            PersonAnim::CarryWalk => 9,
            PersonAnim::CarryStand => 10,
        }
    }
}

/// Outfits over the tribesman body: braves wear none. Firewarrior: horned skull helmet, fire in the
/// hands. Warrior: headband, grey vest. Spy: long dark hair. Preacher (monk): grey pointed helmet,
/// armour, pink shoulder pads.
pub const OUTFIT_FIREWARRIOR: Outfit = Outfit { flags: 0x20, bits: 1 };
pub const OUTFIT_WARRIOR: Outfit = Outfit { flags: 0x20, bits: 2 };
pub const OUTFIT_SPY: Outfit = Outfit { flags: 0x20, bits: 3 };
pub const OUTFIT_PREACHER: Outfit = Outfit { flags: 0x30, bits: 1 };

/// Wildmen (neutral, no layers): their own small set of animations, identified by eye.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WildmanAnim {
    /// Standing (1 frame).
    Stand,
    /// Walking (4 frames).
    Walk,
    /// Sitting on the ground (1 frame): used for praying.
    Sit,
    /// Tumbling in the air (4 frames): used for drowning.
    Flung,
    /// Thrown down, lying (4 frames): used for dying.
    Down,
}

impl WildmanAnim {
    pub fn anim(self) -> usize {
        match self {
            WildmanAnim::Stand => 1,
            WildmanAnim::Walk => 0,
            WildmanAnim::Sit => 3,
            WildmanAnim::Flung => 31,
            WildmanAnim::Down => 47,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_types_to_objects() {
        assert_eq!((tree_object(0), tree_object(1), tree_object(5)), (13, 14, 18));
        assert_eq!((tree_object(6), tree_object(17)), (60, 71));
    }

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
