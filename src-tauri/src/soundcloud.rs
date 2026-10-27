//! SoundCloud: Anmeldung (Login-Cookie „oauth_token“ aus einem Wavely-Fenster), Likes, Playlists,
//! Suche, Künstlerseiten und Streams über die inoffizielle API der SoundCloud-Webseite (api-v2).
//!
//! Die Webseite nutzt eine öffentliche „client_id“, die in ihren Skripten steht; Wavely liest sie
//! dort aus (wie yt-dlp). Songs, von denen SoundCloud ohne Go+ nur 30 Sekunden liefert („SNIP“),
//! spielt Wavely über den passenden YouTube-Music-Song in voller Länge.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State, Url, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::Mutex;

const API: &str = "https://api-v2.soundcloud.com";
const LOGIN_WINDOW: &str = "soundcloud-login";
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
const MAX_ITEMS: usize = 2000;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub struct SoundCloud {
    http: reqwest::Client,
    dir: PathBuf,
    client_id: Mutex<Option<String>>,
    token: Mutex<Option<String>>,
    /// Songs werden für die Wiedergabe einmal komplett geladen (siehe serve_stream); nie zwei gleichzeitig
    preparing: Mutex<()>,
}

impl SoundCloud {
    pub fn new(app: &AppHandle) -> Result<Self, String> {
        let dir = app.path().app_data_dir().map_err(err)?.join("soundcloud");
        std::fs::create_dir_all(&dir).map_err(err)?;
        let token = std::fs::read_to_string(dir.join("token.txt")).ok().filter(|t| !t.trim().is_empty());
        Ok(Self {
            http: reqwest::Client::builder().user_agent(USER_AGENT).build().map_err(err)?,
            dir,
            client_id: Mutex::new(None),
            token: Mutex::new(token.map(|t| t.trim().to_string())),
            preparing: Mutex::new(()),
        })
    }

    async fn save_token(&self, token: Option<String>) {
        let path = self.dir.join("token.txt");
        match &token {
            Some(t) => {
                let _ = std::fs::write(path, t);
            }
            None => {
                let _ = std::fs::remove_file(path);
            }
        }
        *self.token.lock().await = token;
    }

    /// client_id aus den Skripten der SoundCloud-Webseite lesen
    async fn fetch_client_id(&self) -> Result<String, String> {
        let html = self.http.get("https://soundcloud.com/").send().await.map_err(err)?.text().await.map_err(err)?;
        let mut scripts: Vec<&str> = html
            .split("<script")
            .filter_map(|tag| {
                let start = tag.find("src=\"https://a-v2.sndcdn.com/assets/")? + 5;
                let end = tag[start..].find('"')? + start;
                Some(&tag[start..end])
            })
            .collect();
        // Die client_id steht meist in einem der letzten Skripte
        scripts.reverse();
        for src in scripts {
            let js = self.http.get(src).send().await.map_err(err)?.text().await.map_err(err)?;
            for marker in ["client_id:\"", "client_id=\""] {
                if let Some(pos) = js.find(marker) {
                    let id: String = js[pos + marker.len()..].chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
                    if id.len() == 32 {
                        return Ok(id);
                    }
                }
            }
        }
        Err("SoundCloud-Schlüssel nicht gefunden".into())
    }

    async fn client_id(&self, refresh: bool) -> Result<String, String> {
        let mut guard = self.client_id.lock().await;
        if refresh || guard.is_none() {
            *guard = Some(self.fetch_client_id().await?);
        }
        Ok(guard.clone().unwrap())
    }

    /// GET an api-v2 (Pfad oder volle URL aus „next_href“), angemeldet falls möglich
    async fn get(&self, path: &str) -> Result<Value, String> {
        let base = if path.starts_with("http") { path.to_string() } else { format!("{API}{path}") };
        let token = self.token.lock().await.clone();
        for refresh in [false, true] {
            let mut url = Url::parse(&base).map_err(err)?;
            let cid = self.client_id(refresh).await?;
            let pairs: Vec<(String, String)> = url.query_pairs().filter(|(k, _)| k != "client_id").map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
            url.query_pairs_mut().clear().extend_pairs(pairs).append_pair("client_id", &cid);
            let mut request = self.http.get(url);
            if let Some(token) = &token {
                request = request.header("Authorization", format!("OAuth {token}"));
            }
            let response = request.send().await.map_err(err)?;
            let status = response.status();
            if (status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN) && !refresh {
                continue; // client_id evtl. veraltet → neu holen
            }
            if status == reqwest::StatusCode::UNAUTHORIZED && token.is_some() {
                return Err("SoundCloud-Anmeldung abgelaufen – bitte neu anmelden".into());
            }
            if !status.is_success() {
                return Err(format!("SoundCloud antwortete mit {status}"));
            }
            return response.json().await.map_err(err);
        }
        Err("SoundCloud nicht erreichbar".into())
    }

    async fn paginate(&self, path: &str, limit: usize) -> Result<Vec<Value>, String> {
        let mut items = Vec::new();
        let mut next = Some(path.to_string());
        while let Some(url) = next {
            let page = self.get(&url).await?;
            if let Some(list) = page["collection"].as_array() {
                items.extend(list.iter().cloned());
            }
            next = page["next_href"].as_str().map(str::to_string);
            if items.len() >= limit {
                break;
            }
        }
        items.truncate(limit);
        Ok(items)
    }

    async fn me(&self) -> Result<Value, String> {
        if self.token.lock().await.is_none() {
            return Err("Nicht bei SoundCloud angemeldet".into());
        }
        self.get("/me").await
    }

    /// Playlist-Songs vervollständigen: SoundCloud liefert nur die ersten voll, den Rest als reine IDs
    async fn complete_tracks(&self, tracks: Vec<Value>) -> Result<Vec<Value>, String> {
        let missing: Vec<String> = tracks
            .iter()
            .filter(|t| t["title"].is_null())
            .filter_map(|t| t["id"].as_u64().map(|id| id.to_string()))
            .collect();
        let mut full = std::collections::HashMap::new();
        for chunk in missing.chunks(50) {
            let list = self.get(&format!("/tracks?ids={}", chunk.join(","))).await?;
            for t in list.as_array().cloned().unwrap_or_default() {
                if let Some(id) = t["id"].as_u64() {
                    full.insert(id, t);
                }
            }
        }
        Ok(tracks
            .into_iter()
            .map(|t| if t["title"].is_null() { t["id"].as_u64().and_then(|id| full.get(&id).cloned()).unwrap_or(t) } else { t })
            .collect())
    }
}

// --- Umwandlung in die Formate der Oberfläche ----------------------------

/// SoundCloud-Bilder in größerer Auflösung („-large“ ist nur 100×100)
fn big_image(url: Option<&str>) -> Option<String> {
    url.filter(|u| !u.is_empty()).map(|u| u.replace("-large.", "-t500x500."))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScArtistRef {
    id: Option<String>,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScTrack {
    id: String,
    title: String,
    artist: Option<String>,
    artists: Vec<ScArtistRef>,
    album: Option<String>,
    album_id: Option<String>,
    duration: f64,
    artwork_url: Option<String>,
    url: Option<String>,
    /// Nur 30-Sekunden-Ausschnitt verfügbar (Go+) → Wiedergabe über YouTube Music
    snipped: bool,
}

fn to_track(t: &Value) -> Option<ScTrack> {
    if t["kind"].as_str() != Some("track") || t["title"].is_null() {
        return None;
    }
    let user = &t["user"];
    let name = t["publisher_metadata"]["artist"]
        .as_str()
        .filter(|a| !a.is_empty())
        .or(user["username"].as_str())
        .unwrap_or_default()
        .to_string();
    let policy = t["policy"].as_str().unwrap_or("");
    Some(ScTrack {
        id: t["id"].as_u64()?.to_string(),
        title: t["title"].as_str().unwrap_or_default().to_string(),
        artist: (!name.is_empty()).then(|| name.clone()),
        artists: vec![ScArtistRef { id: user["id"].as_u64().map(|i| i.to_string()), name }],
        album: t["publisher_metadata"]["album_title"].as_str().map(str::to_string),
        album_id: None,
        duration: t["full_duration"].as_f64().or(t["duration"].as_f64()).unwrap_or(0.0) / 1000.0,
        artwork_url: big_image(t["artwork_url"].as_str().or(user["avatar_url"].as_str())),
        url: t["permalink_url"].as_str().map(str::to_string),
        snipped: policy == "SNIP" || policy == "BLOCK",
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScCollection {
    id: String,
    kind: &'static str,
    title: String,
    subtitle: Option<String>,
    artwork_url: Option<String>,
    editable: bool,
}

fn to_playlist(p: &Value) -> Option<ScCollection> {
    let is_album = p["is_album"].as_bool().unwrap_or(false) || p["set_type"].as_str() == Some("album");
    let art = p["artwork_url"]
        .as_str()
        .or_else(|| p["tracks"][0]["artwork_url"].as_str())
        .or(p["user"]["avatar_url"].as_str());
    Some(ScCollection {
        id: p["id"].as_u64()?.to_string(),
        kind: if is_album { "album" } else { "playlist" },
        title: p["title"].as_str().unwrap_or_default().to_string(),
        subtitle: p["user"]["username"].as_str().map(str::to_string),
        artwork_url: big_image(art),
        editable: false,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScArtistCard {
    id: String,
    name: String,
    image_url: Option<String>,
}

fn to_artist_card(u: &Value) -> Option<ScArtistCard> {
    Some(ScArtistCard {
        id: u["id"].as_u64()?.to_string(),
        name: u["username"].as_str().unwrap_or_default().to_string(),
        image_url: big_image(u["avatar_url"].as_str()),
    })
}

// --- Anmeldung -----------------------------------------------------------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthEvent {
    logged_in: bool,
    error: Option<String>,
}

#[tauri::command]
pub async fn soundcloud_status(sc: State<'_, SoundCloud>) -> Result<bool, String> {
    Ok(sc.token.lock().await.is_some())
}

/// Öffnet die SoundCloud-Anmeldung. Sobald das Login-Cookie „oauth_token“ gesetzt ist, wird es
/// übernommen und das Fenster geschlossen. Ergebnis kommt als Event `soundcloud-auth`.
#[tauri::command]
pub async fn soundcloud_login(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LOGIN_WINDOW) {
        let _ = window.set_focus();
        return Ok(());
    }
    let data_dir = app.path().app_data_dir().map_err(err)?.join("soundcloud-login-webview");
    let _ = std::fs::remove_dir_all(&data_dir);

    let window = WebviewWindowBuilder::new(
        &app,
        LOGIN_WINDOW,
        WebviewUrl::External("https://soundcloud.com/signin".parse().map_err(err)?),
    )
    .title("Bei SoundCloud anmelden")
    .inner_size(680.0, 820.0)
    .data_directory(data_dir)
    .build()
    .map_err(err)?;

    // SoundCloud meldet im Overlay an, ohne die Seite neu zu laden → Cookie jede Sekunde prüfen,
    // solange das Fenster offen ist (höchstens 15 Minuten).
    // Cookies dürfen unter Windows nicht im Event-Handler gelesen werden, daher eigener Thread.
    std::thread::spawn(move || {
        for _ in 0..900 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if app.get_webview_window(LOGIN_WINDOW).is_none() {
                return; // Fenster wurde geschlossen
            }
            let token = window
                .cookies()
                .ok()
                .and_then(|c| c.into_iter().find(|c| c.name() == "oauth_token").map(|c| c.value().to_string()))
                .filter(|t| !t.is_empty());
            let Some(token) = token else { continue };
            let _ = window.close();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let sc = app.state::<SoundCloud>();
                sc.save_token(Some(token)).await;
                let result = sc.me().await.map(|_| ());
                if result.is_err() {
                    sc.save_token(None).await;
                }
                let _ = app.emit("soundcloud-auth", AuthEvent { logged_in: result.is_ok(), error: result.err() });
            });
            return;
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn soundcloud_logout(sc: State<'_, SoundCloud>) -> Result<(), String> {
    sc.save_token(None).await;
    Ok(())
}

// --- Daten ---------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScAccount {
    id: Option<String>,
    name: Option<String>,
    photo_url: Option<String>,
}

#[tauri::command]
pub async fn soundcloud_account(sc: State<'_, SoundCloud>) -> Result<ScAccount, String> {
    let me = sc.me().await?;
    Ok(ScAccount {
        id: me["id"].as_u64().map(|i| i.to_string()),
        name: me["username"].as_str().map(str::to_string),
        photo_url: big_image(me["avatar_url"].as_str()),
    })
}

#[derive(Serialize)]
pub struct ScLibrary {
    playlists: Vec<ScCollection>,
    albums: Vec<ScCollection>,
    following: Vec<ScArtistCard>,
}

/// Eigene und gelikte Playlists/Alben sowie gefolgte Künstler
#[tauri::command]
pub async fn soundcloud_library(sc: State<'_, SoundCloud>) -> Result<ScLibrary, String> {
    let me = sc.me().await?;
    let id = me["id"].as_u64().ok_or("Konto nicht gefunden")?;
    let own_path = format!("/users/{id}/playlists?limit=50");
    let liked_path = format!("/users/{id}/likes?limit=50");
    let following_path = format!("/users/{id}/followings?limit=50");
    let (own, liked, following) = tokio::join!(
        sc.paginate(&own_path, 500),
        sc.paginate(&liked_path, 500),
        sc.paginate(&following_path, 500),
    );
    let mut sets: Vec<ScCollection> = own?.iter().filter_map(to_playlist).collect();
    sets.extend(liked?.iter().filter_map(|l| to_playlist(&l["playlist"])));
    let (albums, playlists): (Vec<_>, Vec<_>) = sets.into_iter().partition(|c| c.kind == "album");
    Ok(ScLibrary {
        playlists,
        albums,
        following: following.unwrap_or_default().iter().filter_map(to_artist_card).collect(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScCollectionDetail {
    #[serde(flatten)]
    info: ScCollection,
    tracks: Vec<ScTrack>,
}

/// Gelikte Songs (`id = "likes"`) oder eine Playlist/ein Album
#[tauri::command]
pub async fn soundcloud_collection(sc: State<'_, SoundCloud>, id: String) -> Result<ScCollectionDetail, String> {
    if id == "likes" {
        let me = sc.me().await?;
        let user = me["id"].as_u64().ok_or("Konto nicht gefunden")?;
        let likes = sc.paginate(&format!("/users/{user}/track_likes?limit=50"), MAX_ITEMS).await?;
        return Ok(ScCollectionDetail {
            info: ScCollection {
                id,
                kind: "playlist",
                title: "Gelikte Songs".into(),
                subtitle: Some("SoundCloud".into()),
                artwork_url: None,
                editable: false,
            },
            tracks: likes.iter().filter_map(|l| to_track(&l["track"])).collect(),
        });
    }
    let playlist = sc.get(&format!("/playlists/{id}?representation=full")).await?;
    let tracks = sc.complete_tracks(playlist["tracks"].as_array().cloned().unwrap_or_default()).await?;
    Ok(ScCollectionDetail {
        info: to_playlist(&playlist).ok_or("Playlist nicht gefunden")?,
        tracks: tracks.iter().filter_map(to_track).collect(),
    })
}

#[derive(Serialize)]
pub struct ScSearch {
    tracks: Vec<ScTrack>,
    albums: Vec<ScCollection>,
    playlists: Vec<ScCollection>,
    artists: Vec<ScArtistCard>,
}

#[tauri::command]
pub async fn soundcloud_search(sc: State<'_, SoundCloud>, query: String) -> Result<ScSearch, String> {
    let q: String = url_encode(&query);
    let paths = [
        format!("/search/tracks?q={q}&limit=20"),
        format!("/search/albums?q={q}&limit=12"),
        format!("/search/playlists_without_albums?q={q}&limit=12"),
        format!("/search/users?q={q}&limit=12"),
    ];
    let (tracks, albums, playlists, users) =
        tokio::join!(sc.get(&paths[0]), sc.get(&paths[1]), sc.get(&paths[2]), sc.get(&paths[3]));
    let list = |v: Result<Value, String>| v.ok().and_then(|v| v["collection"].as_array().cloned()).unwrap_or_default();
    Ok(ScSearch {
        tracks: list(Ok(tracks?)).iter().filter_map(to_track).collect(),
        albums: list(albums).iter().filter_map(to_playlist).collect(),
        playlists: list(playlists).iter().filter_map(to_playlist).collect(),
        artists: list(users).iter().filter_map(to_artist_card).collect(),
    })
}

fn url_encode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

/// Künstlerseite im selben Format wie bei YouTube Music
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScArtist {
    id: String,
    name: String,
    image_url: Option<String>,
    description: Option<String>,
    subscribers: Option<u64>,
    tracks: Vec<ScTrack>,
    albums: Vec<ScCollection>,
    singles: Vec<ScCollection>,
    playlists: Vec<ScCollection>,
    similar: Vec<ScArtistCard>,
}

#[tauri::command]
pub async fn soundcloud_artist(sc: State<'_, SoundCloud>, id: String) -> Result<ScArtist, String> {
    let paths = [
        format!("/users/{id}"),
        format!("/users/{id}/toptracks?limit=10"),
        format!("/users/{id}/albums?limit=50"),
        format!("/users/{id}/playlists_without_albums?limit=50"),
        format!("/users/{id}/relatedartists?limit=20"),
    ];
    let (user, top, albums, playlists, related) =
        tokio::join!(sc.get(&paths[0]), sc.get(&paths[1]), sc.get(&paths[2]), sc.get(&paths[3]), sc.get(&paths[4]));
    let user = user?;
    let list = |v: Result<Value, String>| v.ok().and_then(|v| v["collection"].as_array().cloned()).unwrap_or_default();
    // Großes Titelbild (Banner), sonst Profilbild
    let banner = user["visuals"]["visuals"][0]["visual_url"].as_str().map(str::to_string);
    Ok(ScArtist {
        image_url: banner.or_else(|| big_image(user["avatar_url"].as_str())),
        name: user["username"].as_str().unwrap_or_default().to_string(),
        description: user["description"].as_str().filter(|d| !d.is_empty()).map(str::to_string),
        subscribers: user["followers_count"].as_u64(),
        tracks: list(top).iter().filter_map(to_track).collect(),
        albums: list(albums).iter().filter_map(to_playlist).collect(),
        singles: Vec::new(),
        playlists: list(playlists).iter().filter_map(to_playlist).collect(),
        similar: list(related).iter().filter_map(to_artist_card).collect(),
        id,
    })
}

/// Ähnliche Songs (für Autoplay)
#[tauri::command]
pub async fn soundcloud_related(sc: State<'_, SoundCloud>, id: String) -> Result<Vec<ScTrack>, String> {
    let r = sc.get(&format!("/tracks/{id}/related?limit=20")).await?;
    Ok(r["collection"].as_array().cloned().unwrap_or_default().iter().filter_map(to_track).collect())
}

/// Zuletzt gehört (SoundCloud-Verlauf)
#[tauri::command]
pub async fn soundcloud_recent(sc: State<'_, SoundCloud>) -> Result<Vec<ScTrack>, String> {
    sc.me().await?;
    let r = sc.get("/me/play-history/tracks?limit=50").await?;
    Ok(r["collection"].as_array().cloned().unwrap_or_default().iter().filter_map(|i| to_track(&i["track"])).collect())
}

/// Direkte Stream-Adresse (MP3) eines Songs. `None`, wenn SoundCloud nur einen Ausschnitt liefert –
/// dann spielt Wavely den passenden YouTube-Music-Song.
#[tauri::command]
pub async fn soundcloud_stream(sc: State<'_, SoundCloud>, id: String) -> Result<Option<String>, String> {
    let track = sc.get(&format!("/tracks/{id}")).await?;
    let policy = track["policy"].as_str().unwrap_or("");
    let transcodings = track["media"]["transcodings"].as_array().cloned().unwrap_or_default();
    // Mit Go+ kann auch ein „SNIP“-Song voll verfügbar sein; vorschau-Streams erkennt man am „preview“-Flag
    let full = transcodings
        .iter()
        .filter(|t| !t["snipped"].as_bool().unwrap_or(false))
        .find(|t| t["format"]["protocol"].as_str() == Some("progressive"));
    let Some(transcoding) = full else {
        return Ok(None);
    };
    if policy == "BLOCK" {
        return Ok(None);
    }
    let auth = track["track_authorization"].as_str().unwrap_or_default();
    let base = transcoding["url"].as_str().ok_or("Stream fehlt")?;
    let info = sc.get(&format!("{base}?track_authorization={auth}")).await?;
    Ok(info["url"].as_str().map(str::to_string))
}

// --- Direkte Wiedergabe (scstream://<id>) ----------------------------------
//
// SoundCloud liefert fast nur noch HLS: eine Startdatei (init.mp4) plus Stücke à 10 Sekunden (.m4s),
// zusammen ein „fragmentiertes MP4“. Wavely lädt alle Stücke parallel, setzt sie zu einer Datei
// zusammen und spielt diese wie eine normale Datei ab (mit Spulen). Geschützte Go+-Songs und
// 30-Sekunden-Ausschnitte gehen nicht – dann antwortet der Dienst mit 404 und der Player nimmt YouTube Music.

/// Zuletzt gespielte Songs bleiben im Zwischenspeicher, damit Wiederholen sofort startet
const CACHE_FILES: usize = 30;

struct Prepared {
    path: PathBuf,
    mime: &'static str,
}

/// Eine Adresse laden (mit zwei Wiederholungen bei Netzwerkfehlern)
async fn fetch_bytes(http: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let mut last = String::new();
    for _ in 0..3 {
        match http.get(url).send().await {
            Ok(r) if r.status().is_success() => return r.bytes().await.map(|b| b.to_vec()).map_err(err),
            Ok(r) => last = format!("SoundCloud antwortete mit {}", r.status()),
            Err(e) => last = err(e),
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
    Err(last)
}

/// Rangfolge der Formate: MP3 am Stück, AAC (160k vor 96k), MP3 als HLS; sonst ungeeignet
fn rank(t: &Value) -> Option<u8> {
    let protocol = t["format"]["protocol"].as_str().unwrap_or("");
    let mime = t["format"]["mime_type"].as_str().unwrap_or("");
    let preset = t["preset"].as_str().unwrap_or("");
    if t["snipped"].as_bool().unwrap_or(false) || protocol.contains("encrypted") || mime.contains("mpegurl") {
        return None;
    }
    match protocol {
        "progressive" if mime.starts_with("audio/mpeg") => Some(0),
        "hls" if mime.starts_with("audio/mp4") && preset.contains("160") => Some(1),
        "hls" if mime.starts_with("audio/mp4") => Some(2),
        "hls" if mime.starts_with("audio/mpeg") => Some(3),
        _ => None,
    }
}

impl SoundCloud {
    fn cache_dir(&self) -> PathBuf {
        self.dir.join("cache")
    }

    fn cached(&self, id: &str) -> Option<Prepared> {
        for (ext, mime) in [("m4a", "audio/mp4"), ("mp3", "audio/mpeg")] {
            let path = self.cache_dir().join(format!("{id}.{ext}"));
            if path.exists() {
                return Some(Prepared { path, mime });
            }
        }
        None
    }

    /// Song vollständig in den Zwischenspeicher laden (oder NOT_PLAYABLE, wenn er nicht direkt spielbar ist)
    async fn prepare(&self, id: &str) -> Result<Prepared, String> {
        let _guard = self.preparing.lock().await;
        if let Some(found) = self.cached(id) {
            return Ok(found);
        }
        let track = self.get(&format!("/tracks/{id}")).await?;
        if matches!(track["policy"].as_str(), Some("BLOCK") | Some("SNIP")) {
            return Err("NOT_PLAYABLE".into());
        }
        let auth = track["track_authorization"].as_str().unwrap_or_default().to_string();
        let transcodings = track["media"]["transcodings"].as_array().cloned().unwrap_or_default();
        let chosen = transcodings
            .iter()
            .filter_map(|t| Some((rank(t)?, t)))
            .min_by_key(|(r, _)| *r)
            .map(|(_, t)| t)
            .ok_or("NOT_PLAYABLE")?;

        let base = chosen["url"].as_str().ok_or("Stream fehlt")?;
        let info = self.get(&format!("{base}?track_authorization={auth}")).await?;
        let url = info["url"].as_str().ok_or("Stream fehlt")?.to_string();
        let is_mp4 = chosen["format"]["mime_type"].as_str().unwrap_or("").starts_with("audio/mp4");
        let (ext, mime) = if is_mp4 { ("m4a", "audio/mp4") } else { ("mp3", "audio/mpeg") };

        let data = if chosen["format"]["protocol"].as_str() == Some("progressive") {
            fetch_bytes(&self.http, &url).await?
        } else {
            let playlist = String::from_utf8(fetch_bytes(&self.http, &url).await?).map_err(err)?;
            let init = playlist
                .lines()
                .find(|l| l.starts_with("#EXT-X-MAP"))
                .and_then(|l| l.split("URI=\"").nth(1))
                .and_then(|rest| rest.split('"').next())
                .map(str::to_string);
            let segments: Vec<String> =
                playlist.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string).collect();
            if segments.is_empty() {
                return Err("Leere Stream-Liste".into());
            }
            // Stücke parallel laden (höchstens 6 gleichzeitig), Reihenfolge bleibt erhalten
            let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(6));
            let tasks: Vec<_> = init
                .into_iter()
                .chain(segments)
                .map(|url| {
                    let semaphore = semaphore.clone();
                    let http = self.http.clone();
                    tauri::async_runtime::spawn(async move {
                        let _permit = semaphore.acquire().await.map_err(err)?;
                        fetch_bytes(&http, &url).await
                    })
                })
                .collect();
            let mut data = Vec::new();
            for task in tasks {
                data.extend(task.await.map_err(err)??);
            }
            data
        };

        let dir = self.cache_dir();
        std::fs::create_dir_all(&dir).map_err(err)?;
        let path = dir.join(format!("{id}.{ext}"));
        std::fs::write(&path, data).map_err(err)?;
        self.trim_cache();
        Ok(Prepared { path, mime })
    }

    fn trim_cache(&self) {
        let Ok(entries) = std::fs::read_dir(self.cache_dir()) else { return };
        let mut files: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .collect();
        if files.len() <= CACHE_FILES {
            return;
        }
        files.sort();
        for (_, path) in files.iter().take(files.len() - CACHE_FILES) {
            let _ = std::fs::remove_file(path);
        }
    }

    /// Antwort auf `scstream://<id>` (mit Range, damit Spulen geht)
    pub async fn serve_stream(&self, id: &str, range: Option<&str>) -> tauri::http::Response<Vec<u8>> {
        use tauri::http::{header, Response, StatusCode};
        let respond = |status: StatusCode, body: Vec<u8>| {
            Response::builder()
                .status(status)
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .body(body)
                .unwrap()
        };
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            return respond(StatusCode::BAD_REQUEST, Vec::new());
        }
        let prepared = match self.prepare(id).await {
            Ok(p) => p,
            Err(e) if e == "NOT_PLAYABLE" => return respond(StatusCode::NOT_FOUND, e.into_bytes()),
            Err(e) => return respond(StatusCode::BAD_GATEWAY, e.into_bytes()),
        };
        let Ok(data) = std::fs::read(&prepared.path) else {
            return respond(StatusCode::INTERNAL_SERVER_ERROR, Vec::new());
        };
        let total = data.len() as u64;
        let (start, end) = range
            .and_then(|r| r.strip_prefix("bytes="))
            .map(|spec| {
                let (a, b) = spec.split_once('-').unwrap_or((spec, ""));
                (a.trim().parse::<u64>().unwrap_or(0), b.trim().parse::<u64>().ok())
            })
            .unwrap_or((0, None));
        if start >= total {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{total}"))
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .body(Vec::new())
                .unwrap();
        }
        let end = end.unwrap_or(total - 1).min(total - 1);
        Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_TYPE, prepared.mime)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{total}"))
            .header(header::CONTENT_LENGTH, end - start + 1)
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(data[start as usize..=end as usize].to_vec())
            .unwrap()
    }
}

/// Nächsten Song schon vorab laden, damit er sofort startet
#[tauri::command]
pub async fn soundcloud_prefetch(sc: State<'_, SoundCloud>, id: String) -> Result<(), String> {
    if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
        sc.prepare(&id).await.map(|_| ())
    } else {
        Ok(())
    }
}
