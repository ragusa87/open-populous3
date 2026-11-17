//! Sound output (docs/architecture.md "Client: audio"). Any system asks for a sound with a `PlaySound`
//! message (a cue, one exact variant or any, and where on the map, if anywhere) or for music with `PlayMusic`;
//! this plugin picks the clip from the `SoundLibrary`, places it for the camera (`audio_space`), mixes it
//! (`audio_mix::Mixer`) and keeps its loudness and balance up to date while it plays. Fx pause with the game,
//! music goes on.
//! Bevy's own `AudioPlugin` (the device) is left out by `--mute=on` and in headless runs: requests are then
//! still resolved, and logged by `--debug-audio`, but nothing is spawned.

use crate::audio_mix::{Channel, Mixer};
use crate::audio_space::{place, virtual_emitter, Listener, OutOfReach, Placement, EAR_GAP};
use crate::camera::CameraRig;
use crate::game_speed::GameSpeed;
use crate::menu::AppState;
use bevy::audio::{AudioSinkPlayback, Volume};
use bevy::prelude::*;
use std::collections::HashMap;

/// A family of sounds, by what it is for (its variants: `chop1`..`chop5`). Filled in as sounds are wired.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SoundCue {
    /// A menu entry picked.
    Click,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MusicTrack {
    Menu,
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

/// Plays a sound on the Fx channel: at a map position (cells), heard from the camera, or `None` everywhere
/// alike (interface sounds).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct PlaySound {
    pub sound: Sound,
    pub at: Option<Vec2>,
}

/// Switches the music to a track (looping), or stops it.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct PlayMusic(pub Option<MusicTrack>);

/// One playable sound.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundClip {
    /// Its number in its family, from its source (`chop4`: 4); 1 for a family of one.
    pub variant: u8,
    pub handle: Handle<AudioSource>,
    /// Where it comes from, for the log: `Soundd2.sdt#41`, `sounds/chop4.ogg`.
    pub reference: String,
}

/// What can be played. Empty until the sound loaders fill it (TODO.md "Sound").
#[derive(Resource, Default, Debug)]
pub struct SoundLibrary {
    pub sounds: HashMap<SoundCue, Vec<SoundClip>>,
    pub music: HashMap<MusicTrack, SoundClip>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Skip {
    NoClip,
    NoVariant(u8),
    OutOfReach(OutOfReach),
    MuteAll,
    DeviceOff,
}

impl SoundLibrary {
    /// The clip for `sound`; `seed` chooses among the variants for `Pick::Any`.
    pub fn pick(&self, sound: Sound, seed: u64) -> Result<&SoundClip, Skip> {
        let clips = self.sounds.get(&sound.cue).filter(|c| !c.is_empty()).ok_or(Skip::NoClip)?;
        match sound.pick {
            Pick::Any => Ok(&clips[(seed % clips.len() as u64) as usize]),
            Pick::Variant(n) => clips.iter().find(|c| c.variant == n).ok_or(Skip::NoVariant(n)),
        }
    }
}

/// A request that plays.
#[derive(Clone, Debug, PartialEq)]
pub struct Played {
    pub clip: SoundClip,
    pub placement: Option<Placement>,
    /// Final linear volume: placement × mixer.
    pub volume: f32,
}

/// Turns a sound request into what to play, or why not.
pub fn resolve_sound(request: &PlaySound, library: &SoundLibrary, listener: &Listener, mixer: &Mixer, device: bool, seed: u64) -> Result<Played, Skip> {
    let clip = library.pick(request.sound, seed)?;
    let placement = request.at.map(|at| place(listener, at)).transpose().map_err(Skip::OutOfReach)?;
    played(clip, placement, mixer.gain(Channel::Fx), mixer, device)
}

pub fn resolve_music(track: MusicTrack, library: &SoundLibrary, mixer: &Mixer, device: bool) -> Result<Played, Skip> {
    let clip = library.music.get(&track).ok_or(Skip::NoClip)?;
    played(clip, None, mixer.gain(Channel::Music), mixer, device)
}

fn played(clip: &SoundClip, placement: Option<Placement>, gain: f32, mixer: &Mixer, device: bool) -> Result<Played, Skip> {
    if mixer.mute_all {
        return Err(Skip::MuteAll);
    }
    if !device {
        return Err(Skip::DeviceOff);
    }
    Ok(Played { volume: gain * placement.map_or(1.0, |p| p.gain), clip: clip.clone(), placement })
}

fn skip_reason(skip: &Skip) -> String {
    match skip {
        Skip::NoClip => "no clip in library".into(),
        Skip::NoVariant(n) => format!("no variant {n}"),
        Skip::OutOfReach(o) => format!("out of range (dist={:.1} >= {:.1})", o.distance, o.silent),
        Skip::MuteAll => "mute all".into(),
        Skip::DeviceOff => "audio device off".into(),
    }
}

/// The `--debug-audio` line for a sound request.
pub fn describe_sound(request: &PlaySound, result: &Result<Played, Skip>) -> String {
    let asked = match request.sound.pick {
        Pick::Any => "any".to_string(),
        Pick::Variant(n) => n.to_string(),
    };
    let at = request.at.map_or("none".to_string(), |p| format!("({:.1},{:.1})", p.x, p.y));
    let cue = format!("{:?}", request.sound.cue);
    match result {
        Ok(p) => {
            let place = p.placement.map_or(String::new(), |pl| format!(" dist={:.1} pan={:.2}", pl.distance, pl.pan));
            format!("audio fx cue={cue}#{} (asked {asked}) ref={} at={at}{place} gain={:.2}", p.clip.variant, p.clip.reference, p.volume)
        }
        Err(skip) => format!("audio fx cue={cue} (asked {asked}) at={at} skipped: {}", skip_reason(skip)),
    }
}

/// The `--debug-audio` line for a music request.
pub fn describe_music(request: PlayMusic, result: Option<&Result<Played, Skip>>) -> String {
    match (request.0, result) {
        (Some(track), Some(Ok(p))) => format!("audio music track={track:?} ref={} gain={:.2} loop", p.clip.reference, p.volume),
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

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundChannel(pub Channel);

/// A sound placed on the map, cells.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Emitter(pub Vec2);

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
            app.insert_resource(AudioOut).add_systems(Startup, spawn_listener);
        }
        app.insert_resource(AudioDebug(self.debug))
            .insert_resource(DeviceMuted(self.device_muted))
            .init_resource::<SoundLibrary>()
            .init_resource::<ListenerState>()
            .init_resource::<CurrentMusic>()
            .init_resource::<Mixer>()
            .add_message::<PlaySound>()
            .add_message::<PlayMusic>()
            .add_systems(Update, (track_listener, music_for_state, play_sounds, play_music, apply_volumes, reap_sinkless).chain());
    }
}

fn spawn_listener(mut commands: Commands) {
    commands.spawn((SpatialListener::new(EAR_GAP), Transform::IDENTITY));
}

fn track_listener(rig: Option<Res<CameraRig>>, mut listener: ResMut<ListenerState>) {
    if let Some(rig) = rig {
        listener.0 = Listener { focus: rig.focus, right: rig.right(), altitude: rig.altitude() };
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

fn play_sounds(
    mut commands: Commands,
    mut requests: MessageReader<PlaySound>,
    library: Res<SoundLibrary>,
    listener: Res<ListenerState>,
    mixer: Res<Mixer>,
    device: Option<Res<AudioOut>>,
    debug: Res<AudioDebug>,
    mut draws: Local<u64>,
) {
    for request in requests.read() {
        *draws += 1;
        let result = resolve_sound(request, &library, &listener.0, &mixer, device.is_some(), seed(*draws));
        if debug.0 {
            info!("{}", describe_sound(request, &result));
        }
        let Ok(played) = result else { continue };
        let pan = played.placement.map_or(0.0, |p| p.pan);
        let mut player = commands.spawn((
            AudioPlayer(played.clip.handle),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(played.volume)).with_spatial(true),
            Transform::from_translation(virtual_emitter(pan)),
            SoundChannel(Channel::Fx),
            Unsunk::default(),
        ));
        if let Some(at) = request.at {
            player.insert(Emitter(at));
        }
    }
}

/// Spreads a running count over all values, for picking variants.
fn seed(n: u64) -> u64 {
    let mut z = n.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn play_music(
    mut commands: Commands,
    mut requests: MessageReader<PlayMusic>,
    library: Res<SoundLibrary>,
    mixer: Res<Mixer>,
    device: Option<Res<AudioOut>>,
    debug: Res<AudioDebug>,
    mut current: ResMut<CurrentMusic>,
    playing: Query<(Entity, &SoundChannel)>,
) {
    for &request in requests.read() {
        if current.0 == request.0 {
            continue;
        }
        current.0 = request.0;
        for (e, ch) in &playing {
            if ch.0 == Channel::Music {
                commands.entity(e).despawn();
            }
        }
        let result = request.0.map(|t| resolve_music(t, &library, &mixer, device.is_some()));
        if debug.0 {
            info!("{}", describe_music(request, result.as_ref()));
        }
        if let Some(Ok(played)) = result {
            commands.spawn((
                AudioPlayer(played.clip.handle),
                PlaybackSettings::LOOP.with_volume(Volume::Linear(played.volume)),
                SoundChannel(Channel::Music),
                Unsunk::default(),
            ));
        }
    }
}

/// The game is not running: fx wait, music goes on.
fn fx_held(state: Option<&State<AppState>>, speed: Option<&GameSpeed>) -> bool {
    let running = state.is_none_or(|s| matches!(s.get(), AppState::Playing | AppState::Ended));
    !running || speed.is_some_and(|s| s.paused)
}

/// Every frame: loudness from the mixer and the camera, balance, and the fx pause.
fn apply_volumes(
    mixer: Res<Mixer>,
    listener: Res<ListenerState>,
    state: Option<Res<State<AppState>>>,
    speed: Option<Res<GameSpeed>>,
    mut spatial: Query<(&SoundChannel, Option<&Emitter>, &mut SpatialAudioSink, &mut Transform)>,
    mut plain: Query<(&SoundChannel, &mut AudioSink)>,
) {
    let held = fx_held(state.as_deref(), speed.as_deref());
    for (ch, emitter, mut sink, mut transform) in &mut spatial {
        let placement = emitter.map(|e| place(&listener.0, e.0).ok());
        let (gain, pan) = match placement {
            None => (1.0, 0.0),
            Some(Some(p)) => (p.gain, p.pan),
            Some(None) => (0.0, 0.0),
        };
        sink.set_volume(Volume::Linear(mixer.gain(ch.0) * gain));
        transform.translation = virtual_emitter(pan);
        hold(&*sink, ch.0 == Channel::Fx && held);
    }
    for (ch, mut sink) in &mut plain {
        sink.set_volume(Volume::Linear(mixer.gain(ch.0)));
        hold(&*sink, ch.0 == Channel::Fx && held);
    }
}

fn hold(sink: &impl AudioSinkPlayback, held: bool) {
    if held != sink.is_paused() {
        if held { sink.pause() } else { sink.play() }
    }
}

/// Players Bevy never gave a sink (no device behind the plugin) would never despawn: drop them.
fn reap_sinkless(
    mut commands: Commands,
    mut waiting: Query<(Entity, &mut Unsunk), (Without<AudioSink>, Without<SpatialAudioSink>)>,
    sunk: Query<Entity, (With<Unsunk>, Or<(With<AudioSink>, With<SpatialAudioSink>)>)>,
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
    use crate::audio_space::SILENT;

    fn clip(variant: u8) -> SoundClip {
        SoundClip { variant, handle: Handle::default(), reference: format!("test#{variant}") }
    }

    fn library(variants: &[u8]) -> SoundLibrary {
        let mut lib = SoundLibrary::default();
        lib.sounds.insert(SoundCue::Click, variants.iter().map(|&v| clip(v)).collect());
        lib.music.insert(MusicTrack::Menu, clip(1));
        lib
    }

    fn full_mixer() -> Mixer {
        Mixer { master: Level::new(100), music: Level::new(100), fx: Level::new(100), mute_all: false }
    }

    #[test]
    fn a_variant_is_exactly_that_one_and_any_draws_from_the_family() {
        let lib = library(&[3, 1, 4, 5]);
        assert_eq!(lib.pick(Sound::variant(SoundCue::Click, 4), 0).unwrap().variant, 4);
        assert_eq!(lib.pick(Sound::variant(SoundCue::Click, 2), 0), Err(Skip::NoVariant(2)), "no fallback to a neighbour");
        let mut seen: Vec<u8> = (0..64).map(|n| lib.pick(Sound::any(SoundCue::Click), seed(n)).unwrap().variant).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen, vec![1, 3, 4, 5]);
        let single = library(&[1]);
        assert_eq!(single.pick(SoundCue::Click.into(), 7).unwrap().variant, 1);
        assert_eq!(single.pick(Sound::variant(SoundCue::Click, 1), 7).unwrap().variant, 1);
        assert_eq!(SoundLibrary::default().pick(SoundCue::Click.into(), 0), Err(Skip::NoClip));
    }

    #[test]
    fn requests_resolve_or_say_why_not() {
        let lib = library(&[1, 2]);
        let near = PlaySound { sound: Sound::variant(SoundCue::Click, 2), at: Some(Vec2::new(1.0, 0.0)) };
        let listener = Listener::default();
        let played = resolve_sound(&near, &lib, &listener, &full_mixer(), true, 0).unwrap();
        assert_eq!((played.clip.variant, played.volume), (2, 1.0));
        let far = PlaySound { at: Some(Vec2::new(SILENT + 1.0, 0.0)), ..near };
        assert!(matches!(resolve_sound(&far, &lib, &listener, &full_mixer(), true, 0), Err(Skip::OutOfReach(_))));
        let muted = Mixer { mute_all: true, ..full_mixer() };
        assert_eq!(resolve_sound(&near, &lib, &listener, &muted, true, 0), Err(Skip::MuteAll));
        assert_eq!(resolve_sound(&near, &lib, &listener, &full_mixer(), false, 0), Err(Skip::DeviceOff));
        assert_eq!(resolve_music(MusicTrack::Game, &lib, &full_mixer(), true), Err(Skip::NoClip));
    }

    #[test]
    fn debug_lines() {
        let lib = library(&[4]);
        let req = PlaySound { sound: Sound::variant(SoundCue::Click, 4), at: Some(Vec2::new(2.0, 0.0)) };
        let ok = resolve_sound(&req, &lib, &Listener::default(), &full_mixer(), true, 0);
        assert_eq!(describe_sound(&req, &ok), "audio fx cue=Click#4 (asked 4) ref=test#4 at=(2.0,0.0) dist=2.0 pan=1.00 gain=1.00");
        let ui = PlaySound { sound: SoundCue::Click.into(), at: None };
        let ok = resolve_sound(&ui, &lib, &Listener::default(), &full_mixer(), true, 0);
        assert_eq!(describe_sound(&ui, &ok), "audio fx cue=Click#4 (asked any) ref=test#4 at=none gain=1.00");
        let missing = PlaySound { sound: Sound::variant(SoundCue::Click, 9), at: None };
        assert_eq!(describe_sound(&missing, &Err(Skip::NoVariant(9))), "audio fx cue=Click (asked 9) at=none skipped: no variant 9");
        let far = Err(Skip::OutOfReach(OutOfReach { distance: 41.0, silent: 28.0 }));
        assert_eq!(describe_sound(&req, &far), "audio fx cue=Click (asked 4) at=(2.0,0.0) skipped: out of range (dist=41.0 >= 28.0)");
        let music = resolve_music(MusicTrack::Menu, &lib, &full_mixer(), true);
        assert_eq!(describe_music(PlayMusic(Some(MusicTrack::Menu)), Some(&music)), "audio music track=Menu ref=test#1 gain=1.00 loop");
        assert_eq!(describe_music(PlayMusic(Some(MusicTrack::Game)), Some(&Err(Skip::DeviceOff))), "audio music track=Game skipped: audio device off");
        assert_eq!(describe_music(PlayMusic(None), None), "audio music stop");
    }

    #[test]
    fn fx_wait_while_the_game_does() {
        assert!(!fx_held(None, None));
        assert!(fx_held(None, Some(&GameSpeed { paused: true, times: 1 })));
        assert!(fx_held(Some(&State::new(AppState::Paused)), None));
        assert!(!fx_held(Some(&State::new(AppState::Ended)), None));
    }

    fn app(device: bool, lib: SoundLibrary) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(SoundPlugin { debug: false, device_muted: false });
        if device {
            app.insert_resource(AudioOut);
        }
        app.insert_resource(lib).insert_resource(full_mixer());
        app
    }

    fn players(app: &mut App) -> Vec<(SoundChannel, Option<Emitter>, PlaybackSettings)> {
        let mut q = app.world_mut().query::<(&SoundChannel, Option<&Emitter>, &PlaybackSettings)>();
        q.iter(app.world()).map(|(c, e, s)| (*c, e.copied(), *s)).collect()
    }

    #[test]
    fn near_sounds_spawn_players_far_or_unknown_ones_do_not() {
        let mut app = app(true, library(&[1]));
        app.world_mut().write_message(PlaySound { sound: SoundCue::Click.into(), at: Some(Vec2::new(3.0, 0.0)) });
        app.world_mut().write_message(PlaySound { sound: SoundCue::Click.into(), at: Some(Vec2::new(60.0, 0.0)) });
        app.world_mut().write_message(PlaySound { sound: Sound::variant(SoundCue::Click, 7), at: None });
        app.update();
        let spawned = players(&mut app);
        assert_eq!(spawned.len(), 1);
        let (ch, emitter, settings) = &spawned[0];
        assert_eq!((ch.0, *emitter), (Channel::Fx, Some(Emitter(Vec2::new(3.0, 0.0)))));
        assert_eq!(settings.volume, Volume::Linear(1.0));
        assert!(settings.spatial);
    }

    #[test]
    fn music_replaces_and_stops_and_nothing_spawns_without_a_device() {
        let mut lib = library(&[1]);
        lib.music.insert(MusicTrack::Game, clip(2));
        let mut app = app(true, lib);
        app.world_mut().write_message(PlayMusic(Some(MusicTrack::Menu)));
        app.update();
        app.world_mut().write_message(PlayMusic(Some(MusicTrack::Game)));
        app.update();
        assert_eq!(players(&mut app).len(), 1, "the new track replaced the old");
        app.world_mut().write_message(PlayMusic(None));
        app.update();
        assert!(players(&mut app).is_empty());

        let mut off = super::tests::app(false, library(&[1]));
        off.world_mut().write_message(PlaySound { sound: SoundCue::Click.into(), at: None });
        off.world_mut().write_message(PlayMusic(Some(MusicTrack::Menu)));
        off.update();
        assert!(players(&mut off).is_empty());
        assert_eq!(off.world().resource::<CurrentMusic>().0, Some(MusicTrack::Menu), "still knows what should play");
    }

    #[test]
    fn players_without_a_sink_are_dropped() {
        let mut app = app(true, library(&[1]));
        app.world_mut().write_message(PlaySound { sound: SoundCue::Click.into(), at: None });
        app.update();
        assert_eq!(players(&mut app).len(), 1);
        for _ in 0..SINK_WAIT {
            app.update();
        }
        assert!(players(&mut app).is_empty());
    }
}
