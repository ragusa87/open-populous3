//! Readers for original Populous: The Beginning (1998) data files.
//!
//! Pure parsing, no engine dependency: reusable by the game, the editor or CLI tools.
//! See `docs/specs/level-format.md` for the byte layout.

pub mod catalog;
pub mod level;
pub mod objects;
pub mod sprites;
pub mod theme;

pub use level::{Level, LevelError, LevelHeader, Thing, MAP_CELLS, MAP_SIZE, WORLD_UNITS_PER_CELL};
pub use objects::{find_file, Atlas, Face, Object, ObjectBank};
pub use sprites::{Sprite, SpriteBank};
pub use theme::{theme_char, Theme};
