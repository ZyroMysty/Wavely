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
    /// Markierungsdatei: angemeldet (damit der Status auch ohne Internet bekannt ist)
    login_marker: std::path::PathBuf,
    ytdlp: YtDlp,
    http: reqwest::Client,
    streams: Mutex<HashMap<String, StreamInfo>>,
}

impl Ytm {
    pub fn new(app: &AppHandle, ytdlp: YtDlp) -> Result<Self, String> {
        let dir = app.path().app_data_dir().map_err(err)?.join("ytm");
        std::fs::create_dir_all(&dir).map_err(err)?;
        let rp = RustyPipe::builder()
            .storage_dir(&dir)
            .lang(Language::De)
            .country(Country::De)
            .build()
            .map_err(err)?;
        let login_marker = dir.join("logged_in");
        Ok(Self {
            login_marker,
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
    /// Einzelne Künstler mit Kanal-ID, damit man sie anklicken kann
    artists: Vec<YtArtistRef>,
    album: Option<String>,
    album_id: Option<String>,
    duration: f64,
    artwork_url: Option<String>,
    url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YtArtistRef {
    id: Option<String>,
    name: String,
}

impl From<TrackItem> for YtTrack {
    fn from(t: TrackItem) -> Self {
        let artist = t.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
        let mut artists: Vec<YtArtistRef> = t
            .artists
            .into_iter()
            .map(|a| YtArtistRef { id: a.id, name: a.name })
            .collect();
        // Hauptkünstler-ID nachtragen, falls YouTube sie nur separat liefert
        if let (Some(first), Some(id)) = (artists.first_mut(), t.artist_id) {
            first.id.get_or_insert(id);
        }
        Self {
            url: format!("https://music.youtube.com/watch?v={}", t.id),
            artwork_url: best_thumbnail(&t.cover),
            id: t.id,
            title: t.name,
            artist: (!artist.is_empty()).then_some(artist),
            artists,
            album_id: t.album.as_ref().map(|a| a.id.clone()),
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
    /// Eigene Playlist, zu der man Songs hinzufügen kann
    editable: bool,
}

impl From<MusicPlaylistItem> for YtCollection {
    fn from(p: MusicPlaylistItem) -> Self {
        // Eigene Playlists: normale Playlist-ID („PL…“, nicht z. B. Recaps „LR…“ oder Likes „LM“),
        // kein fremder Kanal und nicht von YouTube Music selbst erstellt
        let editable = p.id.starts_with("PL") && !p.from_ytm && p.channel.is_none();
        Self {
            editable,
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
            editable: false,
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
    if ytm.login_marker.exists() {
        return Ok(true);
    }
    // Ältere Anmeldungen ohne Markierung einmal online prüfen
    let logged_in = ytm.rp.user_auth_check_cookie().await.is_ok();
    if logged_in {
        let _ = std::fs::write(&ytm.login_marker, b"");
    }
    Ok(logged_in)
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
                    let ytm = app.state::<Ytm>();
                    let result = ytm.rp.user_auth_set_cookie(cookie).await;
                    if result.is_ok() {
                        let _ = std::fs::write(&ytm.login_marker, b"");
                    }
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
    let _ = std::fs::remove_file(&ytm.login_marker);
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
        let album_id = album.id.clone();
        return Ok(YtCollectionDetail {
            info: YtCollection {
                id: album.id,
                kind: "album",
                title: album.name,
                subtitle: (!artist.is_empty()).then_some(artist),
                artwork_url: best_thumbnail(&album.cover),
                editable: false,
            },
            // Album-Tracks haben oft kein Cover/Album gesetzt – vom Album übernehmen
            tracks: album
                .tracks
                .into_iter()
                .map(|t| {
                    let mut track = YtTrack::from(t);
                    track.album.get_or_insert_with(|| album_name.clone());
                    track.album_id.get_or_insert_with(|| album_id.clone());
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
            // Wird in der Oberfläche aus der Bibliothek übernommen
            editable: false,
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

// --- Konto & Bearbeiten (inoffizielle YouTube-Music-Schnittstellen) ------

/// Angemeldete Anfrage an YouTube Music; liefert die Antwort als JSON.
async fn yt_request(ytm: &Ytm, endpoint: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
    let raw = ytm
        .rp
        .query()
        .authenticated()
        .raw(rustypipe::client::ClientType::DesktopMusic, endpoint, &body)
        .await
        .map_err(err)?;
    serde_json::from_str(&raw).map_err(err)
}

fn text_of(value: &serde_json::Value) -> Option<String> {
    value["runs"]
        .as_array()
        .map(|runs| runs.iter().filter_map(|r| r["text"].as_str()).collect::<String>())
        .or_else(|| value["simpleText"].as_str().map(str::to_string))
        .filter(|t| !t.is_empty())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YtAccount {
    name: Option<String>,
    handle: Option<String>,
    photo_url: Option<String>,
}

#[tauri::command]
pub async fn ytm_account(ytm: State<'_, Ytm>) -> Result<YtAccount, String> {
    let response = yt_request(&ytm, "account/account_menu", serde_json::json!({})).await?;
    let header = &response["actions"][0]["openPopupAction"]["popup"]["multiPageMenuRenderer"]["header"]
        ["activeAccountHeaderRenderer"];
    let photo_url = header["accountPhoto"]["thumbnails"]
        .as_array()
        .and_then(|thumbs| thumbs.iter().max_by_key(|t| t["width"].as_u64().unwrap_or(0)))
        .and_then(|t| t["url"].as_str())
        .map(str::to_string);
    Ok(YtAccount {
        name: text_of(&header["accountName"]),
        handle: text_of(&header["channelHandle"]),
        photo_url,
    })
}

/// IDs aller gelikten Songs (für das Herz-Symbol)
#[tauri::command]
pub async fn ytm_liked_ids(ytm: State<'_, Ytm>) -> Result<Vec<String>, String> {
    let query = ytm.rp.query();
    let mut liked = query.music_liked_tracks().await.map_err(err)?;
    liked.tracks.extend_limit(&query, 10_000).await.map_err(err)?;
    Ok(liked.tracks.items.into_iter().map(|t| t.id).collect())
}

#[tauri::command]
pub async fn ytm_set_like(ytm: State<'_, Ytm>, id: String, liked: bool) -> Result<(), String> {
    let endpoint = if liked { "like/like" } else { "like/removelike" };
    yt_request(&ytm, endpoint, serde_json::json!({ "target": { "videoId": id } })).await?;
    Ok(())
}

/// Legt eine neue Playlist an (optional gleich mit Songs) und gibt ihre ID zurück.
#[tauri::command]
pub async fn ytm_create_playlist(
    ytm: State<'_, Ytm>,
    title: String,
    privacy: String,
    video_ids: Vec<String>,
) -> Result<String, String> {
    let privacy = match privacy.as_str() {
        "PUBLIC" | "UNLISTED" => privacy,
        _ => "PRIVATE".to_string(),
    };
    let mut body = serde_json::json!({ "title": title, "description": "", "privacyStatus": privacy });
    if !video_ids.is_empty() {
        body["videoIds"] = serde_json::json!(video_ids);
    }
    let response = yt_request(&ytm, "playlist/create", body).await?;
    response["playlistId"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "YouTube hat keine Playlist angelegt".into())
}

#[tauri::command]
pub async fn ytm_add_to_playlist(
    ytm: State<'_, Ytm>,
    playlist_id: String,
    video_ids: Vec<String>,
) -> Result<(), String> {
    let actions: Vec<_> = video_ids
        .iter()
        .map(|id| serde_json::json!({ "action": "ACTION_ADD_VIDEO", "addedVideoId": id, "dedupeOption": "DEDUPE_OPTION_SKIP" }))
        .collect();
    let playlist_id = playlist_id.strip_prefix("VL").unwrap_or(&playlist_id).to_string();
    let response = yt_request(
        &ytm,
        "browse/edit_playlist",
        serde_json::json!({ "playlistId": playlist_id, "actions": actions }),
    )
    .await?;
    match response["status"].as_str() {
        Some("STATUS_SUCCEEDED") => Ok(()),
        status => Err(format!("YouTube hat das Hinzufügen abgelehnt ({})", status.unwrap_or("unbekannt"))),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YtHistoryEntry {
    track: YtTrack,
    /// Gruppierung von YouTube, z. B. „Heute“, „Gestern“, „Diese Woche“
    group: Option<String>,
}

/// Verlauf von YouTube Music (zuletzt gehörte Songs im Konto)
#[tauri::command]
pub async fn ytm_history(ytm: State<'_, Ytm>) -> Result<Vec<YtHistoryEntry>, String> {
    let query = ytm.rp.query();
    let mut history = query.music_history().await.map_err(err)?;
    history.extend_limit(&query, 200).await.map_err(err)?;
    Ok(history
        .items
        .into_iter()
        .map(|h| YtHistoryEntry {
            group: h.playback_date_txt,
            track: h.item.into(),
        })
        .collect())
}

/// Meldet einen gehörten Song an YouTube, damit er im YouTube-Music-Verlauf erscheint.
/// So macht es auch die YouTube-Music-Webseite: beim Abspielen wird die „videostatsPlaybackUrl“
/// aus der Player-Antwort mit dem Login aufgerufen.
#[tauri::command]
pub async fn ytm_report_play(ytm: State<'_, Ytm>, id: String) -> Result<(), String> {
    use rand::Rng;
    use sha1::{Digest, Sha1};

    // Zufällige Wiedergabe-ID (cpn), 16 Zeichen – vor dem ersten await erzeugen (Zufallsgenerator ist nicht Send)
    let cpn: String = {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut rng = rand::rng();
        (0..16).map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char).collect()
    };

    let player = yt_request(&ytm, "player", serde_json::json!({ "videoId": id })).await?;
    let base = player["playbackTracking"]["videostatsPlaybackUrl"]["baseUrl"]
        .as_str()
        .ok_or("YouTube hat keinen Verlaufs-Link geliefert")?;
    let (cookie, session_index) = ytm.rp.user_auth_cookie_header().ok_or("Nicht angemeldet")?;

    // Anmelde-Header wie im Browser: SAPISIDHASH <Zeit>_<SHA1(Zeit SAPISID Origin)>
    const ORIGIN: &str = "https://music.youtube.com";
    let sapisid = cookie
        .split(';')
        .map(str::trim)
        .find_map(|c| c.strip_prefix("SAPISID=").or_else(|| c.strip_prefix("__Secure-3PAPISID=")))
        .ok_or("Login-Cookie unvollständig")?;
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(err)?
        .as_secs();
    let hash = Sha1::digest(format!("{time} {sapisid} {ORIGIN}").as_bytes());
    let hash: String = hash.iter().map(|b| format!("{b:02x}")).collect();

    let response = ytm
        .http
        .get(format!("{base}&ver=2&c=WEB_REMIX&cpn={cpn}"))
        .header("Cookie", cookie.as_str())
        .header("Authorization", format!("SAPISIDHASH {time}_{hash}"))
        .header("Origin", ORIGIN)
        .header("X-Origin", ORIGIN)
        .header("X-Goog-AuthUser", session_index.as_deref().unwrap_or("0"))
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
        )
        .send()
        .await
        .map_err(err)?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("YouTube antwortete mit {}", response.status()))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YtArtistCard {
    id: String,
    name: String,
    image_url: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YtArtist {
    id: String,
    name: String,
    image_url: Option<String>,
    description: Option<String>,
    subscribers: Option<u64>,
    /// Beliebte Songs
    tracks: Vec<YtTrack>,
    albums: Vec<YtCollection>,
    singles: Vec<YtCollection>,
    playlists: Vec<YtCollection>,
    similar: Vec<YtArtistCard>,
    /// Playlist mit allen Songs des Künstlers
    all_songs_playlist_id: Option<String>,
}

/// Künstlerseite: beliebte Songs, Alben, Singles, Playlists und ähnliche Künstler
#[tauri::command]
pub async fn ytm_artist(ytm: State<'_, Ytm>, id: String) -> Result<YtArtist, String> {
    let artist = ytm.rp.query().music_artist(&id, false).await.map_err(err)?;
    let (albums, singles): (Vec<_>, Vec<_>) = artist
        .albums
        .into_iter()
        .partition(|a| matches!(a.album_type, rustypipe::model::AlbumType::Album));
    Ok(YtArtist {
        image_url: best_thumbnail(&artist.header_image),
        id: artist.id,
        name: artist.name,
        description: artist.description,
        subscribers: artist.subscriber_count,
        tracks: artist.tracks.into_iter().map(Into::into).collect(),
        albums: albums.into_iter().map(Into::into).collect(),
        singles: singles.into_iter().map(Into::into).collect(),
        playlists: artist.playlists.into_iter().map(Into::into).collect(),
        similar: artist
            .similar_artists
            .into_iter()
            .map(|a| YtArtistCard {
                image_url: best_thumbnail(&a.avatar),
                id: a.id,
                name: a.name,
            })
            .collect(),
        all_songs_playlist_id: artist.tracks_playlist_id,
    })
}
