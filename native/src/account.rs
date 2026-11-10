//! Wavely-Konto über die REST-Schnittstellen von Firebase (statt Firebase-JS):
//! Anmeldung mit E-Mail und Passwort (Identity Toolkit), Daten in Firestore.
//!  users/{uid}/plays/{key}     – jedes Hören (Verlauf + Rückblick), nur hinzufügen
//!  users/{uid}/playlists/{id}  – Wavely-Playlists; die neueste Änderung gewinnt, Löschen als Markierung
//!  admins/{uid}                – Admins (bekommen Testversionen)
//! Statt Live-Abfragen wird regelmäßig nachgesehen (alle 60 Sekunden).

use std::{
    collections::HashMap,
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{playlists::Playlist, store::now_ms, App};

/// Öffentliche Werte des Wavely-Firebase-Projekts (wie in firebase.js; geschützt durch die Firestore-Regeln)
const API_KEY: &str = "AIzaSyD1Tu2lqe2vUPmUtWIUttDUfmXwwtlACEg";
const PROJECT: &str = "wavely-cf217";

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Session {
    pub uid: String,
    pub email: String,
    pub name: String,
    pub refresh_token: String,
    /// Kennung dieses Geräts (eigene Einträge nicht doppelt herunterladen)
    pub device: String,
    /// Bis wann Hör-Einträge schon heruntergeladen sind (ms)
    pub downloaded_at: u64,
    /// Bisherige Hörstatistik dieses PCs schon hochgeladen
    pub uploaded: bool,
}

pub struct Account {
    http: reqwest::Client,
    file: std::path::PathBuf,
    pub session: std::sync::Mutex<Option<Session>>,
    token: tokio::sync::Mutex<Option<(String, Instant)>>,
    /// Zuletzt hoch- oder heruntergeladener Stand je Playlist (updatedAt)
    known: std::sync::Mutex<HashMap<String, u64>>,
    pub admin: std::sync::atomic::AtomicBool,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Firebase-Fehlercodes in verständliche Sätze (wie errorText in account.svelte.js)
fn error_text(code: &str) -> String {
    let code = code.split(' ').next().unwrap_or(code);
    match code {
        "INVALID_EMAIL" => "Diese E-Mail-Adresse ist ungültig.".into(),
        "MISSING_PASSWORD" => "Bitte ein Passwort eingeben.".into(),
        "WEAK_PASSWORD" => "Das Passwort muss mindestens 6 Zeichen haben.".into(),
        "EMAIL_EXISTS" => "Mit dieser E-Mail gibt es schon ein Konto – melde dich an.".into(),
        "INVALID_LOGIN_CREDENTIALS" | "INVALID_PASSWORD" => "E-Mail oder Passwort stimmt nicht.".into(),
        "EMAIL_NOT_FOUND" => "Zu dieser E-Mail gibt es kein Konto.".into(),
        "TOO_MANY_ATTEMPTS_TRY_LATER" => "Zu viele Versuche – bitte kurz warten.".into(),
        "USER_DISABLED" => "Dieses Konto ist gesperrt.".into(),
        other => other.to_string(),
    }
}

impl Account {
    pub fn new(paths: &crate::host::Paths) -> Self {
        let file = paths.data.join("account.json");
        let session = std::fs::read_to_string(&file).ok().and_then(|s| serde_json::from_str(&s).ok());
        Self {
            http: reqwest::Client::new(),
            file,
            session: std::sync::Mutex::new(session),
            token: tokio::sync::Mutex::new(None),
            known: Default::default(),
            admin: Default::default(),
        }
    }

    fn save(&self) {
        let session = self.session.lock().unwrap().clone();
        match session {
            Some(s) => {
                if let Ok(json) = serde_json::to_string(&s) {
                    let _ = std::fs::write(&self.file, json);
                }
            }
            None => {
                let _ = std::fs::remove_file(&self.file);
            }
        }
    }

    pub fn logged_in(&self) -> Option<Session> {
        self.session.lock().unwrap().clone()
    }

    async fn auth_request(&self, endpoint: &str, body: Value) -> Result<Value, String> {
        let response = self
            .http
            .post(format!("https://identitytoolkit.googleapis.com/v1/accounts:{endpoint}?key={API_KEY}"))
            .json(&body)
            .send()
            .await
            .map_err(|_| "Keine Verbindung – bist du online?".to_string())?;
        let ok = response.status().is_success();
        let value: Value = response.json().await.map_err(err)?;
        if !ok {
            return Err(error_text(value["error"]["message"].as_str().unwrap_or("Anmeldung fehlgeschlagen")));
        }
        Ok(value)
    }

    fn start_session(&self, value: &Value) -> Session {
        let previous_device = self.session.lock().unwrap().as_ref().map(|s| s.device.clone());
        let email = value["email"].as_str().unwrap_or("").to_string();
        let name = value["displayName"].as_str().filter(|n| !n.is_empty()).map(str::to_string)
            .unwrap_or_else(|| email.split('@').next().unwrap_or("").to_string());
        let session = Session {
            uid: value["localId"].as_str().unwrap_or("").into(),
            email,
            name,
            refresh_token: value["refreshToken"].as_str().unwrap_or("").into(),
            device: previous_device.unwrap_or_else(crate::playlists::new_id),
            downloaded_at: 0,
            uploaded: false,
        };
        *self.session.lock().unwrap() = Some(session.clone());
        self.save();
        session
    }

    pub async fn login(&self, email: &str, password: &str) -> Result<Session, String> {
        let value = self
            .auth_request("signInWithPassword", json!({ "email": email.trim(), "password": password, "returnSecureToken": true }))
            .await?;
        let id_token = value["idToken"].as_str().unwrap_or("").to_string();
        *self.token.lock().await = Some((id_token, Instant::now()));
        Ok(self.start_session(&value))
    }

    pub async fn register(&self, name: &str, email: &str, password: &str) -> Result<Session, String> {
        let value = self
            .auth_request("signUp", json!({ "email": email.trim(), "password": password, "returnSecureToken": true }))
            .await?;
        let id_token = value["idToken"].as_str().unwrap_or("").to_string();
        if !name.trim().is_empty() {
            let _ = self.auth_request("update", json!({ "idToken": id_token, "displayName": name.trim(), "returnSecureToken": false })).await;
        }
        *self.token.lock().await = Some((id_token, Instant::now()));
        let mut value = value;
        if !name.trim().is_empty() {
            value["displayName"] = json!(name.trim());
        }
        Ok(self.start_session(&value))
    }

    /// Konto und alle Daten in der Cloud löschen (wie deleteAccount): erst mit dem Passwort neu
    /// anmelden (Bestätigung), dann Hörverlauf und Playlists in Firestore, zuletzt das Konto selbst.
    /// Was auf diesem PC gespeichert ist, bleibt.
    pub async fn delete_account(&self, password: &str) -> Result<(), String> {
        let session = self.logged_in().ok_or("Nicht angemeldet")?;
        let value = self
            .auth_request("signInWithPassword", json!({ "email": session.email, "password": password, "returnSecureToken": true }))
            .await?;
        let id_token = value["idToken"].as_str().unwrap_or("").to_string();
        *self.token.lock().await = Some((id_token.clone(), Instant::now()));
        for collection in ["plays", "playlists"] {
            let path = format!("users/{}/{collection}", session.uid);
            // Seitenweise (höchstens 300 je Abfrage), bis nichts mehr da ist
            for _ in 0..1000 {
                let names = self.list_names(&path).await?;
                if names.is_empty() {
                    break;
                }
                let writes = names.iter().map(|n| json!({ "delete": doc_name(&format!("{path}/{n}")) })).collect();
                self.commit(writes).await?;
            }
        }
        self.delete_doc(&format!("users/{}", session.uid)).await?;
        self.auth_request("delete", json!({ "idToken": id_token })).await?;
        self.logout();
        Ok(())
    }

    /// Profil-Dokument `users/{uid}` mit Name, Registrierung und „zuletzt aktiv“ anlegen bzw.
    /// auffrischen. Ohne eigenes Dokument (nur Unterordner plays/playlists) zeigt die
    /// Firebase-Konsole Nutzer nur grau/kursiv an. Keine E-Mail-Adresse im Dokument.
    pub async fn touch_profile(&self) -> Result<(), String> {
        let session = self.logged_in().ok_or("Nicht angemeldet")?;
        let path = format!("users/{}", session.uid);
        let mut data = json!({ "name": session.name, "lastActive": format_timestamp(now_ms() / 1000) });
        let existing = self.get_doc(&path).await?;
        if existing.as_ref().is_none_or(|d| d.get("createdAt").is_none()) {
            // Registrierungsdatum aus dem Konto (für Konten von vor diesem Dokument)
            let token = self.id_token().await?;
            let created = self
                .auth_request("lookup", json!({ "idToken": token }))
                .await
                .ok()
                .and_then(|v| v["users"][0]["createdAt"].as_str().and_then(|s| s.parse::<u64>().ok()))
                .map(|ms| ms / 1000)
                .unwrap_or(now_ms() / 1000);
            data["createdAt"] = json!(format_timestamp(created));
        }
        self.set_doc(&path, &data, true).await
    }

    /// Anzeigenamen ändern (wie updateProfile)
    pub async fn rename(&self, name: &str) -> Result<(), String> {
        let token = self.id_token().await?;
        self.auth_request("update", json!({ "idToken": token, "displayName": name.trim(), "returnSecureToken": false })).await?;
        if let Some(session) = self.session.lock().unwrap().as_mut() {
            session.name = name.trim().to_string();
        }
        self.save();
        Ok(())
    }

    pub async fn reset_password(&self, email: &str) -> Result<(), String> {
        self.auth_request("sendOobCode", json!({ "requestType": "PASSWORD_RESET", "email": email.trim() })).await.map(|_| ())
    }

    pub fn logout(&self) {
        *self.session.lock().unwrap() = None;
        self.known.lock().unwrap().clear();
        self.admin.store(false, Ordering::Relaxed);
        self.save();
    }

    /// Gültiges ID-Token (wird nach ~50 Minuten erneuert)
    async fn id_token(&self) -> Result<String, String> {
        let mut token = self.token.lock().await;
        if let Some((t, at)) = token.as_ref() {
            if at.elapsed() < Duration::from_secs(50 * 60) {
                return Ok(t.clone());
            }
        }
        let refresh = self.session.lock().unwrap().as_ref().map(|s| s.refresh_token.clone()).ok_or("Nicht angemeldet")?;
        let response = self
            .http
            .post(format!("https://securetoken.googleapis.com/v1/token?key={API_KEY}"))
            .form(&[("grant_type", "refresh_token"), ("refresh_token", refresh.as_str())])
            .send()
            .await
            .map_err(|_| "Keine Verbindung".to_string())?;
        let ok = response.status().is_success();
        let value: Value = response.json().await.map_err(err)?;
        if !ok {
            return Err(error_text(value["error"]["message"].as_str().unwrap_or("Anmeldung abgelaufen")));
        }
        let id = value["id_token"].as_str().unwrap_or("").to_string();
        if let Some(new_refresh) = value["refresh_token"].as_str() {
            if let Some(s) = self.session.lock().unwrap().as_mut() {
                s.refresh_token = new_refresh.to_string();
            }
            self.save();
        }
        *token = Some((id.clone(), Instant::now()));
        Ok(id)
    }

    fn docs_url(path: &str) -> String {
        format!("https://firestore.googleapis.com/v1/projects/{PROJECT}/databases/(default)/documents/{path}")
    }

    async fn get(&self, url: &str) -> Result<Value, String> {
        let token = self.id_token().await?;
        let response = self.http.get(url).bearer_auth(token).send().await.map_err(err)?;
        let status = response.status();
        let value: Value = response.json().await.unwrap_or(Value::Null);
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(Value::Null);
        }
        if !status.is_success() {
            return Err(firestore_error(&value, status));
        }
        Ok(value)
    }

    async fn post(&self, url: &str, body: Value) -> Result<Value, String> {
        let token = self.id_token().await?;
        let response = self.http.post(url).bearer_auth(token).json(&body).send().await.map_err(err)?;
        let status = response.status();
        let value: Value = response.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(firestore_error(&value, status));
        }
        Ok(value)
    }

    pub async fn check_admin(&self, uid: &str) -> bool {
        let found = self.get(&Self::docs_url(&format!("admins/{uid}"))).await.map(|v| !v.is_null()).unwrap_or(false);
        self.admin.store(found, Ordering::Relaxed);
        found
    }


    // --- Allgemeine Dokumente (Zusammen hören, Feedback) ------------------------------

    /// Dokument schreiben; `merge` = nur die angegebenen Felder ändern
    pub async fn set_doc(&self, path: &str, data: &Value, merge: bool) -> Result<(), String> {
        let mut write = json!({ "update": { "name": doc_name(path), "fields": to_fields(data) } });
        if merge {
            let fields: Vec<String> = data.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
            write["updateMask"] = json!({ "fieldPaths": fields });
        }
        self.commit(vec![write]).await
    }

    /// Dokument lesen (`None`, wenn es das nicht gibt)
    pub async fn get_doc(&self, path: &str) -> Result<Option<Value>, String> {
        let value = self.get(&Self::docs_url(path)).await?;
        Ok((!value.is_null()).then(|| from_fields(&value["fields"])))
    }

    /// Alle Dokumente einer Sammlung (Inhalte)
    pub async fn list_docs(&self, path: &str) -> Result<Vec<Value>, String> {
        let value = self.get(&format!("{}?pageSize=300", Self::docs_url(path))).await?;
        Ok(value["documents"].as_array().cloned().unwrap_or_default().iter().map(|d| from_fields(&d["fields"])).collect())
    }

    /// Alle Dokument-IDs einer Sammlung
    pub async fn list_names(&self, path: &str) -> Result<Vec<String>, String> {
        let value = self.get(&format!("{}?pageSize=300", Self::docs_url(path))).await?;
        Ok(value["documents"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|d| d["name"].as_str().and_then(|n| n.rsplit('/').next()).map(str::to_string))
            .collect())
    }

    pub async fn delete_doc(&self, path: &str) -> Result<(), String> {
        let token = self.id_token().await?;
        self.http.delete(Self::docs_url(path)).bearer_auth(token).send().await.map_err(err)?;
        Ok(())
    }

    /// Feedback schicken (geht auch ohne Konto; lesen kann es nur die Projekt-Inhaberin)
    pub async fn send_feedback(&self, kind: &str, text: &str, contact: &str, with_info: bool) -> Result<(), String> {
        let mut data = json!({
            "kind": kind,
            "text": text.trim().chars().take(5000).collect::<String>(),
            "contact": if contact.trim().is_empty() { Value::Null } else { json!(contact.trim().chars().take(200).collect::<String>()) },
            "uid": self.logged_in().map(|s| json!(s.uid)).unwrap_or(Value::Null),
        });
        if with_info {
            data["version"] = json!(concat!(env!("CARGO_PKG_VERSION"), " (nativ)"));
            data["system"] = json!("Windows");
        }
        let mut request = self
            .http
            .post(format!("{}?key={API_KEY}", Self::docs_url("feedback")))
            .json(&json!({ "fields": { "kind": to_value(&data["kind"]), "text": to_value(&data["text"]), "contact": to_value(&data["contact"]), "uid": to_value(&data["uid"]), "version": to_value(&data["version"]), "system": to_value(&data["system"]), "createdAt": { "timestampValue": format_timestamp(now_ms() / 1000) } } }));
        if self.logged_in().is_some() {
            if let Ok(token) = self.id_token().await {
                request = request.bearer_auth(token);
            }
        }
        let response = request.send().await.map_err(|_| "Keine Verbindung – bist du online?".to_string())?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("Feedback konnte nicht gesendet werden ({})", response.status()))
        }
    }

    /// Alle Playlists des Kontos: (Playlist, gelöscht)
    pub async fn fetch_playlists(&self, uid: &str) -> Result<Vec<(Playlist, bool)>, String> {
        let mut out = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut url = format!("{}?pageSize=300", Self::docs_url(&format!("users/{uid}/playlists")));
            if let Some(t) = &page_token {
                url.push_str(&format!("&pageToken={t}"));
            }
            let value = self.get(&url).await?;
            for doc in value["documents"].as_array().cloned().unwrap_or_default() {
                let id = doc["name"].as_str().unwrap_or("").rsplit('/').next().unwrap_or("").to_string();
                let data = from_fields(&doc["fields"]);
                let deleted = data["deleted"].as_bool().unwrap_or(false);
                let mut playlist: Playlist = serde_json::from_value(data).unwrap_or_default();
                playlist.id = id;
                self.known.lock().unwrap().insert(playlist.id.clone(), playlist.updated_at);
                out.push((playlist, deleted));
            }
            match value["nextPageToken"].as_str() {
                Some(t) => page_token = Some(t.to_string()),
                None => break,
            }
        }
        Ok(out)
    }

    /// Geänderte Playlists hochladen (nur was sich seit dem letzten Abgleich geändert hat)
    pub async fn push_playlists(&self, uid: &str, list: &[Playlist], deleted: &[String]) -> Result<(), String> {
        let mut writes = Vec::new();
        for p in list {
            if self.known.lock().unwrap().get(&p.id) == Some(&p.updated_at) {
                continue;
            }
            // Pfade lokaler Dateien bleiben auf dem PC
            let mut cloud = p.clone();
            for item in cloud.items.iter_mut() {
                if item.track.source == "local" {
                    item.track.path = None;
                }
            }
            let mut data = serde_json::to_value(&cloud).map_err(err)?;
            if let Some(obj) = data.as_object_mut() {
                obj.remove("id");
                obj.insert("deleted".into(), json!(false));
            }
            writes.push(json!({ "update": { "name": doc_name(&format!("users/{uid}/playlists/{}", p.id)), "fields": to_fields(&data) } }));
            self.known.lock().unwrap().insert(p.id.clone(), p.updated_at);
        }
        for id in deleted {
            let now = now_ms();
            let data = json!({ "deleted": true, "updatedAt": now, "items": [], "name": "" });
            writes.push(json!({ "update": { "name": doc_name(&format!("users/{uid}/playlists/{id}")), "fields": to_fields(&data) } }));
            self.known.lock().unwrap().insert(id.clone(), now);
        }
        self.commit(writes).await
    }

    async fn commit(&self, writes: Vec<Value>) -> Result<(), String> {
        for chunk in writes.chunks(400) {
            let url = format!("https://firestore.googleapis.com/v1/projects/{PROJECT}/databases/(default)/documents:commit");
            self.post(&url, json!({ "writes": chunk })).await?;
        }
        Ok(())
    }

    /// Hör-Einträge hochladen (mit Serverzeit „at“)
    pub async fn upload_plays(&self, uid: &str, device: &str, plays: &[Value]) -> Result<(), String> {
        let writes = plays
            .iter()
            .map(|p| {
                let mut entry = p.clone();
                if let Some(obj) = entry.as_object_mut() {
                    obj.insert("dev".into(), json!(device));
                    obj.remove("est");
                    // Lokale Pfade nicht in die Cloud – nur ein Hash
                    if obj.get("src").and_then(|v| v.as_str()) == Some("local") {
                        let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        obj.insert("id".into(), json!(fnv_hash(&id)));
                    }
                }
                json!({
                    "update": { "name": doc_name(&format!("users/{uid}/plays/{}", play_key(p))), "fields": to_fields(&entry) },
                    "updateTransforms": [{ "fieldPath": "at", "setToServerValue": "REQUEST_TIME" }]
                })
            })
            .collect();
        self.commit(writes).await
    }

    /// Hör-Einträge anderer Geräte seit `since` (ms)
    pub async fn fetch_plays(&self, uid: &str, since: u64, device: &str) -> Result<Vec<Value>, String> {
        let url = format!(
            "https://firestore.googleapis.com/v1/projects/{PROJECT}/databases/(default)/documents/users/{uid}:runQuery"
        );
        let seconds = since / 1000;
        let body = json!({
            "structuredQuery": {
                "from": [{ "collectionId": "plays" }],
                "where": { "fieldFilter": { "field": { "fieldPath": "at" }, "op": "GREATER_THAN",
                    "value": { "timestampValue": format_timestamp(seconds) } } },
                "orderBy": [{ "field": { "fieldPath": "at" } }]
            }
        });
        let value = self.post(&url, body).await?;
        Ok(value
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|r| r.get("document").cloned())
            .map(|d| from_fields(&d["fields"]))
            .filter(|p| p["dev"].as_str() != Some(device))
            .map(|mut p| {
                if let Some(obj) = p.as_object_mut() {
                    obj.remove("dev");
                    obj.remove("at");
                }
                p
            })
            .collect())
    }
}

fn firestore_error(value: &Value, status: reqwest::StatusCode) -> String {
    if status == reqwest::StatusCode::FORBIDDEN {
        return "Firebase verweigert den Zugriff".into();
    }
    value["error"]["message"].as_str().map(str::to_string).unwrap_or_else(|| format!("Firebase antwortete mit {status}"))
}

fn doc_name(path: &str) -> String {
    format!("projects/{PROJECT}/databases/(default)/documents/{path}")
}

/// Kurzer Hash (FNV-1a, wie sync.svelte.js) – lokale Song-IDs sind Pfade
pub fn fnv_hash(text: &str) -> String {
    let mut h: u32 = 0x811c9dc5;
    for unit in text.encode_utf16() {
        h ^= unit as u32;
        h = h.wrapping_mul(0x01000193);
    }
    to_base36(h)
}

fn to_base36(mut n: u32) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

pub fn play_key(p: &Value) -> String {
    let id = p["id"].as_str().map(str::to_string).unwrap_or_else(|| p["id"].to_string());
    format!("{}_{}", p["t"].as_u64().unwrap_or(0), fnv_hash(&format!("{}:{}", p["src"].as_str().unwrap_or(""), id)))
}

fn format_timestamp(seconds: u64) -> String {
    // RFC 3339 in UTC aus Unix-Sekunden
    let days = seconds / 86_400;
    let rem = seconds % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Tage → Datum (Algorithmus von Howard Hinnant)
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

// --- Firestore-Werte ↔ JSON ---------------------------------------------------

fn to_value(v: &Value) -> Value {
    match v {
        Value::Null => json!({ "nullValue": null }),
        Value::Bool(b) => json!({ "booleanValue": b }),
        Value::Number(n) if n.is_i64() || n.is_u64() => json!({ "integerValue": n.to_string() }),
        Value::Number(n) => json!({ "doubleValue": n.as_f64() }),
        Value::String(s) => json!({ "stringValue": s }),
        Value::Array(a) => json!({ "arrayValue": { "values": a.iter().map(to_value).collect::<Vec<_>>() } }),
        Value::Object(_) => json!({ "mapValue": { "fields": to_fields(v) } }),
    }
}

fn to_fields(v: &Value) -> Value {
    let mut fields = Map::new();
    if let Some(obj) = v.as_object() {
        for (k, val) in obj {
            fields.insert(k.clone(), to_value(val));
        }
    }
    Value::Object(fields)
}

fn from_value(v: &Value) -> Value {
    if let Some(s) = v.get("stringValue") {
        return s.clone();
    }
    if let Some(i) = v.get("integerValue") {
        return i.as_str().and_then(|s| s.parse::<i64>().ok()).map(|n| json!(n)).unwrap_or(Value::Null);
    }
    if let Some(d) = v.get("doubleValue") {
        return d.clone();
    }
    if let Some(b) = v.get("booleanValue") {
        return b.clone();
    }
    if let Some(t) = v.get("timestampValue") {
        return t.clone();
    }
    if let Some(a) = v.get("arrayValue") {
        return Value::Array(a["values"].as_array().map(|vals| vals.iter().map(from_value).collect()).unwrap_or_default());
    }
    if let Some(m) = v.get("mapValue") {
        return from_fields(&m["fields"]);
    }
    Value::Null
}

fn from_fields(fields: &Value) -> Value {
    let mut out = Map::new();
    if let Some(obj) = fields.as_object() {
        for (k, v) in obj {
            out.insert(k.clone(), from_value(v));
        }
    }
    Value::Object(out)
}

// --- Abgleich -------------------------------------------------------------------

impl App {
    /// Nach dem Start bzw. der Anmeldung: Playlists und Hören abgleichen, dann regelmäßig
    pub fn start_sync(self: &Arc<Self>) {
        let Some(session) = self.account.logged_in() else { return };
        let generation = self.sync_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let app = self.clone();
        self.rt.spawn(async move {
            app.account.check_admin(&session.uid).await;
            let _ = app.account.touch_profile().await;
            app.settings_changed();
            loop {
                if app.sync_generation.load(Ordering::SeqCst) != generation || app.account.logged_in().is_none() {
                    return;
                }
                let result = app.sync_once().await;
                *app.sync_status.lock().unwrap() = match &result {
                    Ok(()) => format!("Abgeglichen um {}", crate::wavely::clock_time()),
                    Err(e) => format!("Abgleich fehlgeschlagen: {e}"),
                };
                app.settings_changed();
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        });
    }

    async fn sync_once(self: &Arc<Self>) -> Result<(), String> {
        let Some(mut session) = self.account.logged_in() else { return Ok(()) };
        // Playlists: erst den Stand der Cloud übernehmen, dann Eigenes hochladen
        let remote = self.account.fetch_playlists(&session.uid).await?;
        let changed = self.playlists.lock().unwrap().apply_remote(remote);
        if changed {
            let a = self.clone();
            let _ = slint::invoke_from_event_loop(move || {
                a.playlists_changed_local();
                a.refresh_if_showing(|p| matches!(p, crate::nav::PageKind::Playlist(_) | crate::nav::PageKind::Home));
            });
        }
        let (list, deleted) = {
            let mut p = self.playlists.lock().unwrap();
            (p.list.clone(), std::mem::take(&mut p.deleted))
        };
        self.account.push_playlists(&session.uid, &list, &deleted).await?;

        // Hören: einmal alles hochladen, danach neue Einträge
        if !session.uploaded {
            let plays = self.store.load_plays(0);
            self.account.upload_plays(&session.uid, &session.device, &plays).await?;
            session.uploaded = true;
        } else {
            let pending = std::mem::take(&mut *self.plays_to_upload.lock().unwrap());
            if !pending.is_empty() {
                if let Err(e) = self.account.upload_plays(&session.uid, &session.device, &pending).await {
                    self.plays_to_upload.lock().unwrap().extend(pending);
                    return Err(e);
                }
            }
        }
        let started = now_ms();
        let foreign = self.account.fetch_plays(&session.uid, session.downloaded_at, &session.device).await?;
        if !foreign.is_empty() {
            self.take_remote_plays(foreign);
        }
        session.downloaded_at = started.saturating_sub(60_000);
        // Sitzung (Fortschritt) speichern, ohne das Token zu überschreiben
        if let Some(s) = self.account.session.lock().unwrap().as_mut() {
            s.downloaded_at = session.downloaded_at;
            s.uploaded = session.uploaded;
        }
        self.account.save();
        Ok(())
    }

    /// Einträge anderer Geräte in Statistik und Verlauf übernehmen
    fn take_remote_plays(self: &Arc<Self>, foreign: Vec<Value>) {
        let existing: std::collections::HashSet<String> = self.store.load_plays(0).iter().map(play_key).collect();
        let mut fresh: Vec<Value> = foreign.into_iter().filter(|p| !existing.contains(&play_key(p))).collect();
        if fresh.is_empty() {
            return;
        }
        fresh.sort_by_key(|p| p["t"].as_u64().unwrap_or(0));
        self.store.append_plays(&fresh);
        let history = {
            let mut h = self.history.lock().unwrap();
            let seen: std::collections::HashSet<String> = h.iter().map(|e| format!("{}:{}", e.played_at, e.track.id)).collect();
            for p in &fresh {
                let entry = crate::store::HistoryEntry {
                    played_at: p["t"].as_u64().unwrap_or(0),
                    track: crate::player::Track {
                        source: p["src"].as_str().unwrap_or("youtube").into(),
                        id: p["id"].as_str().map(str::to_string).unwrap_or_else(|| p["id"].to_string()),
                        title: p["title"].as_str().unwrap_or("").into(),
                        artist: p["artist"].as_str().map(str::to_string),
                        album: p["album"].as_str().map(str::to_string),
                        artwork_url: p["art"].as_str().map(str::to_string),
                        duration: p["d"].as_f64().unwrap_or(0.0),
                        ..Default::default()
                    },
                };
                if !seen.contains(&format!("{}:{}", entry.played_at, entry.track.id)) {
                    h.push(entry);
                }
            }
            h.sort_by(|a, b| b.played_at.cmp(&a.played_at));
            h.truncate(crate::store::MAX_HISTORY);
            h.clone()
        };
        self.store.save_history(&history);
    }

    pub fn account_login(self: &Arc<Self>, email: String, password: String, register_name: Option<String>) {
        *self.account_busy.lock().unwrap() = true;
        *self.account_error.lock().unwrap() = None;
        self.settings_changed();
        let app = self.clone();
        self.rt.spawn(async move {
            let result = match register_name {
                Some(name) => app.account.register(&name, &email, &password).await,
                None => app.account.login(&email, &password).await,
            };
            *app.account_busy.lock().unwrap() = false;
            match result {
                Ok(_) => {
                    app.toast("Angemeldet – Playlists und Verlauf werden abgeglichen", false);
                    app.start_sync();
                }
                Err(e) => *app.account_error.lock().unwrap() = Some(e),
            }
            app.settings_changed();
        });
    }

    pub fn account_logout(self: &Arc<Self>) {
        self.sync_generation.fetch_add(1, Ordering::SeqCst);
        self.account.logout();
        *self.sync_status.lock().unwrap() = String::new();
        self.settings_changed();
        self.toast("Abgemeldet", false);
    }

    /// Konto löschen (Passwort zur Bestätigung)
    pub fn account_delete(self: &Arc<Self>, password: String) {
        let app = self.clone();
        self.rt.spawn(async move {
            // Abgleich anhalten, damit nichts wieder hochgeladen wird
            let generation = app.sync_generation.fetch_add(1, Ordering::SeqCst);
            app.together_leave_async().await;
            match app.account.delete_account(&password).await {
                Ok(()) => {
                    *app.sync_status.lock().unwrap() = String::new();
                    app.toast("Konto und Cloud-Daten gelöscht", false);
                }
                Err(e) => {
                    app.toast(&format!("Konto konnte nicht gelöscht werden: {e}"), true);
                    // Abgleich wieder starten, wenn noch angemeldet
                    if app.account.logged_in().is_some() && app.sync_generation.load(Ordering::SeqCst) == generation + 1 {
                        app.start_sync();
                    }
                }
            }
            app.settings_changed();
        });
    }

    pub fn account_reset(self: &Arc<Self>, email: String) {
        let app = self.clone();
        self.rt.spawn(async move {
            match app.account.reset_password(&email).await {
                Ok(()) => app.toast("E-Mail zum Zurücksetzen ist unterwegs", false),
                Err(e) => app.toast(&e, true),
            }
        });
    }
}
