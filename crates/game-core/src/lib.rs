//! Engine-agnostic, deterministic game simulation (no Bevy, no floats in state).
//!
//! Everything here must give identical results on every lockstep peer.

pub mod command;
pub mod map;
pub mod path;
pub mod site;
pub mod spell;
pub mod spell_book;
pub mod terrain;
pub mod unit;
