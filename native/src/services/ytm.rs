//! YouTube Music: Anmeldung, Bibliothek und Suche über RustyPipe,
//! Audio-Streams über yt-dlp (siehe ytdlp.rs).

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use rustypipe::{
    client::RustyPipe,
    model::{AlbumItem, MusicPlaylistItem, Thumbnail, TrackItem},
    param::{Country, Language},
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::services::ytdlp::YtDlp;

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
pub struct StreamInfo {
    pub url: String,
    pub size: u64,
    pub mime: String,
    /// Header, die YouTube für diese URL erwartet (v. a. User-Agent)
    pub headers: Vec<(String, String)>,
    pub fetched: Instant,
}

pub struct Ytm {
    pub rp: RustyPipe,
    /// Markierungsdatei: angemeldet (damit der Status auch ohne Internet bekannt ist)
    pub login_marker: std::path::PathBuf,
    pub ytdlp: YtDlp,
    pub http: reqwest::Client,
    pub streams: Mutex<HashMap<String, StreamInfo>>,
    /// Signatur-Zeitstempel des aktuellen YouTube-Players (für die Verlaufs-Meldung)
    pub sts: Mutex<Option<(u64, Instant)>>,
}

impl Ytm {
    pub fn new(paths: &crate::host::Paths, ytdlp: YtDlp) -> Result<Self, String> {
        let dir = paths.data.join("ytm");
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
            sts: Mutex::new(None),
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

    /// „signatureTimestamp“ aus dem aktuellen Player-Skript von YouTube Music. Ohne diesen Wert lehnt
    /// YouTube angemeldete Player-Anfragen inzwischen ab („The page needs to be reloaded“).
    async fn signature_timestamp(&self) -> Result<u64, String> {
        if let Some((sts, fetched)) = *self.sts.lock().await {
            if fetched.elapsed() < Duration::from_secs(6 * 60 * 60) {
                return Ok(sts);
            }
        }
        const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
        let html = self.http.get("https://music.youtube.com/").header("User-Agent", UA).send().await.map_err(err)?.text().await.map_err(err)?;
        let js_path = html
            .split("\"jsUrl\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .ok_or("YouTube-Player nicht gefunden")?;
        let code = self
            .http
            .get(format!("https://music.youtube.com{js_path}"))
            .header("User-Agent", UA)
            .send()
            .await
            .map_err(err)?
            .text()
            .await
            .map_err(err)?;
        let sts = ["signatureTimestamp:", "signatureTimestamp="]
            .iter()
            .find_map(|marker| {
                let pos = code.find(marker)? + marker.len();
                code[pos..].chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u64>().ok()
            })
            .ok_or("Signatur-Zeitstempel nicht gefunden")?;
        *self.sts.lock().await = Some((sts, Instant::now()));
        Ok(sts)
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

    /// Lädt `len` Bytes der Audiodatei ab `start`. Ergebnis: Daten und Gesamtgröße.
    pub async fn fetch(&self, id: &str, start: u64, len: u64) -> Result<(Vec<u8>, u64), String> {
        // Beim zweiten Versuch eine frische URL holen (abgelaufen oder gesperrt), beim dritten
        // ohne Premium – YouTube sperrt angemeldete Stream-Adressen manchmal trotzdem
        for attempt in 0..3 {
            if attempt == 2 {
                self.ytdlp.mark_premium_failed();
            }
            let info = self.stream_info(id, attempt > 0).await?;
            if info.size > 0 && start >= info.size {
                return Ok((Vec::new(), info.size));
            }
            let mut end = start + len.min(CHUNK_SIZE) - 1;
            if info.size > 0 {
                end = end.min(info.size - 1);
            }
            let mut request = self.http.get(&info.url);
            for (name, value) in &info.headers {
                request = request.header(name, value);
            }
            let response = request
                .header(reqwest::header::RANGE, format!("bytes={start}-{end}"))
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
            let total = response
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.rsplit('/').next())
                .and_then(|v| v.parse().ok())
                .unwrap_or(info.size);
            let bytes = response.bytes().await.map_err(err)?;
            return Ok((bytes.to_vec(), total));
        }
        Err("Stream ist nicht verfügbar".into())
    }

    /// Dateiendung für den Decoder („webm“ oder „m4a“)
    pub async fn stream_hint(&self, id: &str) -> Option<&'static str> {
        let info = self.stream_info(id, false).await.ok()?;
        Some(if info.mime.contains("mp4") { "m4a" } else { "webm" })
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn best_thumbnail(thumbs: &[Thumbnail]) -> Option<String> {
    thumbs.iter().max_by_key(|t| t.width).map(|t| t.url.clone())
}

// --- Datentypen für die Oberfläche ---------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtTrack {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    /// Einzelne Künstler mit Kanal-ID, damit man sie anklicken kann
    pub artists: Vec<YtArtistRef>,
    pub album: Option<String>,
    pub album_id: Option<String>,
    pub duration: f64,
    pub artwork_url: Option<String>,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtArtistRef {
    pub id: Option<String>,
    pub name: String,
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtCollection {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub artwork_url: Option<String>,
    /// Eigene Playlist, zu der man Songs hinzufügen kann
    pub editable: bool,
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
            kind: "playlist".into(),
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
            kind: "album".into(),
            title: a.name,
            subtitle,
            editable: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtCollectionDetail {
    #[serde(flatten)]
    pub info: YtCollection,
    pub tracks: Vec<YtTrack>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct YtLibrary {
    pub playlists: Vec<YtCollection>,
    pub albums: Vec<YtCollection>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct YtSearch {
    pub tracks: Vec<YtTrack>,
    pub albums: Vec<YtCollection>,
    pub playlists: Vec<YtCollection>,
}

// --- Anmeldung -----------------------------------------------------------

pub async fn ytm_status(ytm: &Ytm) -> Result<bool, String> {
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

pub async fn ytm_logout(ytm: &Ytm) -> Result<(), String> {
    let _ = ytm.rp.user_auth_remove_cookie().await;
    let _ = std::fs::remove_file(&ytm.login_marker);
    Ok(())
}

// --- Bibliothek & Suche --------------------------------------------------

pub async fn ytm_library(ytm: &Ytm) -> Result<YtLibrary, String> {
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

pub async fn ytm_collection(
    ytm: &Ytm,
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
                kind: "album".into(),
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
            kind: "playlist".into(),
            title: playlist.name,
            subtitle: playlist.channel.map(|c| c.name),
            artwork_url: best_thumbnail(&playlist.thumbnail),
            // Wird in der Oberfläche aus der Bibliothek übernommen
            editable: false,
        },
        tracks: playlist.tracks.items.into_iter().map(Into::into).collect(),
    })
}

pub async fn ytm_search(ytm: &Ytm, query: String) -> Result<YtSearch, String> {
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

pub async fn ytm_prefetch(ytm: &Ytm, id: String) -> Result<(), String> {
    ytm.prefetch(&id).await;
    Ok(())
}

/// Audioqualität für Streaming und Downloads setzen. Für „Premium“ bekommt yt-dlp die
/// YouTube-Anmeldung als Cookie-Datei (sonst liefert YouTube keine 256-kbit-Streams).
pub async fn set_audio_quality(
    ytm: &Ytm,
    stream: crate::services::ytdlp::Quality,
    download: crate::services::ytdlp::Quality,
) -> Result<(), String> {
    use crate::services::ytdlp::Quality;
    let path = ytm.login_marker.with_file_name("cookies.txt");
    let wants_premium = stream == Quality::Premium || download == Quality::Premium;
    let cookies = match ytm.rp.user_auth_cookie_header() {
        Some((header, _)) if wants_premium => {
            // Netscape-Format, wie yt-dlp es erwartet
            let expires = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
                + 365 * 86_400;
            let mut text = String::from("# Netscape HTTP Cookie File\n");
            for pair in header.split(';').map(str::trim).filter(|p| !p.is_empty()) {
                if let Some((name, value)) = pair.split_once('=') {
                    text.push_str(&format!(".youtube.com\tTRUE\t/\tTRUE\t{expires}\t{name}\t{value}\n"));
                }
            }
            std::fs::write(&path, text).map_err(err)?;
            Some(path)
        }
        _ => {
            let _ = std::fs::remove_file(&path);
            None
        }
    };
    ytm.ytdlp.set_quality(stream, download, cookies);
    // Schon ermittelte Stream-Adressen gelten für die alte Qualität
    ytm.streams.lock().await.clear();
    Ok(())
}

/// „Radio“ zu einem Song: ähnliche Titel, die YouTube Music danach spielen würde.
pub async fn ytm_radio(ytm: &Ytm, id: String) -> Result<Vec<YtTrack>, String> {
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtAccount {
    pub name: Option<String>,
    pub handle: Option<String>,
    pub photo_url: Option<String>,
}

pub async fn ytm_account(ytm: &Ytm) -> Result<YtAccount, String> {
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
pub async fn ytm_liked_ids(ytm: &Ytm) -> Result<Vec<String>, String> {
    let query = ytm.rp.query();
    let mut liked = query.music_liked_tracks().await.map_err(err)?;
    liked.tracks.extend_limit(&query, 10_000).await.map_err(err)?;
    Ok(liked.tracks.items.into_iter().map(|t| t.id).collect())
}

pub async fn ytm_set_like(ytm: &Ytm, id: String, liked: bool) -> Result<(), String> {
    let endpoint = if liked { "like/like" } else { "like/removelike" };
    yt_request(&ytm, endpoint, serde_json::json!({ "target": { "videoId": id } })).await?;
    Ok(())
}

/// Legt eine neue Playlist an (optional gleich mit Songs) und gibt ihre ID zurück.
pub async fn ytm_create_playlist(
    ytm: &Ytm,
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

pub async fn ytm_add_to_playlist(
    ytm: &Ytm,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtHistoryEntry {
    pub track: YtTrack,
    /// Gruppierung von YouTube, z. B. „Heute“, „Gestern“, „Diese Woche“
    pub group: Option<String>,
}

/// Verlauf von YouTube Music (zuletzt gehörte Songs im Konto)
pub async fn ytm_history(ytm: &Ytm) -> Result<Vec<YtHistoryEntry>, String> {
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
pub async fn ytm_report_play(ytm: &Ytm, id: String) -> Result<(), String> {
    use rand::Rng;
    use sha1::{Digest, Sha1};

    // Zufällige Wiedergabe-ID (cpn), 16 Zeichen – vor dem ersten await erzeugen (Zufallsgenerator ist nicht Send)
    let cpn: String = {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut rng = rand::rng();
        (0..16).map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char).collect()
    };

    let sts = ytm.signature_timestamp().await?;
    let player = yt_request(
        &ytm,
        "player",
        serde_json::json!({
            "videoId": id,
            "playbackContext": { "contentPlaybackContext": { "signatureTimestamp": sts } }
        }),
    )
    .await?;
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtArtistCard {
    pub id: String,
    pub name: String,
    pub image_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtArtist {
    pub id: String,
    pub name: String,
    pub image_url: Option<String>,
    pub description: Option<String>,
    pub subscribers: Option<u64>,
    /// Beliebte Songs
    pub tracks: Vec<YtTrack>,
    pub albums: Vec<YtCollection>,
    pub singles: Vec<YtCollection>,
    pub playlists: Vec<YtCollection>,
    pub similar: Vec<YtArtistCard>,
    /// Playlist mit allen Songs des Künstlers
    pub all_songs_playlist_id: Option<String>,
}

/// Künstlerseite: beliebte Songs, Alben, Singles, Playlists und ähnliche Künstler
pub async fn ytm_artist(ytm: &Ytm, id: String) -> Result<YtArtist, String> {
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

// --- Startseite ---------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtHome {
    /// Meistgehörte Songs in Deutschland
    pub top_tracks: Vec<YtTrack>,
    /// Gerade im Trend
    pub trending: Vec<YtTrack>,
    pub top_artists: Vec<YtArtistCard>,
    pub new_albums: Vec<YtCollection>,
}

/// Charts und Neuerscheinungen für die Startseite (ohne Anmeldung)
pub async fn ytm_home(ytm: &Ytm) -> Result<YtHome, String> {
    let q = ytm.rp.query();
    let (charts, albums) = tokio::join!(q.music_charts(Some(Country::De)), q.music_new_albums());
    let charts = charts.ok();
    let take = |list: Vec<TrackItem>| list.into_iter().take(20).map(Into::into).collect::<Vec<YtTrack>>();
    // YouTube liefert die Chart-Songs inzwischen oft nur als Playlist-Verweis
    async fn tracks_of(ytm: &Ytm, direct: Vec<TrackItem>, playlist: Option<String>) -> Vec<TrackItem> {
        if !direct.is_empty() {
            return direct;
        }
        match playlist {
            Some(id) => ytm.rp.query().music_playlist(&id).await.map(|p| p.tracks.items).unwrap_or_default(),
            None => Vec::new(),
        }
    }
    let (top, trending) = match &charts {
        Some(c) => {
            // Sonst über die Chart-Playlists (Namen sind übersetzt, z. B. „Top 100 der Musikvideos in Deutschland“;
            // die Trend-Liste „Top 20 / Trending 20“ ist eine Album-Playlist mit „OLAK“-ID)
            let top_id = c.playlists.iter().find(|p| p.name.contains("100")).map(|p| p.id.clone());
            let trend_id = c
                .playlists
                .iter()
                .find(|p| p.id.starts_with("OLAK") || p.name.to_lowercase().contains("trend"))
                .map(|p| p.id.clone());
            tokio::join!(
                tracks_of(&ytm, c.top_tracks.clone(), c.top_playlist_id.clone().or(top_id)),
                tracks_of(&ytm, c.trending_tracks.clone(), c.trending_playlist_id.clone().or(trend_id)),
            )
        }
        None => (Vec::new(), Vec::new()),
    };
    Ok(YtHome {
        top_tracks: take(top),
        trending: take(trending),
        top_artists: charts
            .map(|c| {
                c.artists
                    .into_iter()
                    .take(12)
                    .map(|a| YtArtistCard { image_url: best_thumbnail(&a.avatar), id: a.id, name: a.name })
                    .collect()
            })
            .unwrap_or_default(),
        new_albums: albums.map(|a| a.into_iter().take(20).map(Into::into).collect()).unwrap_or_default(),
    })
}
