use std::{
    fs,
    path::{Path, PathBuf},
};

use lofty::prelude::*;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "m4a", "aac", "ogg", "opus", "wav"];
const FOLDER_COVERS: &[&str] = &["cover.jpg", "cover.png", "folder.jpg", "folder.png", "front.jpg"];

/// Wird als `library.json` im Konfigurationsordner der App gespeichert.
#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    pub folders: Vec<String>,
}

#[derive(Clone, Serialize)]
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

fn settings_path(paths: &crate::host::Paths) -> PathBuf {
    paths.data.join("library.json")
}

pub fn load_settings(paths: &crate::host::Paths) -> Settings {
    fs::read_to_string(settings_path(paths))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_settings(paths: &crate::host::Paths, settings: &Settings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(settings_path(paths), json).map_err(|e| e.to_string())
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

/// Rohdaten des Covers (Bild + MIME-Typ): eingebettetes Bild oder cover.jpg & Co. im selben Ordner
pub fn cover_bytes(path: &str) -> Option<(Vec<u8>, String)> {
    let path = Path::new(path);
    let embedded = lofty::read_from_path(path).ok().and_then(|tagged| {
        let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
        let picture = tag.pictures().first()?;
        let mime = picture.mime_type().map(|m| m.as_str().to_string()).unwrap_or("image/jpeg".into());
        Some((picture.data().to_vec(), mime))
    });
    embedded.or_else(|| {
        let dir = path.parent()?;
        FOLDER_COVERS.iter().find_map(|name| {
            let data = fs::read(dir.join(name)).ok()?;
            let mime = if name.ends_with(".png") { "image/png" } else { "image/jpeg" };
            Some((data, mime.to_string()))
        })
    })
}

// --- Ordner beobachten -------------------------------------------------------

/// Beobachtet die Musikordner; neue, gelöschte oder umbenannte Audiodateien melden sich
/// (gesammelt nach 2 Sekunden Ruhe).
#[derive(Default)]
pub struct Watcher(
    std::sync::Mutex<Option<(Vec<String>, notify_debouncer_mini::Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>)>>,
);

impl Watcher {
    /// Ordner beobachten; `changed` läuft, wenn Audiodateien dazukommen oder verschwinden
    pub fn watch(&self, folders: &[String], changed: impl Fn() + Send + 'static) {
        use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult};

        let mut current = self.0.lock().unwrap();
        if current.as_ref().is_some_and(|(watched, _)| watched == folders) {
            return;
        }
        *current = None;
        if folders.is_empty() {
            return;
        }
        let Ok(mut debouncer) = new_debouncer(std::time::Duration::from_secs(2), move |result: DebounceEventResult| {
            if let Ok(events) = result {
                if events.iter().any(|e| is_audio(&e.path)) {
                    changed();
                }
            }
        }) else {
            return;
        };
        for folder in folders {
            let _ = debouncer.watcher().watch(Path::new(folder), RecursiveMode::Recursive);
        }
        *current = Some((folders.to_vec(), debouncer));
    }
}
