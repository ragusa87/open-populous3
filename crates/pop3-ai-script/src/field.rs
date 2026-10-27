//! Field decoding (docs/specs/ai-scripts.md "`cpscr` layout", "Internal variables"): a field's raw
//! `(type, value)` pair becomes a constant, a user variable, or an internal variable.

use crate::raw::RawField;
use std::fmt;

/// A user variable (`$name` in the source language), numbered 0..64 by first use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserVar(pub u8);

/// An internal variable or `INT_ATTR_*` attribute index (field type 2): 0..14 game/tribe state,
/// 1000..1047 the 48 writable attributes, 1048..1246 read-only game state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InternalVar(pub u16);

impl InternalVar {
    /// The 48 `INT_ATTR_*` indices, the only internal variables a script can write (`SET_ATTR_*`
    /// destinations), per "Destinations (`Fdest`) are always a user variable or an `INT_ATTR_*`".
    pub const ATTR_RANGE: std::ops::Range<u16> = 1000..1048;

    pub fn is_attribute(self) -> bool {
        Self::ATTR_RANGE.contains(&self.0)
    }

    /// The editor's name for this index, e.g. `INT_GAME_TURN`, `INT_ATTR_BLAST`. `None` for an
    /// index never seen in the 58 original files (the table only covers those).
    pub fn name(self) -> Option<&'static str> {
        INTERNAL_VAR_NAMES.binary_search_by_key(&self.0, |&(i, _)| i).ok().map(|idx| INTERNAL_VAR_NAMES[idx].1)
    }
}

/// A decoded field: a constant, a user variable, or an internal variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Const(i32),
    UserVar(UserVar),
    Internal(InternalVar),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldError {
    /// Type byte is not 0 (constant), 1 (user variable) or 2 (internal variable) — e.g. an unused
    /// field's `0x03030303` padding.
    UnknownType(u32),
}

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldError::UnknownType(ty) => write!(f, "unknown field type {ty} (padding or corrupt data?)"),
        }
    }
}

impl std::error::Error for FieldError {}

impl Field {
    pub fn decode(raw: RawField) -> Result<Field, FieldError> {
        match raw.ty {
            0 => Ok(Field::Const(raw.value)),
            1 => Ok(Field::UserVar(UserVar(raw.value as u8))),
            2 => Ok(Field::Internal(InternalVar(raw.value as u16))),
            other => Err(FieldError::UnknownType(other)),
        }
    }
}

/// `(index, name)`, sorted by index, from the spec's "Internal variables" table.
const INTERNAL_VAR_NAMES: &[(u16, &str)] = &[
    (0, "INT_GAME_TURN"),
    (1, "INT_MY_NUM_PEOPLE"),
    (2, "INT_BLUE_PEOPLE"),
    (3, "INT_RED_PEOPLE"),
    (4, "INT_YELLOW_PEOPLE"),
    (5, "INT_GREEN_PEOPLE"),
    (6, "INT_MY_NUM_KILLED_BY_HUMAN"),
    (7, "INT_RED_KILLED_BY_HUMAN"),
    (8, "INT_YELLOW_KILLED_BY_HUMAN"),
    (9, "INT_GREEN_KILLED_BY_HUMAN"),
    (10, "INT_WILD_PEOPLE"),
    (11, "INT_BLUE_MANA"),
    (12, "INT_RED_MANA"),
    (13, "INT_YELLOW_MANA"),
    (14, "INT_GREEN_MANA"),
    (1000, "INT_ATTR_EXPANSION"),
    (1001, "INT_ATTR_PREF_SPY_TRAINS"),
    (1002, "INT_ATTR_PREF_RELIGIOUS_TRAINS"),
    (1003, "INT_ATTR_PREF_WARRIOR_TRAINS"),
    (1004, "INT_ATTR_PREF_FIREWARRIOR_TRAINS"),
    (1005, "INT_ATTR_PREF_SPY_PEOPLE"),
    (1006, "INT_ATTR_PREF_RELIGIOUS_PEOPLE"),
    (1007, "INT_ATTR_PREF_WARRIOR_PEOPLE"),
    (1008, "INT_ATTR_PREF_FIREWARRIOR_PEOPLE"),
    (1009, "INT_ATTR_MAX_BUILDINGS_ON_GO"),
    (1010, "INT_ATTR_HOUSE_PERCENTAGE"),
    (1011, "INT_ATTR_AWAY_BRAVE"),
    (1012, "INT_ATTR_AWAY_WARRIOR"),
    (1013, "INT_ATTR_AWAY_RELIGIOUS"),
    (1014, "INT_ATTR_DEFENSE_RAD_INCR"),
    (1015, "INT_ATTR_MAX_DEFENSIVE_ACTIONS"),
    (1016, "INT_ATTR_AWAY_SPY"),
    (1017, "INT_ATTR_AWAY_FIREWARRIOR"),
    (1018, "INT_ATTR_ATTACK_PERCENTAGE"),
    (1019, "INT_ATTR_AWAY_SHAMAN"),
    (1020, "INT_ATTR_PEOPLE_PER_BOAT"),
    (1021, "INT_ATTR_PEOPLE_PER_BALLOON"),
    (1022, "INT_ATTR_DONT_USE_BOATS"),
    (1023, "INT_ATTR_MAX_SPY_ATTACKS"),
    (1024, "INT_ATTR_ENEMY_SPY_MAX_STAND"),
    (1025, "INT_ATTR_MAX_ATTACKS"),
    (1026, "INT_ATTR_EMPTY_AT_WAYPOINT"),
    (1027, "INT_ATTR_SPY_CHECK_FREQUENCY"),
    (1028, "INT_ATTR_RETREAT_VALUE"),
    (1029, "INT_ATTR_BASE_UNDER_ATTACK_RETREAT"),
    (1030, "INT_ATTR_RANDOM_BUILD_SIDE"),
    (1031, "INT_ATTR_USE_PREACHER_FOR_DEFENCE"),
    (1032, "INT_ATTR_SHAMEN_BLAST"),
    (1033, "INT_ATTR_MAX_TRAIN_AT_ONCE"),
    (1034, "INT_ATTR_GROUP_OPTION"),
    (1035, "INT_ATTR_PREF_BOAT_HUTS"),
    (1036, "INT_ATTR_PREF_BALLOON_HUTS"),
    (1037, "INT_ATTR_PREF_BOAT_DRIVERS"),
    (1038, "INT_ATTR_PREF_BALLOON_DRIVERS"),
    (1039, "INT_ATTR_FIGHT_STOP_DISTANCE"),
    (1040, "INT_ATTR_SPY_DISCOVER_CHANCE"),
    (1041, "INT_ATTR_COUNT_PREACH_DAMAGE"),
    (1042, "INT_ATTR_DONT_GROUP_AT_DT"),
    (1043, "INT_ATTR_SPELL_DELAY"),
    (1044, "INT_ATTR_DONT_DELETE_USELESS_BOAT_HOUSE"),
    (1045, "INT_ATTR_BOAT_HOUSE_BROKEN"),
    (1046, "INT_ATTR_DONT_AUTO_TRAIN_PREACHERS"),
    (1047, "INT_ATTR_SPARE"),
    (1048, "INT_MY_MANA"),
    (1049, "INT_M_SPELL_BURN_COST"),
    (1050, "INT_M_SPELL_BLAST_COST"),
    (1051, "INT_M_SPELL_LIGHTNING_COST"),
    (1052, "INT_M_SPELL_TORNADO_COST"),
    (1053, "INT_M_SPELL_SWARM_COST"),
    (1054, "INT_M_SPELL_INVISIBILITY_COST"),
    (1055, "INT_M_SPELL_HYPNOTISM_COST"),
    (1056, "INT_M_SPELL_FIRESTORM_COST"),
    (1057, "INT_M_SPELL_GHOST_ARMY_COST"),
    (1058, "INT_M_SPELL_EROSION_COST"),
    (1059, "INT_M_SPELL_SWAMP_COST"),
    (1060, "INT_M_SPELL_LAND_BRIDGE_COST"),
    (1061, "INT_M_SPELL_ANGEL_OF_DEATH_COST"),
    (1062, "INT_M_SPELL_EARTHQUAKE_COST"),
    (1063, "INT_M_SPELL_FLATTEN_COST"),
    (1064, "INT_M_SPELL_VOLCANO_COST"),
    (1065, "INT_M_SPELL_WRATH_OF_GOD_COST"),
    (1066, "INT_M_BUILDING_SMALL_HUT"),
    (1067, "INT_M_BUILDING_MEDIUM_HUT"),
    (1068, "INT_M_BUILDING_LARGE_HUT"),
    (1069, "INT_M_BUILDING_DRUM_TOWER"),
    (1070, "INT_M_BUILDING_TEMPLE"),
    (1071, "INT_M_BUILDING_SPY_TRAIN"),
    (1072, "INT_M_BUILDING_WARRIOR_TRAIN"),
    (1073, "INT_M_BUILDING_FIREWARRIOR_TRAIN"),
    (1074, "INT_M_BUILDING_RECONVERSION"),
    (1075, "INT_M_BUILDING_WALL_PIECE"),
    (1076, "INT_M_BUILDING_GATE"),
    (1077, "INT_M_BUILDING_CURR_OE_SLOT"),
    (1078, "INT_M_BUILDING_BOAT_HUT"),
    (1079, "INT_M_BUILDING_BOAT_HUT_2"),
    (1080, "INT_M_BUILDING_AIRSHIP_HUT"),
    (1081, "INT_M_BUILDING_AIRSHIP_HUT_2"),
    (1082, "INT_B_BUILDING_SMALL_HUT"),
    (1083, "INT_B_BUILDING_MEDIUM_HUT"),
    (1084, "INT_B_BUILDING_LARGE_HUT"),
    (1085, "INT_B_BUILDING_DRUM_TOWER"),
    (1086, "INT_B_BUILDING_TEMPLE"),
    (1087, "INT_B_BUILDING_SPY_TRAIN"),
    (1088, "INT_B_BUILDING_WARRIOR_TRAIN"),
    (1089, "INT_B_BUILDING_FIREWARRIOR_TRAIN"),
    (1090, "INT_B_BUILDING_RECONVERSION"),
    (1091, "INT_B_BUILDING_WALL_PIECE"),
    (1092, "INT_B_BUILDING_GATE"),
    (1093, "INT_B_BUILDING_CURR_OE_SLOT"),
    (1094, "INT_B_BUILDING_BOAT_HUT"),
    (1095, "INT_B_BUILDING_BOAT_HUT_2"),
    (1096, "INT_B_BUILDING_AIRSHIP_HUT"),
    (1097, "INT_B_BUILDING_AIRSHIP_HUT_2"),
    (1098, "INT_R_BUILDING_SMALL_HUT"),
    (1099, "INT_R_BUILDING_MEDIUM_HUT"),
    (1100, "INT_R_BUILDING_LARGE_HUT"),
    (1101, "INT_R_BUILDING_DRUM_TOWER"),
    (1102, "INT_R_BUILDING_TEMPLE"),
    (1103, "INT_R_BUILDING_SPY_TRAIN"),
    (1104, "INT_R_BUILDING_WARRIOR_TRAIN"),
    (1105, "INT_R_BUILDING_FIREWARRIOR_TRAIN"),
    (1106, "INT_R_BUILDING_RECONVERSION"),
    // 1107 is named INT_M_BUILDING_WALL_PIECE by the editor but is really the red one
    // (ai-scripts.md "Internal variables": an editor quirk, not corrected here).
    (1107, "INT_M_BUILDING_WALL_PIECE"),
    (1108, "INT_R_BUILDING_GATE"),
    (1109, "INT_R_BUILDING_CURR_OE_SLOT"),
    (1110, "INT_R_BUILDING_BOAT_HUT"),
    (1111, "INT_R_BUILDING_BOAT_HUT_2"),
    (1112, "INT_R_BUILDING_AIRSHIP_HUT"),
    (1113, "INT_R_BUILDING_AIRSHIP_HUT_2"),
    (1114, "INT_Y_BUILDING_SMALL_HUT"),
    (1115, "INT_Y_BUILDING_MEDIUM_HUT"),
    (1116, "INT_Y_BUILDING_LARGE_HUT"),
    (1117, "INT_Y_BUILDING_DRUM_TOWER"),
    (1118, "INT_Y_BUILDING_TEMPLE"),
    (1119, "INT_Y_BUILDING_SPY_TRAIN"),
    (1120, "INT_Y_BUILDING_WARRIOR_TRAIN"),
    (1121, "INT_Y_BUILDING_FIREWARRIOR_TRAIN"),
    (1122, "INT_Y_BUILDING_RECONVERSION"),
    (1123, "INT_Y_BUILDING_WALL_PIECE"),
    (1124, "INT_Y_BUILDING_GATE"),
    (1125, "INT_Y_BUILDING_CURR_OE_SLOT"),
    (1126, "INT_Y_BUILDING_BOAT_HUT"),
    (1127, "INT_Y_BUILDING_BOAT_HUT_2"),
    (1128, "INT_Y_BUILDING_AIRSHIP_HUT"),
    (1129, "INT_Y_BUILDING_AIRSHIP_HUT_2"),
    (1130, "INT_G_BUILDING_SMALL_HUT"),
    (1131, "INT_G_BUILDING_MEDIUM_HUT"),
    (1132, "INT_G_BUILDING_LARGE_HUT"),
    (1133, "INT_G_BUILDING_DRUM_TOWER"),
    (1134, "INT_G_BUILDING_TEMPLE"),
    (1135, "INT_G_BUILDING_SPY_TRAIN"),
    (1136, "INT_G_BUILDING_WARRIOR_TRAIN"),
    (1137, "INT_G_BUILDING_FIREWARRIOR_TRAIN"),
    (1138, "INT_G_BUILDING_RECONVERSION"),
    (1139, "INT_G_BUILDING_WALL_PIECE"),
    (1140, "INT_G_BUILDING_GATE"),
    (1141, "INT_G_BUILDING_CURR_OE_SLOT"),
    (1142, "INT_G_BUILDING_BOAT_HUT"),
    (1143, "INT_G_BUILDING_BOAT_HUT_2"),
    (1144, "INT_G_BUILDING_AIRSHIP_HUT"),
    (1145, "INT_G_BUILDING_AIRSHIP_HUT_2"),
    (1146, "INT_M_PERSON_BRAVE"),
    (1147, "INT_M_PERSON_WARRIOR"),
    (1148, "INT_M_PERSON_RELIGIOUS"),
    (1149, "INT_M_PERSON_SPY"),
    (1150, "INT_M_PERSON_FIREWARRIOR"),
    (1151, "INT_M_PERSON_SHAMAN"),
    (1152, "INT_B_PERSON_BRAVE"),
    (1153, "INT_B_PERSON_WARRIOR"),
    (1154, "INT_B_PERSON_RELIGIOUS"),
    (1155, "INT_B_PERSON_SPY"),
    (1156, "INT_B_PERSON_FIREWARRIOR"),
    (1157, "INT_B_PERSON_SHAMAN"),
    (1158, "INT_R_PERSON_BRAVE"),
    (1159, "INT_R_PERSON_WARRIOR"),
    (1160, "INT_R_PERSON_RELIGIOUS"),
    (1161, "INT_R_PERSON_SPY"),
    (1162, "INT_R_PERSON_FIREWARRIOR"),
    (1163, "INT_R_PERSON_SHAMAN"),
    (1164, "INT_Y_PERSON_BRAVE"),
    (1165, "INT_Y_PERSON_WARRIOR"),
    (1166, "INT_Y_PERSON_RELIGIOUS"),
    (1167, "INT_Y_PERSON_SPY"),
    (1168, "INT_Y_PERSON_FIREWARRIOR"),
    (1169, "INT_Y_PERSON_SHAMAN"),
    (1170, "INT_G_PERSON_BRAVE"),
    (1171, "INT_G_PERSON_WARRIOR"),
    (1172, "INT_G_PERSON_RELIGIOUS"),
    (1173, "INT_G_PERSON_SPY"),
    (1174, "INT_G_PERSON_FIREWARRIOR"),
    (1175, "INT_G_PERSON_SHAMAN"),
    (1176, "INT_BLUE_KILLED_BY_ME"),
    (1177, "INT_RED_KILLED_BY_ME"),
    (1178, "INT_YELLOW_KILLED_BY_ME"),
    (1179, "INT_GREEN_KILLED_BY_ME"),
    (1180, "INT_MY_NUM_KILLED_BY_BLUE"),
    (1181, "INT_MY_NUM_KILLED_BY_RED"),
    (1182, "INT_MY_NUM_KILLED_BY_YELLOW"),
    (1183, "INT_MY_NUM_KILLED_BY_GREEN"),
    (1184, "INT_BURN"),
    (1185, "INT_BLAST"),
    (1186, "INT_LIGHTNING"),
    (1187, "INT_TORNADO"),
    (1188, "INT_SWARM"),
    (1189, "INT_INVISIBILITY"),
    (1190, "INT_HYPNOTISM"),
    (1191, "INT_FIRESTORM"),
    (1192, "INT_GHOST_ARMY"),
    (1193, "INT_EROSION"),
    (1194, "INT_SWAMP"),
    (1195, "INT_LAND_BRIDGE"),
    (1196, "INT_ANGEL_OF_DEATH"),
    (1197, "INT_EARTHQUAKE"),
    (1198, "INT_FLATTEN"),
    (1199, "INT_VOLCANO"),
    (1200, "INT_WRATH_OF_GOD"),
    (1201, "INT_BRAVE"),
    (1202, "INT_WARRIOR"),
    (1203, "INT_RELIGIOUS"),
    (1204, "INT_SPY"),
    (1205, "INT_FIREWARRIOR"),
    (1206, "INT_SHAMAN"),
    (1207, "INT_SMALL_HUT"),
    (1208, "INT_MEDIUM_HUT"),
    (1209, "INT_LARGE_HUT"),
    (1210, "INT_DRUM_TOWER"),
    (1211, "INT_TEMPLE"),
    (1212, "INT_SPY_TRAIN"),
    (1213, "INT_WARRIOR_TRAIN"),
    (1214, "INT_FIREWARRIOR_TRAIN"),
    (1215, "INT_RECONVERSION"),
    (1216, "INT_WALL_PIECE"),
    (1217, "INT_GATE"),
    (1218, "INT_BOAT_HUT"),
    (1219, "INT_BOAT_HUT_2"),
    (1220, "INT_AIRSHIP_HUT"),
    (1221, "INT_AIRSHIP_HUT_2"),
    (1222, "INT_NO_SPECIFIC_PERSON"),
    (1223, "INT_NO_SPECIFIC_BUILDING"),
    (1224, "INT_NO_SPECIFIC_SPELL"),
    (1225, "INT_TARGET_SHAMAN"),
    (1226, "INT_M_VEHICLE_BOAT"),
    (1227, "INT_M_VEHICLE_AIRSHIP"),
    (1228, "INT_B_VEHICLE_BOAT"),
    (1229, "INT_B_VEHICLE_AIRSHIP"),
    (1230, "INT_R_VEHICLE_BOAT"),
    (1231, "INT_R_VEHICLE_AIRSHIP"),
    (1232, "INT_Y_VEHICLE_BOAT"),
    (1233, "INT_Y_VEHICLE_AIRSHIP"),
    (1234, "INT_G_VEHICLE_BOAT"),
    (1235, "INT_G_VEHICLE_AIRSHIP"),
    (1236, "INT_CP_FREE_ENTRIES"),
    (1237, "INT_RANDOM_100"),
    (1238, "INT_NUM_SHAMEN_DEFENDERS"),
    (1239, "INT_CAMERA_ANGLE"),
    (1240, "INT_CAMERA_X"),
    (1241, "INT_CAMERA_Z"),
    (1242, "INT_M_SPELL_SHIELD_COST"),
    (1243, "INT_SHIELD"),
    (1244, "INT_CONVERT"),
    (1245, "INT_TELEPORT"),
    (1246, "INT_BLOODLUST"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_all_three_field_types() {
        assert_eq!(Field::decode(RawField { ty: 0, value: -5 }).unwrap(), Field::Const(-5));
        assert_eq!(Field::decode(RawField { ty: 1, value: 3 }).unwrap(), Field::UserVar(UserVar(3)));
        assert_eq!(Field::decode(RawField { ty: 2, value: 0 }).unwrap(), Field::Internal(InternalVar(0)));
    }

    #[test]
    fn rejects_padding() {
        // "Unused fields are filled with 0x03 bytes" (ai-scripts.md "cpscr layout"), so both the
        // type and value words decode to this repeated byte.
        const PADDING_BYTE: u32 = 0x03030303;
        let padding = RawField { ty: PADDING_BYTE, value: PADDING_BYTE as i32 };
        assert_eq!(Field::decode(padding), Err(FieldError::UnknownType(PADDING_BYTE)));
    }

    #[test]
    fn resolves_known_names() {
        assert_eq!(InternalVar(0).name(), Some("INT_GAME_TURN"));
        assert_eq!(InternalVar(1050).name(), Some("INT_M_SPELL_BLAST_COST"));
        assert_eq!(InternalVar(1247).name(), None);
        assert!(!InternalVar(1050).is_attribute());
        assert!(InternalVar(1019).is_attribute());
    }

    #[test]
    fn names_are_sorted_for_binary_search() {
        assert!(INTERNAL_VAR_NAMES.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
