//! The original's sound id table (sound.md "Sound id table"): for each id that plays something from `Soundd2`, its
//! 0-based entry range (one variant at random per play), priority, volume and animation-sync frame. Facts about the
//! file layout, read against the player's own `Soundd2.sdt`; nothing of the sounds themselves. Ids with no sample
//! are left out; the fight-bank ids (punches, swords, yells, hit grunts) too, `POPfightnew.SDT` is not loaded yet
//! (TODO.md "Sound").

/// One id's record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundIdRecord {
    pub id: u8,
    /// First and last `Soundd2` entry, 0-based, inclusive.
    pub first: u16,
    pub last: u16,
    /// Kept first when too many sounds play: 0x20 faint ambience .. 0xFF interface and voices.
    pub priority: u8,
    /// 0..=127.
    pub volume: u8,
    /// The animation frame a positional sound waits for (chop, eat, drink, hammer: 1; stomp: 3). Not applied yet.
    #[allow(dead_code, reason = "work sounds on their animation frame, TODO.md Sound")]
    pub sync: Option<u8>,
}

pub fn record(id: u8) -> Option<&'static SoundIdRecord> {
    SOUND_IDS.iter().find(|r| r.id == id)
}

pub const SOUND_IDS: &[SoundIdRecord] = &[
    SoundIdRecord { id: 0x01, first: 15, last: 19, priority: 0x80, volume: 127, sync: Some(1) },
    SoundIdRecord { id: 0x02, first: 273, last: 276, priority: 0x80, volume: 63, sync: Some(3) },
    SoundIdRecord { id: 0x03, first: 20, last: 24, priority: 0x60, volume: 63, sync: Some(1) },
    SoundIdRecord { id: 0x04, first: 26, last: 32, priority: 0x60, volume: 127, sync: Some(1) },
    SoundIdRecord { id: 0x05, first: 0, last: 0, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x06, first: 364, last: 367, priority: 0x50, volume: 127, sync: None },
    SoundIdRecord { id: 0x09, first: 122, last: 128, priority: 0x80, volume: 80, sync: None },
    SoundIdRecord { id: 0x0A, first: 15, last: 19, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x0B, first: 15, last: 19, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x0C, first: 15, last: 19, priority: 0x80, volume: 127, sync: None },
    // 0x0D fight bank Punch31..Punch1
    // 0x0E fight bank Punch31..Punch1
    SoundIdRecord { id: 0x0F, first: 227, last: 227, priority: 0x80, volume: 63, sync: None },
    SoundIdRecord { id: 0x10, first: 129, last: 143, priority: 0x80, volume: 80, sync: None },
    SoundIdRecord { id: 0x11, first: 10, last: 12, priority: 0x80, volume: 80, sync: None },
    SoundIdRecord { id: 0x12, first: 93, last: 94, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x13, first: 4, last: 6, priority: 0x80, volume: 63, sync: None },
    SoundIdRecord { id: 0x14, first: 68, last: 72, priority: 0x80, volume: 16, sync: Some(1) },
    SoundIdRecord { id: 0x15, first: 25, last: 25, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x16, first: 255, last: 268, priority: 0x60, volume: 45, sync: None },
    SoundIdRecord { id: 0x17, first: 255, last: 255, priority: 0x20, volume: 10, sync: None },
    SoundIdRecord { id: 0x18, first: 41, last: 42, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x19, first: 491, last: 492, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x1A, first: 37, last: 38, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x1B, first: 33, last: 36, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x1C, first: 45, last: 64, priority: 0x40, volume: 32, sync: None },
    SoundIdRecord { id: 0x1D, first: 150, last: 154, priority: 0x40, volume: 32, sync: None },
    SoundIdRecord { id: 0x1E, first: 161, last: 166, priority: 0x40, volume: 32, sync: None },
    SoundIdRecord { id: 0x1F, first: 155, last: 160, priority: 0x40, volume: 32, sync: None },
    SoundIdRecord { id: 0x20, first: 332, last: 335, priority: 0x40, volume: 32, sync: None },
    SoundIdRecord { id: 0x21, first: 441, last: 450, priority: 0x40, volume: 32, sync: None },
    SoundIdRecord { id: 0x22, first: 87, last: 89, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x23, first: 73, last: 73, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0x24, first: 74, last: 74, priority: 0x80, volume: 63, sync: None },
    SoundIdRecord { id: 0x25, first: 75, last: 75, priority: 0x80, volume: 63, sync: None },
    SoundIdRecord { id: 0x26, first: 110, last: 110, priority: 0x80, volume: 63, sync: None },
    // 0x27 fight bank swords8..swords1
    SoundIdRecord { id: 0x28, first: 351, last: 351, priority: 0x80, volume: 30, sync: None },
    SoundIdRecord { id: 0x29, first: 76, last: 76, priority: 0x80, volume: 70, sync: None },
    SoundIdRecord { id: 0x2A, first: 77, last: 77, priority: 0x80, volume: 70, sync: None },
    // 0x2B fight bank sw_flsh6..sw_flsh1
    SoundIdRecord { id: 0x2C, first: 80, last: 82, priority: 0x80, volume: 127, sync: None },
    // 0x2D fight bank Attk01_c..Attk01_a
    // 0x2E fight bank Attk02_b,Attk01_c
    // 0x2F fight bank Attk03_c..Attk03_a
    // 0x30 fight bank Attk99_a
    SoundIdRecord { id: 0x31, first: 83, last: 83, priority: 0x80, volume: 127, sync: None },
    // 0x32 fight bank v_hit10..v_hit01
    SoundIdRecord { id: 0x33, first: 215, last: 218, priority: 0x80, volume: 43, sync: None },
    SoundIdRecord { id: 0x34, first: 90, last: 92, priority: 0x80, volume: 63, sync: None },
    SoundIdRecord { id: 0x35, first: 117, last: 117, priority: 0x80, volume: 96, sync: None },
    SoundIdRecord { id: 0x36, first: 118, last: 121, priority: 0x80, volume: 96, sync: None },
    SoundIdRecord { id: 0x37, first: 191, last: 195, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x38, first: 183, last: 184, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x39, first: 185, last: 186, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x3A, first: 185, last: 186, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x3B, first: 201, last: 204, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x3C, first: 205, last: 206, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x3D, first: 205, last: 206, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x3E, first: 205, last: 206, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x3F, first: 211, last: 214, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x40, first: 211, last: 214, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x41, first: 211, last: 214, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x42, first: 211, last: 214, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x43, first: 177, last: 179, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x44, first: 180, last: 182, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x45, first: 180, last: 182, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x46, first: 199, last: 200, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x47, first: 199, last: 200, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x48, first: 199, last: 200, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x49, first: 207, last: 210, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x4A, first: 207, last: 210, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x4B, first: 207, last: 210, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x4C, first: 189, last: 189, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x4D, first: 190, last: 190, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x4E, first: 487, last: 490, priority: 0x80, volume: 70, sync: None },
    SoundIdRecord { id: 0x4F, first: 483, last: 486, priority: 0x80, volume: 70, sync: None },
    SoundIdRecord { id: 0x50, first: 279, last: 285, priority: 0x40, volume: 32, sync: None },
    SoundIdRecord { id: 0x51, first: 252, last: 254, priority: 0x80, volume: 63, sync: None },
    SoundIdRecord { id: 0x52, first: 269, last: 272, priority: 0x80, volume: 40, sync: None },
    SoundIdRecord { id: 0x53, first: 368, last: 371, priority: 0x50, volume: 90, sync: None },
    SoundIdRecord { id: 0x54, first: 325, last: 331, priority: 0x40, volume: 32, sync: None },
    SoundIdRecord { id: 0x55, first: 336, last: 338, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x56, first: 196, last: 198, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x57, first: 207, last: 210, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x58, first: 170, last: 176, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0x60, first: 146, last: 146, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x63, first: 9, last: 9, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x66, first: 149, last: 149, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x6A, first: 79, last: 79, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x6B, first: 85, last: 85, priority: 0xFF, volume: 127, sync: None },
    SoundIdRecord { id: 0x6C, first: 84, last: 84, priority: 0xFF, volume: 127, sync: None },
    SoundIdRecord { id: 0x6D, first: 144, last: 144, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x6E, first: 147, last: 147, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x6F, first: 148, last: 148, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x70, first: 169, last: 169, priority: 0xFF, volume: 127, sync: None },
    SoundIdRecord { id: 0x71, first: 168, last: 168, priority: 0xFF, volume: 127, sync: None },
    SoundIdRecord { id: 0x72, first: 167, last: 167, priority: 0xFF, volume: 127, sync: None },
    SoundIdRecord { id: 0x73, first: 145, last: 145, priority: 0xFF, volume: 63, sync: None },
    SoundIdRecord { id: 0x74, first: 225, last: 226, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x75, first: 219, last: 220, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x76, first: 219, last: 220, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x77, first: 221, last: 222, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x78, first: 246, last: 247, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x79, first: 238, last: 239, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x7A, first: 240, last: 241, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x7B, first: 236, last: 237, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x7C, first: 232, last: 233, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x7D, first: 234, last: 235, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x7E, first: 228, last: 229, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x7F, first: 244, last: 245, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x80, first: 223, last: 224, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x81, first: 250, last: 251, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x82, first: 242, last: 243, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x83, first: 230, last: 231, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x84, first: 248, last: 249, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x85, first: 225, last: 226, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x86, first: 219, last: 220, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x88, first: 296, last: 296, priority: 0x80, volume: 63, sync: None },
    SoundIdRecord { id: 0x89, first: 317, last: 318, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x8A, first: 294, last: 295, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x8B, first: 286, last: 287, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x8C, first: 286, last: 287, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x8D, first: 290, last: 291, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x8E, first: 323, last: 324, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x8F, first: 311, last: 312, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x90, first: 313, last: 314, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x91, first: 309, last: 310, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x92, first: 301, last: 302, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x93, first: 303, last: 304, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x94, first: 297, last: 298, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x95, first: 319, last: 320, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x96, first: 292, last: 293, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x97, first: 288, last: 289, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x98, first: 315, last: 316, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x99, first: 299, last: 300, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x9A, first: 321, last: 322, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x9B, first: 294, last: 295, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x9C, first: 286, last: 287, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0x9E, first: 7, last: 7, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0x9F, first: 65, last: 67, priority: 0xB4, volume: 96, sync: None },
    SoundIdRecord { id: 0xA1, first: 101, last: 105, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xA2, first: 187, last: 188, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xA3, first: 352, last: 355, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xA4, first: 356, last: 359, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xA5, first: 2, last: 2, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xA6, first: 2, last: 2, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xA7, first: 2, last: 2, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xA8, first: 43, last: 43, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xA9, first: 1, last: 1, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xAA, first: 111, last: 116, priority: 0x60, volume: 32, sync: None },
    SoundIdRecord { id: 0xAB, first: 480, last: 480, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xAC, first: 44, last: 44, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xAD, first: 14, last: 14, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xAE, first: 7, last: 7, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xAF, first: 14, last: 14, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xB0, first: 13, last: 13, priority: 0xE6, volume: 127, sync: None },
    SoundIdRecord { id: 0xB1, first: 2, last: 2, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xB2, first: 106, last: 109, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xB3, first: 8, last: 8, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xB4, first: 78, last: 78, priority: 0x80, volume: 70, sync: None },
    SoundIdRecord { id: 0xB5, first: 86, last: 86, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xB6, first: 3, last: 3, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xB7, first: 7, last: 7, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xB8, first: 7, last: 7, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xB9, first: 7, last: 7, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xBA, first: 7, last: 7, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xBB, first: 7, last: 7, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xBC, first: 95, last: 100, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xBD, first: 347, last: 350, priority: 0x80, volume: 43, sync: None },
    SoundIdRecord { id: 0xBE, first: 343, last: 346, priority: 0x60, volume: 40, sync: Some(1) },
    SoundIdRecord { id: 0xBF, first: 339, last: 342, priority: 0x60, volume: 40, sync: Some(1) },
    SoundIdRecord { id: 0xC0, first: 360, last: 363, priority: 0x50, volume: 80, sync: None },
    SoundIdRecord { id: 0xC1, first: 398, last: 402, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0xC2, first: 372, last: 391, priority: 0x40, volume: 32, sync: None },
    SoundIdRecord { id: 0xC3, first: 392, last: 392, priority: 0xB4, volume: 127, sync: None },
    SoundIdRecord { id: 0xC4, first: 393, last: 394, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0xC5, first: 395, last: 397, priority: 0xB4, volume: 96, sync: None },
    SoundIdRecord { id: 0xC6, first: 403, last: 412, priority: 0x40, volume: 32, sync: None },
    SoundIdRecord { id: 0xC7, first: 430, last: 434, priority: 0x40, volume: 32, sync: None },
    SoundIdRecord { id: 0xC8, first: 418, last: 423, priority: 0x40, volume: 32, sync: None },
    SoundIdRecord { id: 0xC9, first: 424, last: 429, priority: 0x40, volume: 32, sync: None },
    SoundIdRecord { id: 0xCA, first: 413, last: 417, priority: 0x40, volume: 32, sync: None },
    SoundIdRecord { id: 0xCB, first: 435, last: 435, priority: 0xFF, volume: 127, sync: None },
    SoundIdRecord { id: 0xCC, first: 438, last: 438, priority: 0xFF, volume: 127, sync: None },
    SoundIdRecord { id: 0xCD, first: 436, last: 436, priority: 0xFF, volume: 127, sync: None },
    SoundIdRecord { id: 0xCE, first: 437, last: 437, priority: 0xFF, volume: 127, sync: None },
    SoundIdRecord { id: 0xCF, first: 439, last: 439, priority: 0x80, volume: 40, sync: None },
    SoundIdRecord { id: 0xD0, first: 440, last: 440, priority: 0xFF, volume: 127, sync: None },
    SoundIdRecord { id: 0xD1, first: 451, last: 451, priority: 0xFF, volume: 127, sync: None },
    SoundIdRecord { id: 0xD2, first: 452, last: 459, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0xD4, first: 466, last: 469, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0xD5, first: 460, last: 465, priority: 0x40, volume: 32, sync: None },
    SoundIdRecord { id: 0xD7, first: 475, last: 476, priority: 0x80, volume: 96, sync: None },
    SoundIdRecord { id: 0xD8, first: 470, last: 471, priority: 0x80, volume: 70, sync: None },
    SoundIdRecord { id: 0xD9, first: 472, last: 472, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0xDA, first: 473, last: 473, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0xDB, first: 474, last: 474, priority: 0x80, volume: 96, sync: None },
    SoundIdRecord { id: 0xDC, first: 477, last: 477, priority: 0x80, volume: 96, sync: None },
    SoundIdRecord { id: 0xDD, first: 478, last: 478, priority: 0x80, volume: 96, sync: None },
    SoundIdRecord { id: 0xDE, first: 479, last: 479, priority: 0x80, volume: 96, sync: None },
    SoundIdRecord { id: 0xE0, first: 481, last: 481, priority: 0x80, volume: 96, sync: None },
    SoundIdRecord { id: 0xE1, first: 482, last: 482, priority: 0x80, volume: 96, sync: None },
    SoundIdRecord { id: 0xE2, first: 39, last: 40, priority: 0x80, volume: 100, sync: None },
    SoundIdRecord { id: 0xE3, first: 493, last: 493, priority: 0x80, volume: 127, sync: None },
    SoundIdRecord { id: 0xE4, first: 494, last: 494, priority: 0x80, volume: 127, sync: None },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_sorted_unique_and_ranges_in_the_bank() {
        assert!(SOUND_IDS.windows(2).all(|w| w[0].id < w[1].id));
        assert!(SOUND_IDS.iter().all(|r| r.first <= r.last && r.last < 495 && r.volume <= 127));
    }

    #[test]
    fn the_loops_of_the_bank_are_where_sound_md_says() {
        assert_eq!(record(0x6C).map(|r| (r.first, r.last)), Some((84, 84)), "warloop");
        assert_eq!(record(0xC3).map(|r| (r.first, r.last)), Some((392, 392)), "Shield");
        assert_eq!(record(0x01).map(|r| (r.first, r.last, r.sync)), Some((15, 19, Some(1))), "chop");
        assert!(record(0x00).is_none(), "silent");
    }
}
