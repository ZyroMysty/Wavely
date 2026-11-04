//! Ordner der App. Die native Testversion hat einen eigenen Datenordner (`com.wavely.native`),
//! damit sie neben der normalen Wavely laufen kann. Anmeldungen werden beim ersten Start übernommen.

use std::path::{Path, PathBuf};

const IDENTIFIER: &str = "com.wavely.native";
const OLD_IDENTIFIER: &str = "com.wavely.app";

#[derive(Clone)]
pub struct Paths {
    /// %APPDATA%\com.wavely.native (Einstellungen, Anmeldungen, Daten)
    pub data: PathBuf,
    /// %LOCALAPPDATA%\com.wavely.native (Zwischenspeicher)
    pub cache: PathBuf,
}

impl Paths {
    pub fn init() -> Result<Self, String> {
        let roaming = std::env::var_os("APPDATA").map(PathBuf::from).ok_or("APPDATA fehlt")?;
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).ok_or("LOCALAPPDATA fehlt")?;
        let paths = Self { data: roaming.join(IDENTIFIER), cache: local.join(IDENTIFIER) };
        let first_start = !paths.data.exists();
        std::fs::create_dir_all(&paths.data).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&paths.cache).map_err(|e| e.to_string())?;
        if first_start {
            migrate(&roaming.join(OLD_IDENTIFIER), &paths.data);
        }
        Ok(paths)
    }
}

/// Übernimmt Anmeldungen und Daten des Rust-Teils der bisherigen Wavely (nicht die WebView-Daten).
fn migrate(old: &Path, new: &Path) {
    for name in ["ytm", "bin", "spotify", "soundcloud", "lastfm", "stats", "library.json", "playlists.json"] {
        let from = old.join(name);
        if from.is_dir() {
            copy_dir(&from, &new.join(name));
        } else if from.is_file() {
            let _ = std::fs::copy(&from, new.join(name));
        }
    }
}

fn copy_dir(from: &Path, to: &Path) {
    let _ = std::fs::create_dir_all(to);
    let Ok(entries) = std::fs::read_dir(from) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            copy_dir(&path, &to.join(entry.file_name()));
        } else {
            let _ = std::fs::copy(&path, to.join(entry.file_name()));
        }
    }
}

/// Windows im dunklen Modus? (Einstellungen → Personalisierung → Farben)
pub fn system_dark() -> bool {
    #[cfg(windows)]
    {
        use windows::{core::w, Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD}};
        let mut value: u32 = 1;
        let mut size = std::mem::size_of::<u32>() as u32;
        let ok = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
                w!("AppsUseLightTheme"),
                RRF_RT_REG_DWORD,
                None,
                Some(&mut value as *mut u32 as *mut _),
                Some(&mut size),
            )
        };
        ok.is_ok() && value == 0
    }
    #[cfg(not(windows))]
    {
        true
    }
}
