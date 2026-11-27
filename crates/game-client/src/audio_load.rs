//! Fills the `SoundLibrary` from the player's own install (sound.md "Playback in the client"): the music
//! (`PopDrones22`), the drum banks (`PopDrum0..922`) and the effects and voices (`Soundd2`). Nothing is read with
//! `--no-original`.

use crate::audio::{CueInfo, Family, MusicTrack, SoundClip, SoundCue, SoundLibrary};
use crate::audio_ids::{self, SoundIdRecord};
use game_core::spell_book::SpellKind;
use bevy::audio::AudioSource;
use bevy::log::{info, warn};
use pop3_format::sound::{MusicBank, SoundBank};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

const MUSIC_FILE: &str = "PopDrones22.SDT";
const SFX_FILE: &str = "Soundd2.sdt";

/// Every cue looked up in `Soundd2`.
pub fn cues() -> Vec<SoundCue> {
    use SoundCue::*;
    let mut cues = vec![
        Click, PlanPlaced, PlanRefused, PlanTurned, FlybyStart, FlybyCancel, ShamanSelect, FollowerOk, ShamanYes, Death, ShamanDie, EnemyShamanDie, GhostDie, Drown, IntoAir, IntoWind,
        ShamanScream, EnemyShamanScream, Land, ShamanLand, OnFire, Converted, Invisible, InvisibleOff, Mutter, Chop, ChopPickUp, Hammer, Stomp, Eat,
        WildEat, Drink, WildDrink, BuildingDone, Whoosh, BlastImpact, Thunder, Splash, Wind, Swarm, Swamp, ErodeRumble, QuakeRumble, QuakeCrack,
        VolcanoRumble, VolcanoRise, Lava, GroundFire, FireCatches, BuildingFire, WoodCrack, BranchSnap, BridgeCast, BridgeCast2, HypnotismCast,
        GhostArmyCast, ConvertCast, FlattenCast, AngelCast, AngelFall, AngelLand, Throw, BoatRide, BalloonRide, BoatExplode, BalloonPop, StoneRise,
        StoneHeadNod, BattleCrowd, ShieldHum,
    ];
    for n in 1..=4 {
        cues.extend([FollowerBow(n), SpyBow(n), PreacherBow(n), SpyOk(n), PreacherOk(n)]);
    }
    cues.extend(SpellKind::ALL.iter().flat_map(|&s| [CastShout(s), EnemyCastShout(s)]));
    cues.extend(crate::audio_ambience::Layer::ALL.map(Bed));
    cues.extend(crate::audio_ambience::Wilds::ALL.map(Wildlife));
    cues.push(Gulls);
    cues
}

/// A cue's settings from its id's record; the info-window click plays while paused (sound.md "Sound id table":
/// a flag of the play call, not of the record).
pub fn cue_info(cue: SoundCue, record: &SoundIdRecord) -> CueInfo {
    CueInfo { volume: record.volume, priority: record.priority, while_paused: cue == SoundCue::Click }
}

/// The clips of an id's entry range, as WAV, numbered from 1 in range order; empty entries left out.
pub fn id_clips(record: &SoundIdRecord, bank: &SoundBank, data: &[u8]) -> Vec<SoundClip> {
    (record.first..=record.last)
        .filter_map(|i| bank.entries.get(i as usize).map(|e| (i, e)))
        .filter(|(_, e)| !e.is_placeholder() && !e.is_compressed())
        .map(|(i, e)| clip((i - record.first + 1) as u8, e.wav(data), format!("{SFX_FILE}#{i} {} id=0x{:02X}", e.name, record.id)))
        .collect()
}

/// Each cue's family of `Soundd2` entries through the sound id table; cues of a silent id or with no entry left out.
pub fn sfx_families(bank: &SoundBank, data: &[u8]) -> HashMap<SoundCue, Family> {
    let mut by_id: HashMap<u8, Vec<SoundClip>> = HashMap::new();
    let mut sounds = HashMap::new();
    for cue in cues() {
        let Some(record) = audio_ids::record(cue.id()) else { continue };
        let clips = by_id.entry(record.id).or_insert_with(|| id_clips(record, bank, data)).clone();
        if !clips.is_empty() {
            sounds.insert(cue, Family { info: cue_info(cue, record), clips });
        }
    }
    sounds
}
const DRUM_BANKS: u8 = 10;

fn clip(variant: u8, bytes: Vec<u8>, reference: String) -> SoundClip {
    SoundClip { variant, source: AudioSource { bytes: Arc::from(bytes) }, reference }
}

/// Track 1 behind the menus, tracks 2-5 in a game (sound.md "Music and drums"): each its raw Layer II stream.
pub fn music_clips(bank: &MusicBank, data: &[u8]) -> HashMap<MusicTrack, Vec<SoundClip>> {
    let mut music: HashMap<MusicTrack, Vec<SoundClip>> = HashMap::new();
    for (i, entry) in bank.entries.iter().enumerate() {
        let track = if i == 0 { MusicTrack::Menu } else { MusicTrack::Game };
        let clips = music.entry(track).or_default();
        let variant = clips.len() as u8 + 1;
        clips.push(clip(variant, entry.mpeg_data(data).to_vec(), format!("{MUSIC_FILE}#{i} {}", entry.name)));
    }
    music
}

/// A drum bank's loops by entry, entry 0 (the calm loop) included (sound.md "Flags").
pub fn drum_clips(bank: &SoundBank, data: &[u8], file: &str) -> Vec<SoundClip> {
    bank.entries.iter().enumerate().map(|(i, e)| clip(i as u8, e.wav(data), format!("{file}#{i} {}", e.name))).collect()
}

fn drum_file(n: u8) -> String {
    format!("PopDrum{n}22.SDT")
}

/// Whatever of the music and drums is found in `sound_dir`; a missing or broken file is logged and skipped.
pub fn load_original(sound_dir: &Path) -> SoundLibrary {
    let mut library = SoundLibrary::default();
    match read(sound_dir, MUSIC_FILE).and_then(|data| Ok((MusicBank::parse(&data)?, data))) {
        Ok((bank, data)) => library.music = music_clips(&bank, &data),
        Err(e) => warn!("music {MUSIC_FILE} not loaded: {e}"),
    }
    match read(sound_dir, SFX_FILE).and_then(|data| Ok((SoundBank::parse(&data)?, data))) {
        Ok((bank, data)) => library.sounds = sfx_families(&bank, &data),
        Err(e) => warn!("sounds {SFX_FILE} not loaded: {e}"),
    }
    for n in 0..DRUM_BANKS {
        let file = drum_file(n);
        match read(sound_dir, &file).and_then(|data| Ok((SoundBank::parse(&data)?, data))) {
            Ok((bank, data)) => library.drums.push(drum_clips(&bank, &data, &file)),
            Err(e) => warn!("drums {file} not loaded: {e}"),
        }
    }
    info!("sound: {} music tracks, {} drum banks, {} of {} cues from {}", library.music.values().map(Vec::len).sum::<usize>(), library.drums.len(), library.sounds.len(), cues().len(), sound_dir.display());
    library
}

fn read(dir: &Path, name: &str) -> Result<Vec<u8>, pop3_format::level::LevelError> {
    let path = pop3_format::find_file(dir, name).unwrap_or_else(|| dir.join(name));
    Ok(std::fs::read(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn container(entries: &[(&[u8], u8, Vec<u8>)]) -> Vec<u8> {
        let mut offsets = Vec::new();
        let mut body = Vec::new();
        let mut at = 4 + 4 * entries.len();
        for (name, flags, payload) in entries {
            let mut h = vec![0u8; 40];
            h[0..4].copy_from_slice(&40u32.to_le_bytes());
            h[4..8].copy_from_slice(&(payload.len() as u32).to_le_bytes());
            h[8..8 + name.len()].copy_from_slice(name);
            h[24..26].copy_from_slice(&22050u16.to_le_bytes());
            h[26] = 16;
            h[27] = *flags;
            offsets.push(at as u32);
            at += 40 + payload.len();
            body.extend(h);
            body.extend(payload);
        }
        let mut out = (entries.len() as u32).to_le_bytes().to_vec();
        offsets.iter().for_each(|o| out.extend(o.to_le_bytes()));
        out.extend(body);
        out
    }

    #[test]
    fn the_first_track_is_the_menu_one_the_others_the_game_variants() {
        let data = container(&[(b"idea11", 0x25, vec![0xff, 0xf5, 1]), (b"idea13", 0x25, vec![0xff, 0xf5, 2]), (b"idea16", 0x25, vec![0xff, 0xf5, 3])]);
        let music = music_clips(&MusicBank::parse(&data).unwrap(), &data);
        let menu = &music[&MusicTrack::Menu];
        assert_eq!(menu.len(), 1);
        assert_eq!(&*menu[0].source.bytes, &[0xff, 0xf5, 1]);
        let game = &music[&MusicTrack::Game];
        assert_eq!(game.iter().map(|c| c.variant).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(&*game[1].source.bytes, &[0xff, 0xf5, 3]);
        assert_eq!(game[0].reference, "PopDrones22.SDT#1 idea13");
    }

    #[test]
    fn drum_loops_keep_their_entry_index_and_play_as_wav() {
        let data = container(&[(b"0a", 0x83, vec![0; 8]), (b"0a", 0x03, vec![1; 8]), (b"0b", 0x03, vec![2; 8])]);
        let drums = drum_clips(&SoundBank::parse(&data).unwrap(), &data, "PopDrum022.SDT");
        assert_eq!(drums.iter().map(|c| c.variant).collect::<Vec<_>>(), vec![0, 1, 2]);
        assert_eq!(&drums[2].source.bytes[..4], b"RIFF");
        assert_eq!(&drums[2].source.bytes[44..], &[2; 8]);
        assert_eq!(drums[1].reference, "PopDrum022.SDT#1 0a");
    }

    #[test]
    fn a_cue_plays_its_id_s_entry_range_with_its_settings() {
        let mut entries: Vec<(&[u8], u8, Vec<u8>)> = (0..495).map(|_| (&b"x"[..], 0x02u8, vec![0u8; 4])).collect();
        entries[16] = (b"chop2", 0x92, vec![]);
        let data = container(&entries);
        let sounds = sfx_families(&SoundBank::parse(&data).unwrap(), &data);
        let chop = &sounds[&SoundCue::Chop];
        assert_eq!(chop.clips.iter().map(|c| c.variant).collect::<Vec<_>>(), vec![1, 3, 4, 5], "entries 15-19, the empty one left out");
        assert_eq!(chop.clips[0].reference, "Soundd2.sdt#15 x id=0x01");
        assert_eq!(chop.info, CueInfo { volume: 127, priority: 0x80, while_paused: false });
        assert_eq!(sounds[&SoundCue::Land].info.volume, 80);
        assert!(sounds[&SoundCue::Click].info.while_paused);
        assert!(!sounds.contains_key(&SoundCue::CastShout(SpellKind::Teleport)), "no shout of its own");
    }

    #[test]
    fn every_cue_but_the_shoutless_spells_has_a_record() {
        let silent: Vec<SoundCue> = cues().into_iter().filter(|c| audio_ids::record(c.id()).is_none()).collect();
        assert!(silent.iter().all(|c| matches!(c, SoundCue::CastShout(_) | SoundCue::EnemyCastShout(_))), "{silent:?}");
        assert_eq!(silent.len(), 6);
    }

    #[test]
    fn a_folder_without_banks_gives_an_empty_library() {
        let library = load_original(Path::new("/nonexistent-pop3-sound-dir"));
        assert!(library.music.is_empty() && library.drums.is_empty() && library.sounds.is_empty());
    }

    #[test]
    fn the_installed_music_decodes() {
        let Some(install) = pop3_format::install::find_install() else { return };
        let dir = pop3_format::install::subdir(&install, "Sound");
        let Ok(data) = std::fs::read(pop3_format::find_file(&dir, MUSIC_FILE).unwrap_or_else(|| dir.join(MUSIC_FILE))) else { return };
        let music = music_clips(&MusicBank::parse(&data).unwrap(), &data);
        use bevy::audio::{Decodable, Source};
        let decoder = music[&MusicTrack::Menu][0].source.decoder();
        assert_eq!(decoder.sample_rate().get(), 22050);
        assert!(decoder.take(1000).any(|s| s != 0.0));
    }
}
