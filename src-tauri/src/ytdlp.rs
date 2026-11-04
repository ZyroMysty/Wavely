//! yt-dlp als Hilfsprogramm: liefert die Audio-Stream-Adressen fÃ¼r YouTube Music.
//! Es liegt als Kopie im App-Datenordner, damit es sich selbst aktualisieren kann.

use std::{
    path::PathBuf,
    process::Stdio,
    time::{Duration, SystemTime},
};

use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tokio::process::Command;

/// Wie oft yt-dlp nach Updates sucht
const UPDATE_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// Unter Windows kein Konsolenfenster Ã¶ffnen
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// GewÃ¼nschte AudioqualitÃ¤t (Einstellungen â†’ Wiedergabe / Offline)
#[derive(Clone, Copy, PartialEq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    /// ~64 kbit Opus â€“ etwa halber Speicher, klingt dank Opus trotzdem ordentlich
    Low,
    /// Beste QualitÃ¤t ohne Premium (Opus/AAC ~130 kbit)
    #[default]
    High,
    /// 256 kbit AAC â€“ nur mit YouTube Premium (Anmelde-Cookies werden an yt-dlp Ã¼bergeben)
    Premium,
}

impl Quality {
    fn format(self, youtube: bool) -> &'static str {
        match (self, youtube) {
            (Quality::Low, _) => "bestaudio[abr<=80]/bestaudio",
            (Quality::Premium, true) => "bestaudio[abr>200]/bestaudio[ext=webm]/bestaudio",
            (_, true) => "bestaudio[ext=webm]/bestaudio",
            (_, false) => "bestaudio/best",
        }
    }
}

#[derive(Default)]
struct QualitySettings {
    stream: Quality,
    download: Quality,
    /// Cookie-Datei mit der YouTube-Anmeldung (nur fÃ¼r â€žPremiumâ€œ)
    cookies: Option<PathBuf>,
}

/// So lange wird â€žPremiumâ€œ nach einem Fehlschlag Ã¼bersprungen (spart den doppelten yt-dlp-Aufruf)
const PREMIUM_RETRY: Duration = Duration::from_secs(30 * 60);

#[derive(Clone)]
pub struct YtDlp {
    path: PathBuf,
    quality: std::sync::Arc<std::sync::Mutex<QualitySettings>>,
    /// Zeitpunkt, an dem â€žPremiumâ€œ zuletzt fehlgeschlagen ist
    premium_failed: std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>>,
}

pub struct AudioInfo {
    pub url: String,
    pub size: u64,
    pub mime: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Deserialize)]
struct Format {
    url: String,
    ext: Option<String>,
    filesize: Option<u64>,
    filesize_approx: Option<u64>,
    /// â€žhttpsâ€œ fÃ¼r direkte Dateien; â€žm3u8_nativeâ€œ & Co. kann der Player nicht stÃ¼ckweise laden
    protocol: Option<String>,
    #[serde(default)]
    http_headers: std::collections::HashMap<String, String>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl YtDlp {
    /// Kopiert das mitgelieferte yt-dlp beim ersten Start in den App-Datenordner.
    pub fn init(app: &AppHandle) -> Result<Self, String> {
        let dir = app.path().app_data_dir().map_err(err)?.join("bin");
        std::fs::create_dir_all(&dir).map_err(err)?;
        let path = dir.join("yt-dlp.exe");

        if !path.exists() {
            let bundled = std::env::current_exe()
                .map_err(err)?
                .parent()
                .ok_or("App-Ordner nicht gefunden")?
                .join("yt-dlp.exe");
            std::fs::copy(&bundled, &path)
                .map_err(|e| format!("yt-dlp nicht gefunden ({}): {e}", bundled.display()))?;
        }
        Ok(Self { path, quality: Default::default(), premium_failed: Default::default() })
    }

    pub fn set_quality(&self, stream: Quality, download: Quality, cookies: Option<PathBuf>) {
        *self.quality.lock().unwrap() = QualitySettings { stream, download, cookies };
        // Neue Einstellung oder neue Anmeldung: Premium wieder versuchen
        *self.premium_failed.lock().unwrap() = None;
    }

    /// Ob dieser Aufruf â€žPremiumâ€œ (mit Anmelde-Cookies) versuchen soll
    fn wants_premium(&self, download: bool, youtube: bool) -> bool {
        let settings = self.quality.lock().unwrap();
        let quality = if download { settings.download } else { settings.stream };
        let recently_failed = self.premium_failed.lock().unwrap().is_some_and(|t| t.elapsed() < PREMIUM_RETRY);
        quality == Quality::Premium && youtube && settings.cookies.is_some() && !recently_failed
    }

    /// Premium hat nicht geklappt: eine Weile ohne Anmeldung (â€žHochâ€œ) weitermachen
    pub fn mark_premium_failed(&self) {
        *self.premium_failed.lock().unwrap() = Some(std::time::Instant::now());
    }

    /// Formatauswahl (+ Cookies bei â€žPremiumâ€œ) fÃ¼r einen Aufruf. Ohne `premium` wird â€žPremiumâ€œ
    /// zu â€žHochâ€œ ohne Cookies â€“ das klappt auch, wenn YouTube die angemeldete Abfrage verweigert.
    fn quality_args(&self, download: bool, youtube: bool, premium: bool) -> Vec<String> {
        let settings = self.quality.lock().unwrap();
        let mut quality = if download { settings.download } else { settings.stream };
        if quality == Quality::Premium && !premium {
            quality = Quality::High;
        }
        let mut args = vec!["-f".to_string(), quality.format(youtube).to_string()];
        if let (Quality::Premium, true, Some(cookies)) = (quality, youtube, &settings.cookies) {
            args.push("--cookies".into());
            args.push(cookies.to_string_lossy().into_owned());
        }
        args
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.path);
        cmd.stdin(Stdio::null()).kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }

    /// Sucht hÃ¶chstens einmal am Tag nach einer neuen yt-dlp-Version.
    pub async fn update_if_due(&self) {
        let marker = self.path.with_extension("checked");
        let due = std::fs::metadata(&marker)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_none_or(|age| age > UPDATE_INTERVAL);
        if !due {
            return;
        }
        let ok = self
            .command()
            .arg("-U")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .is_ok_and(|s| s.success());
        if ok {
            let _ = std::fs::write(&marker, b"");
        }
    }

    /// LÃ¤dt ein YouTube-Video als Audiodatei nach `dir/<video_id>.<ext>` und gibt den Pfad zurÃ¼ck.
    pub async fn download(&self, video_id: &str, dir: &std::path::Path) -> Result<PathBuf, String> {
        let url = format!("https://music.youtube.com/watch?v={video_id}");
        self.download_url(&url, video_id, dir).await
    }

    /// LÃ¤dt eine beliebige von yt-dlp unterstÃ¼tzte Adresse (z. B. SoundCloud) nach `dir/<name>.<ext>`.
    pub async fn download_url(&self, url: &str, name: &str, dir: &std::path::Path) -> Result<PathBuf, String> {
        let youtube = url.contains("youtube.com");
        if self.wants_premium(true, youtube) {
            match self.try_download(url, name, dir, true).await {
                Ok(path) => return Ok(path),
                Err(_) => self.mark_premium_failed(),
            }
        }
        self.try_download(url, name, dir, false).await
    }

    async fn try_download(&self, url: &str, name: &str, dir: &std::path::Path, premium: bool) -> Result<PathBuf, String> {
        let template = dir.join(format!("{name}.%(ext)s"));
        let youtube = url.contains("youtube.com");
        let output = self
            .command()
            .args(self.quality_args(true, youtube, premium))
            .args([
                "--no-playlist",
                "--no-warnings",
                "--no-progress",
                "-q",
                "--print",
                "after_move:filepath",
                "-o",
            ])
            .arg(&template)
            .arg(url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(err)?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let path = stdout.lines().map(str::trim).rfind(|l| !l.is_empty()).map(PathBuf::from);
        match path {
            Some(path) if output.status.success() && path.exists() => Ok(path),
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let message = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("Download fehlgeschlagen");
                Err(format!("yt-dlp: {message}"))
            }
        }
    }

    /// Stream-Adresse fÃ¼r ein YouTube-Video. Klappt â€žPremiumâ€œ nicht (YouTube verweigert die
    /// angemeldete Abfrage oder liefert nur Segment-Streams), geht es ohne Anmeldung weiter.
    pub async fn audio(&self, video_id: &str) -> Result<AudioInfo, String> {
        if self.wants_premium(false, true) {
            match self.try_audio(video_id, true).await {
                Ok(info) => return Ok(info),
                Err(_) => self.mark_premium_failed(),
            }
        }
        self.try_audio(video_id, false).await
    }

    async fn try_audio(&self, video_id: &str, premium: bool) -> Result<AudioInfo, String> {
        let output = self
            .command()
            .args(self.quality_args(false, true, premium))
            .args([
                "-J",
                "--no-playlist",
                "--no-warnings",
                &format!("https://music.youtube.com/watch?v={video_id}"),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(err)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let message = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("unbekannter Fehler");
            return Err(format!("yt-dlp: {message}"));
        }

        let format: Format = serde_json::from_slice(&output.stdout).map_err(err)?;
        if !format.protocol.as_deref().is_none_or(|p| p.starts_with("http")) {
            return Err("yt-dlp: nur Segment-Stream verfÃ¼gbar".into());
        }
        let mime = match format.ext.as_deref() {
            Some("m4a") | Some("mp4") => "audio/mp4",
            _ => "audio/webm",
        };
        Ok(AudioInfo {
            url: format.url,
            size: format.filesize.or(format.filesize_approx).unwrap_or(0),
            mime: mime.into(),
            headers: format.http_headers.into_iter().collect(),
        })
    }
}
