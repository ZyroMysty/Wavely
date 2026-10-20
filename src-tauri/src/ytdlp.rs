//! yt-dlp als Hilfsprogramm: liefert die Audio-Stream-Adressen für YouTube Music.
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
/// Unter Windows kein Konsolenfenster öffnen
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct YtDlp {
    path: PathBuf,
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
        Ok(Self { path })
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.path);
        cmd.stdin(Stdio::null()).kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }

    /// Sucht höchstens einmal am Tag nach einer neuen yt-dlp-Version.
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

    pub async fn audio(&self, video_id: &str) -> Result<AudioInfo, String> {
        let output = self
            .command()
            .args([
                "-f",
                "bestaudio[ext=webm]/bestaudio",
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
