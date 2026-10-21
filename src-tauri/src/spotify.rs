//! Spotify: Anmeldung (OAuth mit PKCE, ohne Client Secret), Bibliothek, Suche, Künstler.
//!
//! Spotify liefert ohne Premium keinen Ton über die Web API. Zum Abspielen sucht Wavely denselben
//! Song bei YouTube Music (`spotify_resolve`) und spielt ihn über den normalen Player ab.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State, Url, WebviewUrl, WebviewWindowBuilder};

use crate::ytm::Ytm;

/// Client ID der Spotify-App aus dem Developer Dashboard (öffentlich, kein Geheimnis)
const CLIENT_ID: &str = "5d9c0db17c284f5a959e0baa2294f80e";
/// Muss genau so im Spotify-Dashboard als Redirect URI eingetragen sein
const REDIRECT_URI: &str = "http://127.0.0.1:43827/callback";
const SCOPES: &str = "user-read-private playlist-read-private playlist-read-collaborative user-library-read \
                      user-read-recently-played user-follow-read";
const LOGIN_WINDOW: &str = "spotify-login";
const API: &str = "https://api.spotify.com/v1";
/// Obergrenze beim Nachladen langer Listen
const MAX_ITEMS: usize = 2000;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[derive(Clone, Serialize, Deserialize)]
struct Token {
    access_token: String,
    refresh_token: String,
    /// Unix-Zeit in Sekunden
    expires_at: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

pub struct Spotify {
    http: reqwest::Client,
    dir: PathBuf,
    token: tokio::sync::Mutex<Option<Token>>,
    /// Laufende Anmeldung: (code_verifier, state)
    pending: Mutex<Option<(String, String)>>,
    /// Zuordnung Spotify-Song → YouTube-Video, damit jeder Song nur einmal gesucht wird
    matches: Mutex<HashMap<String, String>>,
}

impl Spotify {
    pub fn new(app: &AppHandle) -> Result<Self, String> {
        let dir = app.path().app_data_dir().map_err(err)?.join("spotify");
        std::fs::create_dir_all(&dir).map_err(err)?;
        let token = std::fs::read_to_string(dir.join("token.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        let matches = std::fs::read_to_string(dir.join("matches.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Ok(Self {
            http: reqwest::Client::new(),
            dir,
            token: tokio::sync::Mutex::new(token),
            pending: Mutex::new(None),
            matches: Mutex::new(matches),
        })
    }

    fn save_token(&self, token: Option<&Token>) {
        let path = self.dir.join("token.json");
        match token {
            Some(token) => {
                if let Ok(json) = serde_json::to_string(token) {
                    let _ = std::fs::write(path, json);
                }
            }
            None => {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    async fn request_token(&self, form: &[(&str, &str)]) -> Result<Token, String> {
        let response = self
            .http
            .post("https://accounts.spotify.com/api/token")
            .form(form)
            .send()
            .await
            .map_err(err)?;
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(format!("Spotify-Anmeldung fehlgeschlagen: {text}"));
        }
        let data: TokenResponse = response.json().await.map_err(err)?;
        Ok(Token {
            access_token: data.access_token,
            refresh_token: data.refresh_token.unwrap_or_default(),
            expires_at: now() + data.expires_in,
        })
    }

    /// Gültiges Zugangstoken (wird bei Bedarf mit dem Refresh-Token erneuert)
    async fn access_token(&self, force_refresh: bool) -> Result<String, String> {
        let mut guard = self.token.lock().await;
        let token = guard.as_ref().ok_or("Nicht bei Spotify angemeldet")?.clone();
        if !force_refresh && token.expires_at > now() + 60 {
            return Ok(token.access_token);
        }
        let mut fresh = self
            .request_token(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", &token.refresh_token),
                ("client_id", CLIENT_ID),
            ])
            .await?;
        // Spotify schickt nicht immer ein neues Refresh-Token
        if fresh.refresh_token.is_empty() {
            fresh.refresh_token = token.refresh_token;
        }
        self.save_token(Some(&fresh));
        let access = fresh.access_token.clone();
        *guard = Some(fresh);
        Ok(access)
    }

    /// GET an die Web API (Pfad wie „/me“ oder volle URL aus „next“)
    async fn get(&self, path: &str) -> Result<Value, String> {
        let url = if path.starts_with("http") { path.to_string() } else { format!("{API}{path}") };
        let mut refreshed = false;
        let mut waited = false;
        loop {
            let token = self.access_token(false).await?;
            let response = self.http.get(&url).bearer_auth(&token).send().await.map_err(err)?;
            let status = response.status();
            if status == reqwest::StatusCode::UNAUTHORIZED && !refreshed {
                refreshed = true;
                self.access_token(true).await?;
                continue;
            }
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS && !waited {
                waited = true;
                let wait = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(2)
                    .min(10);
                tokio::time::sleep(Duration::from_secs(wait)).await;
                continue;
            }
            if status == reqwest::StatusCode::FORBIDDEN {
                return Err("Spotify verweigert den Zugriff. Ist deine Spotify-E-Mail im Developer Dashboard unter „User Management“ eingetragen?".into());
            }
            if !status.is_success() {
                let text = response.text().await.unwrap_or_default();
                return Err(format!("Spotify antwortete mit {status}: {text}"));
            }
            return response.json().await.map_err(err);
        }
    }

    /// Lädt eine Seiten-Liste („items“ + „next“) bis `limit` Einträge.
    async fn paginate(&self, path: &str, limit: usize) -> Result<Vec<Value>, String> {
        let mut items = Vec::new();
        let mut next = Some(path.to_string());
        while let Some(url) = next {
            let page = self.get(&url).await?;
            if let Some(list) = page["items"].as_array() {
                items.extend(list.iter().cloned());
            }
            next = page["next"].as_str().map(str::to_string);
            if items.len() >= limit {
                break;
            }
        }
        items.truncate(limit);
        Ok(items)
    }
}

impl Spotify {
    /// Songliste einer öffentlichen Playlist aus dem Spotify-Embed-Player (ohne Anmeldung, max. 100 Songs).
    /// Album und Cover pro Song liefert der Embed nicht – dafür wird das Playlist-Cover genommen.
    async fn embed_playlist(&self, id: &str, cover: Option<String>) -> Result<Vec<SpTrack>, String> {
        let html = self
            .http
            .get(format!("https://open.spotify.com/embed/playlist/{id}"))
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36")
            .send()
            .await
            .map_err(err)?
            .text()
            .await
            .map_err(err)?;
        const MARKER: &str = "<script id=\"__NEXT_DATA__\" type=\"application/json\">";
        let start = html.find(MARKER).ok_or("Playlist ist nicht öffentlich")? + MARKER.len();
        let end = html[start..].find("</script>").ok_or("Embed unvollständig")? + start;
        let data: Value = serde_json::from_str(&html[start..end]).map_err(err)?;
        let list = data["props"]["pageProps"]["state"]["data"]["entity"]["trackList"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok(list
            .iter()
            .filter_map(|t| {
                let id = t["uri"].as_str()?.strip_prefix("spotify:track:")?.to_string();
                let artist = t["subtitle"].as_str().unwrap_or_default().to_string();
                Some(SpTrack {
                    url: Some(format!("https://open.spotify.com/track/{id}")),
                    id,
                    title: t["title"].as_str().unwrap_or_default().to_string(),
                    artists: artist
                        .split(", ")
                        .filter(|a| !a.is_empty())
                        .map(|a| SpArtistRef { id: None, name: a.to_string() })
                        .collect(),
                    artist: (!artist.is_empty()).then_some(artist),
                    album: None,
                    album_id: None,
                    duration: t["duration"].as_f64().unwrap_or(0.0) / 1000.0,
                    artwork_url: cover.clone(),
                })
            })
            .collect())
    }
}

// --- Umwandlung in die Formate der Oberfläche ----------------------------

fn best_image(images: &Value) -> Option<String> {
    images
        .as_array()?
        .iter()
        .max_by_key(|i| i["width"].as_u64().unwrap_or(0))
        .and_then(|i| i["url"].as_str())
        .map(str::to_string)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpArtistRef {
    id: Option<String>,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpTrack {
    id: String,
    title: String,
    artist: Option<String>,
    artists: Vec<SpArtistRef>,
    album: Option<String>,
    album_id: Option<String>,
    duration: f64,
    artwork_url: Option<String>,
    url: Option<String>,
}

/// Song aus der Web API; `album` ersetzt fehlende Albumdaten (bei Album-Songlisten)
fn to_track(t: &Value, album: Option<&Value>) -> Option<SpTrack> {
    if t["type"].as_str() != Some("track") || t["id"].is_null() {
        return None; // Podcast-Folgen, lokale Dateien u. Ä. überspringen
    }
    let album = if t["album"].is_object() { &t["album"] } else { album? };
    let artists: Vec<SpArtistRef> = t["artists"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|a| SpArtistRef {
                    id: a["id"].as_str().map(str::to_string),
                    name: a["name"].as_str().unwrap_or_default().to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    let artist = artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
    Some(SpTrack {
        id: t["id"].as_str()?.to_string(),
        title: t["name"].as_str().unwrap_or_default().to_string(),
        artist: (!artist.is_empty()).then_some(artist),
        artists,
        album: album["name"].as_str().map(str::to_string),
        album_id: album["id"].as_str().map(str::to_string),
        duration: t["duration_ms"].as_f64().unwrap_or(0.0) / 1000.0,
        artwork_url: best_image(&album["images"]),
        url: t["external_urls"]["spotify"].as_str().map(str::to_string),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpCollection {
    id: String,
    kind: &'static str,
    title: String,
    subtitle: Option<String>,
    artwork_url: Option<String>,
    editable: bool,
}

fn to_playlist(p: &Value) -> Option<SpCollection> {
    Some(SpCollection {
        id: p["id"].as_str()?.to_string(),
        kind: "playlist",
        title: p["name"].as_str().unwrap_or_default().to_string(),
        subtitle: p["owner"]["display_name"].as_str().map(str::to_string),
        artwork_url: best_image(&p["images"]),
        editable: false,
    })
}

fn to_album(a: &Value) -> Option<SpCollection> {
    let artist = a["artists"]
        .as_array()
        .map(|l| l.iter().filter_map(|x| x["name"].as_str()).collect::<Vec<_>>().join(", "))
        .unwrap_or_default();
    let year = a["release_date"].as_str().and_then(|d| d.get(..4));
    Some(SpCollection {
        id: a["id"].as_str()?.to_string(),
        kind: "album",
        title: a["name"].as_str().unwrap_or_default().to_string(),
        subtitle: match year {
            Some(y) if !artist.is_empty() => Some(format!("{artist} · {y}")),
            Some(y) => Some(y.to_string()),
            None => (!artist.is_empty()).then_some(artist),
        },
        artwork_url: best_image(&a["images"]),
        editable: false,
    })
}

// --- Anmeldung -----------------------------------------------------------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthEvent {
    logged_in: bool,
    error: Option<String>,
}

fn random_string(len: usize) -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..len).map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char).collect()
}

#[tauri::command]
pub async fn spotify_status(spotify: State<'_, Spotify>) -> Result<bool, String> {
    Ok(spotify.token.lock().await.is_some())
}

/// Öffnet die Spotify-Anmeldung in einem eigenen Fenster. Die Weiterleitung auf die Redirect-URI
/// wird abgefangen (kein lokaler Server nötig); Ergebnis kommt als Event `spotify-auth`.
#[tauri::command]
pub async fn spotify_login(app: AppHandle, spotify: State<'_, Spotify>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LOGIN_WINDOW) {
        let _ = window.set_focus();
        return Ok(());
    }
    let verifier = random_string(64);
    let state = random_string(16);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    *spotify.pending.lock().unwrap() = Some((verifier, state.clone()));

    let mut url = Url::parse("https://accounts.spotify.com/authorize").map_err(err)?;
    url.query_pairs_mut()
        .append_pair("client_id", CLIENT_ID)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("scope", SCOPES)
        .append_pair("state", &state)
        .append_pair("code_challenge_method", "S256")
        .append_pair("code_challenge", &challenge);

    let handle = app.clone();
    WebviewWindowBuilder::new(&app, LOGIN_WINDOW, WebviewUrl::External(url))
        .title("Bei Spotify anmelden")
        .inner_size(480.0, 720.0)
        .on_navigation(move |target| {
            if !target.as_str().starts_with(REDIRECT_URI) {
                return true;
            }
            let params: HashMap<String, String> = target.query_pairs().into_owned().collect();
            let app = handle.clone();
            tauri::async_runtime::spawn(async move {
                if let Some(window) = app.get_webview_window(LOGIN_WINDOW) {
                    let _ = window.close();
                }
                let result = finish_login(&app, params).await;
                let _ = app.emit(
                    "spotify-auth",
                    AuthEvent { logged_in: result.is_ok(), error: result.err() },
                );
            });
            false // Weiterleitung nicht laden – es läuft kein Server unter 127.0.0.1
        })
        .build()
        .map_err(err)?;
    Ok(())
}

async fn finish_login(app: &AppHandle, params: HashMap<String, String>) -> Result<(), String> {
    let spotify = app.state::<Spotify>();
    if let Some(error) = params.get("error") {
        return Err(if error == "access_denied" { "Anmeldung abgebrochen".into() } else { error.clone() });
    }
    let (verifier, state) = spotify.pending.lock().unwrap().take().ok_or("Keine laufende Anmeldung")?;
    if params.get("state") != Some(&state) {
        return Err("Ungültige Antwort von Spotify".into());
    }
    let code = params.get("code").ok_or("Kein Code von Spotify erhalten")?;
    let token = spotify
        .request_token(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT_URI),
            ("client_id", CLIENT_ID),
            ("code_verifier", &verifier),
        ])
        .await?;
    spotify.save_token(Some(&token));
    *spotify.token.lock().await = Some(token);
    Ok(())
}

#[tauri::command]
pub async fn spotify_logout(spotify: State<'_, Spotify>) -> Result<(), String> {
    *spotify.token.lock().await = None;
    spotify.save_token(None);
    Ok(())
}

// --- Daten ---------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpAccount {
    name: Option<String>,
    photo_url: Option<String>,
    product: Option<String>,
}

#[tauri::command]
pub async fn spotify_account(spotify: State<'_, Spotify>) -> Result<SpAccount, String> {
    let me = spotify.get("/me").await?;
    Ok(SpAccount {
        name: me["display_name"].as_str().or(me["id"].as_str()).map(str::to_string),
        photo_url: best_image(&me["images"]),
        product: me["product"].as_str().map(str::to_string),
    })
}

#[derive(Serialize)]
pub struct SpLibrary {
    playlists: Vec<SpCollection>,
    albums: Vec<SpCollection>,
}

#[tauri::command]
pub async fn spotify_library(spotify: State<'_, Spotify>) -> Result<SpLibrary, String> {
    let (playlists, albums) = tokio::join!(
        spotify.paginate("/me/playlists?limit=50", 500),
        spotify.paginate("/me/albums?limit=50", 500),
    );
    Ok(SpLibrary {
        playlists: playlists?.iter().filter_map(to_playlist).collect(),
        albums: albums?.iter().filter_map(|i| to_album(&i["album"])).collect(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpCollectionDetail {
    #[serde(flatten)]
    info: SpCollection,
    tracks: Vec<SpTrack>,
}

/// Lieblingssongs (`id = "liked"`), Playlist oder Album mit allen Songs
#[tauri::command]
pub async fn spotify_collection(
    spotify: State<'_, Spotify>,
    kind: String,
    id: String,
) -> Result<SpCollectionDetail, String> {
    if id == "liked" {
        let items = spotify.paginate("/me/tracks?limit=50", MAX_ITEMS).await?;
        return Ok(SpCollectionDetail {
            info: SpCollection {
                id,
                kind: "playlist",
                title: "Lieblingssongs".into(),
                subtitle: Some("Spotify".into()),
                artwork_url: None,
                editable: false,
            },
            tracks: items.iter().filter_map(|i| to_track(item_of(i), None)).collect(),
        });
    }

    if kind == "album" {
        let album = spotify.get(&format!("/albums/{id}")).await?;
        let list = if album["tracks"].is_object() { &album["tracks"] } else { &album["items"] };
        let mut tracks: Vec<Value> = list["items"].as_array().cloned().unwrap_or_default();
        if let Some(next) = list["next"].as_str() {
            tracks.extend(spotify.paginate(next, MAX_ITEMS).await?);
        }
        return Ok(SpCollectionDetail {
            info: to_album(&album).ok_or("Album nicht gefunden")?,
            tracks: tracks.iter().filter_map(|t| to_track(t, Some(&album))).collect(),
        });
    }

    let playlist = spotify.get(&format!("/playlists/{id}")).await?;
    let info = to_playlist(&playlist).ok_or("Playlist nicht gefunden")?;

    // Seit Februar 2026 gibt Spotify neuen Apps die Songs nur bei eigenen oder gemeinsamen Playlists.
    let own = spotify
        .paginate(&format!("/playlists/{id}/items?limit=50"), MAX_ITEMS)
        .await
        .map(|items| items.iter().filter_map(|i| to_track(item_of(i), None)).collect::<Vec<_>>())
        .unwrap_or_default();
    if !own.is_empty() {
        return Ok(SpCollectionDetail { info, tracks: own });
    }
    // Fremde Playlists: Songliste aus dem öffentlichen Spotify-Embed-Player (max. 100 Songs)
    let tracks = spotify.embed_playlist(&id, info.artwork_url.clone()).await?;
    Ok(SpCollectionDetail { info, tracks })
}

/// Eintrag einer Playlist/Liste: neues Feld „item“, früher „track“
fn item_of(entry: &Value) -> &Value {
    if entry["item"].is_object() { &entry["item"] } else { &entry["track"] }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpArtistCard {
    id: String,
    name: String,
    image_url: Option<String>,
}

#[derive(Serialize)]
pub struct SpSearch {
    tracks: Vec<SpTrack>,
    albums: Vec<SpCollection>,
    playlists: Vec<SpCollection>,
    artists: Vec<SpArtistCard>,
}

fn to_artist_card(a: &Value) -> Option<SpArtistCard> {
    Some(SpArtistCard {
        id: a["id"].as_str()?.to_string(),
        name: a["name"].as_str().unwrap_or_default().to_string(),
        image_url: best_image(&a["images"]),
    })
}

#[tauri::command]
pub async fn spotify_search(spotify: State<'_, Spotify>, query: String) -> Result<SpSearch, String> {
    let mut url = Url::parse(&format!("{API}/search")).map_err(err)?;
    url.query_pairs_mut()
        .append_pair("q", &query)
        .append_pair("type", "track,album,playlist,artist")
        .append_pair("limit", "10"); // Spotify erlaubt neuen Apps max. 10
    let r = spotify.get(url.as_str()).await?;
    let list = |key: &str| r[key]["items"].as_array().cloned().unwrap_or_default();
    Ok(SpSearch {
        tracks: list("tracks").iter().filter_map(|t| to_track(t, None)).collect(),
        albums: list("albums").iter().filter_map(to_album).collect(),
        playlists: list("playlists").iter().filter_map(to_playlist).collect(),
        artists: list("artists").iter().filter_map(to_artist_card).collect(),
    })
}

/// Künstlerseite im selben Format wie bei YouTube Music (für die gemeinsame Ansicht)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpArtist {
    id: String,
    name: String,
    image_url: Option<String>,
    description: Option<String>,
    subscribers: Option<u64>,
    tracks: Vec<SpTrack>,
    albums: Vec<SpCollection>,
    singles: Vec<SpCollection>,
    playlists: Vec<SpCollection>,
    similar: Vec<SpArtistCard>,
}

#[tauri::command]
pub async fn spotify_artist(spotify: State<'_, Spotify>, id: String) -> Result<SpArtist, String> {
    let artist_path = format!("/artists/{id}");
    // Spotify erlaubt neuen Apps hier nur noch kleine Seiten
    let albums_path = format!("/artists/{id}/albums?include_groups=album,single&limit=10");
    let (artist, albums) = tokio::join!(spotify.get(&artist_path), spotify.paginate(&albums_path, 200));
    let artist = artist?;
    // „Top-Songs“ hat Spotify für neue Apps entfernt → beliebte Songs über die Suche nach dem Künstler
    let name = artist["name"].as_str().unwrap_or_default().to_string();
    let mut search = Url::parse(&format!("{API}/search")).map_err(err)?;
    search
        .query_pairs_mut()
        .append_pair("q", &format!("artist:\"{name}\""))
        .append_pair("type", "track")
        .append_pair("limit", "10");
    let top = spotify.get(search.as_str()).await;
    // Zweite Seite für mehr Songs (Suche liefert max. 10 pro Anfrage)
    search.query_pairs_mut().append_pair("offset", "10");
    let top2 = spotify.get(search.as_str()).await;
    let albums = albums.unwrap_or_default();
    let group = |g: &str| -> Vec<SpCollection> {
        albums
            .iter()
            .filter(|a| a["album_group"].as_str().or(a["album_type"].as_str()) == Some(g))
            .filter_map(to_album)
            .collect()
    };
    let genres = artist["genres"]
        .as_array()
        .map(|g| g.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "))
        .filter(|g| !g.is_empty());
    Ok(SpArtist {
        id: id.clone(),
        name,
        image_url: best_image(&artist["images"]),
        description: genres.map(|g| format!("Genres: {g}")),
        subscribers: artist["followers"]["total"].as_u64(),
        tracks: [top, top2]
            .into_iter()
            .filter_map(|r| r.ok())
            .flat_map(|t| t["tracks"]["items"].as_array().cloned().unwrap_or_default())
            .collect::<Vec<_>>()
            .iter()
            .filter(|t| t["artists"].as_array().is_some_and(|a| a.iter().any(|x| x["id"].as_str() == Some(id.as_str()))))
            .filter_map(|t| to_track(t, None))
            .collect(),
        albums: group("album"),
        singles: group("single"),
        playlists: Vec::new(),
        // „Ähnliche Künstler“ bietet Spotify für neue Apps nicht mehr an
        similar: Vec::new(),
    })
}

/// Zuletzt gehört (Spotify-Verlauf)
#[tauri::command]
pub async fn spotify_recent(spotify: State<'_, Spotify>) -> Result<Vec<SpTrack>, String> {
    let r = spotify.get("/me/player/recently-played?limit=50").await?;
    Ok(r["items"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|i| to_track(&i["track"], None))
        .collect())
}

// --- Abspielen über YouTube Music ----------------------------------------

/// Kleinbuchstaben, Klammerzusätze („(feat. …)“, „- Remastered 2011“) entfernt – für den Titelvergleich
fn normalize(title: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in title.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let out = out.split(" - ").next().unwrap_or(&out);
    out.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()
}

/// Sucht den passenden YouTube-Music-Song zu einem Spotify-Song und gibt die Video-ID zurück.
#[tauri::command]
pub async fn spotify_resolve(
    spotify: State<'_, Spotify>,
    ytm: State<'_, Ytm>,
    id: String,
    title: String,
    artist: Option<String>,
    duration: f64,
) -> Result<String, String> {
    resolve(&spotify, &ytm, id, title, artist, duration).await
}

pub async fn resolve(
    spotify: &Spotify,
    ytm: &Ytm,
    id: String,
    title: String,
    artist: Option<String>,
    duration: f64,
) -> Result<String, String> {
    if let Some(video) = spotify.matches.lock().unwrap().get(&id) {
        return Ok(video.clone());
    }
    let first_artist = artist.as_deref().unwrap_or("").split(',').next().unwrap_or("").trim().to_string();
    let query = format!("{first_artist} {title}");
    let result = ytm.rp().query().music_search_tracks(&query).await.map_err(err)?;
    let wanted_title = normalize(&title);
    let wanted_artist = first_artist.to_lowercase();

    // Bewertung: gleiche Länge (wichtigstes Merkmal), gleicher Titel, gleicher Künstler
    let best = result
        .items
        .items
        .iter()
        .take(10)
        .map(|t| {
            let diff = t.duration.map(|d| (d as f64 - duration).abs()).unwrap_or(30.0);
            let mut score = -diff.min(60.0);
            if normalize(&t.name) == wanted_title {
                score += 20.0;
            } else if normalize(&t.name).contains(&wanted_title) || wanted_title.contains(&normalize(&t.name)) {
                score += 8.0;
            }
            if t.artists.iter().any(|a| a.name.to_lowercase() == wanted_artist) {
                score += 15.0;
            }
            (score, t)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, t)| t.id.clone())
        .ok_or("Kein passender Song bei YouTube Music gefunden")?;

    let mut matches = spotify.matches.lock().unwrap();
    matches.insert(id, best.clone());
    if let Ok(json) = serde_json::to_string(&*matches) {
        let _ = std::fs::write(spotify.dir.join("matches.json"), json);
    }
    Ok(best)
}
