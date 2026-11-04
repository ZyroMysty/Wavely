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

/// Privater Arbeitsspeicher dieses Prozesses (Bytes)
pub fn private_bytes() -> u64 {
    #[cfg(windows)]
    {
        use windows::Win32::System::{
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX},
            Threading::GetCurrentProcess,
        };
        let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
        let ok = unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                &mut counters as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            )
        };
        if ok.is_ok() {
            return counters.PrivateUsage as u64;
        }
        0
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// Abstand der Ortszeit zu UTC in Millisekunden (inkl. Sommerzeit)
pub fn local_offset_ms() -> i64 {
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
        let to_ms = |t: windows::Win32::Foundation::SYSTEMTIME| {
            days_from_civil(t.wYear as i64, t.wMonth as i64, t.wDay as i64) * 86_400_000
                + t.wHour as i64 * 3_600_000
                + t.wMinute as i64 * 60_000
        };
        let (local, utc) = unsafe { (GetLocalTime(), GetSystemTime()) };
        // Auf Minuten runden (beide Uhren werden kurz nacheinander gelesen)
        ((to_ms(local) - to_ms(utc)) as f64 / 60_000.0).round() as i64 * 60_000
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// Datum → Tage seit 1970 (Algorithmus von Howard Hinnant)
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Tage seit 1970 → (Jahr, Monat, Tag)
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}

/// Text in die Zwischenablage
pub fn copy_text(text: &str) {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(text.to_string());
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
