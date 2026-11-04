//! Nur eine Wavely gleichzeitig: Ein zweiter Start schickt seine Argumente (z. B. Einladungslink
//! „wavely://join/CODE“) über eine lokale Verbindung an die laufende Wavely und beendet sich.
//! Außerdem: Start mit Windows (Registry „Run“).

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
};

/// Port nur für die Verbindung zwischen zwei Wavely-Starts auf diesem PC
const PORT: u16 = 43829;

/// Läuft schon eine Wavely? Dann Argumente hinschicken und `true`.
pub fn forward_to_running() -> bool {
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", PORT)) else { return false };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let _ = stream.write_all(format!("WAVELY\n{}", args.join("\n")).as_bytes());
    true
}

/// Auf zweite Starts hören; `on_args` bekommt deren Argumente
pub fn listen(on_args: impl Fn(Vec<String>) + Send + 'static) {
    let Ok(listener) = TcpListener::bind(("127.0.0.1", PORT)) else { return };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut text = String::new();
            if stream.read_to_string(&mut text).is_ok() {
                if let Some(rest) = text.strip_prefix("WAVELY\n") {
                    on_args(rest.lines().map(str::to_string).filter(|l| !l.is_empty()).collect());
                } else if text == "WAVELY" {
                    on_args(Vec::new());
                }
            }
        }
    });
}

/// Einladungscode aus „wavely://join/CODE“
pub fn code_from_link(arg: &str) -> Option<String> {
    let code = arg.strip_prefix("wavely://join/")?.trim_end_matches('/');
    (code.len() >= 6 && code.chars().take(6).all(|c| c.is_ascii_alphanumeric())).then(|| code[..6].to_uppercase())
}

#[cfg(windows)]
mod registry {
    use windows::{
        core::{w, HSTRING, PCWSTR},
        Win32::System::Registry::{RegCloseKey, RegCreateKeyW, RegDeleteValueW, RegGetValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ},
    };

    const RUN: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
    const NAME: PCWSTR = w!("Wavely Native");

    pub fn autostart_enabled() -> bool {
        let mut size = 0u32;
        unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN, NAME, RRF_RT_REG_SZ, None, None, Some(&mut size)) }.is_ok()
    }

    pub fn set_autostart(enabled: bool) {
        unsafe {
            let mut key = HKEY::default();
            if RegCreateKeyW(HKEY_CURRENT_USER, RUN, &mut key).is_err() {
                return;
            }
            if enabled {
                if let Ok(exe) = std::env::current_exe() {
                    let value = HSTRING::from(format!("\"{}\" --autostart", exe.display()));
                    let bytes = std::slice::from_raw_parts(value.as_ptr() as *const u8, (value.len() + 1) * 2);
                    let _ = RegSetValueExW(key, NAME, None, REG_SZ, Some(bytes));
                }
            } else {
                let _ = RegDeleteValueW(key, NAME);
            }
            let _ = RegCloseKey(key);
        }
    }
}

#[cfg(windows)]
pub use registry::{autostart_enabled, set_autostart};
