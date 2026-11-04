//! Gespeicherte Daten als Dateien im App-Ordner (statt localStorage der WebView-Version):
//! - `settings.json`: Einstellungen
//! - `resume.json`: Warteschlange zum Fortsetzen nach dem Neustart
//! - `history.json`: Verlauf (300 Einträge, neueste zuerst)
//! - `playlists.json`: Wavely-Playlists (gleiches Format wie bisher)
//! - `stats/plays.jsonl`: jedes Hören für den Rückblick (gleiches Format wie bisher)

use std::{io::Write, path::PathBuf, sync::Mutex};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::player::{Repeat, Track};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// "system" | "dark" | "light"
    pub theme: String,
    pub accent: String,
    pub start_view: String,
    pub volume: f32,
    pub autoplay: bool,
    pub discord_enabled: bool,
    pub discord_display: String,
    pub sync_you_tube_history: bool,
    pub normalize: bool,
    pub eq_enabled: bool,
    pub eq_preset: String,
    pub eq_gains: Vec<f32>,
    pub playback_rate: f32,
    pub stream_quality: String,
    pub download_quality: String,
    pub sidebar_collapsed: bool,
    pub lastfm_scrobble: bool,
    pub test_channel: bool,
    pub output_device: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            accent: "#ff4f7b".into(),
            start_view: "home".into(),
            volume: 0.6,
            autoplay: true,
            discord_enabled: true,
            discord_display: "details".into(),
            sync_you_tube_history: true,
            normalize: false,
            eq_enabled: false,
            eq_preset: "flat".into(),
            eq_gains: vec![0.0; 10],
            playback_rate: 1.0,
            stream_quality: "high".into(),
            download_quality: "high".into(),
            sidebar_collapsed: false,
            lastfm_scrobble: true,
            test_channel: false,
            output_device: String::new(),
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Resume {
    pub queue: Vec<Track>,
    pub index: usize,
    pub position: f64,
    pub shuffle: bool,
    pub repeat: Repeat,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub track: Track,
    /// Unix-Millisekunden
    pub played_at: u64,
}

pub const MAX_HISTORY: usize = 300;

pub struct Store {
    dir: PathBuf,
    lock: Mutex<()>,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl Store {
    pub fn new(paths: &crate::host::Paths) -> Self {
        let _ = std::fs::create_dir_all(paths.data.join("stats"));
        Self { dir: paths.data.clone(), lock: Mutex::new(()) }
    }

    fn read<T: DeserializeOwned + Default>(&self, name: &str) -> T {
        let _guard = self.lock.lock().unwrap();
        std::fs::read_to_string(self.dir.join(name)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    /// Erst in eine Zwischendatei schreiben, damit bei einem Absturz nichts kaputtgeht
    fn write<T: Serialize>(&self, name: &str, value: &T) {
        let Ok(json) = serde_json::to_string(value) else { return };
        let _guard = self.lock.lock().unwrap();
        let path = self.dir.join(name);
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }

    pub fn settings(&self) -> Settings {
        self.read("settings.json")
    }
    pub fn save_settings(&self, settings: &Settings) {
        self.write("settings.json", settings)
    }

    pub fn resume(&self) -> Resume {
        self.read("resume.json")
    }
    pub fn save_resume(&self, resume: &Resume) {
        self.write("resume.json", resume)
    }

    /// Gemessene Lautheit je Song („quelle:id“ → LUFS)
    pub fn loudness(&self) -> std::collections::HashMap<String, f32> {
        self.read("loudness.json")
    }
    pub fn save_loudness(&self, map: &std::collections::HashMap<String, f32>) {
        self.write("loudness.json", map)
    }

    pub fn history(&self) -> Vec<HistoryEntry> {
        self.read("history.json")
    }
    pub fn save_history(&self, history: &[HistoryEntry]) {
        self.write("history.json", &history)
    }

    pub fn playlists_raw(&self) -> serde_json::Value {
        self.read::<serde_json::Value>("playlists.json")
    }
    pub fn save_playlists_raw(&self, value: &serde_json::Value) {
        self.write("playlists.json", value)
    }

    /// Gehörte Songs anhängen (je eine Zeile JSON)
    pub fn append_plays(&self, entries: &[serde_json::Value]) {
        let _guard = self.lock.lock().unwrap();
        let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join("stats").join("plays.jsonl")) else {
            return;
        };
        for entry in entries {
            if let Ok(line) = serde_json::to_string(entry) {
                let _ = writeln!(file, "{line}");
            }
        }
    }

    pub fn load_plays(&self, since: u64) -> Vec<serde_json::Value> {
        let _guard = self.lock.lock().unwrap();
        std::fs::read_to_string(self.dir.join("stats").join("plays.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|entry| entry["t"].as_u64().unwrap_or(0) >= since)
            .collect()
    }
}
