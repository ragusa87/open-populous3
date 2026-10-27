//! Tokens, `DO` commands and parameter keywords (docs/specs/ai-scripts.md "Code", "Token tables",
//! "DO parameter kinds"). A code word `< 1000` is an operand (a field index); `>= 1000` is a token.

use crate::field::Field;

/// Raw code-word values (docs/specs/ai-scripts.md "Token tables"), named so parsing never compares
/// against a bare number.
pub mod word {
    /// Words below this are operands (field indices); at or above, a token or keyword.
    pub const TOKEN_BASE: u16 = 1000;

    pub const IF: u16 = 1000;
    pub const ELSE: u16 = 1001;
    pub const ENDIF: u16 = 1002;
    pub const BEGIN: u16 = 1003;
    pub const END: u16 = 1004;
    pub const EVERY: u16 = 1005;
    pub const DO: u16 = 1006;
    pub const SET: u16 = 1007;
    pub const INCREMENT: u16 = 1008;
    pub const DECREMENT: u16 = 1009;
    pub const GT: u16 = 1012;
    pub const LT: u16 = 1013;
    pub const EQ: u16 = 1014;
    pub const NE: u16 = 1015;
    pub const GE: u16 = 1016;
    pub const LE: u16 = 1017;
    pub const SCRIPT_END: u16 = 1019;
    pub const AND: u16 = 1020;
    pub const OR: u16 = 1021;
    pub const ON: u16 = 1022;
    pub const OFF: u16 = 1023;
    pub const MULTIPLY: u16 = 1025;
    pub const DIVIDE: u16 = 1026;

    pub const COUNT_WILD: u16 = 1058;
    pub const ATTACK_MARKER: u16 = 1070;
    pub const ATTACK_BUILDING: u16 = 1071;
    pub const ATTACK_PERSON: u16 = 1072;
    pub const ATTACK_NORMAL: u16 = 1078;
    pub const ATTACK_BY_BOAT: u16 = 1079;
    pub const ATTACK_BY_BALLOON: u16 = 1080;
    pub const GUARD_NORMAL: u16 = 1087;
    pub const GUARD_WITH_GHOSTS: u16 = 1088;
    pub const SPELL_TYPE: u16 = 1099;
    pub const BUILDING_TYPE: u16 = 1100;
    pub const BOAT_TYPE: u16 = 1104;
    pub const BALLOON_TYPE: u16 = 1105;
    pub const BLUE: u16 = 1118;
    pub const RED: u16 = 1119;
    pub const YELLOW: u16 = 1120;
    pub const GREEN: u16 = 1121;

    pub const FLYBY_CREATE_NEW: u16 = 1205;
    pub const FLYBY_START: u16 = 1206;
    pub const FLYBY_STOP: u16 = 1207;
    pub const FLYBY_ALLOW_INTERRUPT: u16 = 1208;
    pub const FLYBY_SET_EVENT_POS: u16 = 1209;
    pub const FLYBY_SET_EVENT_ANGLE: u16 = 1210;
    pub const FLYBY_SET_EVENT_ZOOM: u16 = 1211;
    pub const FLYBY_SET_EVENT_TOOLTIP: u16 = 1213;
    pub const FLYBY_SET_END_TARGET: u16 = 1214;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Gt,
    Lt,
    Eq,
    Ne,
    Ge,
    Le,
}

/// Control tokens and operators (everything but `DO` commands and parameter keywords).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    If,
    Else,
    Endif,
    Begin,
    End,
    Every,
    Do,
    Set,
    Increment,
    Decrement,
    Cmp(CmpOp),
    ScriptEnd,
    And,
    Or,
    On,
    Off,
    Multiply,
    Divide,
}

impl Token {
    pub fn from_word(w: u16) -> Option<Token> {
        Some(match w {
            word::IF => Token::If,
            word::ELSE => Token::Else,
            word::ENDIF => Token::Endif,
            word::BEGIN => Token::Begin,
            word::END => Token::End,
            word::EVERY => Token::Every,
            word::DO => Token::Do,
            word::SET => Token::Set,
            word::INCREMENT => Token::Increment,
            word::DECREMENT => Token::Decrement,
            word::GT => Token::Cmp(CmpOp::Gt),
            word::LT => Token::Cmp(CmpOp::Lt),
            word::EQ => Token::Cmp(CmpOp::Eq),
            word::NE => Token::Cmp(CmpOp::Ne),
            word::GE => Token::Cmp(CmpOp::Ge),
            word::LE => Token::Cmp(CmpOp::Le),
            word::SCRIPT_END => Token::ScriptEnd,
            word::AND => Token::And,
            word::OR => Token::Or,
            word::ON => Token::On,
            word::OFF => Token::Off,
            word::MULTIPLY => Token::Multiply,
            word::DIVIDE => Token::Divide,
            _ => return None,
        })
    }
}

/// A `DO` command id (the spec's "DO commands" table, values 1028..1223). A thin newtype over the
/// token value, like `InternalVar`: looked up in a sorted table rather than a 90-variant enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommandId(pub u16);

impl CommandId {
    pub fn from_word(word: u16) -> Option<CommandId> {
        COMMANDS.binary_search_by_key(&word, |c| c.0).ok().map(|_| CommandId(word))
    }

    /// The editor's script spelling, e.g. `ATTACK`, `STATE_AUTO_ATTACK`.
    pub fn name(self) -> &'static str {
        COMMANDS[COMMANDS.binary_search_by_key(&self.0, |c| c.0).expect("CommandId always valid")].1
    }

    /// Looks a command up by its editor spelling, so callers (tests, a future decompiler) never
    /// have to spell out its raw word.
    pub fn from_name(name: &str) -> Option<CommandId> {
        COMMANDS.iter().find(|c| c.1 == name).map(|c| CommandId(c.0))
    }

    /// Fixed parameter list for this command, in order.
    pub fn params(self) -> &'static [ParamKind] {
        COMMANDS[COMMANDS.binary_search_by_key(&self.0, |c| c.0).expect("CommandId always valid")].2
    }
}

/// One `DO` parameter slot's kind (spec's "DO parameter kinds" table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// One operand.
    Field,
    /// One operand, always a user variable (a query's output).
    VarField,
    OnOff,
    Team,
    TeamWild,
    TargetType,
    AttackType,
    GuardType,
    OneShotType,
    VehicleType,
}

/// A decoded `DO` argument, one per the command's `ParamKind` list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arg {
    Field(Field),
    VarField(crate::field::UserVar),
    OnOff(bool),
    Team(Team),
    TeamWild(TeamWild),
    TargetType(TargetType),
    AttackType(AttackType),
    GuardType(GuardType),
    OneShotType(OneShotType),
    VehicleType(VehicleType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamColor {
    Blue,
    Red,
    Yellow,
    Green,
}

impl TeamColor {
    pub fn from_word(w: u16) -> Option<TeamColor> {
        Some(match w {
            word::BLUE => TeamColor::Blue,
            word::RED => TeamColor::Red,
            word::YELLOW => TeamColor::Yellow,
            word::GREEN => TeamColor::Green,
            _ => return None,
        })
    }
}

/// A `team` parameter: a tribe keyword, or an operand (spec: "team | token BLUE/RED/YELLOW/GREEN,
/// or an operand").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    Color(TeamColor),
    Field(Field),
}

/// A `team_wild` parameter: a team, or `COUNT_WILD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamWild {
    Wild,
    Team(Team),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetType {
    Marker,
    Building,
    Person,
}

impl TargetType {
    pub fn from_word(w: u16) -> Option<TargetType> {
        Some(match w {
            word::ATTACK_MARKER => TargetType::Marker,
            word::ATTACK_BUILDING => TargetType::Building,
            word::ATTACK_PERSON => TargetType::Person,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackType {
    Normal,
    ByBoat,
    ByBalloon,
}

impl AttackType {
    pub fn from_word(w: u16) -> Option<AttackType> {
        Some(match w {
            word::ATTACK_NORMAL => AttackType::Normal,
            word::ATTACK_BY_BOAT => AttackType::ByBoat,
            word::ATTACK_BY_BALLOON => AttackType::ByBalloon,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardType {
    Normal,
    WithGhosts,
}

impl GuardType {
    pub fn from_word(w: u16) -> Option<GuardType> {
        Some(match w {
            word::GUARD_NORMAL => GuardType::Normal,
            word::GUARD_WITH_GHOSTS => GuardType::WithGhosts,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OneShotType {
    Spell,
    Building,
}

impl OneShotType {
    pub fn from_word(w: u16) -> Option<OneShotType> {
        Some(match w {
            word::SPELL_TYPE => OneShotType::Spell,
            word::BUILDING_TYPE => OneShotType::Building,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VehicleType {
    Boat,
    Balloon,
}

impl VehicleType {
    pub fn from_word(w: u16) -> Option<VehicleType> {
        Some(match w {
            word::BOAT_TYPE => VehicleType::Boat,
            word::BALLOON_TYPE => VehicleType::Balloon,
            _ => return None,
        })
    }
}

use ParamKind::*;

/// `(id, name, params)`, sorted by id, transcribed from the spec's "DO commands" table (states
/// 1028-1051 plus the command table). Values never emitted by the compiler (1049, 1052-1057,
/// 1060-1063, "Defined, never used") are intentionally absent.
#[rustfmt::skip]
const COMMANDS: &[(u16, &str, &[ParamKind])] = &[
    (1028, "STATE_CONSTRUCT_BUILDING", &[OnOff]),
    (1029, "STATE_FETCH_WOOD", &[OnOff]),
    (1030, "STATE_SHAMAN_GET_WILDS", &[OnOff]),
    (1031, "STATE_HOUSE_A_PERSON", &[OnOff]),
    (1032, "STATE_SEND_GHOSTS", &[OnOff]),
    (1033, "STATE_STATE_BRING_NEW_PEOPLE_BACK", &[OnOff]),
    (1034, "STATE_TRAIN_PEOPLE", &[OnOff]),
    (1035, "STATE_POPULATE_DRUM_TOWER", &[OnOff]),
    (1036, "STATE_DEFEND", &[OnOff]),
    (1037, "STATE_DEFEND_BASE", &[OnOff]),
    (1038, "STATE_SPELL_DEFENCE", &[Field, Field, OnOff]),
    (1039, "STATE_PREACH", &[OnOff]),
    (1040, "STATE_BUILD_WALLS", &[OnOff]),
    (1041, "STATE_SABOTAGE", &[OnOff]),
    (1042, "STATE_SPELL_OFFENSIVE", &[OnOff]),
    (1043, "STATE_FIREWARRIOR_DEFEND", &[OnOff]),
    (1044, "STATE_BUILD_VEHICLE", &[OnOff]),
    (1045, "STATE_FETCH_LOST_PEOPLE", &[OnOff]),
    (1046, "STATE_FETCH_LOST_VEHICLE", &[OnOff]),
    (1047, "STATE_FETCH_FAR_VEHICLE", &[OnOff]),
    (1048, "STATE_AUTO_ATTACK", &[OnOff]),
    (1050, "STATE_FLATTEN_BASE", &[OnOff]),
    (1051, "STATE_BUILD_OUTER_DEFENCES", &[OnOff]),
    (1059, "ATTACK", &[Team, Field, TargetType, Field, Field, Field, Field, Field, AttackType, Field, Field, Field, Field]),
    (1064, "SPELL_ATTACK", &[Field, Field, Field]),
    (1065, "RESET_BASE_MARKER", &[]),
    (1066, "SET_BASE_MARKER", &[Field]),
    (1067, "SET_BASE_RADIUS", &[Field]),
    (1068, "COUNT_PEOPLE_IN_MARKER", &[TeamWild, Field, Field, VarField]),
    (1069, "SET_DRUM_TOWER_POS", &[Field, Field]),
    (1073, "CONVERT_AT_MARKER", &[Field]),
    (1074, "PREACH_AT_MARKER", &[Field]),
    (1075, "SEND_GHOST_PEOPLE", &[Field]),
    (1076, "GET_SPELLS_CAST", &[Team, Field, VarField]),
    (1077, "GET_NUM_ONE_OFF_SPELLS", &[Team, Field, VarField]),
    (1081, "SET_ATTACK_VARIABLE", &[VarField]),
    (1082, "BUILD_DRUM_TOWER", &[Field, Field]),
    (1083, "GUARD_AT_MARKER", &[Field, Field, Field, Field, Field, GuardType]),
    (1084, "GUARD_BETWEEN_MARKERS", &[Field, Field, Field, Field, Field, Field, GuardType]),
    (1085, "GET_HEIGHT_AT_POS", &[Field, VarField]),
    (1086, "SEND_ALL_PEOPLE_TO_MARKER", &[Field]),
    (1089, "RESET_CONVERT_MARKER", &[]),
    (1090, "SET_CONVERT_MARKER", &[Field]),
    (1091, "SET_MARKER_ENTRY", &[Field, Field, Field, Field, Field, Field, Field]),
    (1092, "MARKER_ENTRIES", &[Field, Field, Field, Field]),
    (1093, "CLEAR_GUARDING_FROM", &[Field, Field, Field, Field]),
    (1094, "SET_BUILDING_DIRECTION", &[Field]),
    (1095, "TRAIN_PEOPLE_NOW", &[Field, Field]),
    (1096, "PRAY_AT_HEAD", &[Field, Field]),
    (1097, "PUT_PERSON_IN_DT", &[Field, Field, Field]),
    (1098, "I_HAVE_ONE_SHOT", &[OneShotType, Field, VarField]),
    (1101, "VEHICLE_PATROL", &[Field, Field, Field, Field, Field, VehicleType]),
    (1102, "DEFEND_SHAMEN", &[Field]),
    (1103, "SEND_SHAMEN_DEFENDERS_HOME", &[]),
    (1106, "IS_BUILDING_NEAR", &[Field, Field, Field, Team, Field, VarField]),
    (1107, "BUILD_AT", &[Field, Field, Field, Field]),
    (1108, "SET_SPELL_ENTRY", &[Field, Field, Field, Field, Field, Field]),
    (1109, "DELAY_MAIN_DRUM_TOWER", &[]),
    (1110, "BUILD_MAIN_DRUM_TOWER", &[]),
    (1111, "ZOOM_TO", &[Field, Field, Field]),
    (1112, "DISABLE_USER_INPUTS", &[]),
    (1113, "ENABLE_USER_INPUTS", &[]),
    (1114, "OPEN_DIALOG", &[Field]),
    (1115, "GIVE_ONE_SHOT", &[Field, Team]),
    (1116, "CLEAR_STANDING_PEOPLE", &[]),
    (1117, "ONLY_STAND_AT_MARKERS", &[]),
    (1122, "NAV_CHECK", &[Team, TargetType, Field, Field, VarField]),
    (1123, "TARGET_FIREWARRIORS", &[]),
    (1124, "DONT_TARGET_FIREWARRIORS", &[]),
    (1125, "TARGET_BLUE_SHAMAN", &[]),
    (1126, "DONT_TARGET_BLUE_SHAMAN", &[]),
    (1127, "TARGET_BLUE_DRUM_TOWERS", &[]),
    (1128, "DONT_TARGET_BLUE_DRUM_TOWERS", &[]),
    (1129, "HAS_BLUE_KILLED_A_GHOST", &[VarField]),
    (1130, "COUNT_GUARD_FIRES", &[Field, Field, Field, VarField]),
    (1131, "GET_HEAD_TRIGGER_COUNT", &[Field, Field, VarField]),
    (1132, "MOVE_SHAMAN_TO_MARKER", &[Field]),
    (1133, "TRACK_SHAMAN_TO_ANGLE", &[Field]),
    (1134, "TRACK_SHAMAN_EXTRA_BOLLOCKS", &[Field]),
    (1135, "IS_SHAMAN_AVAILABLE_FOR_ATTACK", &[VarField]),
    (1136, "PARTIAL_BUILDING_COUNT", &[]),
    (1137, "SEND_BLUE_PEOPLE_TO_MARKER", &[Field]),
    (1138, "GIVE_MANA_TO_PLAYER", &[Team, Field]),
    (1139, "IS_PLAYER_IN_WORLD_VIEW", &[VarField]),
    (1140, "SET_AUTO_BUILD", &[OnOff]),
    (1141, "DESELECT_ALL_BLUE_PEOPLE", &[]),
    (1142, "FLASH_BUTTON", &[Field, OnOff]),
    (1143, "TURN_PANEL_ON", &[Field]),
    (1144, "GIVE_PLAYER_SPELL", &[Team, Field]),
    (1145, "HAS_PLAYER_BEEN_IN_ENCYC", &[VarField]),
    (1146, "IS_BLUE_SHAMAN_SELECTED", &[VarField]),
    (1147, "CLEAR_SHAMAN_LEFT_CLICK", &[]),
    (1148, "CLEAR_SHAMAN_RIGHT_CLICK", &[]),
    (1149, "IS_SHAMAN_ICON_LEFT_CLICKED", &[VarField]),
    (1150, "IS_SHAMAN_ICON_RIGHT_CLICKED", &[VarField]),
    (1151, "TRIGGER_THING", &[Field]),
    (1152, "TRACK_TO_MARKER", &[Field]),
    (1153, "CAMERA_ROTATION", &[Field]),
    (1154, "STOP_CAMERA_ROTATION", &[]),
    (1155, "COUNT_BLUE_SHAPES", &[VarField]),
    (1156, "COUNT_BLUE_IN_HOUSES", &[VarField]),
    (1157, "HAS_HOUSE_INFO_BEEN_SHOWN", &[VarField]),
    (1158, "CLEAR_HOUSE_INFO_FLAG", &[]),
    (1159, "SET_AUTO_HOUSE", &[OnOff]),
    (1160, "COUNT_BLUE_WITH_BUILD_COMMAND", &[VarField]),
    (1161, "DONT_HOUSE_SPECIALISTS", &[OnOff]),
    (1162, "TARGET_PLAYER_DT_AND_S", &[Team]),
    (1163, "REMOVE_PLAYER_THING", &[Team, Field]),
    (1164, "SET_REINCARNATION", &[OnOff]),
    (1165, "EXTRA_WOOD_COLLECTION", &[OnOff]),
    (1166, "SET_WOOD_COLLECTION_RADII", &[Field, Field, Field, Field]),
    (1167, "GET_NUM_PEOPLE_CONVERTED", &[Team, VarField]),
    (1168, "GET_NUM_PEOPLE_BEING_PREACHED", &[Team, VarField]),
    (1169, "TRIGGER_LEVEL_LOST", &[]),
    (1170, "TRIGGER_LEVEL_WON", &[]),
    (1171, "REMOVE_HEAD_AT_POS", &[Field, Field]),
    (1172, "SET_BUCKET_USAGE", &[OnOff]),
    (1173, "SET_BUCKET_COUNT_FOR_SPELL", &[Field, Field]),
    (1174, "CREATE_MSG_NARRATIVE", &[Field]),
    (1175, "CREATE_MSG_OBJECTIVE", &[Field]),
    (1176, "CREATE_MSG_INFORMATION", &[Field]),
    (1177, "CREATE_MSG_INFORMATION_ZOOM", &[Field, Field, Field, Field]),
    (1178, "SET_MSG_ZOOM", &[Field, Field, Field]),
    (1179, "SET_MSG_TIMEOUT", &[Field]),
    (1180, "SET_MSG_DELETE_ON_OK", &[]),
    (1181, "SET_MSG_RETURN_ON_OK", &[]),
    (1182, "SET_MSG_DELETE_ON_RMB_ZOOM", &[]),
    (1183, "SET_MSG_OPEN_DLG_ON_RMB_ZOOM", &[]),
    (1184, "SET_MSG_CREATE_RETURN_MSG_ON_RMB_ZOOM", &[]),
    (1185, "SET_MSG_OPEN_DLG_ON_RMB_DELETE", &[]),
    (1186, "SET_MSG_ZOOM_ON_LMB_OPEN_DLG", &[]),
    (1187, "SET_MSG_AUTO_OPEN_DLG", &[]),
    (1188, "SET_SPECIAL_NO_BLDG_PANEL", &[OnOff]),
    (1189, "SET_MSG_OK_SAVE_EXIT_DLG", &[]),
    (1190, "FIX_WILD_IN_AREA", &[Field, Field, Field]),
    (1191, "CHECK_IF_PERSON_PREACHED_TO", &[VarField, VarField, VarField]),
    (1192, "COUNT_ANGELS", &[Team, VarField]),
    (1193, "SET_NO_BLUE_REINC", &[]),
    (1194, "IS_SHAMAN_IN_AREA", &[Team, Field, Field, VarField]),
    (1195, "FORCE_TOOLTIP", &[Field, Field, Field, Field]),
    (1196, "SET_DEFENCE_RADIUS", &[Field]),
    (1197, "MARVELLOUS_HOUSE_DEATH", &[]),
    (1198, "CALL_TO_ARMS", &[]),
    (1199, "DELETE_SMOKE_STUFF", &[Field, Field, Field]),
    (1200, "SET_TIMER_GOING", &[Field]),
    (1201, "REMOVE_TIMER", &[]),
    (1202, "HAS_TIMER_REACHED_ZERO", &[VarField]),
    (1203, "START_REINC_NOW", &[]),
    (1204, "TURN_PUSH", &[OnOff]),
    (1205, "FLYBY_CREATE_NEW", &[]),
    (1206, "FLYBY_START", &[]),
    (1207, "FLYBY_STOP", &[]),
    (1208, "FLYBY_ALLOW_INTERRUPT", &[OnOff]),
    (1209, "FLYBY_SET_EVENT_POS", &[Field, Field, Field, Field]),
    (1210, "FLYBY_SET_EVENT_ANGLE", &[Field, Field, Field]),
    (1211, "FLYBY_SET_EVENT_ZOOM", &[Field, Field, Field]),
    (1212, "FLYBY_SET_EVENT_INT_POINT", &[Field, Field, Field, Field]),
    (1213, "FLYBY_SET_EVENT_TOOLTIP", &[Field, Field, Field, Field, Field]),
    (1214, "FLYBY_SET_END_TARGET", &[Field, Field, Field, Field]),
    (1215, "FLYBY_SET_MESSAGE", &[Field, Field]),
    (1216, "KILL_TEAM_IN_AREA", &[Field, Field, Field]),
    (1217, "CLEAR_ALL_MSG", &[]),
    (1218, "SET_MSG_ID", &[Field]),
    (1219, "GET_MSG_ID", &[VarField]),
    (1220, "KILL_ALL_MSG_ID", &[Field]),
    (1221, "GIVE_UP_AND_SULK", &[OnOff]),
    (1222, "AUTO_MESSAGES", &[OnOff]),
    (1223, "IS_PRISON_ON_LEVEL", &[Field]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_sorted_and_unique() {
        assert!(COMMANDS.windows(2).all(|w| w[0].0 < w[1].0), "COMMANDS must be sorted by id for binary_search");
    }

    #[test]
    fn commands_are_in_range_and_never_defined_unused_ones() {
        const NEVER_USED: &[u16] = &[1049, 1052, 1053, 1054, 1055, 1056, 1057, 1060, 1061, 1062, 1063];
        for &(id, _, _) in COMMANDS {
            assert!((1028..=1223).contains(&id), "{id} out of DO command range");
            assert!(!NEVER_USED.contains(&id), "{id} is defined but never used by the compiler");
        }
    }

    #[test]
    fn looks_up_attack_and_its_thirteen_params() {
        let attack = CommandId::from_name("ATTACK").unwrap();
        assert_eq!(attack.params().len(), 13);
        assert_eq!(CommandId::from_word(attack.0), Some(attack), "from_word and from_name agree");
    }

    #[test]
    fn rejects_an_unknown_word_and_a_keyword_slot() {
        assert_eq!(CommandId::from_word(word::COUNT_WILD), None, "COUNT_WILD is a parameter keyword, not a command");
        assert_eq!(CommandId::from_word(9999), None, "9999 is not a word used by the format at all");
    }

    #[test]
    fn decodes_token_and_keyword_words() {
        assert_eq!(Token::from_word(word::EQ), Some(Token::Cmp(CmpOp::Eq)));
        assert_eq!(TeamColor::from_word(word::BLUE), Some(TeamColor::Blue));
        assert_eq!(AttackType::from_word(word::ATTACK_BY_BALLOON), Some(AttackType::ByBalloon));
        assert_eq!(TargetType::from_word(word::ATTACK_BUILDING), Some(TargetType::Building));
    }
}
