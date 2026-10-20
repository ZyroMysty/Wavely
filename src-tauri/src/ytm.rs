//! YouTube Music: Anmeldung, Bibliothek, Suche und Audio-Streams über RustyPipe.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use rustypipe::{
    client::RustyPipe,
    model::{AlbumItem, MusicPlaylistItem, Thumbnail, TrackItem},
    param::{Country, Language, StreamFilter},
};
use serde::Serialize;
use tauri::{
    webview::PageLoadEvent, AppHandle, Emitter, Manager, State, Url, WebviewUrl,
    WebviewWindowBuilder,
};

const LOGIN_WINDOW: &str = "ytm-login";
const LOGIN_URL: &str = "https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fmusic.youtube.com%2F";
/// Sonder-ID für „Lieblingssongs“
const LIKED_ID: &str = "LM";
/// Obergrenze beim Nachladen langer Playlists
const MAX_TRACKS: usize = 2000;

pub struct Ytm {
    rp: RustyPipe,
}

impl Ytm {
    pub fn new(app: &AppHandle) -> Result<Self, String> {
        let dir = app.path().app_data_dir().map_err(err)?.join("ytm");
        std::fs::create_dir_all(&dir).map_err(err)?;
        let rp = RustyPipe::builder()
            .storage_dir(dir)
            .lang(Language::De)
            .country(Country::De)
            .build()
            .map_err(err)?;
        Ok(Self { rp })
    }
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

    let mut playlist = if id == LIKED_ID {
        query.music_liked_tracks().await
    } else {
        query.music_playlist(&id).await
    }
    .map_err(err)?;
    playlist.tracks.extend_limit(&query, MAX_TRACKS).await.map_err(err)?;

    Ok(YtCollectionDetail {
        info: YtCollection {
            id: playlist.id,
            kind: "playlist",
            title: if id == LIKED_ID { "Lieblingssongs".into() } else { playlist.name },
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

// --- Wiedergabe ----------------------------------------------------------

/// Liefert eine direkte Audio-URL. Die URLs laufen nach einigen Stunden ab,
/// deshalb wird sie erst beim Abspielen geholt.
#[tauri::command]
pub async fn ytm_stream(ytm: State<'_, Ytm>, id: String) -> Result<String, String> {
    let player = ytm.rp.query().player(&id).await.map_err(err)?;
    player
        .select_audio_stream(&StreamFilter::default())
        .map(|s| s.url.clone())
        .ok_or_else(|| "Kein Audio-Stream gefunden".into())
}
