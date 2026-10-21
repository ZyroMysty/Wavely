//! Offline-Speicher für YouTube-Music-Songs: Download über yt-dlp, Verzeichnis in `offline/index.json`.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Semaphore;

use crate::{artwork::Artwork, ytdlp::YtDlp};

/// Gleichzeitige Downloads
const PARALLEL_DOWNLOADS: usize = 2;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineTrack {
    id: String,
    title: String,
    artist: Option<String>,
    album: Option<String>,
    duration: f64,
    artwork_url: Option<String>,
    url: Option<String>,
    /// Pfad der heruntergeladenen Datei (leer, solange der Download läuft)
    #[serde(default)]
    path: String,
    /// Einzeln gespeichert (nicht nur als Teil einer Playlist)
    #[serde(default)]
    pinned: bool,
    /// "youtube" (Standard) oder "spotify"
    #[serde(default)]
    source: Option<String>,
    /// YouTube-Video, das heruntergeladen wird (bei Spotify-Songs die gefundene Zuordnung)
    #[serde(default)]
    download_id: Option<String>,
    /// Direkte Adresse für yt-dlp (z. B. SoundCloud-Song)
    #[serde(default)]
    download_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineCollection {
    id: String,
    kind: String,
    title: String,
    subtitle: Option<String>,
    artwork_url: Option<String>,
    track_ids: Vec<String>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Index {
    tracks: HashMap<String, OfflineTrack>,
    collections: Vec<OfflineCollection>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInput {
    id: String,
    title: String,
    artist: Option<String>,
    album: Option<String>,
    duration: f64,
    artwork_url: Option<String>,
    url: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    download_id: Option<String>,
    #[serde(default)]
    download_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionInput {
    id: String,
    kind: String,
    title: String,
    subtitle: Option<String>,
    artwork_url: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    id: String,
    /// "queued" | "downloading" | "done" | "error"
    state: &'static str,
    error: Option<String>,
}

pub struct Offline {
    dir: PathBuf,
    ytdlp: YtDlp,
    index: Mutex<Index>,
    limit: Arc<Semaphore>,
}

impl Offline {
    pub fn new(app: &AppHandle, ytdlp: YtDlp) -> Result<Self, String> {
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("offline");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        // Der Player darf die gespeicherten Dateien direkt laden
        let _ = app.asset_protocol_scope().allow_directory(&dir, false);

        let mut index: Index = std::fs::read_to_string(dir.join("index.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        // Abgebrochene Downloads (z. B. App geschlossen) verwerfen
        index.tracks.retain(|_, t| !t.path.is_empty() && std::path::Path::new(&t.path).exists());
        let known: HashSet<String> = index.tracks.keys().cloned().collect();
        for c in &mut index.collections {
            c.track_ids.retain(|id| known.contains(id));
        }

        Ok(Self {
            dir,
            ytdlp,
            index: Mutex::new(index),
            limit: Arc::new(Semaphore::new(PARALLEL_DOWNLOADS)),
        })
    }

    fn save(&self, index: &Index) {
        if let Ok(json) = serde_json::to_string_pretty(index) {
            let _ = std::fs::write(self.dir.join("index.json"), json);
        }
    }

    fn snapshot(&self) -> OfflineView {
        let index = self.index.lock().unwrap();
        OfflineView {
            tracks: index.tracks.values().filter(|t| !t.path.is_empty()).cloned().collect(),
            collections: index.collections.clone(),
        }
    }

    /// Löscht Songs, die weder einzeln gespeichert noch Teil einer gespeicherten Playlist sind.
    fn cleanup(&self, index: &mut Index) {
        let referenced: HashSet<&String> = index.collections.iter().flat_map(|c| &c.track_ids).collect();
        let orphaned: Vec<String> = index
            .tracks
            .values()
            .filter(|t| !t.pinned && !referenced.contains(&t.id))
            .map(|t| t.id.clone())
            .collect();
        for id in orphaned {
            if let Some(track) = index.tracks.remove(&id) {
                // Datei nur löschen, wenn kein anderer Song sie nutzt (Spotify-Songs können denselben YouTube-Song haben)
                let shared = index.tracks.values().any(|t| t.path == track.path);
                if !track.path.is_empty() && !shared {
                    let _ = std::fs::remove_file(&track.path);
                }
            }
        }
    }
}

#[derive(Serialize)]
pub struct OfflineView {
    tracks: Vec<OfflineTrack>,
    collections: Vec<OfflineCollection>,
}

fn emit(app: &AppHandle, id: &str, state: &'static str, error: Option<String>) {
    let _ = app.emit("offline-progress", Progress { id: id.to_string(), state, error });
}

/// Entfernt einen Song, dessen Download fehlgeschlagen ist.
fn drop_failed(offline: &Offline, id: &str) {
    let mut index = offline.index.lock().unwrap();
    index.tracks.remove(id);
    for c in &mut index.collections {
        c.track_ids.retain(|t| t != id);
    }
    offline.save(&index);
}

enum Target {
    /// YouTube-Video (ID)
    YouTube(String),
    /// Direkte Adresse für yt-dlp
    Url(String),
}

/// Was heruntergeladen wird: YouTube-Video (auch als Ersatz für Spotify/SoundCloud) oder eine Adresse.
async fn download_target(app: &AppHandle, id: &str) -> Result<Target, String> {
    let track = {
        let offline = app.state::<Offline>();
        let index = offline.index.lock().unwrap();
        index.tracks.get(id).cloned().ok_or("Song wurde entfernt")?
    };
    if let Some(video) = track.download_id {
        return Ok(Target::YouTube(video));
    }
    if let Some(url) = track.download_url {
        return Ok(Target::Url(url));
    }
    if !matches!(track.source.as_deref(), Some("spotify") | Some("soundcloud")) {
        return Ok(Target::YouTube(track.id));
    }
    let video = crate::spotify::resolve(
        &app.state::<crate::spotify::Spotify>(),
        &app.state::<crate::ytm::Ytm>(),
        track.id.clone(),
        track.title.clone(),
        track.artist.clone(),
        track.duration,
    )
    .await?;
    let offline = app.state::<Offline>();
    let mut index = offline.index.lock().unwrap();
    if let Some(t) = index.tracks.get_mut(id) {
        t.download_id = Some(video.clone());
    }
    Ok(Target::YouTube(video))
}

#[tauri::command]
pub async fn offline_list(offline: State<'_, Offline>) -> Result<OfflineView, String> {
    Ok(offline.snapshot())
}

/// Speichert Songs offline. Mit `collection` wird die Playlist/das Album als Ganzes gemerkt.
#[tauri::command]
pub async fn offline_save(
    app: AppHandle,
    offline: State<'_, Offline>,
    tracks: Vec<TrackInput>,
    collection: Option<CollectionInput>,
) -> Result<(), String> {
    let mut to_download = Vec::new();
    {
        let mut index = offline.index.lock().unwrap();
        for t in &tracks {
            let entry = index.tracks.entry(t.id.clone()).or_insert_with(|| {
                to_download.push(t.id.clone());
                OfflineTrack {
                    id: t.id.clone(),
                    title: t.title.clone(),
                    artist: t.artist.clone(),
                    album: t.album.clone(),
                    duration: t.duration,
                    artwork_url: t.artwork_url.clone(),
                    url: t.url.clone(),
                    path: String::new(),
                    pinned: false,
                    source: t.source.clone(),
                    download_id: t.download_id.clone(),
                    download_url: t.download_url.clone(),
                }
            });
            if collection.is_none() {
                entry.pinned = true;
            }
        }
        if let Some(c) = &collection {
            index.collections.retain(|existing| existing.id != c.id);
            index.collections.push(OfflineCollection {
                id: c.id.clone(),
                kind: c.kind.clone(),
                title: c.title.clone(),
                subtitle: c.subtitle.clone(),
                artwork_url: c.artwork_url.clone(),
                track_ids: tracks.iter().map(|t| t.id.clone()).collect(),
            });
        }
        offline.save(&index);
    }

    // Cover gleich mit in den Cache legen
    let artwork_urls: Vec<String> = tracks
        .iter()
        .filter_map(|t| t.artwork_url.clone())
        .chain(collection.as_ref().and_then(|c| c.artwork_url.clone()))
        .collect();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let artwork = handle.state::<Artwork>();
        for url in artwork_urls {
            artwork.prefetch(&url).await;
        }
    });

    for id in to_download {
        emit(&app, &id, "queued", None);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let offline = app.state::<Offline>();
            let _permit = offline.limit.clone().acquire_owned().await;
            // Inzwischen wieder entfernt?
            if !offline.index.lock().unwrap().tracks.contains_key(&id) {
                return;
            }
            emit(&app, &id, "downloading", None);
            // Spotify-/SoundCloud-Ausschnitt-Songs: passenden YouTube-Song suchen (falls noch nicht bekannt)
            let target = match download_target(&app, &id).await {
                Ok(target) => target,
                Err(e) => {
                    drop_failed(&offline, &id);
                    emit(&app, &id, "error", Some(e));
                    return;
                }
            };
            let result = match &target {
                Target::YouTube(video) => offline.ytdlp.download(video, &offline.dir).await,
                Target::Url(url) => offline.ytdlp.download_url(url, &format!("sc-{id}"), &offline.dir).await,
            };
            match result {
                Ok(path) => {
                    let mut index = offline.index.lock().unwrap();
                    match index.tracks.get_mut(&id) {
                        Some(track) => track.path = path.to_string_lossy().into_owned(),
                        None => {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                    offline.save(&index);
                    drop(index);
                    emit(&app, &id, "done", None);
                }
                Err(e) => {
                    drop_failed(&offline, &id);
                    emit(&app, &id, "error", Some(e));
                }
            }
        });
    }
    Ok(())
}

/// Entfernt eine gespeicherte Playlist (`collection_id`) oder einzelne Songs (`track_ids`).
#[tauri::command]
pub async fn offline_remove(
    offline: State<'_, Offline>,
    collection_id: Option<String>,
    track_ids: Option<Vec<String>>,
) -> Result<OfflineView, String> {
    {
        let mut index = offline.index.lock().unwrap();
        if let Some(id) = collection_id {
            index.collections.retain(|c| c.id != id);
        }
        for id in track_ids.unwrap_or_default() {
            if let Some(track) = index.tracks.get_mut(&id) {
                track.pinned = false;
            }
            for c in &mut index.collections {
                c.track_ids.retain(|t| t != &id);
            }
        }
        offline.cleanup(&mut index);
        offline.save(&index);
    }
    Ok(offline.snapshot())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineStats {
    songs: usize,
    bytes: u64,
    folder: String,
}

#[tauri::command]
pub async fn offline_stats(offline: State<'_, Offline>) -> Result<OfflineStats, String> {
    let index = offline.index.lock().unwrap();
    let saved: Vec<&OfflineTrack> = index.tracks.values().filter(|t| !t.path.is_empty()).collect();
    let bytes = saved
        .iter()
        .filter_map(|t| std::fs::metadata(&t.path).ok())
        .map(|m| m.len())
        .sum();
    Ok(OfflineStats {
        songs: saved.len(),
        bytes,
        folder: offline.dir.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub async fn offline_open_folder(offline: State<'_, Offline>) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(&offline.dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Löscht alle Offline-Songs und gespeicherten Playlists.
#[tauri::command]
pub async fn offline_clear(offline: State<'_, Offline>) -> Result<OfflineView, String> {
    {
        let mut index = offline.index.lock().unwrap();
        for track in index.tracks.values() {
            if !track.path.is_empty() {
                let _ = std::fs::remove_file(&track.path);
            }
        }
        *index = Index::default();
        offline.save(&index);
    }
    Ok(offline.snapshot())
}
