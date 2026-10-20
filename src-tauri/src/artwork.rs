//! Cover-Bilder von YouTube über Rust laden und auf der Festplatte zwischenspeichern.
//! Googles Bildserver sperren das Fenster mit 429, wenn viele Bilder gleichzeitig laden.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::PathBuf,
    time::Duration,
};

use percent_encoding::percent_decode_str;
use tauri::{
    http::{header, Response, StatusCode},
    AppHandle, Manager,
};
use tokio::sync::Semaphore;

/// Nur Bilder von YouTube/Google laden (kein offener Proxy)
const ALLOWED_HOSTS: &[&str] = &["googleusercontent.com", "ytimg.com", "ggpht.com", "gstatic.com"];
const MAX_PARALLEL: usize = 6;
const MAX_ATTEMPTS: u32 = 4;

pub struct Artwork {
    http: reqwest::Client,
    dir: PathBuf,
    limit: Semaphore,
}

impl Artwork {
    pub fn new(app: &AppHandle) -> Result<Self, String> {
        let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("artwork");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(Self {
            http: reqwest::Client::new(),
            dir,
            limit: Semaphore::new(MAX_PARALLEL),
        })
    }

    /// Beantwortet `artwork://<url-kodierte Bild-URL>`.
    pub async fn serve(&self, encoded: &str) -> Response<Vec<u8>> {
        let url = percent_decode_str(encoded).decode_utf8_lossy().into_owned();
        match self.load(&url).await {
            Ok(bytes) => Response::builder()
                .header(header::CONTENT_TYPE, sniff_mime(&bytes))
                .header(header::CACHE_CONTROL, "max-age=604800")
                .body(bytes)
                .unwrap(),
            Err(message) => Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(message.into_bytes())
                .unwrap(),
        }
    }

    /// Lädt ein Bild in den Cache (z. B. für Offline-Songs).
    pub async fn prefetch(&self, url: &str) {
        let _ = self.load(url).await;
    }

    async fn load(&self, url: &str) -> Result<Vec<u8>, String> {
        let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
        let host = parsed.host_str().unwrap_or("");
        let allowed = parsed.scheme() == "https"
            && ALLOWED_HOSTS.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")));
        if !allowed {
            return Err(format!("Bildquelle nicht erlaubt: {host}"));
        }

        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        let file = self.dir.join(format!("{:016x}", hasher.finish()));
        if let Ok(bytes) = tokio::fs::read(&file).await {
            return Ok(bytes);
        }

        let _permit = self.limit.acquire().await.map_err(|e| e.to_string())?;
        for attempt in 1..=MAX_ATTEMPTS {
            let response = self.http.get(parsed.clone()).send().await.map_err(|e| e.to_string())?;
            let status = response.status();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt < MAX_ATTEMPTS {
                tokio::time::sleep(Duration::from_millis(700 * attempt as u64)).await;
                continue;
            }
            if !status.is_success() {
                return Err(format!("Bild nicht verfügbar ({status})"));
            }
            let bytes = response.bytes().await.map_err(|e| e.to_string())?.to_vec();
            let _ = tokio::fs::write(&file, &bytes).await;
            return Ok(bytes);
        }
        Err("Bild nicht verfügbar".into())
    }
}

fn sniff_mime(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        [b'G', b'I', b'F', ..] => "image/gif",
        _ => "image/jpeg",
    }
}
