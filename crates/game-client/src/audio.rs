//! Sound output (docs/architecture.md "Client: audio", sound.md "Playback in the client"). Systems ask with
//! messages: `PlaySound` (a one-shot), `PlayLoop`/`StopLoop` (a sound that goes on until its owner stops it,
//! sent again to move it), `PlayMusic`. This plugin picks the clip from the `SoundLibrary`, places it for the
//! camera (`audio_space`), mixes it (`audio_mix::Mixer`, the sound's own volume) and keeps its gains up to date
//! while it plays, through our own stereo source (`audio_source`). Positional sounds wait while the game is
//! paused or behind a menu; flat (UI) sounds and music go on.
//! Bevy's `AudioPlugin` (the device) is left out by `--mute=on` and in headless runs: requests are then still
//! resolved, and logged by `--debug-audio`, but nothing is spawned.

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

/// Plays a sound once on the Fx channel: at a map position (cells), heard from the camera, or `None` flat
/// (interface sounds).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct PlaySound {
    pub sound: Sound,
    pub at: Option<Vec2>,
}

/// Names a loop for its owner (e.g. from a model's id and what the loop is), to move or stop it.
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

/// Starts a loop on the Fx channel, or moves it if `key` is already playing (send it again as its owner moves).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct PlayLoop {
    pub key: LoopKey,
    pub sound: Sound,
    pub kind: LoopKind,
    pub at: Option<Vec2>,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StopLoop(pub LoopKey);

/// Switches the music to a track (looping), or stops it.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct PlayMusic(pub Option<MusicTrack>);

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
}

impl Default for CueInfo {
    fn default() -> Self {
        CueInfo { volume: 127, priority: 0 }
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

/// What can be played. Empty until the sound loaders fill it (TODO.md "Sound").
#[derive(Resource, Default, Debug)]
pub struct SoundLibrary {
    pub sounds: HashMap<SoundCue, Family>,
    pub music: HashMap<MusicTrack, Vec<SoundClip>>,
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

/// What a sound request plays, or why not.
pub fn resolve_sound(sound: Sound, at: Option<Vec2>, library: &SoundLibrary, listener: &Listener, mixer: &Mixer, device: bool, seed: u64) -> Result<Played, Skip> {
    let (clip, info) = library.pick(sound, seed)?;
    let placement = at.map(|at| place(listener, at)).transpose().map_err(Skip::OutOfReach)?;
    played(clip, info, placement, mixer.gain(Channel::Fx), mixer, device)
}

pub fn resolve_music(track: MusicTrack, library: &SoundLibrary, mixer: &Mixer, device: bool, seed: u64) -> Result<Played, Skip> {
    let clip = library.pick_music(track, seed)?;
    played(clip, CueInfo::default(), None, mixer.gain(Channel::Music), mixer, device)
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

pub fn too_soon(last: Option<Duration>, now: Duration) -> bool {
    last.is_some_and(|t| now.saturating_sub(t) < REPEAT_GAP)
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

fn skip_reason(skip: &Skip) -> String {
    match skip {
        Skip::NoClip => "no clip in library".into(),
        Skip::NoVariant(n) => format!("no variant {n}"),
        Skip::OutOfReach(o) => format!("out of range (dist={:.1})", o.distance),
        Skip::TooSoon => "same sound under 250 ms ago".into(),
        Skip::Busy => "too many sounds".into(),
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

/// The `--debug-audio` line for a music request.
pub fn describe_music(request: PlayMusic, result: Option<&Result<Played, Skip>>) -> String {
    match (request.0, result) {
        (Some(track), Some(Ok(p))) => format!("audio music track={track:?} ref={} gain={:.2} loop", p.clip.reference, p.gains.0),
        (Some(track), Some(Err(skip))) => format!("audio music track={track:?} skipped: {}", skip_reason(skip)),
        _ => "audio music stop".to_string(),
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
    /// A variety loop told to stop: its current variant finishes, no next one.
    stopping: bool,
    /// Why it last could not play, logged once.
    skipped: Option<Skip>,
}

/// The loops their owners keep going.
#[derive(Resource, Debug, Default)]
struct Loops(BTreeMap<LoopKey, LoopState>);

/// One playing sound.
#[derive(Component, Debug, Clone)]
pub struct Voice {
    pub channel: Channel,
    pub cue: Option<SoundCue>,
    pub info: CueInfo,
    /// Map position, cells; None plays flat.
    pub at: Option<Vec2>,
    pub loop_key: Option<LoopKey>,
    gains: Arc<Gains>,
}

/// Frames a player has waited for Bevy to give it a sink.
#[derive(Component, Clone, Copy, Debug, Default)]
struct Unsunk(u32);

/// Frames after which a player still without a sink is dropped (no device behind it).
const SINK_WAIT: u32 = 120;

pub struct SoundPlugin {
    pub debug: bool,
    pub device_muted: bool,
}

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<bevy::audio::AudioPlugin>() {
            app.insert_resource(AudioOut).add_audio_source::<PannedSound>();
        }
        app.insert_resource(AudioDebug(self.debug))
            .insert_resource(DeviceMuted(self.device_muted))
            .init_resource::<SoundLibrary>()
            .init_resource::<ListenerState>()
            .init_resource::<CurrentMusic>()
            .init_resource::<LastStarts>()
            .init_resource::<Draws>()
            .init_resource::<Loops>()
            .init_resource::<Mixer>()
            .add_message::<PlaySound>()
            .add_message::<PlayLoop>()
            .add_message::<StopLoop>()
            .add_message::<PlayMusic>()
            .add_systems(Update, (track_listener, music_for_state, play_sounds, run_loops, play_music, apply_voices, reap_sinkless).chain());
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

/// The playing effects, ranked.
fn ranks(voices: &Query<(Entity, &Voice)>, listener: &Listener) -> Vec<(Entity, Rank)> {
    let fx: Vec<(Entity, &Voice)> = voices.iter().filter(|(_, v)| v.channel == Channel::Fx).collect();
    fx.iter()
        .map(|(e, v)| {
            let copies = fx.iter().filter(|(_, o)| o.cue == v.cue).count();
            let closeness = v.at.map_or(1.0, |at| place(listener, at).map_or(0.0, |p| p.gain));
            (*e, Rank { priority: v.info.priority, copies, closeness })
        })
        .collect()
}

/// Makes room for a new effect: cuts the lowest-ranked one if the new one outranks it.
fn admit(played: &Played, cue: SoundCue, voices: &Query<(Entity, &Voice)>, listener: &Listener, commands: &mut Commands) -> Result<(), Skip> {
    let ranked = ranks(voices, listener);
    let copies = voices.iter().filter(|(_, v)| v.cue == Some(cue)).count() + 1;
    let new = Rank { priority: played.info.priority, copies, closeness: played.placement.map_or(1.0, |p| p.gain) };
    let list: Vec<Rank> = ranked.iter().map(|r| r.1).collect();
    if let Some(i) = make_room(&list, new)? {
        commands.entity(ranked[i].0).despawn();
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
    mut last: ResMut<LastStarts>,
    mut draws: ResMut<Draws>,
    voices: Query<(Entity, &Voice)>,
) {
    let now = time.elapsed();
    for request in requests.read() {
        let cue = request.sound.cue;
        let mut result = resolve_sound(request.sound, request.at, &library, &listener.0, &mixer, out.on(), draws.next());
        if result.is_ok() && too_soon(last.0.get(&cue).copied(), now) {
            result = Err(Skip::TooSoon);
        }
        if let Ok(played) = &result
            && let Err(skip) = admit(played, cue, &voices, &listener.0, &mut out.commands)
        {
            result = Err(skip);
        }
        if out.debug.0 {
            info!("{}", describe_fx("fx", request.sound, request.at, &result));
        }
        let Ok(played) = result else { continue };
        last.0.insert(cue, now);
        let voice = Voice { channel: Channel::Fx, cue: Some(cue), info: played.info, at: request.at, loop_key: None, gains: Arc::default() };
        out.spawn(&played, voice, PlaybackSettings::DESPAWN);
    }
}

/// Keeps every registered loop playing while in reach: starts it, the next variant of a variety loop, follows
/// its owner's moves, and stops it when told.
fn run_loops(
    mut out: Output,
    mut starts: MessageReader<PlayLoop>,
    mut stops: MessageReader<StopLoop>,
    mut loops: ResMut<Loops>,
    library: Res<SoundLibrary>,
    listener: Res<ListenerState>,
    mixer: Res<Mixer>,
    mut draws: ResMut<Draws>,
    mut voices: ParamSet<(Query<(Entity, &Voice)>, Query<&mut Voice>)>,
) {
    for request in starts.read() {
        let entry = loops.0.entry(request.key).or_insert(LoopState { sound: request.sound, kind: request.kind, at: request.at, stopping: false, skipped: None });
        entry.at = request.at;
        entry.stopping = false;
    }
    for mut voice in &mut voices.p1() {
        if let Some(state) = voice.loop_key.and_then(|k| loops.0.get(&k)) {
            voice.at = state.at;
        }
    }
    for StopLoop(key) in stops.read() {
        let Some(state) = loops.0.get_mut(key) else { continue };
        match state.kind {
            LoopKind::Repeat => {
                loops.0.remove(key);
                for (e, v) in &voices.p0() {
                    if v.loop_key == Some(*key) {
                        out.commands.entity(e).despawn();
                    }
                }
            }
            LoopKind::Variety => state.stopping = true,
        }
        if out.debug.0 {
            info!("audio fx loop={} stop", key.0);
        }
    }
    let playing: Vec<LoopKey> = voices.p0().iter().filter_map(|(_, v)| v.loop_key).collect();
    let idle: Vec<LoopKey> = loops.0.keys().copied().filter(|k| !playing.contains(k)).collect();
    for key in idle {
        let state = loops.0[&key];
        if state.stopping {
            loops.0.remove(&key);
            continue;
        }
        let mut result = resolve_sound(state.sound, state.at, &library, &listener.0, &mixer, out.on(), draws.next());
        if let Ok(played) = &result
            && let Err(skip) = admit(played, state.sound.cue, &voices.p0(), &listener.0, &mut out.commands)
        {
            result = Err(skip);
        }
        let skipped = result.as_ref().err().copied();
        let news = match (skipped, state.skipped) {
            (None, _) => true,
            (Some(Skip::OutOfReach(_)), Some(Skip::OutOfReach(_))) => false,
            (now, before) => now != before,
        };
        if out.debug.0 && news {
            info!("{}", describe_fx(&format!("fx loop={} start", key.0), state.sound, state.at, &result));
        }
        loops.0.get_mut(&key).expect("listed above").skipped = skipped;
        let Ok(played) = result else { continue };
        let settings = match state.kind {
            LoopKind::Repeat => PlaybackSettings::LOOP,
            LoopKind::Variety => PlaybackSettings::DESPAWN,
        };
        let voice = Voice { channel: Channel::Fx, cue: Some(state.sound.cue), info: played.info, at: state.at, loop_key: Some(key), gains: Arc::default() };
        out.spawn(&played, voice, settings);
    }
}

fn play_music(
    mut out: Output,
    mut requests: MessageReader<PlayMusic>,
    library: Res<SoundLibrary>,
    mixer: Res<Mixer>,
    mut current: ResMut<CurrentMusic>,
    mut draws: ResMut<Draws>,
    voices: Query<(Entity, &Voice)>,
) {
    for &request in requests.read() {
        if current.0 == request.0 {
            continue;
        }
        current.0 = request.0;
        for (e, v) in &voices {
            if v.channel == Channel::Music {
                out.commands.entity(e).despawn();
            }
        }
        let result = request.0.map(|t| resolve_music(t, &library, &mixer, out.on(), draws.next()));
        if out.debug.0 {
            info!("{}", describe_music(request, result.as_ref()));
        }
        if let Some(Ok(played)) = result {
            let voice = Voice { channel: Channel::Music, cue: None, info: played.info, at: None, loop_key: None, gains: Arc::default() };
            out.spawn(&played, voice, PlaybackSettings::LOOP);
        }
    }
}

/// The game is not running: positional sounds wait, flat ones and music go on.
fn world_held(state: Option<&State<AppState>>, speed: Option<&GameSpeed>) -> bool {
    let running = state.is_none_or(|s| matches!(s.get(), AppState::Playing | AppState::Ended));
    !running || speed.is_some_and(|s| s.paused)
}

/// Every frame: gains from the mixer, the sound's volume and the camera; a positional sound out of reach is
/// stopped (a loop starts again once back in reach); positional sounds wait while the world does.
fn apply_voices(
    mut commands: Commands,
    mixer: Res<Mixer>,
    listener: Res<ListenerState>,
    state: Option<Res<State<AppState>>>,
    speed: Option<Res<GameSpeed>>,
    voices: Query<(Entity, &Voice, Option<&AudioSink>)>,
) {
    let held = world_held(state.as_deref(), speed.as_deref());
    for (e, voice, sink) in &voices {
        let placement = match voice.at.map(|at| place(&listener.0, at)) {
            Some(Err(_)) => {
                commands.entity(e).despawn();
                continue;
            }
            Some(Ok(p)) => Some(p),
            None => None,
        };
        let (l, r) = voice_gains(mixer.gain(voice.channel) * voice.info.gain(), placement);
        voice.gains.set(l, r);
        if let Some(sink) = sink {
            let wait = held && voice.at.is_some();
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
        let played = resolve_sound(sound, Some(right_of_ear(0.0)), &lib, &l, &full_mixer(), true, 0).unwrap();
        assert_eq!(played.clip.variant, 2);
        assert!(near(played.gains.0, played.gains.1), "centred");
        let flat = resolve_sound(sound, None, &lib, &l, &full_mixer(), true, 0).unwrap();
        assert_eq!(flat.gains, (1.0, 1.0), "flat sounds play full on both sides");
        let far = resolve_sound(sound, Some(right_of_ear(CUTOFF)), &lib, &l, &full_mixer(), true, 0);
        assert!(matches!(far, Err(Skip::OutOfReach(_))));
        let muted = Mixer { mute_all: true, ..full_mixer() };
        assert_eq!(resolve_sound(sound, None, &lib, &l, &muted, true, 0).unwrap_err(), Skip::MuteAll);
        assert_eq!(resolve_sound(sound, None, &lib, &l, &full_mixer(), false, 0).unwrap_err(), Skip::DeviceOff);
        assert_eq!(resolve_music(MusicTrack::Menu, &SoundLibrary::default(), &full_mixer(), true, 0).unwrap_err(), Skip::NoClip);
    }

    #[test]
    fn gains_take_the_sounds_own_volume_falloff_and_balance() {
        let half = CueInfo { volume: 64, priority: 0 };
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
    fn debug_lines() {
        let lib = library(&[4]);
        let l = Listener::default();
        let sound = Sound::variant(SoundCue::Click, 4);
        let at = Some(right_of_ear(24.0));
        let ok = resolve_sound(sound, at, &lib, &l, &full_mixer(), true, 0);
        assert_eq!(describe_fx("fx", sound, at, &ok), "audio fx cue=Click#4 (asked 4) ref=test#4 at=(24.0,16.0) dist=24.0 pan=1.00 gain=0.00/0.75");
        let ui = resolve_sound(SoundCue::Click.into(), None, &lib, &l, &full_mixer(), true, 0);
        assert_eq!(describe_fx("fx", SoundCue::Click.into(), None, &ui), "audio fx cue=Click#4 (asked any) ref=test#4 at=none gain=1.00/1.00");
        assert_eq!(
            describe_fx("fx loop=7 start", Sound::variant(SoundCue::Click, 9), None, &Err(Skip::NoVariant(9))),
            "audio fx loop=7 start cue=Click (asked 9) at=none skipped: no variant 9"
        );
        let far = Err(Skip::OutOfReach(OutOfReach { distance: 51.0 }));
        assert_eq!(describe_fx("fx", sound, at, &far), "audio fx cue=Click (asked 4) at=(24.0,16.0) skipped: out of range (dist=51.0)");
        let music = resolve_music(MusicTrack::Menu, &lib, &full_mixer(), true, 0);
        assert_eq!(describe_music(PlayMusic(Some(MusicTrack::Menu)), Some(&music)), "audio music track=Menu ref=test#1 gain=1.00 loop");
        assert_eq!(describe_music(PlayMusic(Some(MusicTrack::Game)), Some(&Err(Skip::DeviceOff))), "audio music track=Game skipped: audio device off");
        assert_eq!(describe_music(PlayMusic(None), None), "audio music stop");
    }

    #[test]
    fn the_world_waits_while_the_game_does() {
        assert!(!world_held(None, None));
        assert!(world_held(None, Some(&GameSpeed { paused: true, times: 1 })));
        assert!(world_held(Some(&State::new(AppState::Paused)), None));
        assert!(world_held(Some(&State::new(AppState::Menu)), None));
        assert!(!world_held(Some(&State::new(AppState::Ended)), None));
    }

    fn app(device: bool, lib: SoundLibrary) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default())).add_plugins(SoundPlugin { debug: false, device_muted: false });
        if device {
            app.insert_resource(AudioOut).init_asset::<PannedSound>();
        }
        app.insert_resource(lib).insert_resource(full_mixer());
        app
    }

    fn voices(app: &mut App) -> Vec<(Entity, Voice, PlaybackSettings)> {
        let mut q = app.world_mut().query::<(Entity, &Voice, &PlaybackSettings)>();
        q.iter(app.world()).map(|(e, v, s)| (e, v.clone(), *s)).collect()
    }

    fn send<M: Message>(app: &mut App, m: M) {
        app.world_mut().write_message(m);
    }

    #[test]
    fn near_sounds_play_far_unknown_or_repeated_ones_do_not() {
        let mut app = app(true, library(&[1]));
        send(&mut app, PlaySound { sound: SoundCue::Click.into(), at: Some(right_of_ear(3.0)) });
        send(&mut app, PlaySound { sound: SoundCue::Click.into(), at: Some(right_of_ear(60.0)) });
        send(&mut app, PlaySound { sound: Sound::variant(SoundCue::Click, 7), at: None });
        send(&mut app, PlaySound { sound: SoundCue::Click.into(), at: None });
        app.update();
        let spawned = voices(&mut app);
        assert_eq!(spawned.len(), 1, "far, unknown variant, and the same sound again within 250 ms");
        let (_, voice, settings) = &spawned[0];
        assert_eq!((voice.channel, voice.at), (Channel::Fx, Some(right_of_ear(3.0))));
        assert!(matches!(settings.mode, PlaybackMode::Despawn));
        let (l, r) = voice.gains.get();
        assert!(r > l && r > 0.0);
    }

    #[test]
    fn a_sound_out_of_reach_stops_and_a_loop_comes_back_with_the_camera() {
        let mut app = app(true, library(&[1]));
        let at = Some(right_of_ear(3.0));
        send(&mut app, PlaySound { sound: SoundCue::Click.into(), at });
        send(&mut app, PlayLoop { key: LoopKey(1), sound: SoundCue::Click.into(), kind: LoopKind::Repeat, at });
        app.update();
        assert_eq!(voices(&mut app).len(), 2);
        app.world_mut().resource_mut::<ListenerState>().0.focus = Vec2::new(64.0, 0.0);
        app.update();
        assert!(voices(&mut app).is_empty(), "both stopped out of reach");
        app.world_mut().resource_mut::<ListenerState>().0.focus = Vec2::ZERO;
        app.update();
        let back = voices(&mut app);
        assert_eq!(back.len(), 1, "the loop, not the one-shot");
        assert_eq!(back[0].1.loop_key, Some(LoopKey(1)));
    }

    #[test]
    fn a_repeat_loop_follows_its_owner_and_stops_at_once() {
        let mut app = app(true, library(&[1]));
        let key = LoopKey(9);
        send(&mut app, PlayLoop { key, sound: SoundCue::Click.into(), kind: LoopKind::Repeat, at: Some(right_of_ear(1.0)) });
        app.update();
        send(&mut app, PlayLoop { key, sound: SoundCue::Click.into(), kind: LoopKind::Repeat, at: Some(right_of_ear(-2.0)) });
        app.update();
        let v = voices(&mut app);
        assert_eq!(v.len(), 1, "moved, not started again");
        assert_eq!(v[0].1.at, Some(right_of_ear(-2.0)));
        assert!(matches!(v[0].2.mode, PlaybackMode::Loop));
        send(&mut app, StopLoop(key));
        app.update();
        assert!(voices(&mut app).is_empty());
    }

    #[test]
    fn a_variety_loop_plays_variant_after_variant_and_finishes_the_last_when_stopped() {
        let mut app = app(true, library(&[1, 2, 3]));
        let key = LoopKey(2);
        send(&mut app, PlayLoop { key, sound: SoundCue::Click.into(), kind: LoopKind::Variety, at: None });
        app.update();
        let first = voices(&mut app);
        assert_eq!(first.len(), 1);
        assert!(matches!(first[0].2.mode, PlaybackMode::Despawn));
        app.world_mut().despawn(first[0].0);
        app.update();
        assert_eq!(voices(&mut app).len(), 1, "the next variant once one ends");
        send(&mut app, StopLoop(key));
        app.update();
        let last = voices(&mut app);
        assert_eq!(last.len(), 1, "the current variant finishes");
        app.world_mut().despawn(last[0].0);
        app.update();
        app.update();
        assert!(voices(&mut app).is_empty(), "no next one");
    }

    #[test]
    fn a_full_mix_lets_a_better_sound_in() {
        let mut app = app(true, library(&[1]));
        for i in 0..MAX_FX {
            send(&mut app, PlayLoop { key: LoopKey(i as u64), sound: SoundCue::Click.into(), kind: LoopKind::Repeat, at: Some(right_of_ear(10.0 + i as f32)) });
        }
        app.update();
        assert_eq!(voices(&mut app).len(), MAX_FX);
        let mut lib = library(&[1]);
        lib.sounds.get_mut(&SoundCue::Click).unwrap().info.priority = 5;
        app.insert_resource(lib);
        send(&mut app, PlaySound { sound: SoundCue::Click.into(), at: None });
        app.update();
        let v = voices(&mut app);
        assert_eq!(v.len(), MAX_FX, "one cut for it");
        assert!(v.iter().any(|(_, voice, _)| voice.loop_key.is_none()));
    }

    #[test]
    fn music_replaces_and_stops_and_nothing_spawns_without_a_device() {
        let mut app = app(true, library(&[1]));
        send(&mut app, PlayMusic(Some(MusicTrack::Menu)));
        app.update();
        send(&mut app, PlayMusic(Some(MusicTrack::Game)));
        app.update();
        let v = voices(&mut app);
        assert_eq!(v.len(), 1, "the new track replaced the old");
        assert_eq!(v[0].1.channel, Channel::Music);
        assert!(matches!(v[0].2.mode, PlaybackMode::Loop));
        send(&mut app, PlayMusic(None));
        app.update();
        assert!(voices(&mut app).is_empty());

        let mut off = super::tests::app(false, library(&[1]));
        send(&mut off, PlaySound { sound: SoundCue::Click.into(), at: None });
        send(&mut off, PlayLoop { key: LoopKey(1), sound: SoundCue::Click.into(), kind: LoopKind::Repeat, at: None });
        send(&mut off, PlayMusic(Some(MusicTrack::Menu)));
        off.update();
        assert!(voices(&mut off).is_empty());
        assert_eq!(off.world().resource::<CurrentMusic>().0, Some(MusicTrack::Menu), "still knows what should play");
    }

    #[test]
    fn players_without_a_sink_are_dropped() {
        let mut app = app(true, library(&[1]));
        send(&mut app, PlaySound { sound: SoundCue::Click.into(), at: None });
        app.update();
        assert_eq!(voices(&mut app).len(), 1);
        for _ in 0..SINK_WAIT {
            app.update();
        }
        assert!(voices(&mut app).is_empty());
    }
}
