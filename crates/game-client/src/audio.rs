//! Sound output (docs/architecture.md "Client: audio", sound.md "Playback in the client"). Systems ask with
//! messages: `PlaySound` (a one-shot), `PlayLoop`/`StopLoop` (a sound that goes on until its owner stops it,
//! sent again to move it or change its volume), `PlayMusic` (a track, with drum loops chained under it) and
//! `SetIntensity` (how much the local player fights, for the drums). This plugin picks the clip from the
//! `SoundLibrary`, places it for the camera (`audio_space`), mixes it (`audio_mix::Mixer`, the sound's own
//! volume) and keeps its gains up to date while it plays, through our own stereo source (`audio_source`).
//! Stopped sounds fade out. Bevy's `AudioPlugin` (the device) is left out by `--mute=on` and in headless runs:
//! requests are then still resolved, and logged by `--debug-audio`, but nothing is spawned.

use crate::audio_drums::{DrumState, Intensity};
use crate::audio_mix::{Channel, Mixer};
use crate::audio_source::{Gains, PannedSound};
use crate::audio_space::{equal_power, place, Listener, OutOfReach, Placement};
use crate::camera::CameraRig;
use crate::game_speed::GameSpeed;
use crate::menu::AppState;
use bevy::audio::{AddAudioSource, AudioSinkPlayback, AudioSource};
use bevy::prelude::*;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

/// A family of sounds, one of the original's sound ids (sound.md "Sound ids"): its variants (`chop1`..`chop5`)
/// are interchangeable. Filled in as sounds are wired.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SoundCue {
    /// A menu entry picked.
    Click,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MusicTrack {
    /// Behind the menus (`idea11`).
    Menu,
    /// In a game: one of its variants at random (tracks 2-5).
    Game,
}

/// Which variant of a family to play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// One at random.
    Any,
    /// Exactly this one (4: `chop4`); nothing if the library has no such variant.
    Variant(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sound {
    pub cue: SoundCue,
    pub pick: Pick,
}

impl Sound {
    #[allow(dead_code, reason = "API for the sound callers to come (TODO.md Sound)")]
    pub fn any(cue: SoundCue) -> Self {
        Sound { cue, pick: Pick::Any }
    }

    #[allow(dead_code, reason = "API for the sound callers to come (TODO.md Sound)")]
    pub fn variant(cue: SoundCue, n: u8) -> Self {
        Sound { cue, pick: Pick::Variant(n) }
    }
}

impl From<SoundCue> for Sound {
    fn from(cue: SoundCue) -> Self {
        Sound { cue, pick: Pick::Any }
    }
}

/// Whose sound it is (a unit's id): an owner gets no other sound while one of its sounds plays, so per-tick
/// sounds (crackle, eating) carry on instead of stacking (sound.md "Sound ids").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SoundOwner(pub u64);

/// Plays a sound once on the Fx channel: at a map position (cells), heard from the camera, or flat (interface
/// sounds).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct PlaySound {
    pub sound: Sound,
    pub at: Option<Vec2>,
    pub owner: Option<SoundOwner>,
}

impl PlaySound {
    pub fn flat(sound: impl Into<Sound>) -> Self {
        PlaySound { sound: sound.into(), at: None, owner: None }
    }

    #[allow(dead_code, reason = "API for the sound callers to come (TODO.md Sound)")]
    pub fn at(sound: impl Into<Sound>, at: Vec2) -> Self {
        PlaySound { sound: sound.into(), at: Some(at), owner: None }
    }

    #[allow(dead_code, reason = "API for the sound callers to come (TODO.md Sound)")]
    pub fn owned_by(self, owner: SoundOwner) -> Self {
        PlaySound { owner: Some(owner), ..self }
    }
}

/// Names a loop for its owner (e.g. from a model's id and what the loop is), to move, re-weigh or stop it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LoopKey(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code, reason = "API for the sound callers to come (TODO.md Sound)")]
pub enum LoopKind {
    /// The same clip over and over (battle crowd, shield hum, rumbles).
    Repeat,
    /// A fresh variant each time one ends (fire, wind, swarm, swamp, guard post, boat/balloon); stopping lets
    /// the current one finish.
    Variety,
}

/// Starts a loop on the Fx channel, or updates it if `key` is already going: send it again as its owner moves
/// or as its weight changes (the ambience layers, the battle crowd).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct PlayLoop {
    pub key: LoopKey,
    pub sound: Sound,
    pub kind: LoopKind,
    pub at: Option<Vec2>,
    /// 0..1, times the sound's own volume.
    pub volume: f32,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StopLoop(pub LoopKey);

/// Switches the music to a track (looping, with drum loops under it), or stops it.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct PlayMusic(pub Option<MusicTrack>);

/// How much the local player fights, for the next drum loop.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetIntensity(pub Intensity);

/// One playable sound.
#[derive(Clone, Debug)]
pub struct SoundClip {
    /// Its number in its family, from its source (`chop4`: 4); 1 for a family of one.
    pub variant: u8,
    pub source: AudioSource,
    /// Where it comes from, for the log: `Soundd2.sdt#41`, `sounds/chop4.ogg`.
    pub reference: String,
}

/// A sound id's own settings (sound.md "Sound ids").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CueInfo {
    /// 0..=127.
    pub volume: u8,
    /// Higher is kept first when too many sounds play.
    pub priority: u8,
    /// Plays while the game is paused (the original's info window clicks).
    pub while_paused: bool,
}

impl Default for CueInfo {
    fn default() -> Self {
        CueInfo { volume: 127, priority: 0, while_paused: false }
    }
}

impl CueInfo {
    pub fn gain(&self) -> f32 {
        self.volume.min(127) as f32 / 127.0
    }
}

#[derive(Clone, Debug, Default)]
pub struct Family {
    pub info: CueInfo,
    pub clips: Vec<SoundClip>,
}

/// What can be played: filled from the install (`audio_load`), empty with `--no-original`.
#[derive(Resource, Default, Debug)]
pub struct SoundLibrary {
    pub sounds: HashMap<SoundCue, Family>,
    pub music: HashMap<MusicTrack, Vec<SoundClip>>,
    /// Drum banks, each its loops by entry (0 calm, 1 attacking, 2 melee, 3 the extra melee one).
    pub drums: Vec<Vec<SoundClip>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Skip {
    NoClip,
    NoVariant(u8),
    OutOfReach(OutOfReach),
    /// The same sound started under `REPEAT_GAP` ago.
    TooSoon,
    /// `MAX_FX` sounds play, none ranked below this one.
    Busy,
    /// Its owner has a sound playing.
    OwnerBusy,
    /// The game is paused or behind a menu, and this sound waits for it.
    Held,
    MuteAll,
    DeviceOff,
}

fn pick_from(clips: &[SoundClip], pick: Pick, seed: u64) -> Result<&SoundClip, Skip> {
    if clips.is_empty() {
        return Err(Skip::NoClip);
    }
    match pick {
        Pick::Any => Ok(&clips[(seed % clips.len() as u64) as usize]),
        Pick::Variant(n) => clips.iter().find(|c| c.variant == n).ok_or(Skip::NoVariant(n)),
    }
}

impl SoundLibrary {
    /// The clip for `sound` and its family's settings; `seed` chooses among the variants for `Pick::Any`.
    pub fn pick(&self, sound: Sound, seed: u64) -> Result<(&SoundClip, CueInfo), Skip> {
        let family = self.sounds.get(&sound.cue).ok_or(Skip::NoClip)?;
        Ok((pick_from(&family.clips, sound.pick, seed)?, family.info))
    }

    pub fn pick_music(&self, track: MusicTrack, seed: u64) -> Result<&SoundClip, Skip> {
        pick_from(self.music.get(&track).map_or(&[][..], |c| c.as_slice()), Pick::Any, seed)
    }
}

/// A request that plays.
#[derive(Clone, Debug)]
pub struct Played {
    pub clip: SoundClip,
    pub info: CueInfo,
    pub placement: Option<Placement>,
    /// Left and right: mixer x the sound's own volume x falloff x balance.
    pub gains: (f32, f32),
}

/// Left/right gains: `volume` flat on both sides, or faded and balanced by a placement.
pub fn voice_gains(volume: f32, placement: Option<Placement>) -> (f32, f32) {
    match placement {
        None => (volume, volume),
        Some(p) => {
            let (l, r) = equal_power(p.pan);
            (l * volume * p.gain, r * volume * p.gain)
        }
    }
}

/// What a sound request plays, or why not; `weight` scales it (a loop's volume).
pub fn resolve_sound(sound: Sound, at: Option<Vec2>, weight: f32, library: &SoundLibrary, listener: &Listener, mixer: &Mixer, device: bool, seed: u64) -> Result<Played, Skip> {
    let (clip, info) = library.pick(sound, seed)?;
    let placement = at.map(|at| place(listener, at)).transpose().map_err(Skip::OutOfReach)?;
    played(clip, info, placement, mixer.gain(Channel::Fx) * weight, mixer, device)
}

pub fn resolve_music(clip: Result<&SoundClip, Skip>, mixer: &Mixer, device: bool) -> Result<Played, Skip> {
    played(clip?, CueInfo::default(), None, mixer.gain(Channel::Music), mixer, device)
}

fn played(clip: &SoundClip, info: CueInfo, placement: Option<Placement>, gain: f32, mixer: &Mixer, device: bool) -> Result<Played, Skip> {
    if mixer.mute_all {
        return Err(Skip::MuteAll);
    }
    if !device {
        return Err(Skip::DeviceOff);
    }
    Ok(Played { gains: voice_gains(gain * info.gain(), placement), clip: clip.clone(), info, placement })
}

/// The same sound never restarts within this (sound.md "Mixing").
pub const REPEAT_GAP: Duration = Duration::from_millis(250);
/// Effect sounds playing at once (sound.md "Mixing").
pub const MAX_FX: usize = 10;
/// Fade-out of a stopped effect, seconds: short (sound.md "Mixing"; the length is a guess).
pub const FX_FADE: f32 = 0.1;
/// Fade-out of stopped music, seconds: longer (sound.md "Mixing"; the length is a guess).
pub const MUSIC_FADE: f32 = 1.5;

pub fn too_soon(last: Option<Duration>, now: Duration) -> bool {
    last.is_some_and(|t| now.saturating_sub(t) < REPEAT_GAP)
}

/// Where the game stands, for what may play (sound.md "Sound ids").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moment {
    Running,
    /// The game paused (P): only sounds flagged for it play.
    Paused,
    /// A menu over the game or the main menu: only flat (interface) sounds play.
    Overlay,
}

pub fn moment(state: Option<&State<AppState>>, speed: Option<&GameSpeed>) -> Moment {
    match state.map(|s| *s.get()) {
        Some(AppState::Playing | AppState::Ended) | None if speed.is_some_and(|s| s.paused) => Moment::Paused,
        Some(AppState::Playing | AppState::Ended) | None => Moment::Running,
        Some(_) => Moment::Overlay,
    }
}

/// An effect waits at this moment (music never does).
pub fn waits(moment: Moment, positional: bool, info: &CueInfo) -> bool {
    match moment {
        Moment::Running => false,
        Moment::Paused => !info.while_paused,
        Moment::Overlay => positional,
    }
}

/// How much a playing (or new) effect is worth keeping: its priority first, then fewer copies of the same
/// sound, then closeness (sound.md "Mixing": the quietest-ranked is cut; the order of these is our reading).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rank {
    pub priority: u8,
    pub copies: usize,
    pub closeness: f32,
}

impl Rank {
    fn key(&self) -> (u8, std::cmp::Reverse<usize>, f32) {
        (self.priority, std::cmp::Reverse(self.copies), self.closeness)
    }

    fn below(&self, other: &Rank) -> bool {
        self.key().partial_cmp(&other.key()) == Some(std::cmp::Ordering::Less)
    }
}

/// Room for a new effect among `playing`: Ok(None) free, Ok(Some(i)) by cutting `playing[i]`, Err if all
/// outrank it.
pub fn make_room(playing: &[Rank], new: Rank) -> Result<Option<usize>, Skip> {
    if playing.len() < MAX_FX {
        return Ok(None);
    }
    let lowest = (0..playing.len()).min_by(|&a, &b| playing[a].key().partial_cmp(&playing[b].key()).unwrap_or(std::cmp::Ordering::Equal));
    match lowest {
        Some(i) if playing[i].below(&new) => Ok(Some(i)),
        _ => Err(Skip::Busy),
    }
}

/// A stopping sound's gain factor, from 1 down to 0 over its fade.
pub fn fade_factor(left: f32, total: f32) -> f32 {
    if total <= 0.0 { 0.0 } else { (left / total).clamp(0.0, 1.0) }
}

fn skip_reason(skip: &Skip) -> String {
    match skip {
        Skip::NoClip => "no clip in library".into(),
        Skip::NoVariant(n) => format!("no variant {n}"),
        Skip::OutOfReach(o) => format!("out of range (dist={:.1})", o.distance),
        Skip::TooSoon => "same sound under 250 ms ago".into(),
        Skip::Busy => "too many sounds".into(),
        Skip::OwnerBusy => "its owner has a sound playing".into(),
        Skip::Held => "the game is paused or behind a menu".into(),
        Skip::MuteAll => "mute all".into(),
        Skip::DeviceOff => "audio device off".into(),
    }
}

/// The `--debug-audio` line for an effect: `what` is `fx` or `fx loop=K start`.
pub fn describe_fx(what: &str, sound: Sound, at: Option<Vec2>, result: &Result<Played, Skip>) -> String {
    let asked = match sound.pick {
        Pick::Any => "any".to_string(),
        Pick::Variant(n) => n.to_string(),
    };
    let at = at.map_or("none".to_string(), |p| format!("({:.1},{:.1})", p.x, p.y));
    let cue = format!("{:?}", sound.cue);
    match result {
        Ok(p) => {
            let place = p.placement.map_or(String::new(), |pl| format!(" dist={:.1} pan={:.2}", pl.distance, pl.pan));
            format!("audio {what} cue={cue}#{} (asked {asked}) ref={} at={at}{place} gain={:.2}/{:.2}", p.clip.variant, p.clip.reference, p.gains.0, p.gains.1)
        }
        Err(skip) => format!("audio {what} cue={cue} (asked {asked}) at={at} skipped: {}", skip_reason(skip)),
    }
}

/// The `--debug-audio` line for music: `what` is `track=Game` or `drums bank=3 entry=1`.
pub fn describe_music(what: &str, result: &Result<Played, Skip>) -> String {
    match result {
        Ok(p) => format!("audio music {what} ref={} gain={:.2}", p.clip.reference, p.gains.0),
        Err(skip) => format!("audio music {what} skipped: {}", skip_reason(skip)),
    }
}

/// Bevy's audio device is there (its `AudioPlugin` was added).
#[derive(Resource, Debug)]
pub struct AudioOut;

/// `--debug-audio`: every request logged.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct AudioDebug(pub bool);

/// `--mute=on` left the device closed for this run.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DeviceMuted(pub bool);

#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct ListenerState(pub Listener);

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CurrentMusic(pub Option<MusicTrack>);

/// The drum layer under the music.
#[derive(Resource, Debug, Default)]
struct Drums {
    /// The bank chosen when the music started; None: no drums.
    bank: Option<usize>,
    state: DrumState,
    intensity: Intensity,
    /// Why the last loop could not play, logged once.
    skipped: Option<Skip>,
}

/// When each sound last started, real time.
#[derive(Resource, Debug, Default)]
struct LastStarts(HashMap<SoundCue, Duration>);

/// Variant draws so far.
#[derive(Resource, Debug, Default)]
struct Draws(u64);

impl Draws {
    /// The next seed: a running count spread over all values.
    fn next(&mut self) -> u64 {
        self.0 += 1;
        let mut z = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LoopState {
    sound: Sound,
    kind: LoopKind,
    at: Option<Vec2>,
    volume: f32,
    /// A variety loop told to stop: its current variant finishes, no next one.
    stopping: bool,
    /// Has played: coming back (its next variant, back in reach, after being cut) it only takes a free slot,
    /// never cuts another sound, or loops would keep cutting each other.
    started: bool,
    /// Why it last could not play, logged once.
    skipped: Option<Skip>,
}

/// The loops their owners keep going.
#[derive(Resource, Debug, Default)]
struct Loops(BTreeMap<LoopKey, LoopState>);

/// What a voice is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Effect,
    Track,
    Drum,
}

/// One playing sound.
#[derive(Component, Debug, Clone)]
pub struct Voice {
    pub role: Role,
    pub cue: Option<SoundCue>,
    pub info: CueInfo,
    /// Map position, cells; None plays flat.
    pub at: Option<Vec2>,
    pub loop_key: Option<LoopKey>,
    pub owner: Option<SoundOwner>,
    /// A loop's volume, 0..1.
    pub weight: f32,
    gains: Arc<Gains>,
}

impl Voice {
    fn new(role: Role) -> Self {
        Voice { role, cue: None, info: CueInfo::default(), at: None, loop_key: None, owner: None, weight: 1.0, gains: Arc::default() }
    }

    fn channel(&self) -> Channel {
        match self.role {
            Role::Effect => Channel::Fx,
            Role::Track | Role::Drum => Channel::Music,
        }
    }
}

/// A stopped sound fading out, seconds left of `total`; despawned at the end.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Fading {
    pub left: f32,
    pub total: f32,
}

/// Frames a player has waited for Bevy to give it a sink.
#[derive(Component, Clone, Copy, Debug, Default)]
struct Unsunk(u32);

/// Frames after which a player still without a sink is dropped (no device behind it).
const SINK_WAIT: u32 = 120;

pub struct SoundPlugin {
    pub debug: bool,
    pub device_muted: bool,
    /// The install's `Sound` folder; None with `--no-original`.
    pub original_dir: Option<std::path::PathBuf>,
}

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<bevy::audio::AudioPlugin>() {
            app.insert_resource(AudioOut).add_audio_source::<PannedSound>();
        }
        app.insert_resource(AudioDebug(self.debug))
            .insert_resource(DeviceMuted(self.device_muted))
            .insert_resource(self.original_dir.as_deref().map(crate::audio_load::load_original).unwrap_or_default())
            .init_resource::<ListenerState>()
            .init_resource::<CurrentMusic>()
            .init_resource::<Drums>()
            .init_resource::<LastStarts>()
            .init_resource::<Draws>()
            .init_resource::<Loops>()
            .init_resource::<Mixer>()
            .add_message::<PlaySound>()
            .add_message::<PlayLoop>()
            .add_message::<StopLoop>()
            .add_message::<PlayMusic>()
            .add_message::<SetIntensity>()
            .add_systems(Update, (track_listener, music_for_state, play_sounds, run_loops, play_music, run_drums, apply_voices, reap_sinkless).chain());
    }
}

/// Spawning needs the device and our sound asset type (registered with it).
#[derive(bevy::ecs::system::SystemParam)]
struct Output<'w, 's> {
    commands: Commands<'w, 's>,
    device: Option<Res<'w, AudioOut>>,
    assets: Option<ResMut<'w, Assets<PannedSound>>>,
    debug: Res<'w, AudioDebug>,
}

impl Output<'_, '_> {
    fn on(&self) -> bool {
        self.device.is_some() && self.assets.is_some()
    }

    fn spawn(&mut self, played: &Played, voice: Voice, settings: PlaybackSettings) -> Option<Entity> {
        let assets = self.assets.as_mut()?;
        let gains = Gains::new(played.gains.0, played.gains.1);
        let handle = assets.add(PannedSound { source: played.clip.source.clone(), gains: gains.clone() });
        Some(self.commands.spawn((AudioPlayer(handle), settings, Voice { gains, ..voice }, Unsunk::default())).id())
    }

    fn log(&self, line: impl FnOnce() -> String) {
        if self.debug.0 {
            info!("{}", line());
        }
    }
}

/// Starts a sound's fade-out (kept if already fading).
fn fade_out(commands: &mut Commands, e: Entity, secs: f32) {
    commands.entity(e).insert_if_new(Fading { left: secs, total: secs });
}

fn track_listener(rig: Option<Res<CameraRig>>, mut listener: ResMut<ListenerState>) {
    if let Some(rig) = rig {
        listener.0 = Listener { focus: rig.focus, forward: rig.forward(), right: rig.right() };
    }
}

/// The menu's music behind the menus, the game's in a game.
fn music_for_state(state: Option<Res<State<AppState>>>, current: Res<CurrentMusic>, mut music: MessageWriter<PlayMusic>) {
    let Some(state) = state.filter(|s| s.is_changed()) else { return };
    let track = match state.get() {
        AppState::Menu => MusicTrack::Menu,
        AppState::Playing => MusicTrack::Game,
        _ => return,
    };
    if current.0 != Some(track) {
        music.write(PlayMusic(Some(track)));
    }
}

type Playing<'w, 's> = Query<'w, 's, (Entity, &'static Voice), Without<Fading>>;

/// The playing effects (not those fading out), ranked.
fn ranks(voices: &Playing, listener: &Listener) -> Vec<(Entity, Rank)> {
    let fx: Vec<(Entity, &Voice)> = voices.iter().filter(|(_, v)| v.role == Role::Effect).collect();
    fx.iter()
        .map(|(e, v)| {
            let copies = fx.iter().filter(|(_, o)| o.cue == v.cue).count();
            let closeness = v.at.map_or(1.0, |at| place(listener, at).map_or(0.0, |p| p.gain));
            (*e, Rank { priority: v.info.priority, copies, closeness })
        })
        .collect()
}

/// Makes room for a new effect: fades out the lowest-ranked one if the new one outranks it and `may_cut`.
fn admit(played: &Played, cue: SoundCue, may_cut: bool, voices: &Playing, listener: &Listener, commands: &mut Commands) -> Result<(), Skip> {
    let ranked = ranks(voices, listener);
    if !may_cut && ranked.len() >= MAX_FX {
        return Err(Skip::Busy);
    }
    let copies = voices.iter().filter(|(_, v)| v.cue == Some(cue)).count() + 1;
    let new = Rank { priority: played.info.priority, copies, closeness: played.placement.map_or(1.0, |p| p.gain) };
    let list: Vec<Rank> = ranked.iter().map(|r| r.1).collect();
    if let Some(i) = make_room(&list, new)? {
        fade_out(commands, ranked[i].0, FX_FADE);
    }
    Ok(())
}

fn play_sounds(
    mut out: Output,
    mut requests: MessageReader<PlaySound>,
    library: Res<SoundLibrary>,
    listener: Res<ListenerState>,
    mixer: Res<Mixer>,
    time: Res<Time<Real>>,
    state: Option<Res<State<AppState>>>,
    speed: Option<Res<GameSpeed>>,
    mut last: ResMut<LastStarts>,
    mut draws: ResMut<Draws>,
    voices: Playing,
) {
    let now = time.elapsed();
    let moment = moment(state.as_deref(), speed.as_deref());
    for request in requests.read() {
        let cue = request.sound.cue;
        let mut result = resolve_sound(request.sound, request.at, 1.0, &library, &listener.0, &mixer, out.on(), draws.next());
        if let Ok(played) = &result {
            if waits(moment, request.at.is_some(), &played.info) {
                result = Err(Skip::Held);
            } else if request.owner.is_some_and(|o| voices.iter().any(|(_, v)| v.owner == Some(o))) {
                result = Err(Skip::OwnerBusy);
            } else if too_soon(last.0.get(&cue).copied(), now) {
                result = Err(Skip::TooSoon);
            } else if let Err(skip) = admit(played, cue, true, &voices, &listener.0, &mut out.commands) {
                result = Err(skip);
            }
        }
        out.log(|| describe_fx("fx", request.sound, request.at, &result));
        let Ok(played) = result else { continue };
        last.0.insert(cue, now);
        let voice = Voice { cue: Some(cue), info: played.info, at: request.at, owner: request.owner, ..Voice::new(Role::Effect) };
        out.spawn(&played, voice, PlaybackSettings::DESPAWN);
    }
}

/// Logged when a loop starts, or when why it cannot changes (out of reach only once).
fn news(now: Option<Skip>, before: Option<Skip>) -> bool {
    match (now, before) {
        (None, _) => true,
        (Some(Skip::OutOfReach(_)), Some(Skip::OutOfReach(_))) => false,
        (now, before) => now != before,
    }
}

/// Keeps every registered loop playing while in reach: starts it, the next variant of a variety loop, follows
/// its owner's moves and volume, and stops it when told.
fn run_loops(
    mut out: Output,
    mut starts: MessageReader<PlayLoop>,
    mut stops: MessageReader<StopLoop>,
    mut loops: ResMut<Loops>,
    library: Res<SoundLibrary>,
    listener: Res<ListenerState>,
    mixer: Res<Mixer>,
    state: Option<Res<State<AppState>>>,
    speed: Option<Res<GameSpeed>>,
    mut draws: ResMut<Draws>,
    mut voices: ParamSet<(Playing, Query<&mut Voice, Without<Fading>>)>,
) {
    for request in starts.read() {
        let fresh = LoopState { sound: request.sound, kind: request.kind, at: request.at, volume: request.volume, stopping: false, started: false, skipped: None };
        let entry = loops.0.entry(request.key).or_insert(fresh);
        entry.at = request.at;
        entry.volume = request.volume;
        entry.stopping = false;
    }
    for mut voice in &mut voices.p1() {
        if let Some(state) = voice.loop_key.and_then(|k| loops.0.get(&k)) {
            voice.at = state.at;
            voice.weight = state.volume;
        }
    }
    for StopLoop(key) in stops.read() {
        let Some(state) = loops.0.get_mut(key) else { continue };
        match state.kind {
            LoopKind::Repeat => {
                loops.0.remove(key);
                for (e, v) in &voices.p0() {
                    if v.loop_key == Some(*key) {
                        fade_out(&mut out.commands, e, FX_FADE);
                    }
                }
            }
            LoopKind::Variety => state.stopping = true,
        }
        out.log(|| format!("audio fx loop={} stop", key.0));
    }
    let moment = moment(state.as_deref(), speed.as_deref());
    let playing: Vec<LoopKey> = voices.p0().iter().filter_map(|(_, v)| v.loop_key).collect();
    let idle: Vec<LoopKey> = loops.0.keys().copied().filter(|k| !playing.contains(k)).collect();
    for key in idle {
        let state = loops.0[&key];
        if state.stopping {
            loops.0.remove(&key);
            continue;
        }
        let mut result = resolve_sound(state.sound, state.at, state.volume, &library, &listener.0, &mixer, out.on(), draws.next());
        if let Ok(played) = &result {
            if waits(moment, state.at.is_some(), &played.info) {
                result = Err(Skip::Held);
            } else if let Err(skip) = admit(played, state.sound.cue, !state.started, &voices.p0(), &listener.0, &mut out.commands) {
                result = Err(skip);
            }
        }
        let skipped = result.as_ref().err().copied();
        if news(skipped, state.skipped) {
            out.log(|| describe_fx(&format!("fx loop={} start", key.0), state.sound, state.at, &result));
        }
        let entry = loops.0.get_mut(&key).expect("listed above");
        entry.skipped = skipped;
        let Ok(played) = result else { continue };
        entry.started = true;
        let settings = match state.kind {
            LoopKind::Repeat => PlaybackSettings::LOOP,
            LoopKind::Variety => PlaybackSettings::DESPAWN,
        };
        let voice = Voice { cue: Some(state.sound.cue), info: played.info, at: state.at, loop_key: Some(key), weight: state.volume, ..Voice::new(Role::Effect) };
        out.spawn(&played, voice, settings);
    }
}

/// A new track (or none): the old one and its drums fade out, a drum bank is drawn for the new one.
fn play_music(
    mut out: Output,
    mut requests: MessageReader<PlayMusic>,
    library: Res<SoundLibrary>,
    mixer: Res<Mixer>,
    mut current: ResMut<CurrentMusic>,
    mut drums: ResMut<Drums>,
    mut draws: ResMut<Draws>,
    voices: Playing,
) {
    for &request in requests.read() {
        if current.0 == request.0 {
            continue;
        }
        current.0 = request.0;
        for (e, v) in &voices {
            if v.role != Role::Effect {
                fade_out(&mut out.commands, e, MUSIC_FADE);
            }
        }
        let Some(track) = request.0 else {
            *drums = Drums { intensity: drums.intensity, ..Drums::default() };
            out.log(|| "audio music stop".to_string());
            continue;
        };
        let seed = draws.next();
        let bank = (!library.drums.is_empty()).then(|| (draws.next() % library.drums.len() as u64) as usize);
        *drums = Drums { bank, intensity: drums.intensity, ..Drums::default() };
        let result = resolve_music(library.pick_music(track, seed), &mixer, out.on());
        out.log(|| describe_music(&format!("track={track:?}"), &result));
        if let Ok(played) = result {
            out.spawn(&played, Voice::new(Role::Track), PlaybackSettings::LOOP);
        }
    }
}

/// Chains the drum loops: when one ends, the next from the fight intensity.
fn run_drums(mut out: Output, mut intensities: MessageReader<SetIntensity>, mut drums: ResMut<Drums>, library: Res<SoundLibrary>, mixer: Res<Mixer>, voices: Playing) {
    if let Some(SetIntensity(i)) = intensities.read().last() {
        drums.intensity = *i;
    }
    let Some(bank) = drums.bank else { return };
    if voices.iter().any(|(_, v)| v.role == Role::Drum) {
        return;
    }
    let loops = library.drums.get(bank).map_or(&[][..], |b| b.as_slice());
    let intensity = drums.intensity;
    let entry = drums.state.next(intensity, loops.len());
    let result = resolve_music(loops.get(entry).ok_or(Skip::NoClip), &mixer, out.on());
    let skipped = result.as_ref().err().copied();
    if skipped.is_none() || skipped != drums.skipped {
        out.log(|| describe_music(&format!("drums bank={bank} entry={entry} ({intensity:?})"), &result));
    }
    drums.skipped = skipped;
    if let Ok(played) = result {
        out.spawn(&played, Voice::new(Role::Drum), PlaybackSettings::DESPAWN);
    }
}

/// Every frame: gains from the mixer, the sound's volume and weight, the camera, and its fade-out; a positional
/// sound out of reach fades out (a loop starts again once back in reach); effects wait while the game does.
fn apply_voices(
    mut commands: Commands,
    mixer: Res<Mixer>,
    listener: Res<ListenerState>,
    time: Res<Time<Real>>,
    state: Option<Res<State<AppState>>>,
    speed: Option<Res<GameSpeed>>,
    mut voices: Query<(Entity, &Voice, Option<&mut Fading>, Option<&AudioSink>)>,
) {
    let moment = moment(state.as_deref(), speed.as_deref());
    let dt = time.delta_secs();
    for (e, voice, fading, sink) in &mut voices {
        let placement = match voice.at.map(|at| place(&listener.0, at)) {
            Some(Err(_)) => {
                if fading.is_none() {
                    fade_out(&mut commands, e, FX_FADE);
                }
                None
            }
            Some(Ok(p)) => Some(p),
            None => None,
        };
        let fade = match fading {
            Some(mut f) => {
                f.left -= dt;
                if f.left <= 0.0 {
                    commands.entity(e).despawn();
                    continue;
                }
                fade_factor(f.left, f.total)
            }
            None => 1.0,
        };
        let volume = mixer.gain(voice.channel()) * voice.info.gain() * voice.weight * fade;
        let (l, r) = match (voice.at, placement) {
            (Some(_), None) => (0.0, 0.0),
            (_, p) => voice_gains(volume, p),
        };
        voice.gains.set(l, r);
        if let Some(sink) = sink {
            let wait = voice.role == Role::Effect && waits(moment, voice.at.is_some(), &voice.info);
            if wait != sink.is_paused() {
                if wait { sink.pause() } else { sink.play() }
            }
        }
    }
}

/// Players Bevy never gave a sink (no device behind the plugin) would never despawn: drop them.
fn reap_sinkless(
    mut commands: Commands,
    mut waiting: Query<(Entity, &mut Unsunk), Without<AudioSink>>,
    sunk: Query<Entity, (With<Unsunk>, With<AudioSink>)>,
    debug: Res<AudioDebug>,
    mut told: Local<bool>,
) {
    for e in &sunk {
        commands.entity(e).remove::<Unsunk>();
    }
    for (e, mut wait) in &mut waiting {
        wait.0 += 1;
        if wait.0 >= SINK_WAIT {
            commands.entity(e).despawn();
            if debug.0 && !std::mem::replace(&mut *told, true) {
                info!("audio: no sink after {SINK_WAIT} frames, no audio device? dropping such sounds");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_mix::Level;
    use crate::audio_space::{CUTOFF, LISTENER_AHEAD};
    use bevy::audio::PlaybackMode;

    fn clip(variant: u8) -> SoundClip {
        SoundClip { variant, source: AudioSource { bytes: Arc::from(Vec::new()) }, reference: format!("test#{variant}") }
    }

    fn library(variants: &[u8]) -> SoundLibrary {
        let mut lib = SoundLibrary::default();
        lib.sounds.insert(SoundCue::Click, Family { info: CueInfo::default(), clips: variants.iter().map(|&v| clip(v)).collect() });
        lib.music.insert(MusicTrack::Menu, vec![clip(1)]);
        lib.music.insert(MusicTrack::Game, vec![clip(2), clip(3), clip(4), clip(5)]);
        lib.drums = vec![(0..4).map(|e| clip(10 + e)).collect()];
        lib
    }

    fn full_mixer() -> Mixer {
        Mixer { master: Level::new(100), music: Level::new(100), fx: Level::new(100), mute_all: false }
    }

    /// A point `d` cells to the right of the default listener's ear.
    fn right_of_ear(d: f32) -> Vec2 {
        Vec2::new(d, LISTENER_AHEAD)
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn click_loop(key: u64, kind: LoopKind, at: Option<Vec2>) -> PlayLoop {
        PlayLoop { key: LoopKey(key), sound: SoundCue::Click.into(), kind, at, volume: 1.0 }
    }

    #[test]
    fn a_variant_is_exactly_that_one_and_any_draws_from_the_family() {
        let lib = library(&[3, 1, 4, 5]);
        assert_eq!(lib.pick(Sound::variant(SoundCue::Click, 4), 0).unwrap().0.variant, 4);
        assert_eq!(lib.pick(Sound::variant(SoundCue::Click, 2), 0).unwrap_err(), Skip::NoVariant(2), "no fallback to a neighbour");
        let mut draws = Draws::default();
        let mut seen: Vec<u8> = (0..64).map(|_| lib.pick(Sound::any(SoundCue::Click), draws.next()).unwrap().0.variant).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen, vec![1, 3, 4, 5]);
        let single = library(&[1]);
        assert_eq!(single.pick(SoundCue::Click.into(), 7).unwrap().0.variant, 1);
        assert_eq!(single.pick(Sound::variant(SoundCue::Click, 1), 7).unwrap().0.variant, 1);
        assert_eq!(SoundLibrary::default().pick(SoundCue::Click.into(), 0).unwrap_err(), Skip::NoClip);
        let games: Vec<u8> = (0..4).map(|s| lib.pick_music(MusicTrack::Game, s).unwrap().variant).collect();
        assert_eq!(games, vec![2, 3, 4, 5], "a game's track is one of its variants");
    }

    #[test]
    fn requests_resolve_or_say_why_not() {
        let lib = library(&[1, 2]);
        let l = Listener::default();
        let sound = Sound::variant(SoundCue::Click, 2);
        let played = resolve_sound(sound, Some(right_of_ear(0.0)), 1.0, &lib, &l, &full_mixer(), true, 0).unwrap();
        assert_eq!(played.clip.variant, 2);
        assert!(near(played.gains.0, played.gains.1), "centred");
        let flat = resolve_sound(sound, None, 1.0, &lib, &l, &full_mixer(), true, 0).unwrap();
        assert_eq!(flat.gains, (1.0, 1.0), "flat sounds play full on both sides");
        assert_eq!(resolve_sound(sound, None, 0.25, &lib, &l, &full_mixer(), true, 0).unwrap().gains, (0.25, 0.25), "a loop's weight");
        let far = resolve_sound(sound, Some(right_of_ear(CUTOFF)), 1.0, &lib, &l, &full_mixer(), true, 0);
        assert!(matches!(far, Err(Skip::OutOfReach(_))));
        let muted = Mixer { mute_all: true, ..full_mixer() };
        assert_eq!(resolve_sound(sound, None, 1.0, &lib, &l, &muted, true, 0).unwrap_err(), Skip::MuteAll);
        assert_eq!(resolve_sound(sound, None, 1.0, &lib, &l, &full_mixer(), false, 0).unwrap_err(), Skip::DeviceOff);
        assert_eq!(resolve_music(SoundLibrary::default().pick_music(MusicTrack::Menu, 0), &full_mixer(), true).unwrap_err(), Skip::NoClip);
    }

    #[test]
    fn gains_take_the_sounds_own_volume_falloff_and_balance() {
        let half = CueInfo { volume: 64, ..CueInfo::default() };
        assert!((half.gain() - 0.5).abs() < 0.01);
        let right = Placement { gain: 0.5, pan: 1.0, distance: 10.0 };
        let (l, r) = voice_gains(0.8, Some(right));
        assert!(near(l, 0.0) && near(r, 0.4));
        assert_eq!(voice_gains(0.8, None), (0.8, 0.8));
    }

    #[test]
    fn the_same_sound_waits_250ms() {
        let ms = Duration::from_millis;
        assert!(!too_soon(None, ms(0)));
        assert!(too_soon(Some(ms(1000)), ms(1249)));
        assert!(!too_soon(Some(ms(1000)), ms(1250)));
    }

    #[test]
    fn a_full_mix_cuts_the_lowest_ranked_for_a_better_one() {
        let r = |priority, copies, closeness| Rank { priority, copies, closeness };
        let mut playing = vec![r(1, 1, 0.9); MAX_FX - 1];
        assert_eq!(make_room(&playing, r(0, 1, 0.1)), Ok(None), "room left");
        playing.push(r(1, 1, 0.2));
        assert_eq!(make_room(&playing, r(1, 1, 0.5)), Ok(Some(MAX_FX - 1)), "the farthest of equal priority");
        assert_eq!(make_room(&playing, r(1, 1, 0.1)), Err(Skip::Busy));
        assert_eq!(make_room(&playing, r(2, 5, 0.0)), Ok(Some(MAX_FX - 1)), "priority first");
        playing[3] = r(1, 4, 1.0);
        assert_eq!(make_room(&playing, r(1, 1, 0.1)), Ok(Some(3)), "many copies of one sound go first");
    }

    #[test]
    fn what_plays_while_paused_or_behind_a_menu() {
        let plain = CueInfo::default();
        let flagged = CueInfo { while_paused: true, ..plain };
        let paused = GameSpeed { paused: true, times: 1 };
        assert_eq!(moment(None, None), Moment::Running);
        assert_eq!(moment(Some(&State::new(AppState::Playing)), Some(&paused)), Moment::Paused);
        assert_eq!(moment(Some(&State::new(AppState::Paused)), None), Moment::Overlay);
        assert_eq!(moment(Some(&State::new(AppState::Menu)), Some(&paused)), Moment::Overlay);
        assert_eq!(moment(Some(&State::new(AppState::Ended)), None), Moment::Running);
        assert!(!waits(Moment::Running, true, &plain));
        assert!(waits(Moment::Paused, false, &plain), "paused: even interface sounds wait");
        assert!(!waits(Moment::Paused, true, &flagged), "unless flagged for it");
        assert!(waits(Moment::Overlay, true, &flagged), "a menu: only interface sounds");
        assert!(!waits(Moment::Overlay, false, &plain));
    }

    #[test]
    fn fades_go_from_full_to_nothing() {
        assert_eq!(fade_factor(1.0, 1.0), 1.0);
        assert_eq!(fade_factor(0.25, 1.0), 0.25);
        assert_eq!(fade_factor(-1.0, 1.0), 0.0);
        assert_eq!(fade_factor(1.0, 0.0), 0.0);
    }

    #[test]
    fn debug_lines() {
        let lib = library(&[4]);
        let l = Listener::default();
        let sound = Sound::variant(SoundCue::Click, 4);
        let at = Some(right_of_ear(24.0));
        let ok = resolve_sound(sound, at, 1.0, &lib, &l, &full_mixer(), true, 0);
        assert_eq!(describe_fx("fx", sound, at, &ok), "audio fx cue=Click#4 (asked 4) ref=test#4 at=(24.0,16.0) dist=24.0 pan=1.00 gain=0.00/0.75");
        let ui = resolve_sound(SoundCue::Click.into(), None, 1.0, &lib, &l, &full_mixer(), true, 0);
        assert_eq!(describe_fx("fx", SoundCue::Click.into(), None, &ui), "audio fx cue=Click#4 (asked any) ref=test#4 at=none gain=1.00/1.00");
        assert_eq!(
            describe_fx("fx loop=7 start", Sound::variant(SoundCue::Click, 9), None, &Err(Skip::NoVariant(9))),
            "audio fx loop=7 start cue=Click (asked 9) at=none skipped: no variant 9"
        );
        let far = Err(Skip::OutOfReach(OutOfReach { distance: 51.0 }));
        assert_eq!(describe_fx("fx", sound, at, &far), "audio fx cue=Click (asked 4) at=(24.0,16.0) skipped: out of range (dist=51.0)");
        let music = resolve_music(lib.pick_music(MusicTrack::Menu, 0), &full_mixer(), true);
        assert_eq!(describe_music("track=Menu", &music), "audio music track=Menu ref=test#1 gain=1.00");
        assert_eq!(describe_music("drums bank=0 entry=2 (Melee)", &Err(Skip::DeviceOff)), "audio music drums bank=0 entry=2 (Melee) skipped: audio device off");
        assert_eq!(describe_fx("fx", sound, None, &Err(Skip::OwnerBusy)), "audio fx cue=Click (asked 4) at=none skipped: its owner has a sound playing");
    }

    fn app(device: bool, lib: SoundLibrary) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default())).add_plugins(SoundPlugin { debug: false, device_muted: false, original_dir: None });
        if device {
            app.insert_resource(AudioOut).init_asset::<PannedSound>();
        }
        app.insert_resource(lib).insert_resource(full_mixer());
        app
    }

    /// The voices playing, not those fading out.
    fn voices(app: &mut App) -> Vec<(Entity, Voice, PlaybackSettings)> {
        let mut q = app.world_mut().query_filtered::<(Entity, &Voice, &PlaybackSettings), Without<Fading>>();
        q.iter(app.world()).map(|(e, v, s)| (e, v.clone(), *s)).collect()
    }

    fn fading(app: &mut App) -> usize {
        app.world_mut().query::<&Fading>().iter(app.world()).count()
    }

    fn effects(app: &mut App) -> Vec<(Entity, Voice, PlaybackSettings)> {
        voices(app).into_iter().filter(|v| v.1.role == Role::Effect).collect()
    }

    fn send<M: Message>(app: &mut App, m: M) {
        app.world_mut().write_message(m);
    }

    #[test]
    fn near_sounds_play_far_unknown_or_repeated_ones_do_not() {
        let mut app = app(true, library(&[1]));
        send(&mut app, PlaySound::at(SoundCue::Click, right_of_ear(3.0)));
        send(&mut app, PlaySound::at(SoundCue::Click, right_of_ear(60.0)));
        send(&mut app, PlaySound::flat(Sound::variant(SoundCue::Click, 7)));
        send(&mut app, PlaySound::flat(SoundCue::Click));
        app.update();
        let spawned = effects(&mut app);
        assert_eq!(spawned.len(), 1, "far, unknown variant, and the same sound again within 250 ms");
        let (_, voice, settings) = &spawned[0];
        assert_eq!(voice.at, Some(right_of_ear(3.0)));
        assert!(matches!(settings.mode, PlaybackMode::Despawn));
        let (l, r) = voice.gains.get();
        assert!(r > l && r > 0.0);
    }

    #[test]
    fn an_owner_gets_one_sound_at_a_time() {
        let mut app = app(true, library(&[1]));
        let owner = SoundOwner(42);
        send(&mut app, PlaySound::at(SoundCue::Click, right_of_ear(1.0)).owned_by(owner));
        app.update();
        app.world_mut().resource_mut::<LastStarts>().0.clear();
        send(&mut app, PlaySound::at(SoundCue::Click, right_of_ear(1.0)).owned_by(owner));
        send(&mut app, PlaySound::at(SoundCue::Click, right_of_ear(1.0)).owned_by(SoundOwner(7)));
        app.update();
        assert_eq!(effects(&mut app).len(), 2, "the second sound of 42 waits for the first to end");
    }

    #[test]
    fn a_paused_game_starts_no_effects_but_lets_flagged_ones_through() {
        let mut lib = library(&[1]);
        lib.sounds.get_mut(&SoundCue::Click).unwrap().info.while_paused = true;
        let mut app = app(true, library(&[1]));
        app.insert_resource(GameSpeed { paused: true, times: 1 });
        send(&mut app, PlaySound::flat(SoundCue::Click));
        app.update();
        assert!(effects(&mut app).is_empty());
        app.insert_resource(lib);
        send(&mut app, PlaySound::flat(SoundCue::Click));
        app.update();
        assert_eq!(effects(&mut app).len(), 1);
    }

    #[test]
    fn a_sound_out_of_reach_fades_out_and_a_loop_comes_back_with_the_camera() {
        let mut app = app(true, library(&[1]));
        let at = Some(right_of_ear(3.0));
        send(&mut app, PlaySound { sound: SoundCue::Click.into(), at, owner: None });
        send(&mut app, click_loop(1, LoopKind::Repeat, at));
        app.update();
        assert_eq!(effects(&mut app).len(), 2);
        app.world_mut().resource_mut::<ListenerState>().0.focus = Vec2::new(64.0, 0.0);
        app.update();
        assert!(effects(&mut app).is_empty(), "both stopped out of reach");
        assert_eq!(fading(&mut app), 2, "fading out");
        app.world_mut().resource_mut::<ListenerState>().0.focus = Vec2::ZERO;
        for _ in 0..3 {
            app.update();
        }
        let back = effects(&mut app);
        assert!(back.iter().all(|v| v.1.loop_key == Some(LoopKey(1))), "the loop, not the one-shot");
    }

    #[test]
    fn a_repeat_loop_follows_its_owner_and_fades_when_stopped() {
        let mut app = app(true, library(&[1]));
        send(&mut app, click_loop(9, LoopKind::Repeat, Some(right_of_ear(1.0))));
        app.update();
        send(&mut app, PlayLoop { at: Some(right_of_ear(-2.0)), volume: 0.5, ..click_loop(9, LoopKind::Repeat, None) });
        app.update();
        let v = effects(&mut app);
        assert_eq!(v.len(), 1, "moved, not started again");
        assert_eq!((v[0].1.at, v[0].1.weight), (Some(right_of_ear(-2.0)), 0.5));
        assert!(matches!(v[0].2.mode, PlaybackMode::Loop));
        let (l, r) = v[0].1.gains.get();
        assert!(l > r && l < 0.5, "on the left, at half volume: {l} {r}");
        send(&mut app, StopLoop(LoopKey(9)));
        app.update();
        assert!(effects(&mut app).is_empty());
        assert_eq!(fading(&mut app), 1);
    }

    #[test]
    fn a_variety_loop_plays_variant_after_variant_and_finishes_the_last_when_stopped() {
        let mut app = app(true, library(&[1, 2, 3]));
        send(&mut app, click_loop(2, LoopKind::Variety, None));
        app.update();
        let first = effects(&mut app);
        assert_eq!(first.len(), 1);
        assert!(matches!(first[0].2.mode, PlaybackMode::Despawn));
        app.world_mut().despawn(first[0].0);
        app.update();
        assert_eq!(effects(&mut app).len(), 1, "the next variant once one ends");
        send(&mut app, StopLoop(LoopKey(2)));
        app.update();
        let last = effects(&mut app);
        assert_eq!((last.len(), fading(&mut app)), (1, 0), "the current variant finishes, no fade");
        app.world_mut().despawn(last[0].0);
        app.update();
        app.update();
        assert!(effects(&mut app).is_empty(), "no next one");
    }

    #[test]
    fn a_full_mix_lets_a_better_sound_in() {
        let mut app = app(true, library(&[1]));
        for i in 0..MAX_FX {
            send(&mut app, click_loop(i as u64, LoopKind::Repeat, Some(right_of_ear(10.0 + i as f32))));
        }
        app.update();
        assert_eq!(effects(&mut app).len(), MAX_FX);
        let mut lib = library(&[1]);
        lib.sounds.get_mut(&SoundCue::Click).unwrap().info.priority = 5;
        app.insert_resource(lib);
        send(&mut app, PlaySound::flat(SoundCue::Click));
        app.update();
        let v = effects(&mut app);
        assert_eq!((v.len(), fading(&mut app)), (MAX_FX, 1), "one fades out for it");
        assert!(v.iter().any(|(_, voice, _)| voice.loop_key.is_none()));
        for _ in 0..3 {
            app.update();
        }
        let loops = effects(&mut app).iter().filter(|v| v.1.loop_key.is_some()).count();
        assert_eq!(loops, MAX_FX - 1, "the cut loop waits for a free slot instead of cutting another");
    }

    #[test]
    fn music_plays_its_track_and_chains_drums_from_the_fighting() {
        let mut app = app(true, library(&[1]));
        send(&mut app, PlayMusic(Some(MusicTrack::Menu)));
        app.update();
        let roles = |app: &mut App| {
            let mut r: Vec<String> = voices(app).iter().map(|v| format!("{:?}", v.1.role)).collect();
            r.sort();
            r
        };
        assert_eq!(roles(&mut app), vec!["Drum", "Track"]);
        let drum = voices(&mut app).into_iter().find(|v| v.1.role == Role::Drum).unwrap();
        assert!(matches!(drum.2.mode, PlaybackMode::Despawn), "each drum loop ends, the next is chosen");
        send(&mut app, SetIntensity(Intensity::Melee));
        app.world_mut().despawn(drum.0);
        app.update();
        assert_eq!(app.world().resource::<Drums>().state, {
            let mut s = DrumState::default();
            s.next(Intensity::Calm, 4);
            s.next(Intensity::Melee, 4);
            s
        });
        send(&mut app, PlayMusic(Some(MusicTrack::Game)));
        app.update();
        assert_eq!(roles(&mut app), vec!["Drum", "Track"], "the new track and its drums");
        assert_eq!(fading(&mut app), 2, "the old track and drums fade out");
        send(&mut app, PlayMusic(None));
        app.update();
        assert!(voices(&mut app).is_empty());
    }

    #[test]
    fn nothing_spawns_without_a_device() {
        let mut app = app(false, library(&[1]));
        send(&mut app, PlaySound::flat(SoundCue::Click));
        send(&mut app, click_loop(1, LoopKind::Repeat, None));
        send(&mut app, PlayMusic(Some(MusicTrack::Menu)));
        app.update();
        assert!(voices(&mut app).is_empty());
        assert_eq!(app.world().resource::<CurrentMusic>().0, Some(MusicTrack::Menu), "still knows what should play");
    }

    #[test]
    fn players_without_a_sink_are_dropped() {
        let mut app = app(true, library(&[1]));
        send(&mut app, PlaySound::flat(SoundCue::Click));
        app.update();
        assert_eq!(voices(&mut app).len(), 1);
        for _ in 0..SINK_WAIT {
            app.update();
        }
        assert!(voices(&mut app).is_empty());
    }
}
