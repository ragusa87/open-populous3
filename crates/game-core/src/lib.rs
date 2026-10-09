//! Engine-agnostic, deterministic game simulation (no Bevy, no floats in state).
//!
//! Everything here must give identical results on every lockstep peer.

pub mod build_book;
pub mod building;
pub mod campfire;
pub mod command;
pub mod enter;
pub mod map;
pub mod path;
pub mod placement;
pub mod site;
pub mod slots;
pub mod spell;
pub mod spell_book;
pub mod terrain;
pub mod tree;
pub mod unit;
pub mod wood;
pub mod work;
