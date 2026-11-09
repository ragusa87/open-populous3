//! The single run condition every manual-input system (camera, selection/orders, spell-casting) gates
//! itself on, composing the four independent "player can't act right now" cases so a call site chains
//! one `.run_if(not(...))` instead of several (TODO.md "Computer players (AI)"): `armageddon::input_locked`
//! (a match resolving, or the game already over), a flyby playing (`flyby::flyby_active`), the player's
//! own shaman mid build/rebuild ritual (`sites::site_build_active`), and a level script holding
//! `DISABLE_USER_INPUTS` (`GameMap::user_inputs_disabled`, `game_core::ai_script`).

use crate::armageddon::{self, ArmageddonWatch};
use crate::flyby;
use crate::menu::AppState;
use crate::sites;
use crate::world::CurrentMap;
use bevy::prelude::*;

pub fn input_locked(watch: Res<ArmageddonWatch>, state: Res<State<AppState>>, map: Res<CurrentMap>) -> bool {
    armageddon::input_locked(watch, state) || flyby::flyby_active(Res::clone(&map)) || sites::site_build_active(Res::clone(&map)) || map.0.user_inputs_disabled
}
