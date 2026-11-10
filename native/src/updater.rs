//! Updates von Wavely Native: `updates/latest-native.json` im Branch „native“ verweist auf
//! einen GitHub-Release mit dem Installer. Der Installer ist mit dem Wavely-Schlüssel signiert
//! (gleiches Verfahren wie Tauri: minisign); ohne gültige Signatur wird nichts installiert.

use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;

use crate::App;

const MANIFEST: &str = "https://raw.githubusercontent.com/ZyroMysty/Wavely/native/updates/latest-native.json";
/// Öffentlicher Schlüssel (wie in src-tauri/tauri.conf.json)
const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEUxQUQyQzQyMDk2QkQzRTYKUldUbTAyc0pRaXl0NGZSTGo5YWpKbnJCMTBZNFc3UFhaSFRqdkJsNS9Xd2hhNVFCdGVZRjRSUzkK";

#[derive(Clone, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub notes: Option<String>,
    pub url: String,
    /// Inhalt der .sig-Datei (Base64)
    pub signature: String,
}

fn newer(remote: &str, current: &str) -> bool {
    match (semver::Version::parse(remote), semver::Version::parse(current)) {
        (Ok(r), Ok(c)) => r > c,
        _ => false,
    }
}

fn verify(data: &[u8], signature_b64: &str) -> Result<(), String> {
    let key_text = String::from_utf8(STANDARD.decode(PUBKEY).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let key_line = key_text.lines().nth(1).ok_or("Schlüssel ungültig")?;
    let key = minisign_verify::PublicKey::from_base64(key_line).map_err(|e| e.to_string())?;
    let sig_text = String::from_utf8(STANDARD.decode(signature_b64.trim()).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let signature = minisign_verify::Signature::decode(&sig_text).map_err(|e| e.to_string())?;
    key.verify(data, &signature, false).map_err(|_| "Die Signatur des Updates stimmt nicht – Installation abgebrochen.".to_string())
}

impl App {
    /// Nach Updates suchen; `manual`: auch melden, wenn es keins gibt
    pub fn check_update(self: &Arc<Self>, manual: bool) {
        let app = self.clone();
        self.rt.spawn(async move {
            let result: Result<Manifest, String> = async {
                let response = reqwest::get(MANIFEST).await.map_err(|_| "Keine Verbindung".to_string())?;
                if !response.status().is_success() {
                    return Err(format!("Update-Server antwortete mit {}", response.status()));
                }
                response.json::<Manifest>().await.map_err(|e| e.to_string())
            }
            .await;
            match result {
                Ok(m) if newer(&m.version, env!("CARGO_PKG_VERSION")) => {
                    // Hinweis-Fenster nur einmal pro Version (oder wenn selbst gesucht)
                    let known = app.update.lock().unwrap().replace(m.clone()).is_some_and(|old| old.version == m.version);
                    if manual || !known {
                        let notes = m.notes.clone().unwrap_or_default();
                        // Markdown-Spiegelstriche als Aufzählungspunkte
                        let notes = notes.lines().map(|l| l.strip_prefix("- ").map(|r| format!("•  {r}")).unwrap_or(l.to_string())).collect::<Vec<_>>().join("\n");
                        let version = m.version.clone();
                        app.ui(move |ui| {
                            let n = slint::ComponentHandle::global::<crate::UpdateNotice>(ui);
                            n.set_version(version.into());
                            n.set_notes(notes.into());
                            n.set_open(true);
                        });
                    }
                    app.settings_changed();
                }
                Ok(_) if manual => app.toast("Wavely ist aktuell", false),
                Err(e) if manual => app.toast(&format!("Update-Suche fehlgeschlagen: {e}"), true),
                _ => {}
            }
        });
    }

    /// Gefundenes Update laden, prüfen und still installieren (Wavely startet danach neu)
    pub fn install_update(self: &Arc<Self>) {
        let Some(manifest) = self.update.lock().unwrap().clone() else { return };
        self.toast(&format!("Update {} wird geladen…", manifest.version), false);
        let app = self.clone();
        self.rt.spawn(async move {
            let result: Result<std::path::PathBuf, String> = async {
                let bytes = reqwest::get(&manifest.url).await.map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;
                verify(&bytes, &manifest.signature)?;
                let path = std::env::temp_dir().join(format!("wavely-native-{}-setup.exe", manifest.version));
                std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
                Ok(path)
            }
            .await;
            match result {
                Ok(path) => {
                    // Erst speichern und beenden, dann installiert der Installer und startet neu
                    let _ = slint::invoke_from_event_loop(move || {
                        app.stats_flush();
                        app.save_resume();
                        app.save_settings();
                        if std::process::Command::new(&path).arg("/S").spawn().is_ok() {
                            let _ = slint::quit_event_loop();
                        }
                    });
                }
                Err(e) => app.toast(&format!("Update fehlgeschlagen: {e}"), true),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    /// Prüft den zuletzt gebauten Installer (falls vorhanden) gegen seine Signatur
    #[test]
    fn signature_of_built_installer() {
        let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dist");
        let setup = dist.join(format!("Wavely-Native_{}_x64-setup.exe", env!("CARGO_PKG_VERSION")));
        let (Ok(data), Ok(sig)) = (std::fs::read(&setup), std::fs::read_to_string(setup.with_extension("exe.sig"))) else { return };
        super::verify(&data, &sig).expect("Signatur gültig");
        let mut broken = data.clone();
        broken[100] ^= 1;
        assert!(super::verify(&broken, &sig).is_err());
    }
}
