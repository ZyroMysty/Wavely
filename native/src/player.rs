//! Warteschlange und Wiedergabesteuerung (entspricht player.svelte.js).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    audio::{
        source::{FetchChunk, HttpSource},
        Command, Generation,
    },
    services::ytm::Ytm,
};

/// Song aus einer beliebigen Quelle (gleiches JSON-Format wie in der bisherigen Wavely)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Track {
    /// "youtube" | "local" | "soundcloud" | "spotify"
    pub source: String,
    #[serde(deserialize_with = "string_or_number")]
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    /// Einzelne Künstler mit Kanal-ID (anklickbar)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<ArtistRef>,
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_id: Option<String>,
    pub duration: f64,
    pub artwork_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Lokale Datei
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// SoundCloud: nur 30-Sekunden-Ausschnitt
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub snipped: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ArtistRef {
    pub id: Option<String>,
    pub name: String,
}

/// SoundCloud-IDs sind in alten Daten Zahlen
fn string_or_number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => s,
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    })
}

impl From<crate::services::ytm::YtTrack> for Track {
    fn from(t: crate::services::ytm::YtTrack) -> Self {
        Self {
            source: "youtube".into(),
            id: t.id,
            title: t.title,
            artist: t.artist,
            artists: t.artists.into_iter().map(|a| ArtistRef { id: a.id, name: a.name }).collect(),
            album: t.album,
            album_id: t.album_id,
            duration: t.duration,
            artwork_url: t.artwork_url,
            url: Some(t.url),
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

#[derive(Default)]
pub struct State {
    pub queue: Vec<Track>,
    pub index: Option<usize>,
    pub playing: bool,
    pub loading: bool,
    pub position: f64,
    pub duration: f64,
    pub volume: f32,
    pub muted: bool,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub error: Option<String>,
    /// Ereignisse der Audio-Engine gehören zu diesem Ladevorgang
    pub generation: Generation,
    /// Ursprüngliche Reihenfolge (für das Aufheben der Zufallswiedergabe)
    pub original: Vec<Track>,
    /// Fehler hintereinander
    pub error_streak: u32,
    /// Ab hier stehen Autoplay-Vorschläge in der Warteschlange
    pub auto_from: Option<usize>,
}

impl State {
    pub fn current(&self) -> Option<&Track> {
        self.index.and_then(|i| self.queue.get(i))
    }
}

/// Die Kubik-Kurve macht den Regler im leisen Bereich feiner (wie bisher)
pub fn gain(volume: f32, muted: bool) -> f32 {
    if muted {
        0.0
    } else {
        volume.powi(3)
    }
}

/// Startet das Laden eines Songs im Hintergrund. Liefert die Generation über `engine.load`.
/// Quelle: eigene Datei (lokal oder offline gespeichert), SoundCloud direkt, sonst YouTube-Stream
/// (Spotify- und SoundCloud-Songs über den passenden YouTube-Song).
pub fn start_load(
    app: &Arc<crate::App>,
    track: Track,
    start_at: f64,
    on_generation: impl FnOnce(Result<Generation, String>) + Send + 'static,
) {
    let engine = app.engine.clone();
    let ytm: Arc<Ytm> = app.ytm.clone();
    engine.send(Command::Stop);
    let handle = app.rt.clone();
    let lufs = app.loudness.lock().unwrap().get(&format!("{}:{}", track.source, track.id)).copied();
    let app = app.clone();
    handle.clone().spawn(async move {
        let result = async {
            let file_path = track
                .path
                .clone()
                .filter(|p| std::path::Path::new(p).exists())
                .or_else(|| app.offline.path_of(&track.id));
            if let Some(path) = &file_path {
                let file = std::fs::File::open(path).map_err(|e| format!("Datei nicht lesbar: {e}"))?;
                let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).map(str::to_lowercase);
                return Ok(engine.load(Box::new(file), ext, start_at, lufs));
            }
            if let Some(source) = app.soundcloud_source(&track).await {
                return Ok(engine.load(source.0, Some(source.1), start_at, lufs));
            }
            let id = app.resolve_video(&track).await?;
            let hint = ytm.stream_hint(&id).await.map(str::to_string);
            let fetch_ytm = ytm.clone();
            let fetch: FetchChunk = Arc::new(move |start, len| {
                let ytm = fetch_ytm.clone();
                let id = id.clone();
                handle.block_on(async move { ytm.fetch(&id, start, len).await })
            });
            // Das erste Stück blockierend holen – nicht im Tokio-Thread
            let source = tokio::task::spawn_blocking(move || HttpSource::open(fetch)).await.map_err(|e| e.to_string())??;
            Ok(engine.load(Box::new(source), hint, start_at, lufs))
        }
        .await;
        on_generation(result);
    });
}

pub fn shuffled<T>(mut list: Vec<T>) -> Vec<T> {
    use rand::seq::SliceRandom;
    list.shuffle(&mut rand::rng());
    list
}
