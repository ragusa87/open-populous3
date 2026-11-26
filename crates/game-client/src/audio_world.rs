//! What happens on the map turned into sounds, by the original's sound ids (sound.md "Sound id table"): people
//! dying, drowning, thrown and landing, casting, burning, converted, turning invisible; work; spells and their
//! effects; vehicles; buildings burning and finished; the battle crowd and the shield hum. The simulation knows
//! nothing of sound: each system compares what it sees with what it saw last frame (nothing sounds for what is
//! there when a map loads). Pure choices, thin systems.

use crate::audio::{ListenerState, LoopKey, LoopKind, PlayLoop, PlaySound, Sound, SoundCue, SoundOwner, StopLoop};
use crate::audio_space::{place, Listener};
use crate::world::CurrentMap;
use bevy::prelude::*;
use game_core::building::{BuildingId, Stage};
use game_core::effect::{Effect, EffectKind};
use game_core::id::Id;
use game_core::map::GameMap;
use game_core::spell_book::SpellKind;
use game_core::time::Tick;
use game_core::tribes::Owner;
use game_core::unit::{Action, Unit, UnitId, UnitKind};
use game_core::vehicle::{State as VehicleState, Vehicle, VehicleId, VehicleKind};
use std::collections::{BTreeMap, BTreeSet};

/// A map position in world units, as the audio wants it: cells.
pub fn cells(x: u16, z: u16) -> Vec2 {
    Vec2::new(x as f32, z as f32) / pop3_format::WORLD_UNITS_PER_CELL as f32
}

/// A stable draw from a few numbers, for the original's random picks (which of two ids, a 1-in-n roll).
pub fn roll(a: u64, b: u64) -> u64 {
    let mut z = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_add(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// What of a person's doings makes a sound when it starts or ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Other,
    Tumbling,
    Whirled,
    Drowning,
    Dying,
    Casting(Option<SpellKind>),
}

pub fn phase(unit: &Unit) -> Phase {
    match unit.action {
        Action::Tumbling => Phase::Tumbling,
        Action::Whirled { .. } => Phase::Whirled,
        Action::Drowning { .. } => Phase::Drowning,
        Action::Dying { .. } | Action::Dead { .. } | Action::Spirit { .. } => Phase::Dying,
        Action::Casting { .. } => Phase::Casting(unit.casting()),
        _ => Phase::Other,
    }
}

/// A person as last seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    pub phase: Phase,
    pub kind: UnitKind,
    /// Its vertical speed, while thrown.
    pub fall: i16,
    pub invisible: bool,
}

pub fn seen(unit: &Unit) -> Seen {
    Seen { phase: phase(unit), kind: unit.kind, fall: unit.motion.velocity.y, invisible: unit.statuses.invisible.is_on() }
}

/// Rising faster than this, a thrown person screams (sound `0xC4`).
pub const SCREAM_RISE: i16 = 90;
/// Landing this fast or faster (falling), a thrown person grunts (`0x09`, `0x1B`: the same threshold as the dust,
/// "Person physics" [exe]).
pub const GRUNT_FALL: i16 = -100;

/// The sounds of a person going from `before` to `now` (sound.md "Sound id table"): a death cry (the shaman's own,
/// a ghost's poof; the angel none here, its roar is not modelled); the drowning cry; a scream on being thrown (a
/// shaman at any speed, the others rising faster than 90) or sucked into a whirlwind; a grunt landing at -100 or
/// faster and living (not a wildman); the casting shaman's shout (ours: as her cast starts, the original's at its
/// launch); a wildman converted; turning invisible and back.
pub fn person_cues(before: Option<Seen>, now: Seen, unit: &Unit, own: bool) -> Vec<SoundCue> {
    let Some(before) = before else { return Vec::new() };
    let mut cues = Vec::new();
    let shaman = now.kind == UnitKind::Shaman;
    let scream = if own { SoundCue::ShamanScream } else { SoundCue::EnemyShamanScream };
    if before.phase != now.phase {
        match now.phase {
            Phase::Dying => cues.extend(match now.kind {
                _ if unit.statuses.ghost => Some(SoundCue::GhostDie),
                UnitKind::Shaman if own => Some(SoundCue::ShamanDie),
                UnitKind::Shaman => Some(SoundCue::EnemyShamanDie),
                UnitKind::AngelOfDeath => None,
                _ => Some(SoundCue::Death),
            }),
            Phase::Drowning => cues.push(SoundCue::Drown),
            Phase::Tumbling if before.phase != Phase::Whirled => {
                if shaman {
                    cues.push(scream);
                } else if now.fall > SCREAM_RISE {
                    cues.push(SoundCue::IntoAir);
                }
            }
            Phase::Whirled => cues.push(if shaman { scream } else { SoundCue::IntoWind }),
            Phase::Casting(Some(spell)) if shaman => cues.push(if own { SoundCue::CastShout(spell) } else { SoundCue::EnemyCastShout(spell) }),
            _ => {}
        }
        if before.phase == Phase::Tumbling && matches!(now.phase, Phase::Other | Phase::Casting(_)) && before.fall <= GRUNT_FALL {
            match now.kind {
                UnitKind::Shaman => cues.push(SoundCue::ShamanLand),
                UnitKind::Wildman | UnitKind::AngelOfDeath => {}
                _ => cues.push(SoundCue::Land),
            }
        }
    }
    if before.kind == UnitKind::Wildman && now.kind != UnitKind::Wildman {
        cues.push(SoundCue::Converted);
    }
    if now.invisible != before.invisible {
        cues.push(if now.invisible { SoundCue::Invisible } else { SoundCue::InvisibleOff });
    }
    cues
}

/// Turns between two work sounds of one worker. Ours: the original plays one per animation cycle on its sync
/// frame (sound.md "Sound id table"), the client does not see the frame.
pub const WORK_EVERY: u32 = 8;

/// A wildman mutters 1 time in this many turns idle.
pub const MUTTER_ODDS: u64 = 256;

/// The sound a person makes this turn while it works or burns, its own (one at a time per owner: a per-turn sound
/// carries on rather than stacking). `draw` picks between the two eat/drink ids and rolls the mutter.
pub fn turn_cue(unit: &Unit, now: Tick, draw: u64) -> Option<SoundCue> {
    let beat = (now.to_wire().wrapping_add(unit.id.raw())).is_multiple_of(WORK_EVERY);
    match unit.action {
        Action::OnFire { .. } => Some(SoundCue::OnFire),
        Action::Grazing { drink, .. } => Some(match (drink, draw % 2 == 0) {
            (false, true) => SoundCue::Eat,
            (false, false) => SoundCue::WildEat,
            (true, true) => SoundCue::Drink,
            (true, false) => SoundCue::WildDrink,
        }),
        Action::Idle if unit.kind == UnitKind::Wildman => (draw % MUTTER_ODDS == 0).then_some(SoundCue::Mutter),
        Action::Chopping { .. } | Action::Hacking { .. } if beat => Some(SoundCue::Chop),
        Action::PickingUpWood { .. } if beat => Some(SoundCue::ChopPickUp),
        Action::Building { .. } | Action::Hammering if beat => Some(SoundCue::Hammer),
        Action::Flattening { .. } if beat => Some(SoundCue::Stomp),
        _ => None,
    }
}

/// The one-shots an effect makes when it appears.
pub fn effect_cues(kind: &EffectKind) -> Vec<SoundCue> {
    use SoundCue::*;
    match kind {
        EffectKind::Splash => vec![Splash],
        EffectKind::Explosion | EffectKind::PanickingBlast(_) => vec![BlastImpact],
        EffectKind::SimpleBlast(_) => vec![Whoosh],
        EffectKind::Lightning(_) => vec![Thunder],
        EffectKind::GhostArmy(_) => vec![GhostArmyCast],
        EffectKind::LandBridge(_) => vec![BridgeCast, BridgeCast2],
        EffectKind::AngelOfDeath(_) => vec![AngelCast, AngelFall],
        EffectKind::AngelOfDeathShockwave(_) => vec![AngelLand],
        EffectKind::Flatten(_) => vec![FlattenCast],
        EffectKind::Convert(_) => vec![ConvertCast],
        EffectKind::Hypnotism(_) => vec![HypnotismCast],
        EffectKind::Firebrand { .. } => vec![Throw],
        EffectKind::GroundFire => vec![FireCatches],
        _ => Vec::new(),
    }
}

/// Turns left under which the whirlwind's wind stops.
pub const WIND_TAIL: u32 = 12;

/// The loop an effect keeps going while it lives: a variety loop (a fresh variant each time) or the same sample.
/// The whirlwind's wind stops once fewer than `WIND_TAIL` turns remain; the lava flow's goes on to its end here
/// (the original stops it once the flow stops growing, not modelled).
pub fn effect_loop(kind: &EffectKind, left: u32) -> Option<(SoundCue, LoopKind)> {
    match kind {
        EffectKind::Whirlwind(_) if left >= WIND_TAIL => Some((SoundCue::Wind, LoopKind::Variety)),
        EffectKind::Swarm(_) => Some((SoundCue::Swarm, LoopKind::Variety)),
        EffectKind::Swamp(_) => Some((SoundCue::Swamp, LoopKind::Variety)),
        EffectKind::GroundFire => Some((SoundCue::GroundFire, LoopKind::Variety)),
        EffectKind::Erode(_) => Some((SoundCue::ErodeRumble, LoopKind::Repeat)),
        EffectKind::Earthquake(_) => Some((SoundCue::QuakeRumble, LoopKind::Repeat)),
        EffectKind::Volcano(_) => Some((SoundCue::VolcanoRumble, LoopKind::Repeat)),
        EffectKind::Lava(_) => Some((SoundCue::Lava, LoopKind::Repeat)),
        _ => None,
    }
}

/// A sound an effect makes at one point of its life, as its turns left go from `before` to `now`: the earthquake's
/// crack halfway (turn 60 of 120), the volcano's cone starting to rise (turn 80 of 160).
pub fn effect_moment(kind: &EffectKind, total: u32, before: u32, now: u32) -> Option<SoundCue> {
    let (cue, at) = match kind {
        EffectKind::Earthquake(_) => (SoundCue::QuakeCrack, total / 2),
        EffectKind::Volcano(_) => (SoundCue::VolcanoRise, total / 2),
        _ => return None,
    };
    (before > at && now <= at).then_some(cue)
}

/// A vehicle's moving loop, from setting off to parking (`0x4F` boat, `0x4E` balloon).
pub fn vehicle_loop(kind: VehicleKind, state: &VehicleState) -> Option<SoundCue> {
    matches!(state, VehicleState::Moving { .. }).then_some(match kind {
        VehicleKind::Boat => SoundCue::BoatRide,
        VehicleKind::Balloon => SoundCue::BalloonRide,
    })
}

fn wrecked(state: &VehicleState) -> bool {
    matches!(state, VehicleState::Sinking { .. } | VehicleState::Rising { .. })
}

/// A vehicle destroyed: going down or up, or gone without either (blown up).
pub fn vehicle_end(kind: VehicleKind, before: &VehicleState, now: Option<&VehicleState>) -> Option<SoundCue> {
    let ends = match now {
        Some(now) => wrecked(now) && !wrecked(before),
        None => !wrecked(before),
    };
    ends.then_some(match kind {
        VehicleKind::Boat => SoundCue::BoatExplode,
        VehicleKind::Balloon => SoundCue::BalloonPop,
    })
}

/// A building crackles while its burn countdown is above this (`0x53`: its fire's first turns).
pub const CRACKLE_ABOVE: u32 = 79;

/// Fights before the ones the battle crowd hears (`0x6C`: silent below 7).
pub const QUIET_FIGHTS: usize = 6;

/// The battle crowd's volume, 0..1: the summed closeness (0..1 each) of every fight after the 6th, capped.
pub fn crowd_volume(closeness: &[f32]) -> f32 {
    closeness.iter().skip(QUIET_FIGHTS).sum::<f32>().min(1.0)
}

/// Loop keys, one namespace per kind of owner.
fn key(kind: u64, raw: u32) -> LoopKey {
    LoopKey(kind << 32 | raw as u64)
}
const KEY_EFFECT: u64 = 2;
const KEY_VEHICLE: u64 = 3;
const KEY_BUILDING: u64 = 4;
const KEY_CROWD: LoopKey = LoopKey(5 << 32);
const KEY_SHIELD: LoopKey = LoopKey(6 << 32);

/// The loops one kind of owner keeps going: sends each frame's wanted set, stops what dropped out of it.
#[derive(Default)]
pub struct Kept(BTreeSet<LoopKey>);

impl Kept {
    pub fn update(&mut self, wanted: Vec<PlayLoop>, plays: &mut MessageWriter<PlayLoop>, stops: &mut MessageWriter<StopLoop>) {
        let now: BTreeSet<LoopKey> = wanted.iter().map(|l| l.key).collect();
        stops.write_batch(self.0.difference(&now).map(|&k| StopLoop(k)));
        plays.write_batch(wanted);
        self.0 = now;
    }
}

pub fn looping(key: LoopKey, cue: SoundCue, kind: LoopKind, at: Option<Vec2>, volume: f32) -> PlayLoop {
    PlayLoop { key, sound: Sound::from(cue), kind, at, volume }
}

/// The turn last seen: a turn going back is a new map, what was seen before is forgotten.
#[derive(Default)]
struct Turn(Option<Tick>);

impl Turn {
    /// (a new turn since the last frame, a new map).
    fn advance(&mut self, map: &GameMap) -> (bool, bool) {
        let renewed = self.0.is_some_and(|t| map.now.since(t) < 0);
        let ticked = self.0 != Some(map.now);
        self.0 = Some(map.now);
        (ticked, renewed)
    }
}

fn own(owner: Owner) -> bool {
    owner == Owner::Tribe(crate::units::PLAYER)
}

#[derive(Default)]
struct UnitWatch {
    turn: Turn,
    seen: BTreeMap<UnitId, Seen>,
}

fn unit_sounds(map: Res<CurrentMap>, mut watch: Local<UnitWatch>, mut sounds: MessageWriter<PlaySound>) {
    let map = &map.0;
    let watch = &mut *watch;
    let (ticked, renewed) = watch.turn.advance(map);
    if renewed {
        watch.seen.clear();
    }
    let mut seen_now = BTreeMap::new();
    let mut invisible_said = false;
    for unit in &map.units {
        let at = cells(unit.x, unit.z);
        let now = seen(unit);
        for cue in person_cues(watch.seen.get(&unit.id).copied(), now, unit, own(unit.owner)) {
            // The invisibility sweep sounds once, not once per person it hides.
            if cue == SoundCue::Invisible && std::mem::replace(&mut invisible_said, true) {
                continue;
            }
            sounds.write(PlaySound::at(cue, at));
        }
        seen_now.insert(unit.id, now);
        if ticked && unit.inside.is_none() {
            if let Some(cue) = turn_cue(unit, map.now, roll(unit.id.raw() as u64, map.now.to_wire() as u64)) {
                sounds.write(PlaySound::at(cue, at).owned_by(SoundOwner(unit.id.raw() as u64)));
            }
        }
    }
    watch.seen = seen_now;
}

#[derive(Default)]
struct EffectWatch {
    turn: Turn,
    /// Each effect's turns left.
    left: BTreeMap<Id<Effect>, u32>,
    loops: Kept,
}

fn effect_sounds(map: Res<CurrentMap>, mut watch: Local<EffectWatch>, mut sounds: MessageWriter<PlaySound>, mut plays: MessageWriter<PlayLoop>, mut stops: MessageWriter<StopLoop>) {
    let map = &map.0;
    let watch = &mut *watch;
    let first = watch.turn.0.is_none();
    if watch.turn.advance(map).1 {
        watch.left.clear();
    }
    let mut loops = Vec::new();
    let mut left_now = BTreeMap::new();
    for (id, effect) in map.effects.iter() {
        let at = cells(effect.x, effect.z);
        let left = effect.left.left().get();
        match watch.left.get(&id) {
            None if !first => {
                sounds.write_batch(effect_cues(&effect.kind).into_iter().map(|cue| PlaySound::at(cue, at)));
            }
            Some(&before) => {
                sounds.write_batch(effect_moment(&effect.kind, effect.left.total().get(), before, left).map(|cue| PlaySound::at(cue, at)));
            }
            None => {}
        }
        if let Some((cue, kind)) = effect_loop(&effect.kind, left) {
            loops.push(looping(key(KEY_EFFECT, id.raw()), cue, kind, Some(at), 1.0));
        }
        left_now.insert(id, left);
    }
    watch.left = left_now;
    watch.loops.update(loops, &mut plays, &mut stops);
}

#[derive(Default)]
struct ShotWatch {
    turn: Turn,
    shots: BTreeSet<(u8, u32)>,
}

/// A shot launched whooshes (`0xA1`): Blast's fireball, a firewarrior's bolt.
fn shot_sounds(map: Res<CurrentMap>, mut watch: Local<ShotWatch>, mut sounds: MessageWriter<PlaySound>) {
    let map = &map.0;
    let first = watch.turn.0.is_none();
    if watch.turn.advance(map).1 {
        watch.shots.clear();
    }
    let shots: Vec<((u8, u32), Vec2)> =
        map.fireballs.iter().map(|(id, f)| ((0, id.raw()), cells(f.at.0, f.at.1))).chain(map.bolts.iter().map(|(id, b)| ((1, id.raw()), cells(b.at.0, b.at.1)))).collect();
    if !first {
        sounds.write_batch(shots.iter().filter(|(k, _)| !watch.shots.contains(k)).map(|(_, at)| PlaySound::at(SoundCue::Whoosh, *at)));
    }
    watch.shots = shots.into_iter().map(|(k, _)| k).collect();
}

#[derive(Default)]
struct VehicleWatch {
    turn: Turn,
    seen: BTreeMap<VehicleId, (VehicleKind, VehicleState, Vec2)>,
    loops: Kept,
}

fn vehicle_sounds(map: Res<CurrentMap>, mut watch: Local<VehicleWatch>, mut sounds: MessageWriter<PlaySound>, mut plays: MessageWriter<PlayLoop>, mut stops: MessageWriter<StopLoop>) {
    let map = &map.0;
    let watch = &mut *watch;
    if watch.turn.advance(map).1 {
        watch.seen.clear();
    }
    let now: BTreeMap<VehicleId, (VehicleKind, VehicleState, Vec2)> = map.vehicles.iter().map(|(id, v): (VehicleId, &Vehicle)| (id, (v.kind, v.state.clone(), cells(v.x, v.z)))).collect();
    for (id, (kind, before, at)) in &watch.seen {
        let state = now.get(id).map(|(_, s, _)| s);
        if let Some(cue) = vehicle_end(*kind, before, state) {
            sounds.write(PlaySound::at(cue, now.get(id).map_or(*at, |v| v.2)));
        }
    }
    let loops = now.iter().filter_map(|(id, (kind, state, at))| vehicle_loop(*kind, state).map(|cue| looping(key(KEY_VEHICLE, id.raw()), cue, LoopKind::Variety, Some(*at), 1.0))).collect();
    watch.seen = now;
    watch.loops.update(loops, &mut plays, &mut stops);
}

#[derive(Default)]
struct BuildingWatch {
    turn: Turn,
    stages: BTreeMap<BuildingId, Stage>,
    loops: Kept,
}

/// A building's fire crackling for its first turns, and the bell when one is finished.
fn building_sounds(map: Res<CurrentMap>, mut watch: Local<BuildingWatch>, mut sounds: MessageWriter<PlaySound>, mut plays: MessageWriter<PlayLoop>, mut stops: MessageWriter<StopLoop>) {
    let map = &map.0;
    let watch = &mut *watch;
    if watch.turn.advance(map).1 {
        watch.stages.clear();
    }
    let mut loops = Vec::new();
    let mut stages = BTreeMap::new();
    for b in &map.buildings {
        let at = cells(b.x, b.z);
        let stage = b.stage();
        if stage == Stage::Built && matches!(watch.stages.get(&b.id), Some(Stage::UnderConstruction { .. })) {
            sounds.write(PlaySound::at(SoundCue::BuildingDone, at));
        }
        if b.burning.is_some_and(|c| c.left().get() > CRACKLE_ABOVE) {
            loops.push(looping(key(KEY_BUILDING, b.id.raw()), SoundCue::BuildingFire, LoopKind::Variety, Some(at), 1.0));
        }
        stages.insert(b.id, stage);
    }
    watch.stages = stages;
    watch.loops.update(loops, &mut plays, &mut stops);
}

/// The battle crowd, flat, over the fights in reach.
fn crowd_sound(map: Res<CurrentMap>, listener: Res<ListenerState>, mut kept: Local<Kept>, mut plays: MessageWriter<PlayLoop>, mut stops: MessageWriter<StopLoop>) {
    let closeness: Vec<f32> = map.0.coordinators.melees.iter().map(|(_, m)| place(&listener.0, cells(m.at.0, m.at.1)).map_or(0.0, |p| p.gain)).collect();
    let volume = crowd_volume(&closeness);
    let loops = if volume > 0.0 { vec![looping(KEY_CROWD, SoundCue::BattleCrowd, LoopKind::Repeat, None, volume)] } else { Vec::new() };
    kept.update(loops, &mut plays, &mut stops);
}

/// A shielded person alive within reach of the listener (ours for the original's "drawn near the camera").
pub fn shield_in_reach<'a>(units: impl IntoIterator<Item = &'a Unit>, listener: &Listener) -> bool {
    units.into_iter().any(|u| u.is_alive() && u.statuses.shield.is_on() && place(listener, cells(u.x, u.z)).is_ok())
}

/// The shield's hum, flat, while a shielded person is near.
fn shield_sound(map: Res<CurrentMap>, listener: Res<ListenerState>, mut kept: Local<Kept>, mut plays: MessageWriter<PlayLoop>, mut stops: MessageWriter<StopLoop>) {
    let loops = shield_in_reach(&map.0.units, &listener.0).then(|| looping(KEY_SHIELD, SoundCue::ShieldHum, LoopKind::Repeat, None, 1.0)).into_iter().collect();
    kept.update(loops, &mut plays, &mut stops);
}

pub struct AudioWorldPlugin;

impl Plugin for AudioWorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (unit_sounds, effect_sounds, shot_sounds, vehicle_sounds, building_sounds, crowd_sound, shield_sound).in_set(crate::menu::Gameplay));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use UnitKind::*;

    fn person(kind: UnitKind) -> Unit {
        Unit::new(Id::from_raw(1), Owner::Tribe(crate::units::PLAYER), kind, (0, 0))
    }

    fn at(phase: Phase, kind: UnitKind) -> Seen {
        Seen { phase, kind, fall: 0, invisible: false }
    }

    fn cues(before: Seen, now: Seen, own: bool) -> Vec<SoundCue> {
        person_cues(Some(before), now, &person(now.kind), own)
    }

    #[test]
    fn nothing_sounds_the_first_time_a_person_is_seen() {
        assert!(person_cues(None, at(Phase::Dying, Brave), &person(Brave), true).is_empty());
    }

    #[test]
    fn a_death_cries_once_the_shaman_with_her_own_line() {
        assert_eq!(cues(at(Phase::Other, Brave), at(Phase::Dying, Brave), true), vec![SoundCue::Death]);
        assert_eq!(cues(at(Phase::Other, Shaman), at(Phase::Dying, Shaman), true), vec![SoundCue::ShamanDie]);
        assert_eq!(cues(at(Phase::Other, Shaman), at(Phase::Dying, Shaman), false), vec![SoundCue::EnemyShamanDie]);
        assert!(cues(at(Phase::Dying, Brave), at(Phase::Dying, Brave), true).is_empty());
        assert!(cues(at(Phase::Other, AngelOfDeath), at(Phase::Dying, AngelOfDeath), false).is_empty());
    }

    #[test]
    fn a_ghost_poofs_instead_of_crying() {
        let mut ghost = person(Warrior);
        ghost.statuses.ghost = true;
        assert_eq!(person_cues(Some(at(Phase::Other, Warrior)), at(Phase::Dying, Warrior), &ghost, true), vec![SoundCue::GhostDie]);
    }

    #[test]
    fn only_a_fast_throw_screams_but_a_shaman_always_does() {
        let rising = |kind, fall| Seen { fall, ..at(Phase::Tumbling, kind) };
        assert_eq!(cues(at(Phase::Other, Warrior), rising(Warrior, 91), false), vec![SoundCue::IntoAir]);
        assert!(cues(at(Phase::Other, Warrior), rising(Warrior, 90), false).is_empty());
        assert_eq!(cues(at(Phase::Other, Shaman), rising(Shaman, 0), true), vec![SoundCue::ShamanScream]);
        assert_eq!(cues(at(Phase::Other, Shaman), rising(Shaman, 0), false), vec![SoundCue::EnemyShamanScream]);
        assert_eq!(cues(at(Phase::Other, Brave), at(Phase::Whirled, Brave), true), vec![SoundCue::IntoWind]);
        assert!(cues(at(Phase::Whirled, Brave), rising(Brave, 200), true).is_empty(), "thrown out of the whirlwind: already screamed");
    }

    #[test]
    fn a_hard_landing_grunts_unless_it_kills_or_drowns() {
        let falling = |kind| Seen { fall: -100, ..at(Phase::Tumbling, kind) };
        assert_eq!(cues(falling(Warrior), at(Phase::Other, Warrior), false), vec![SoundCue::Land]);
        assert_eq!(cues(falling(Shaman), at(Phase::Other, Shaman), false), vec![SoundCue::ShamanLand]);
        assert!(cues(falling(Wildman), at(Phase::Other, Wildman), false).is_empty());
        assert!(cues(Seen { fall: -99, ..falling(Warrior) }, at(Phase::Other, Warrior), false).is_empty(), "soft");
        assert_eq!(cues(falling(Warrior), at(Phase::Dying, Warrior), false), vec![SoundCue::Death]);
        assert_eq!(cues(falling(Warrior), at(Phase::Drowning, Warrior), false), vec![SoundCue::Drown]);
    }

    #[test]
    fn a_casting_shaman_shouts_her_spell_by_tribe() {
        let cast = Phase::Casting(Some(SpellKind::Blast));
        assert_eq!(cues(at(Phase::Other, Shaman), at(cast, Shaman), true), vec![SoundCue::CastShout(SpellKind::Blast)]);
        assert_eq!(cues(at(Phase::Other, Shaman), at(cast, Shaman), false), vec![SoundCue::EnemyCastShout(SpellKind::Blast)]);
        assert!(cues(at(Phase::Other, Shaman), at(Phase::Casting(None), Shaman), true).is_empty());
    }

    #[test]
    fn a_converted_wildman_chimes_and_invisibility_sounds_both_ways() {
        assert_eq!(cues(at(Phase::Other, Wildman), at(Phase::Other, Brave), false), vec![SoundCue::Converted]);
        let hidden = Seen { invisible: true, ..at(Phase::Other, Brave) };
        assert_eq!(cues(at(Phase::Other, Brave), hidden, true), vec![SoundCue::Invisible]);
        assert_eq!(cues(hidden, at(Phase::Other, Brave), true), vec![SoundCue::InvisibleOff]);
    }

    #[test]
    fn the_crowd_hears_only_fights_after_the_sixth() {
        assert_eq!(crowd_volume(&[1.0; 6]), 0.0);
        assert_eq!(crowd_volume(&[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.25]), 0.25);
        assert_eq!(crowd_volume(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.75]), 1.0, "capped");
    }

    #[test]
    fn vehicles_sound_under_way_and_when_destroyed() {
        let moving = VehicleState::Moving { to: (0, 0) };
        assert_eq!(vehicle_loop(VehicleKind::Boat, &moving), Some(SoundCue::BoatRide));
        assert_eq!(vehicle_loop(VehicleKind::Balloon, &moving), Some(SoundCue::BalloonRide));
        assert_eq!(vehicle_loop(VehicleKind::Boat, &VehicleState::Parked), None);
        let sinking = VehicleState::Sinking { depth: 0, pitch: Default::default() };
        assert_eq!(vehicle_end(VehicleKind::Boat, &moving, Some(&sinking)), Some(SoundCue::BoatExplode));
        assert_eq!(vehicle_end(VehicleKind::Boat, &sinking, None), None, "gone under after sinking");
        assert_eq!(vehicle_end(VehicleKind::Balloon, &moving, None), Some(SoundCue::BalloonPop));
        assert_eq!(vehicle_end(VehicleKind::Balloon, &moving, Some(&moving)), None);
    }

    #[test]
    fn effects_sound_by_kind() {
        assert_eq!(effect_cues(&EffectKind::Splash), vec![SoundCue::Splash]);
        assert_eq!(effect_cues(&EffectKind::Explosion), vec![SoundCue::BlastImpact]);
        assert!(effect_cues(&EffectKind::Smoke).is_empty());
        assert_eq!(effect_loop(&EffectKind::GroundFire, 5), Some((SoundCue::GroundFire, LoopKind::Variety)));
        assert_eq!(effect_loop(&EffectKind::Splash, 5), None);
    }

    #[test]
    fn work_sounds_on_their_beat_eating_and_burning_every_turn() {
        let mut u = person(Brave);
        u.action = Action::Hammering;
        let beats = (0..WORK_EVERY * 2).filter(|&t| turn_cue(&u, Tick::from_wire(t), 0).is_some()).count();
        assert_eq!(beats, 2);
        u.action = Action::Grazing { drink: false, left: game_core::time::Countdown::new(game_core::time::Ticks::new(99)) };
        assert!((0..4).all(|t| turn_cue(&u, Tick::from_wire(t), 0).is_some()), "eating back to back");
        u.action = Action::OnFire { away: (0, 0), left: game_core::time::Countdown::new(game_core::time::Ticks::new(5)) };
        assert!((0..4).all(|t| turn_cue(&u, Tick::from_wire(t), 0) == Some(SoundCue::OnFire)));
    }

    #[test]
    fn loop_keys_never_collide_across_kinds() {
        assert_ne!(key(KEY_EFFECT, 7), key(KEY_VEHICLE, 7));
        assert_ne!(key(KEY_BUILDING, 0), KEY_CROWD);
    }
}
