//! Eigene Wavely-Daten als Dateien im App-Ordner (statt im Browser-Speicher, der begrenzt ist):
//! - `playlists.json`: Wavely-Playlists (Songs aus allen Quellen gemischt)
//! - `stats/plays.jsonl`: jedes Hören mit gehörter Zeit – Grundlage für den Rückblick
//!   (eine Zeile pro Eintrag, damit Anhängen schnell bleibt)

use std::{
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

use tauri::{AppHandle, Manager, State};

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub struct Store {
    dir: PathBuf,
    lock: Mutex<()>,
}

impl Store {
    pub fn new(app: &AppHandle) -> Result<Self, String> {
        let dir = app.path().app_data_dir().map_err(err)?;
        std::fs::create_dir_all(dir.join("stats")).map_err(err)?;
        Ok(Self { dir, lock: Mutex::new(()) })
    }

    fn playlists_path(&self) -> PathBuf {
        self.dir.join("playlists.json")
    }

    fn plays_path(&self) -> PathBuf {
        self.dir.join("stats").join("plays.jsonl")
    }
}

/// Playlists als JSON-Text (leer = noch keine)
#[tauri::command]
pub fn playlists_load(store: State<'_, Store>) -> Result<String, String> {
    let _guard = store.lock.lock().unwrap();
    Ok(std::fs::read_to_string(store.playlists_path()).unwrap_or_default())
}

/// Playlists speichern (erst in eine Zwischendatei, damit bei einem Absturz nichts verloren geht)
#[tauri::command]
pub fn playlists_save(store: State<'_, Store>, json: String) -> Result<(), String> {
    serde_json::from_str::<serde_json::Value>(&json).map_err(|_| "Ungültige Playlist-Daten".to_string())?;
    let _guard = store.lock.lock().unwrap();
    let tmp = store.playlists_path().with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(err)?;
    std::fs::rename(&tmp, store.playlists_path()).map_err(err)
}

/// Gehörte Songs anhängen (je ein JSON-Objekt)
#[tauri::command]
pub fn stats_append(store: State<'_, Store>, entries: Vec<serde_json::Value>) -> Result<(), String> {
    let _guard = store.lock.lock().unwrap();
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(store.plays_path()).map_err(err)?;
    for entry in entries {
        writeln!(file, "{}", serde_json::to_string(&entry).map_err(err)?).map_err(err)?;
    }
    Ok(())
}

/// Alle gehörten Songs ab `since` (Unix-Millisekunden) – Zeilen, die nicht lesbar sind, werden übersprungen
#[tauri::command]
pub fn stats_load(store: State<'_, Store>, since: Option<u64>) -> Result<Vec<serde_json::Value>, String> {
    let _guard = store.lock.lock().unwrap();
    let text = std::fs::read_to_string(store.plays_path()).unwrap_or_default();
    let since = since.unwrap_or(0);
    Ok(text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|entry| entry["t"].as_u64().unwrap_or(0) >= since)
        .collect())
}
