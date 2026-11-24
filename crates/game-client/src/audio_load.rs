//! Fills the `SoundLibrary` from the player's own install (sound.md "Playback in the client"): the music
//! (`PopDrones22`) and the drum banks (`PopDrum0..922`). Nothing is read with `--no-original`.

use crate::audio::{MusicTrack, SoundClip, SoundLibrary};
use bevy::audio::AudioSource;
use bevy::log::{info, warn};
use pop3_format::sound::{MusicBank, SoundBank};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

const MUSIC_FILE: &str = "PopDrones22.SDT";
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
    for n in 0..DRUM_BANKS {
        let file = drum_file(n);
        match read(sound_dir, &file).and_then(|data| Ok((SoundBank::parse(&data)?, data))) {
            Ok((bank, data)) => library.drums.push(drum_clips(&bank, &data, &file)),
            Err(e) => warn!("drums {file} not loaded: {e}"),
        }
    }
    info!("sound: {} music tracks, {} drum banks from {}", library.music.values().map(Vec::len).sum::<usize>(), library.drums.len(), sound_dir.display());
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
    fn a_folder_without_banks_gives_an_empty_library() {
        let library = load_original(Path::new("/nonexistent-pop3-sound-dir"));
        assert!(library.music.is_empty() && library.drums.is_empty());
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
