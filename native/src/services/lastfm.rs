//! Last.fm: Anmeldung und Scrobbeln (gehörte Songs ins Last.fm-Profil eintragen).
//!
//! API-Schlüssel und Secret kommen beim Bauen aus Umgebungsvariablen (siehe build.rs,
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
use url::Url;

const API_KEY: Option<&str> = option_env!("LASTFM_API_KEY");
const API_SECRET: Option<&str> = option_env!("LASTFM_API_SECRET");
const API: &str = "https://ws.audioscrobbler.com/2.0/";
/// Muss so als Callback URL bei der Last.fm-API-App eingetragen sein (kleiner Server nimmt sie entgegen)
const CALLBACK: &str = "http://127.0.0.1:43828/lastfm";
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
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    /// Sekunden
    pub duration: Option<u64>,
    /// Startzeit (Unix-Sekunden)
    pub timestamp: u64,
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
    pub fn new(paths: &crate::host::Paths) -> Result<Self, String> {
        let dir = paths.data.join("lastfm");
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
    pub available: bool,
    pub user: Option<String>,
    pub pending: usize,
}

pub async fn lastfm_status(lastfm: &LastFm) -> Result<LastFmStatus, String> {
    Ok(LastFmStatus {
        available: keys().is_ok(),
        user: lastfm.session.lock().unwrap().as_ref().map(|s| s.name.clone()),
        pending: lastfm.queue.lock().unwrap().len(),
    })
}

/// Last.fm-Anmeldung im Standardbrowser; ein kleiner Server nimmt die Weiterleitung mit dem Token an.
pub async fn lastfm_login(lastfm: &LastFm) -> Result<(), String> {
    let (key, _) = keys()?;
    let mut url = Url::parse("https://www.last.fm/api/auth/").map_err(err)?;
    url.query_pairs_mut().append_pair("api_key", key).append_pair("cb", CALLBACK);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:43828").await.map_err(|e| format!("Anmeldung nicht möglich: {e}"))?;
    let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url.as_str()]).spawn();
    let wait = async {
        loop {
            let (mut socket, _) = listener.accept().await.map_err(err)?;
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = vec![0u8; 8192];
            let n = socket.read(&mut buf).await.map_err(err)?;
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
            if !path.starts_with("/lastfm") {
                let _ = socket.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n").await;
                continue;
            }
            let full = Url::parse(&format!("http://127.0.0.1{path}")).map_err(err)?;
            let params: HashMap<String, String> = full.query_pairs().into_owned().collect();
            let result = finish_login(lastfm, params.get("token").cloned()).await;
            let text = match &result {
                Ok(()) => "Mit Last.fm verbunden – du kannst dieses Fenster schließen.".to_string(),
                Err(e) => format!("Verbinden fehlgeschlagen: {e}"),
            };
            let body = format!("<!doctype html><meta charset=utf-8><title>Wavely</title><body style=\"font-family:Segoe UI;background:#0e0e11;color:#f3f3f6;display:grid;place-items:center;height:90vh\"><p>{text}</p>");
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{body}", body.len());
            let _ = socket.write_all(response.as_bytes()).await;
            return result;
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(600), wait).await.map_err(|_| "Anmeldung abgelaufen".to_string())?
}

async fn finish_login(lastfm: &LastFm, token: Option<String>) -> Result<(), String> {
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

pub async fn lastfm_logout(lastfm: &LastFm) -> Result<(), String> {
    *lastfm.session.lock().unwrap() = None;
    let _ = std::fs::remove_file(lastfm.dir.join("session.json"));
    Ok(())
}

/// „Hört gerade“ im Last.fm-Profil (Fehler sind egal)
pub async fn lastfm_now_playing(lastfm: &LastFm, track: Scrobble) -> Result<(), String> {
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
pub async fn lastfm_scrobble(lastfm: &LastFm, track: Scrobble) -> Result<usize, String> {
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

/// Stand ohne Warten (für die Einstellungsseite)
pub fn lastfm_status_now(lastfm: &LastFm) -> LastFmStatus {
    LastFmStatus {
        available: keys().is_ok(),
        user: lastfm.session.lock().unwrap().as_ref().map(|s| s.name.clone()),
        pending: lastfm.queue.lock().unwrap().len(),
    }
}

/// Beim Start: liegengebliebene Scrobbles nachreichen
pub async fn flush_pending(lastfm: &LastFm) {
    let _ = lastfm.flush().await;
}
