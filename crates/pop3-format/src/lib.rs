//! Readers for original Populous: The Beginning (1998) data files.
//!
//! Pure parsing, no engine dependency: reusable by the game, the editor or CLI tools.
//! See `docs/specs/level-format.md` for the byte layout.

pub mod level;

pub use level::{Level, LevelError, LevelHeader, Thing, MAP_CELLS, MAP_SIZE};
