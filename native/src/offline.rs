//! Offline-Speicher: Songs über yt-dlp herunterladen, Verzeichnis in `offline/index.json`
//! (gleiches Format wie bisher). Beim ersten Start wird das Verzeichnis der bisherigen Wavely
//! übernommen – die Dateien bleiben, wo sie sind.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};

use crate::{
    icon,
    nav::PageKind,
    pages::{CardData, PageData, RowData, Target},
    player::Track,
    App,
};

/// Gleichzeitige Downloads
const PARALLEL_DOWNLOADS: usize = 2;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OfflineTrack {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: f64,
    pub artwork_url: Option<String>,
    pub url: Option<String>,
    /// Pfad der heruntergeladenen Datei (leer, solange der Download läuft)
    pub path: String,
    /// Einzeln gespeichert (nicht nur als Teil einer Playlist)
    pub pinned: bool,
    /// "youtube" (Standard), "spotify" oder "soundcloud"
    pub source: Option<String>,
    /// YouTube-Video, das heruntergeladen wird (bei Spotify-Songs die gefundene Zuordnung)
    pub download_id: Option<String>,
    /// Direkte Adresse für yt-dlp (z. B. SoundCloud-Song)
    pub download_url: Option<String>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OfflineCollection {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub artwork_url: Option<String>,
    pub track_ids: Vec<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Index {
    pub tracks: HashMap<String, OfflineTrack>,
    pub collections: Vec<OfflineCollection>,
}

pub struct Offline {
    pub dir: PathBuf,
    pub index: Mutex<Index>,
    /// Laufende Downloads (Song-IDs)
    pub pending: Mutex<HashSet<String>>,
    limit: Arc<tokio::sync::Semaphore>,
}

impl OfflineTrack {
    pub fn track(&self) -> Track {
        Track {
            source: self.source.clone().unwrap_or_else(|| "youtube".into()),
            id: self.id.clone(),
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            duration: self.duration,
            artwork_url: self.artwork_url.clone(),
            url: self.url.clone(),
            ..Default::default()
        }
    }
}

impl Offline {
    pub fn new(paths: &crate::host::Paths) -> Self {
        let dir = paths.data.join("offline");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("index.json");
        if !file.exists() {
            // Verzeichnis der bisherigen Wavely übernehmen (Pfade zeigen auf deren Dateien)
            if let Some(old) = std::env::var_os("APPDATA").map(PathBuf::from) {
                let _ = std::fs::copy(old.join("com.wavely.app").join("offline").join("index.json"), &file);
            }
        }
        let mut index: Index = std::fs::read_to_string(&file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        // Abgebrochene Downloads verwerfen
        index.tracks.retain(|_, t| !t.path.is_empty() && std::path::Path::new(&t.path).exists());
        let known: HashSet<String> = index.tracks.keys().cloned().collect();
        for c in &mut index.collections {
            c.track_ids.retain(|id| known.contains(id));
        }
        Self { dir, index: Mutex::new(index), pending: Default::default(), limit: Arc::new(tokio::sync::Semaphore::new(PARALLEL_DOWNLOADS)) }
    }

    fn save(&self, index: &Index) {
        if let Ok(json) = serde_json::to_string_pretty(index) {
            let _ = std::fs::write(self.dir.join("index.json"), json);
        }
    }

    /// Gespeicherte Datei eines Songs
    pub fn path_of(&self, id: &str) -> Option<String> {
        self.index.lock().unwrap().tracks.get(id).map(|t| t.path.clone()).filter(|p| !p.is_empty())
    }

    pub fn is_saved_collection(&self, id: &str) -> bool {
        self.index.lock().unwrap().collections.iter().any(|c| c.id == id)
    }

    /// Löscht Songs, die weder einzeln gespeichert noch Teil einer gespeicherten Playlist sind
    fn cleanup(&self, index: &mut Index) {
        let referenced: HashSet<String> = index.collections.iter().flat_map(|c| c.track_ids.clone()).collect();
        let orphaned: Vec<String> =
            index.tracks.values().filter(|t| !t.pinned && !referenced.contains(&t.id)).map(|t| t.id.clone()).collect();
        for id in orphaned {
            if let Some(track) = index.tracks.remove(&id) {
                let shared = index.tracks.values().any(|t| t.path == track.path);
                if !track.path.is_empty() && !shared {
                    let _ = std::fs::remove_file(&track.path);
                }
            }
        }
    }

    pub fn remove(&self, collection: Option<&str>, track: Option<&str>) {
        let mut index = self.index.lock().unwrap();
        if let Some(id) = collection {
            index.collections.retain(|c| c.id != id);
        }
        if let Some(id) = track {
            if let Some(t) = index.tracks.get_mut(id) {
                t.pinned = false;
            }
            for c in &mut index.collections {
                c.track_ids.retain(|t| t != id);
            }
        }
        self.cleanup(&mut index);
        self.save(&index);
    }

    pub fn clear(&self) {
        let mut index = self.index.lock().unwrap();
        for t in index.tracks.values() {
            if !t.path.is_empty() && t.path.starts_with(&*self.dir.to_string_lossy()) {
                let _ = std::fs::remove_file(&t.path);
            }
        }
        *index = Index::default();
        self.save(&index);
    }

    pub fn stats(&self) -> (usize, u64) {
        let index = self.index.lock().unwrap();
        let saved: Vec<&OfflineTrack> = index.tracks.values().filter(|t| !t.path.is_empty()).collect();
        (saved.len(), saved.iter().filter_map(|t| std::fs::metadata(&t.path).ok()).map(|m| m.len()).sum())
    }
}

pub fn page(app: &Arc<App>) -> PageData {
    let mut p = PageData::default();
    p.title("Offline");
    let (songs, bytes) = app.offline.stats();
    let pending = app.offline.pending.lock().unwrap().len();
    let mut info = format!("{songs} Songs · {:.0} MB", bytes as f64 / 1_048_576.0);
    if pending > 0 {
        info.push_str(&format!(" · {pending} werden gespeichert…"));
    }
    p.message(info);
    let index = app.offline.index.lock().unwrap();
    let mut tracks: Vec<OfflineTrack> = index.tracks.values().filter(|t| !t.path.is_empty()).cloned().collect();
    tracks.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    if tracks.is_empty() {
        p.message("Speichere Playlists, Alben oder einzelne Songs über „Offline speichern“ – dann laufen sie auch ohne Internet.");
        return p;
    }
    p.buttons(&[("play", "Abspielen", icon::PLAY), ("shuffle", "Zufall", icon::SHUFFLE), ("offline-folder", "Ordner öffnen", icon::FOLDER)]);
    if !index.collections.is_empty() {
        p.heading("Playlists & Alben");
        p.cards(
            index
                .collections
                .iter()
                .map(|c| CardData {
                    title: c.title.clone(),
                    subtitle: format!("{} Songs", c.track_ids.len()),
                    art: c.artwork_url.clone(),
                    round: false,
                    target: Target::Action(format!("offline-collection:{}", c.id)),
                })
                .collect(),
        );
    }
    p.heading("Alle Songs");
    p.tracks(tracks.iter().map(OfflineTrack::track));
    p.spacer();
    p
}

/// Seite einer gespeicherten Playlist
pub fn collection_page(app: &Arc<App>, id: &str) -> PageData {
    let mut p = PageData::default();
    let index = app.offline.index.lock().unwrap();
    let Some(c) = index.collections.iter().find(|c| c.id == id) else {
        p.title("Offline");
        p.message("Diese Playlist ist nicht mehr gespeichert.");
        return p;
    };
    p.hero("Offline gespeichert", &c.title, &format!("{} Songs", c.track_ids.len()), c.artwork_url.clone(), false);
    p.buttons(&[("play", "Abspielen", icon::PLAY), ("shuffle", "Zufall", icon::SHUFFLE), ("offline-remove-collection", "Offline entfernen", icon::TRASH)]);
    p.tracks(c.track_ids.iter().filter_map(|t| index.tracks.get(t)).map(OfflineTrack::track));
    p.spacer();
    p
}

impl App {
    /// Songs offline speichern; mit `collection` wird die Playlist/das Album als Ganzes gemerkt
    pub fn offline_save(self: &Arc<Self>, tracks: Vec<Track>, collection: Option<OfflineCollection>) {
        let mut to_download = Vec::new();
        {
            let mut index = self.offline.index.lock().unwrap();
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
                        source: Some(t.source.clone()),
                        // SoundCloud direkt über die Song-Adresse
                        download_url: (t.source == "soundcloud").then(|| t.url.clone()).flatten(),
                        ..Default::default()
                    }
                });
                if collection.is_none() {
                    entry.pinned = true;
                }
            }
            if let Some(mut c) = collection.clone() {
                c.track_ids = tracks.iter().map(|t| t.id.clone()).collect();
                index.collections.retain(|e| e.id != c.id);
                index.collections.push(c);
            }
            self.offline.save(&index);
        }
        let count = to_download.len();
        if count > 0 {
            self.toast(&format!("{count} {} werden offline gespeichert…", if count == 1 { "Song" } else { "Songs" }), false);
        }
        for id in to_download {
            self.offline.pending.lock().unwrap().insert(id.clone());
            let app = self.clone();
            self.rt.spawn(async move {
                let Ok(_permit) = app.offline.limit.clone().acquire_owned().await else { return };
                let track = app.offline.index.lock().unwrap().tracks.get(&id).cloned();
                let result = match track {
                    None => Err("Song wurde entfernt".to_string()),
                    Some(t) => app.offline_download(&t).await,
                };
                app.offline.pending.lock().unwrap().remove(&id);
                match result {
                    Ok(path) => {
                        let mut index = app.offline.index.lock().unwrap();
                        match index.tracks.get_mut(&id) {
                            Some(t) => t.path = path.to_string_lossy().into_owned(),
                            None => {
                                let _ = std::fs::remove_file(&path);
                            }
                        }
                        app.offline.save(&index);
                    }
                    Err(e) => {
                        let mut index = app.offline.index.lock().unwrap();
                        index.tracks.remove(&id);
                        for c in &mut index.collections {
                            c.track_ids.retain(|t| t != &id);
                        }
                        app.offline.save(&index);
                        drop(index);
                        app.toast(&format!("Speichern fehlgeschlagen: {e}"), true);
                    }
                }
                if app.offline.pending.lock().unwrap().is_empty() {
                    app.toast("Offline gespeichert", false);
                }
                let a = app.clone();
                let _ = slint::invoke_from_event_loop(move || a.refresh_if_showing(|p| matches!(p, PageKind::Offline | PageKind::OfflineCollection(_))));
            });
        }
    }

    async fn offline_download(self: &Arc<Self>, t: &OfflineTrack) -> Result<PathBuf, String> {
        let ytdlp = &self.ytm.ytdlp;
        if let Some(url) = &t.download_url {
            return ytdlp.download_url(url, &format!("sc-{}", t.id), &self.offline.dir).await;
        }
        let video = match (&t.download_id, t.source.as_deref()) {
            (Some(v), _) => v.clone(),
            (None, Some("spotify") | Some("soundcloud")) => {
                let video = self.resolve_video(&t.track()).await?;
                if let Some(e) = self.offline.index.lock().unwrap().tracks.get_mut(&t.id) {
                    e.download_id = Some(video.clone());
                }
                video
            }
            _ => t.id.clone(),
        };
        ytdlp.download(&video, &self.offline.dir).await
    }

    pub fn offline_action(self: &Arc<Self>, action: &str) -> bool {
        match action {
            "offline-folder" => {
                let _ = std::process::Command::new("explorer").arg(&self.offline.dir).spawn();
            }
            "offline-clear" => {
                self.offline.clear();
                self.toast("Alle Offline-Songs gelöscht", false);
                self.settings_changed();
            }
            "offline-remove-collection" => {
                if let Some(PageKind::OfflineCollection(id)) = self.current_page() {
                    self.offline.remove(Some(&id), None);
                    self.back();
                }
            }
            other => {
                if let Some(id) = other.strip_prefix("offline-collection:") {
                    self.open(PageKind::OfflineCollection(id.to_string()));
                } else {
                    return false;
                }
            }
        }
        true
    }

    /// Offline-Knopf auf YouTube-Album- und Playlistseiten
    pub fn offline_toggle_collection(self: &Arc<Self>, id: &str, kind: &str, title: &str, art: Option<String>, tracks: Vec<Track>) {
        if self.offline.is_saved_collection(id) {
            self.offline.remove(Some(id), None);
            self.toast("Offline-Songs entfernt", false);
        } else {
            self.offline_save(
                tracks,
                Some(OfflineCollection {
                    id: id.into(),
                    kind: kind.into(),
                    title: title.into(),
                    subtitle: None,
                    artwork_url: art,
                    track_ids: Vec::new(),
                }),
            );
        }
    }
}

#[allow(dead_code)]
fn _row(_: RowData) {}
