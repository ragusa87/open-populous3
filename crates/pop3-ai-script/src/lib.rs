//! Parser, statement tree and interpreter abstraction for the original computer-player scripts
//! (`cpscrNNN.dat` / `cpatrNNN.dat`, "PopScript"). See docs/specs/ai-scripts.md. No Bevy, no game
//! logic: `Env::do_call` hands every `DO` command to its caller undecided.

pub mod ast;
pub mod cpatr;
pub mod field;
pub mod flyby;
pub mod interp;
pub mod raw;
pub mod token;

pub use ast::{parse, AstError, Script};
pub use cpatr::ComputerPlayerAttrs;
pub use raw::{RawError, RawScript};
