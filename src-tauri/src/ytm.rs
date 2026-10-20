//! YouTube Music: Anmeldung, Bibliothek und Suche über RustyPipe,
//! Audio-Streams über yt-dlp (siehe ytdlp.rs).

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use rustypipe::{
    client::RustyPipe,
    model::{AlbumItem, MusicPlaylistItem, Thumbnail, TrackItem},
    param::{Country, Language},
};
use serde::Serialize;
use tauri::{
    http::{header, Response, StatusCode},
    webview::PageLoadEvent,
    AppHandle, Emitter, Manager, State, Url, WebviewUrl, WebviewWindowBuilder,
};
use tokio::sync::Mutex;

use crate::ytdlp::YtDlp;

const LOGIN_WINDOW: &str = "ytm-login";
const LOGIN_URL: &str = "https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fmusic.youtube.com%2F";
/// Sonder-ID für „Lieblingssongs“
const LIKED_ID: &str = "LM";
/// Obergrenze beim Nachladen langer Playlists
const MAX_TRACKS: usize = 2000;
/// Größe der Stücke, in denen die Audiodatei an den Player geht
const CHUNK_SIZE: u64 = 1024 * 1024;
/// Stream-URLs sind ca. 6 Stunden gültig; etwas früher erneuern
const STREAM_MAX_AGE: Duration = Duration::from_secs(4 * 60 * 60);

#[derive(Clone)]
struct StreamInfo {
    url: String,
    size: u64,
    mime: String,
    /// Header, die YouTube für diese URL erwartet (v. a. User-Agent)
    headers: Vec<(String, String)>,
    fetched: Instant,
}

pub struct Ytm {
    rp: RustyPipe,
    ytdlp: YtDlp,
    http: reqwest::Client,
    streams: Mutex<HashMap<String, StreamInfo>>,
}

impl Ytm {
    pub fn new(app: &AppHandle, ytdlp: YtDlp) -> Result<Self, String> {
        let dir = app.path().app_data_dir().map_err(err)?.join("ytm");
        std::fs::create_dir_all(&dir).map_err(err)?;
        let rp = RustyPipe::builder()
            .storage_dir(dir)
            .lang(Language::De)
            .country(Country::De)
            .build()
            .map_err(err)?;
        Ok(Self {
            rp,
            ytdlp,
            http: reqwest::Client::new(),
            streams: Mutex::new(HashMap::new()),
        })
    }

    async fn stream_info(&self, id: &str, refresh: bool) -> Result<StreamInfo, String> {
        if !refresh {
            if let Some(info) = self.streams.lock().await.get(id) {
                if info.fetched.elapsed() < STREAM_MAX_AGE {
                    return Ok(info.clone());
                }
            }
        }
        // Sperre nicht während yt-dlp läuft halten (dauert ein paar Sekunden)
        let audio = self.ytdlp.audio(id).await?;
        let info = StreamInfo {
            url: audio.url,
            size: audio.size,
            mime: audio.mime,
            headers: audio.headers,
            fetched: Instant::now(),
        };
        self.streams.lock().await.insert(id.to_string(), info.clone());
        Ok(info)
    }

    pub fn rp(&self) -> &RustyPipe {
        &self.rp
    }

    pub async fn update_tools(&self) {
        self.ytdlp.update_if_due().await;
    }

    /// Holt die Stream-Adresse schon vorab, damit der nächste Song sofort startet.
    pub async fn prefetch(&self, id: &str) {
        let _ = self.stream_info(id, false).await;
    }

    /// Beantwortet eine Anfrage des Players an `ytstream://<videoId>` mit einem Stück der Audiodatei.
    /// Der Player fordert die restlichen Stücke dann selbst nach.
    pub async fn serve_stream(&self, id: &str, range: Option<&str>) -> Response<Vec<u8>> {
        match self.fetch_chunk(id, range).await {
            Ok(response) => response,
            Err(message) => Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(message.into_bytes())
                .unwrap(),
        }
    }

    async fn fetch_chunk(&self, id: &str, range: Option<&str>) -> Result<Response<Vec<u8>>, String> {
        let (start, requested_end) = parse_range(range);

        // Beim zweiten Versuch eine frische URL holen (abgelaufen oder gesperrt)
        for refresh in [false, true] {
            let info = self.stream_info(id, refresh).await?;
            if info.size > 0 && start >= info.size {
                return Ok(Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{}", info.size))
                    .body(Vec::new())
                    .unwrap());
            }

            let mut end = start + CHUNK_SIZE - 1;
            if let Some(requested) = requested_end {
                end = end.min(requested);
            }
            if info.size > 0 {
                end = end.min(info.size - 1);
            }

            let mut request = self.http.get(&info.url);
            for (name, value) in &info.headers {
                request = request.header(name, value);
            }
            let response = request
                .header(header::RANGE, format!("bytes={start}-{end}"))
                .send()
                .await
                .map_err(err)?;
            let status = response.status();
            if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::GONE {
                continue;
            }
            if !status.is_success() {
                return Err(format!("YouTube antwortete mit {status}"));
            }

            let bytes = response.bytes().await.map_err(err)?;
            let end = start + (bytes.len() as u64).saturating_sub(1);
            let total = if info.size > 0 { info.size.to_string() } else { "*".into() };
            return Ok(Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, info.mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{total}"))
                .header(header::CONTENT_LENGTH, bytes.len())
                .body(bytes.to_vec())
                .unwrap());
        }
        Err("Stream ist nicht verfügbar".into())
    }
}

/// Liest `bytes=START-END` aus dem Range-Header (END optional).
fn parse_range(range: Option<&str>) -> (u64, Option<u64>) {
    let Some(spec) = range.and_then(|r| r.strip_prefix("bytes=")) else {
        return (0, None);
    };
    let (start, end) = spec.split_once('-').unwrap_or((spec, ""));
    (start.trim().parse().unwrap_or(0), end.trim().parse().ok())
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn best_thumbnail(thumbs: &[Thumbnail]) -> Option<String> {
    thumbs.iter().max_by_key(|t| t.width).map(|t| t.url.clone())
}

// --- Datentypen für die Oberfläche ---------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YtTrack {
    id: String,
    title: String,
    artist: Option<String>,
    album: Option<String>,
    duration: f64,
    artwork_url: Option<String>,
    url: String,
}

impl From<TrackItem> for YtTrack {
    fn from(t: TrackItem) -> Self {
        let artist = t.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
        Self {
            url: format!("https://music.youtube.com/watch?v={}", t.id),
            artwork_url: best_thumbnail(&t.cover),
            id: t.id,
            title: t.name,
            artist: (!artist.is_empty()).then_some(artist),
            album: t.album.map(|a| a.name),
            duration: t.duration.unwrap_or(0) as f64,
        }
    }
}

/// Playlist oder Album als Kachel
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YtCollection {
    id: String,
    kind: &'static str,
    title: String,
    subtitle: Option<String>,
    artwork_url: Option<String>,
}

impl From<MusicPlaylistItem> for YtCollection {
    fn from(p: MusicPlaylistItem) -> Self {
        Self {
            subtitle: p.channel.map(|c| c.name),
            artwork_url: best_thumbnail(&p.thumbnail),
            id: p.id,
            kind: "playlist",
            title: p.name,
        }
    }
}

impl From<AlbumItem> for YtCollection {
    fn from(a: AlbumItem) -> Self {
        let artist = a.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
        let subtitle = match a.year {
            Some(year) if !artist.is_empty() => Some(format!("{artist} · {year}")),
            Some(year) => Some(year.to_string()),
            None => (!artist.is_empty()).then_some(artist),
        };
        Self {
            artwork_url: best_thumbnail(&a.cover),
            id: a.id,
            kind: "album",
            title: a.name,
            subtitle,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YtCollectionDetail {
    #[serde(flatten)]
    info: YtCollection,
    tracks: Vec<YtTrack>,
}

#[derive(Serialize)]
pub struct YtLibrary {
    playlists: Vec<YtCollection>,
    albums: Vec<YtCollection>,
}

#[derive(Serialize)]
pub struct YtSearch {
    tracks: Vec<YtTrack>,
    albums: Vec<YtCollection>,
    playlists: Vec<YtCollection>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthEvent {
    logged_in: bool,
    error: Option<String>,
}

// --- Anmeldung -----------------------------------------------------------

#[tauri::command]
pub async fn ytm_status(ytm: State<'_, Ytm>) -> Result<bool, String> {
    Ok(ytm.rp.user_auth_check_cookie().await.is_ok())
}

/// Öffnet ein Fenster mit der Google-Anmeldung. Sobald music.youtube.com angemeldet geladen ist,
/// werden die Cookies übernommen und das Fenster geschlossen. Ergebnis kommt als Event `ytm-auth`.
#[tauri::command]
pub async fn ytm_login(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LOGIN_WINDOW) {
        let _ = window.set_focus();
        return Ok(());
    }

    // Eigenes, frisches Browserprofil nur für die Anmeldung
    let data_dir = app.path().app_data_dir().map_err(err)?.join("login-webview");
    let _ = std::fs::remove_dir_all(&data_dir);

    let done = Arc::new(AtomicBool::new(false));
    let handle = app.clone();
    WebviewWindowBuilder::new(&app, LOGIN_WINDOW, WebviewUrl::External(LOGIN_URL.parse().map_err(err)?))
        .title("Bei YouTube Music anmelden")
        .inner_size(500.0, 720.0)
        .data_directory(data_dir)
        .on_page_load(move |window, payload| {
            if payload.event() != PageLoadEvent::Finished
                || payload.url().host_str() != Some("music.youtube.com")
                || done.load(Ordering::SeqCst)
            {
                return;
            }
            let app = handle.clone();
            let done = done.clone();
            // Cookies dürfen unter Windows nicht im Event-Handler gelesen werden (Deadlock)
            std::thread::spawn(move || {
                let url: Url = "https://music.youtube.com".parse().unwrap();
                let Ok(cookies) = window.cookies_for_url(url) else { return };
                if !cookies.iter().any(|c| c.name() == "SAPISID") {
                    return; // Noch nicht angemeldet
                }
                if done.swap(true, Ordering::SeqCst) {
                    return;
                }
                let cookie = cookies
                    .iter()
                    .map(|c| format!("{}={}", c.name(), c.value()))
                    .collect::<Vec<_>>()
                    .join("; ");
                let _ = window.close();

                tauri::async_runtime::spawn(async move {
                    let result = app.state::<Ytm>().rp.user_auth_set_cookie(cookie).await;
                    let _ = app.emit(
                        "ytm-auth",
                        AuthEvent {
                            logged_in: result.is_ok(),
                            error: result.err().map(err),
                        },
                    );
                });
            });
        })
        .build()
        .map_err(err)?;
    Ok(())
}

#[tauri::command]
pub async fn ytm_logout(ytm: State<'_, Ytm>) -> Result<(), String> {
    let _ = ytm.rp.user_auth_remove_cookie().await;
    Ok(())
}

// --- Bibliothek & Suche --------------------------------------------------

#[tauri::command]
pub async fn ytm_library(ytm: State<'_, Ytm>) -> Result<YtLibrary, String> {
    let query = ytm.rp.query();
    let (playlists, albums) = tokio::join!(
        async {
            let mut p = query.music_saved_playlists().await?;
            p.extend_limit(&query, 500).await?;
            Ok::<_, rustypipe::error::Error>(p.items)
        },
        async {
            let mut p = query.music_saved_albums().await?;
            p.extend_limit(&query, 500).await?;
            Ok::<_, rustypipe::error::Error>(p.items)
        },
    );
    Ok(YtLibrary {
        playlists: playlists.map_err(err)?.into_iter().map(Into::into).collect(),
        albums: albums.map_err(err)?.into_iter().map(Into::into).collect(),
    })
}

#[tauri::command]
pub async fn ytm_collection(
    ytm: State<'_, Ytm>,
    kind: String,
    id: String,
) -> Result<YtCollectionDetail, String> {
    let query = ytm.rp.query();

    if kind == "album" {
        let album = query.music_album(&id).await.map_err(err)?;
        let artist = album.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
        let album_name = album.name.clone();
        return Ok(YtCollectionDetail {
            info: YtCollection {
                id: album.id,
                kind: "album",
                title: album.name,
                subtitle: (!artist.is_empty()).then_some(artist),
                artwork_url: best_thumbnail(&album.cover),
            },
            // Album-Tracks haben oft kein Cover/Album gesetzt – vom Album übernehmen
            tracks: album
                .tracks
                .into_iter()
                .map(|t| {
                    let mut track = YtTrack::from(t);
                    track.album.get_or_insert_with(|| album_name.clone());
                    if track.artwork_url.is_none() {
                        track.artwork_url = best_thumbnail(&album.cover);
                    }
                    track
                })
                .collect(),
        });
    }

    // Öffentliche Playlists gehen ohne Anmeldung; private findet YouTube nur mit Anmeldung
    let (query, playlist) = if id == LIKED_ID {
        let playlist = query.music_liked_tracks().await;
        (query, playlist)
    } else {
        match query.music_playlist(&id).await {
            Ok(playlist) => (query, Ok(playlist)),
            Err(_) => {
                let query = ytm.rp.query().authenticated();
                let playlist = query.music_playlist(&id).await;
                (query, playlist)
            }
        }
    };
    let mut playlist = playlist.map_err(err)?;
    playlist.tracks.extend_limit(&query, MAX_TRACKS).await.map_err(err)?;

    Ok(YtCollectionDetail {
        info: YtCollection {
            id: playlist.id,
            kind: "playlist",
            title: playlist.name,
            subtitle: playlist.channel.map(|c| c.name),
            artwork_url: best_thumbnail(&playlist.thumbnail),
        },
        tracks: playlist.tracks.items.into_iter().map(Into::into).collect(),
    })
}

#[tauri::command]
pub async fn ytm_search(ytm: State<'_, Ytm>, query: String) -> Result<YtSearch, String> {
    let q = ytm.rp.query();
    let (tracks, albums, playlists) = tokio::join!(
        q.music_search_tracks(&query),
        q.music_search_albums(&query),
        q.music_search_playlists(&query, false),
    );
    Ok(YtSearch {
        tracks: tracks.map_err(err)?.items.items.into_iter().map(Into::into).collect(),
        albums: albums.map(|r| r.items.items.into_iter().map(Into::into).collect()).unwrap_or_default(),
        playlists: playlists.map(|r| r.items.items.into_iter().map(Into::into).collect()).unwrap_or_default(),
    })
}

#[tauri::command]
pub async fn ytm_prefetch(ytm: State<'_, Ytm>, id: String) -> Result<(), String> {
    ytm.prefetch(&id).await;
    Ok(())
}

/// „Radio“ zu einem Song: ähnliche Titel, die YouTube Music danach spielen würde.
#[tauri::command]
pub async fn ytm_radio(ytm: State<'_, Ytm>, id: String) -> Result<Vec<YtTrack>, String> {
    let radio = ytm.rp.query().music_radio_track(&id).await.map_err(err)?;
    Ok(radio
        .items
        .into_iter()
        .filter(|t| t.id != id)
        .map(Into::into)
        .collect())
}
