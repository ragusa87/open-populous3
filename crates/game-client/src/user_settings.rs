//! The player's settings, kept between runs as JSON in the platform's config directory
//! (`$XDG_CONFIG_HOME` or `~/.config` on Linux, `%APPDATA%` on Windows, `~/Library/Application Support` on
//! macOS), under `open-populous/settings.json`; `POP3_CONFIG_DIR` points elsewhere. Never inside the original
//! install. Read once at launch; written when `SaveSettings` is sent (leaving the Audio page), never by a
//! headless run. A missing or unreadable field falls back to its default alone.

use crate::audio_mix::{Level, Mixer};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const APP_DIR: &str = "open-populous";
const FILE: &str = "settings.json";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct UserSettings {
    pub audio: AudioSettings,
}

/// Volumes in percent.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct AudioSettings {
    pub master: u32,
    pub music: u32,
    pub fx: u32,
    pub mute_all: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings::from(&Mixer::default())
    }
}

impl From<&Mixer> for AudioSettings {
    fn from(m: &Mixer) -> Self {
        AudioSettings { master: m.master.percent().into(), music: m.music.percent().into(), fx: m.fx.percent().into(), mute_all: m.mute_all }
    }
}

impl AudioSettings {
    pub fn mixer(&self) -> Mixer {
        Mixer { master: Level::new(self.master), music: Level::new(self.music), fx: Level::new(self.fx), mute_all: self.mute_all }
    }
}

/// Settings from the file's text; defaults for what is missing, all defaults if it is not valid JSON.
pub fn parse(text: &str) -> Result<UserSettings, serde_json::Error> {
    serde_json::from_str(text)
}

pub fn to_json(settings: &UserSettings) -> String {
    serde_json::to_string_pretty(settings).expect("settings serialize")
}

/// `override_dir/settings.json` if given, else `<config>/open-populous/settings.json`.
pub fn settings_path_from(override_dir: Option<PathBuf>, config_dir: Option<PathBuf>) -> Option<PathBuf> {
    match override_dir {
        Some(dir) => Some(dir.join(FILE)),
        None => config_dir.map(|c| c.join(APP_DIR).join(FILE)),
    }
}

pub fn settings_path() -> Option<PathBuf> {
    settings_path_from(std::env::var_os("POP3_CONFIG_DIR").filter(|v| !v.is_empty()).map(PathBuf::from), dirs::config_dir())
}

pub fn load(path: &Path) -> UserSettings {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text).unwrap_or_else(|e| {
            warn!("settings: {} is not valid ({e}), using the defaults", path.display());
            UserSettings::default()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => UserSettings::default(),
        Err(e) => {
            warn!("settings: cannot read {} ({e}), using the defaults", path.display());
            UserSettings::default()
        }
    }
}

/// Writes beside it first, then renames over it: a crash mid-write never leaves half a file.
pub fn save(path: &Path, settings: &UserSettings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, to_json(settings))?;
    std::fs::rename(&tmp, path)
}

/// Where the settings are written; None: kept in memory only (headless, or no config directory).
#[derive(Resource, Debug, Default)]
pub struct SettingsFile(pub Option<PathBuf>);

/// Writes the current settings to `SettingsFile`.
#[derive(Message, Debug, Clone, Copy, Default)]
pub struct SaveSettings;

pub struct UserSettingsPlugin;

impl Plugin for UserSettingsPlugin {
    fn build(&self, app: &mut App) {
        let path = settings_path();
        if path.is_none() {
            warn!("settings: no config directory found, settings are not kept");
        }
        let settings = path.as_deref().map(load).unwrap_or_default();
        let writable = path.filter(|_| !crate::dev::headless());
        app.insert_resource(settings.audio.mixer())
            .insert_resource(SettingsFile(writable))
            .add_message::<SaveSettings>()
            .add_systems(Update, save_on_request);
    }
}

fn save_on_request(mut requests: MessageReader<SaveSettings>, file: Res<SettingsFile>, mixer: Res<Mixer>) {
    if requests.read().count() == 0 {
        return;
    }
    let Some(path) = &file.0 else { return };
    let settings = UserSettings { audio: AudioSettings::from(&*mixer) };
    match save(path, &settings) {
        Ok(()) => info!("settings: saved to {}", path.display()),
        Err(e) => warn!("settings: cannot write {} ({e})", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("open-populous-settings-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trips_through_json() {
        let s = UserSettings { audio: AudioSettings { master: 30, music: 0, fx: 100, mute_all: true } };
        assert_eq!(parse(&to_json(&s)).unwrap(), s);
    }

    #[test]
    fn missing_fields_keep_their_defaults() {
        let s = parse(r#"{"audio": {"music": 10}}"#).unwrap();
        assert_eq!(s.audio, AudioSettings { music: 10, ..AudioSettings::default() });
        assert_eq!(parse("{}").unwrap(), UserSettings::default());
        assert_eq!(parse(r#"{"audio": {"fx": 50}, "later": 1}"#).unwrap().audio.fx, 50, "unknown keys from a newer version are skipped");
        assert!(parse("not json").is_err());
    }

    #[test]
    fn volumes_past_100_are_clamped() {
        let m = parse(r#"{"audio": {"master": 900}}"#).unwrap().audio.mixer();
        assert_eq!(m.master, Level::new(100));
    }

    #[test]
    fn path_is_the_override_or_the_config_dir() {
        assert_eq!(settings_path_from(Some("/x".into()), Some("/c".into())), Some(PathBuf::from("/x/settings.json")));
        assert_eq!(settings_path_from(None, Some("/c".into())), Some(PathBuf::from("/c/open-populous/settings.json")));
        assert_eq!(settings_path_from(None, None), None);
    }

    #[test]
    fn saved_settings_load_back_and_bad_files_give_defaults() {
        let dir = scratch("save");
        let path = dir.join("sub").join(FILE);
        let s = UserSettings { audio: AudioSettings { master: 20, music: 40, fx: 60, mute_all: true } };
        save(&path, &s).unwrap();
        assert_eq!(load(&path), s);
        std::fs::write(&path, "{ broken").unwrap();
        assert_eq!(load(&path), UserSettings::default());
        assert_eq!(load(&dir.join("missing.json")), UserSettings::default());
    }
}
