//! Update-Kanäle: „Stabil“ für alle (neuestes GitHub-Release) und „Test“ nur für Admins
//! (updates/latest-test.json im Repository, verweist auf GitHub-Pre-Releases wie 0.15.0-test.1).
//! Ob jemand Admin ist, entscheidet die Oberfläche über das Wavely-Konto (Firestore „admins“).

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

const STABLE: &str = "https://github.com/ZyroMysty/Wavely/releases/latest/download/latest.json";
const TEST: &str = "https://raw.githubusercontent.com/ZyroMysty/Wavely/main/updates/latest-test.json";

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Gefundenes Update, bis es installiert wird
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    version: String,
    notes: Option<String>,
    /// Testversion (nur für Admins)
    test: bool,
}

/// `downgrade`: Wer eine Testversion hat und den Test-Kanal verlässt, bekommt die nächste
/// stabile Version, auch wenn deren Nummer kleiner ist.
async fn check(app: &AppHandle, url: &str, downgrade: bool) -> Result<Option<Update>, String> {
    app.updater_builder()
        .endpoints(vec![url.parse().map_err(err)?])
        .map_err(err)?
        .version_comparator(move |current, remote| {
            remote.version > current || (downgrade && !current.pre.is_empty() && remote.version != current)
        })
        .build()
        .map_err(err)?
        .check()
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn update_check(app: AppHandle, pending: State<'_, PendingUpdate>, test: bool) -> Result<Option<UpdateInfo>, String> {
    let stable = check(&app, STABLE, !test).await;
    // Test-Kanal: Fehler (z. B. noch keine Testversion veröffentlicht) zählen nicht
    let testing = if test { check(&app, TEST, false).await.ok().flatten() } else { None };

    let chosen = match (stable, testing) {
        (Ok(Some(s)), Some(t)) => {
            let newer_test = t.version.parse::<semver::Version>().ok() > s.version.parse::<semver::Version>().ok();
            Some(if newer_test { t } else { s })
        }
        (Ok(s), t) => s.or(t),
        (Err(_), Some(t)) => Some(t),
        (Err(e), None) => return Err(e),
    };
    let info = chosen.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        notes: u.body.clone(),
        test: u.version.contains('-'),
    });
    *pending.0.lock().await = chosen;
    Ok(info)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    received: u64,
    total: Option<u64>,
    finished: bool,
}

/// Lädt das gefundene Update (Signatur wird geprüft) und startet den Installer;
/// Fortschritt kommt als Event `update-progress`.
#[tauri::command]
pub async fn update_install(app: AppHandle, pending: State<'_, PendingUpdate>) -> Result<(), String> {
    let update = pending.0.lock().await.take().ok_or("Kein Update gefunden")?;
    let mut received = 0u64;
    let progress_app = app.clone();
    let done_app = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                received += chunk as u64;
                let _ = progress_app.emit("update-progress", Progress { received, total, finished: false });
            },
            move || {
                let _ = done_app.emit("update-progress", Progress { received: 0, total: None, finished: true });
            },
        )
        .await
        .map_err(err)
}
