use std::{
    fs,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use lofty::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use walkdir::WalkDir;

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "m4a", "aac", "ogg", "opus", "wav"];
const FOLDER_COVERS: &[&str] = &["cover.jpg", "cover.png", "folder.jpg", "folder.png", "front.jpg"];

/// Wird als `library.json` im Konfigurationsordner der App gespeichert.
#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    pub folders: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTrack {
    pub id: String,
    pub path: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track_no: Option<u32>,
    /// Länge in Sekunden
    pub duration: f64,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("library.json"))
}

pub fn load_settings(app: &AppHandle) -> Settings {
    settings_path(app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(settings_path(app)?, json).map_err(|e| e.to_string())
}

/// Gibt die Ordner für das asset-Protokoll frei, damit der Player die Dateien laden darf.
pub fn allow_folders(app: &AppHandle, folders: &[String]) {
    let scope = app.asset_protocol_scope();
    for folder in folders {
        let _ = scope.allow_directory(folder, true);
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

fn non_empty(value: Option<std::borrow::Cow<'_, str>>) -> Option<String> {
    value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn read_track(path: &Path) -> LocalTrack {
    let path_str = path.to_string_lossy().into_owned();
    let mut track = LocalTrack {
        id: path_str.clone(),
        path: path_str,
        title: path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        artist: None,
        album: None,
        track_no: None,
        duration: 0.0,
    };

    if let Ok(tagged) = lofty::read_from_path(path) {
        track.duration = tagged.properties().duration().as_secs_f64();
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(title) = non_empty(tag.title()) {
                track.title = title;
            }
            track.artist = non_empty(tag.artist());
            track.album = non_empty(tag.album());
            track.track_no = tag.track();
        }
    }
    track
}

pub fn scan(folders: &[String]) -> Vec<LocalTrack> {
    let mut tracks: Vec<LocalTrack> = folders
        .iter()
        .flat_map(|folder| WalkDir::new(folder).follow_links(true))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && is_audio(entry.path()))
        .map(|entry| read_track(entry.path()))
        .collect();

    // Gleiche Datei aus überlappenden Ordnern nur einmal anzeigen
    tracks.sort_by(|a, b| a.id.cmp(&b.id));
    tracks.dedup_by(|a, b| a.id == b.id);

    tracks.sort_by_cached_key(|t| {
        (
            t.artist.as_deref().unwrap_or("").to_lowercase(),
            t.album.as_deref().unwrap_or("").to_lowercase(),
            t.track_no.unwrap_or(u32::MAX),
            t.title.to_lowercase(),
        )
    });
    tracks
}

/// Liefert das Cover als data-URL: eingebettetes Bild oder cover.jpg & Co. im selben Ordner.
pub fn cover(path: &str) -> Option<String> {
    let path = Path::new(path);

    if let Ok(tagged) = lofty::read_from_path(path) {
        let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
        if let Some(picture) = tag.and_then(|t| t.pictures().first()) {
            let mime = picture.mime_type().map(|m| m.as_str()).unwrap_or("image/jpeg");
            return Some(format!("data:{mime};base64,{}", STANDARD.encode(picture.data())));
        }
    }

    let dir = path.parent()?;
    FOLDER_COVERS.iter().find_map(|name| {
        let data = fs::read(dir.join(name)).ok()?;
        let mime = if name.ends_with(".png") { "image/png" } else { "image/jpeg" };
        Some(format!("data:{mime};base64,{}", STANDARD.encode(data)))
    })
}
