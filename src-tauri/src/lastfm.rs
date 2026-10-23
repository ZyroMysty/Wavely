//! Last.fm: Anmeldung und Scrobbeln (gehörte Songs ins Last.fm-Profil eintragen).
//!
//! API-Schlüssel und Secret kommen beim Bauen aus Umgebungsvariablen (siehe scripts/build.mjs,
//! ~/.tauri/lastfm.json) und stehen nicht im Repository. Ohne sie ist Last.fm in Wavely ausgeblendet.
//! Scrobbles, die wegen fehlendem Internet nicht ankommen, werden gespeichert und später nachgereicht.

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::Mutex,
};

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State, Url, WebviewUrl, WebviewWindowBuilder};

const API_KEY: Option<&str> = option_env!("LASTFM_API_KEY");
const API_SECRET: Option<&str> = option_env!("LASTFM_API_SECRET");
const API: &str = "https://ws.audioscrobbler.com/2.0/";
/// Muss so als Callback URL bei der Last.fm-API-App eingetragen sein (wird abgefangen, kein Server nötig)
const CALLBACK: &str = "http://127.0.0.1:43828/lastfm";
const LOGIN_WINDOW: &str = "lastfm-login";
/// Last.fm nimmt höchstens 50 Scrobbles pro Anfrage an
const BATCH: usize = 50;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Clone, Serialize, Deserialize)]
struct Session {
    key: String,
    name: String,
}

/// Ein gehörter Song (vom Player geschickt)
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scrobble {
    artist: String,
    title: String,
    album: Option<String>,
    /// Sekunden
    duration: Option<u64>,
    /// Startzeit (Unix-Sekunden)
    timestamp: u64,
}

pub struct LastFm {
    http: reqwest::Client,
    dir: PathBuf,
    session: Mutex<Option<Session>>,
    /// Noch nicht angekommene Scrobbles
    queue: Mutex<Vec<Scrobble>>,
    flushing: tokio::sync::Mutex<()>,
}

impl LastFm {
    pub fn new(app: &AppHandle) -> Result<Self, String> {
        let dir = app.path().app_data_dir().map_err(err)?.join("lastfm");
        std::fs::create_dir_all(&dir).map_err(err)?;
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
        Ok(Self {
            http: reqwest::Client::new(),
            session: Mutex::new(read("session.json").and_then(|s| serde_json::from_str(&s).ok())),
            queue: Mutex::new(read("queue.json").and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()),
            flushing: tokio::sync::Mutex::new(()),
            dir,
        })
    }

    fn save_queue(&self, queue: &[Scrobble]) {
        if let Ok(json) = serde_json::to_string(queue) {
            let _ = std::fs::write(self.dir.join("queue.json"), json);
        }
    }

    fn session_key(&self) -> Option<String> {
        self.session.lock().unwrap().as_ref().map(|s| s.key.clone())
    }

    /// Signierter API-Aufruf (POST): Parameter alphabetisch + Secret → MD5
    async fn call(&self, method: &str, mut params: BTreeMap<String, String>) -> Result<Value, String> {
        let (key, secret) = keys()?;
        params.insert("method".into(), method.into());
        params.insert("api_key".into(), key.into());
        let mut raw = String::new();
        for (k, v) in &params {
            raw.push_str(k);
            raw.push_str(v);
        }
        raw.push_str(secret);
        let sig = Md5::digest(raw.as_bytes()).iter().map(|b| format!("{b:02x}")).collect::<String>();
        params.insert("api_sig".into(), sig);
        params.insert("format".into(), "json".into());

        let response = self.http.post(API).form(&params).send().await.map_err(|e| format!("NETWORK: {e}"))?;
        let status = response.status();
        let data: Value = response.json().await.unwrap_or_default();
        if let Some(code) = data["error"].as_u64() {
            let message = data["message"].as_str().unwrap_or("unbekannter Fehler");
            // 9 = Sitzung ungültig (z. B. Zugriff in Last.fm entzogen)
            return Err(if code == 9 { "SESSION".into() } else { format!("Last.fm: {message}") });
        }
        if !status.is_success() {
            return Err(format!("NETWORK: Last.fm antwortete mit {status}"));
        }
        Ok(data)
    }

    /// Gespeicherte Scrobbles in Paketen nachreichen
    async fn flush(&self) -> Result<usize, String> {
        let _guard = self.flushing.lock().await;
        let Some(sk) = self.session_key() else { return Ok(0) };
        let mut sent = 0;
        loop {
            let batch: Vec<Scrobble> = self.queue.lock().unwrap().iter().take(BATCH).cloned().collect();
            if batch.is_empty() {
                return Ok(sent);
            }
            let mut params = BTreeMap::new();
            params.insert("sk".to_string(), sk.clone());
            for (i, s) in batch.iter().enumerate() {
                params.insert(format!("artist[{i}]"), s.artist.clone());
                params.insert(format!("track[{i}]"), s.title.clone());
                params.insert(format!("timestamp[{i}]"), s.timestamp.to_string());
                if let Some(album) = s.album.as_ref().filter(|a| !a.is_empty()) {
                    params.insert(format!("album[{i}]"), album.clone());
                }
                if let Some(duration) = s.duration {
                    params.insert(format!("duration[{i}]"), duration.to_string());
                }
            }
            match self.call("track.scrobble", params).await {
                Ok(_) => {
                    let mut queue = self.queue.lock().unwrap();
                    let n = batch.len().min(queue.len());
                    queue.drain(..n);
                    self.save_queue(&queue);
                    sent += n;
                }
                Err(e) if e == "SESSION" => {
                    *self.session.lock().unwrap() = None;
                    let _ = std::fs::remove_file(self.dir.join("session.json"));
                    return Err("Last.fm-Anmeldung ist abgelaufen – bitte neu verbinden".into());
                }
                // Ohne Internet bleiben sie in der Warteschlange
                Err(e) => return Err(e),
            }
        }
    }
}

fn keys() -> Result<(&'static str, &'static str), String> {
    match (API_KEY, API_SECRET) {
        (Some(k), Some(s)) if !k.is_empty() && !s.is_empty() => Ok((k, s)),
        _ => Err("Last.fm ist in dieser Wavely-Version nicht eingerichtet".into()),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastFmStatus {
    /// API-Schlüssel beim Bauen vorhanden
    available: bool,
    user: Option<String>,
    pending: usize,
}

#[tauri::command]
pub async fn lastfm_status(lastfm: State<'_, LastFm>) -> Result<LastFmStatus, String> {
    Ok(LastFmStatus {
        available: keys().is_ok(),
        user: lastfm.session.lock().unwrap().as_ref().map(|s| s.name.clone()),
        pending: lastfm.queue.lock().unwrap().len(),
    })
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthEvent {
    logged_in: bool,
    error: Option<String>,
}

/// Last.fm-Anmeldung im eigenen Fenster; die Weiterleitung mit dem Token wird abgefangen.
#[tauri::command]
pub async fn lastfm_login(app: AppHandle) -> Result<(), String> {
    let (key, _) = keys()?;
    if let Some(window) = app.get_webview_window(LOGIN_WINDOW) {
        let _ = window.set_focus();
        return Ok(());
    }
    let mut url = Url::parse("https://www.last.fm/api/auth/").map_err(err)?;
    url.query_pairs_mut().append_pair("api_key", key).append_pair("cb", CALLBACK);
    let handle = app.clone();
    WebviewWindowBuilder::new(&app, LOGIN_WINDOW, WebviewUrl::External(url))
        .title("Mit Last.fm verbinden")
        .inner_size(520.0, 760.0)
        .on_navigation(move |target| {
            if !target.as_str().starts_with(CALLBACK) {
                return true;
            }
            let params: HashMap<String, String> = target.query_pairs().into_owned().collect();
            let app = handle.clone();
            tauri::async_runtime::spawn(async move {
                if let Some(window) = app.get_webview_window(LOGIN_WINDOW) {
                    let _ = window.close();
                }
                let result = finish_login(&app, params.get("token").cloned()).await;
                let _ = app.emit("lastfm-auth", AuthEvent { logged_in: result.is_ok(), error: result.err() });
            });
            false
        })
        .build()
        .map_err(err)?;
    Ok(())
}

async fn finish_login(app: &AppHandle, token: Option<String>) -> Result<(), String> {
    let lastfm = app.state::<LastFm>();
    let token = token.ok_or("Anmeldung abgebrochen")?;
    let mut params = BTreeMap::new();
    params.insert("token".to_string(), token);
    let data = lastfm.call("auth.getSession", params).await?;
    let session = Session {
        key: data["session"]["key"].as_str().ok_or("Keine Sitzung von Last.fm erhalten")?.to_string(),
        name: data["session"]["name"].as_str().unwrap_or_default().to_string(),
    };
    let _ = std::fs::write(lastfm.dir.join("session.json"), serde_json::to_string(&session).map_err(err)?);
    *lastfm.session.lock().unwrap() = Some(session);
    let _ = lastfm.flush().await;
    Ok(())
}

#[tauri::command]
pub async fn lastfm_logout(lastfm: State<'_, LastFm>) -> Result<(), String> {
    *lastfm.session.lock().unwrap() = None;
    let _ = std::fs::remove_file(lastfm.dir.join("session.json"));
    Ok(())
}

/// „Hört gerade“ im Last.fm-Profil (Fehler sind egal)
#[tauri::command]
pub async fn lastfm_now_playing(lastfm: State<'_, LastFm>, track: Scrobble) -> Result<(), String> {
    let Some(sk) = lastfm.session_key() else { return Ok(()) };
    let mut params = BTreeMap::new();
    params.insert("sk".to_string(), sk);
    params.insert("artist".to_string(), track.artist);
    params.insert("track".to_string(), track.title);
    if let Some(album) = track.album.filter(|a| !a.is_empty()) {
        params.insert("album".to_string(), album);
    }
    if let Some(duration) = track.duration {
        params.insert("duration".to_string(), duration.to_string());
    }
    lastfm.call("track.updateNowPlaying", params).await.map(|_| ())
}

/// Song scrobbeln; ohne Internet wird er gespeichert und später nachgereicht.
#[tauri::command]
pub async fn lastfm_scrobble(lastfm: State<'_, LastFm>, track: Scrobble) -> Result<usize, String> {
    if lastfm.session_key().is_none() {
        return Ok(0);
    }
    {
        let mut queue = lastfm.queue.lock().unwrap();
        queue.push(track);
        lastfm.save_queue(&queue);
    }
    match lastfm.flush().await {
        Ok(n) => Ok(n),
        Err(e) if e.starts_with("NETWORK") => Ok(0),
        Err(e) => Err(e),
    }
}

/// Beim Start: liegengebliebene Scrobbles nachreichen
pub async fn flush_pending(app: &AppHandle) {
    let _ = app.state::<LastFm>().flush().await;
}
