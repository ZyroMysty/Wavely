//! Warteschlange und Wiedergabesteuerung (entspricht player.svelte.js).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    audio::{
        source::{FetchChunk, HttpSource},
        Command, Engine, Generation,
    },
    services::ytm::Ytm,
};

/// Song aus einer beliebigen Quelle
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// "youtube" | "local" | "soundcloud" | "spotify"
    pub source: String,
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: f64,
    pub artwork_url: Option<String>,
    /// Lokale Datei
    pub path: Option<String>,
}

impl From<crate::services::ytm::YtTrack> for Track {
    fn from(t: crate::services::ytm::YtTrack) -> Self {
        Self {
            source: "youtube".into(),
            id: t.id,
            title: t.title,
            artist: t.artist,
            album: t.album,
            duration: t.duration,
            artwork_url: t.artwork_url,
            path: None,
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
pub fn start_load(
    rt: &tokio::runtime::Handle,
    engine: Arc<Engine>,
    ytm: Arc<Ytm>,
    track: Track,
    start_at: f64,
    on_generation: impl FnOnce(Result<Generation, String>) + Send + 'static,
) {
    engine.send(Command::Stop);
    let handle = rt.clone();
    rt.spawn(async move {
        let result = async {
            if let Some(path) = &track.path {
                let file = std::fs::File::open(path).map_err(|e| format!("Datei nicht lesbar: {e}"))?;
                let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).map(str::to_lowercase);
                return Ok(engine.load(Box::new(file), ext, start_at));
            }
            let id = track.id.clone();
            let hint = ytm.stream_hint(&id).await.map(str::to_string);
            let fetch_ytm = ytm.clone();
            let fetch: FetchChunk = Arc::new(move |start, len| {
                let ytm = fetch_ytm.clone();
                let id = id.clone();
                handle.block_on(async move { ytm.fetch(&id, start, len).await })
            });
            // Das erste Stück blockierend holen – nicht im Tokio-Thread
            let source = tokio::task::spawn_blocking(move || HttpSource::open(fetch)).await.map_err(|e| e.to_string())??;
            Ok(engine.load(Box::new(source), hint, start_at))
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
