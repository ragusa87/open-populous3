//! Fills the `SoundLibrary` from the player's own install (sound.md "Playback in the client"): the music
//! (`PopDrones22`), the drum banks (`PopDrum0..922`) and the effects and voices (`Soundd2`). Nothing is read with
//! `--no-original`.

use crate::audio::{Family, MusicTrack, SoundClip, SoundCue, SoundLibrary};
use bevy::audio::AudioSource;
use bevy::log::{info, warn};
use pop3_format::sound::{MusicBank, SoundBank};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

const MUSIC_FILE: &str = "PopDrones22.SDT";
const SFX_FILE: &str = "Soundd2.sdt";

/// Every cue looked up in `Soundd2`.
pub const CUES: &[SoundCue] = &[
    SoundCue::Click,
    SoundCue::FlybyStart,
    SoundCue::FlybyCancel,
    SoundCue::FollowerBow(1),
    SoundCue::FollowerBow(2),
    SoundCue::FollowerBow(3),
    SoundCue::SpyBow(1),
    SoundCue::SpyBow(2),
    SoundCue::PreacherBow,
    SoundCue::ShamanSelect,
    SoundCue::FollowerOk,
    SoundCue::SpyOk(1),
    SoundCue::SpyOk(2),
    SoundCue::PreacherOk,
    SoundCue::ShamanYes,
];

/// The `Soundd2` name family a cue plays. A guess (ours) from the entry names: the original picks entries through
/// its sound id table, not in docs/ yet (TODO.md "Sound"); the voices follow sound.md "Voices", the menu click and
/// the flyby sounds are picked by name alone.
pub fn family_name(cue: SoundCue) -> &'static str {
    match cue {
        SoundCue::Click => "confirm",
        SoundCue::FlybyStart => "Zoom_in",
        SoundCue::FlybyCancel => "Zoom_out",
        SoundCue::FollowerBow(1) => "v_bow",
        SoundCue::FollowerBow(2) => "v_bow2_",
        SoundCue::FollowerBow(_) => "v_bow3_",
        SoundCue::SpyBow(1) => "v_spy_bow",
        SoundCue::SpyBow(_) => "v_spy_bow2_",
        SoundCue::PreacherBow => "v_pre_bow",
        SoundCue::ShamanSelect => "Sv_sel",
        SoundCue::FollowerOk => "v_ok",
        SoundCue::SpyOk(1) => "v_spy_ok",
        SoundCue::SpyOk(_) => "v_spy_ok2_",
        SoundCue::PreacherOk => "v_pre_ok",
        SoundCue::ShamanYes => "Sv_yes",
    }
}

/// The variant number of `name` in the family `family`: what follows is digits only (`v_bow003`: 3), or nothing
/// (`Sv_blast`: 1). `v_bow2_001` is not in `v_bow`.
pub fn variant_of(name: &str, family: &str) -> Option<u8> {
    let rest = name.strip_prefix(family)?;
    if rest.is_empty() {
        return Some(1);
    }
    rest.bytes().all(|b| b.is_ascii_digit()).then(|| rest.parse::<u32>().map_or(1, |n| n.clamp(1, 255) as u8))
}

/// Each cue's family of `Soundd2` entries, as WAV; cues with no entry are left out.
pub fn sfx_families(bank: &SoundBank, data: &[u8]) -> HashMap<SoundCue, Family> {
    let mut sounds = HashMap::new();
    for &cue in CUES {
        let family = family_name(cue);
        let clips: Vec<SoundClip> = bank
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.is_placeholder() && !e.is_compressed())
            .filter_map(|(i, e)| variant_of(&e.name, family).map(|v| clip(v, e.wav(data), format!("{SFX_FILE}#{i} {}", e.name))))
            .collect();
        if !clips.is_empty() {
            sounds.insert(cue, Family { clips, ..Family::default() });
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
    info!("sound: {} music tracks, {} drum banks, {} of {} cues from {}", library.music.values().map(Vec::len).sum::<usize>(), library.drums.len(), library.sounds.len(), CUES.len(), sound_dir.display());
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
    fn a_family_is_its_name_then_digits_only() {
        assert_eq!(variant_of("v_bow003", "v_bow"), Some(3));
        assert_eq!(variant_of("v_bow2_001", "v_bow"), None);
        assert_eq!(variant_of("v_bow2_001", "v_bow2_"), Some(1));
        assert_eq!(variant_of("Sv_yes2", "Sv_yes"), Some(2));
        assert_eq!(variant_of("Zoom_in", "Zoom_in"), Some(1));
        assert_eq!(variant_of("En_Sv_yes2", "Sv_yes"), None);
    }

    #[test]
    fn cues_gather_their_family_and_skip_empty_entries() {
        let data = container(&[(b"v_bow001", 0x02, vec![1; 4]), (b"v_bow2_001", 0x02, vec![2; 4]), (b"v_bow002", 0x02, vec![3; 4]), (b"v_bow003", 0x92, vec![])]);
        let sounds = sfx_families(&SoundBank::parse(&data).unwrap(), &data);
        assert_eq!(sounds[&SoundCue::FollowerBow(1)].clips.iter().map(|c| c.variant).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(sounds[&SoundCue::FollowerBow(2)].clips.len(), 1);
        assert!(!sounds.contains_key(&SoundCue::ShamanYes));
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
