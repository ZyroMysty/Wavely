//! Windows-Anbindung: Medientasten/Overlay (media.rs) und Taskleisten-Buttons (taskbar.rs).
//! Alles läuft im UI-Thread; Tastendrücke und Klicks kommen über `action` zurück.

pub mod hotkeys;
pub mod media;
pub mod mini;
pub mod taskbar;

use std::sync::OnceLock;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::HWND;

static ACTION: OnceLock<Box<dyn Fn(&str) + Send + Sync>> = OnceLock::new();

/// Aktion aus Windows („play“, „pause“, „toggle“, „next“, „previous“, „like“)
fn action(name: &str) {
    if let Some(handler) = ACTION.get() {
        handler(name);
    }
}

/// Fenster-Handle des Slint-Fensters
pub fn hwnd(window: &slint::Window) -> Option<HWND> {
    match window.window_handle().window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut _)),
        _ => None,
    }
}

/// Nach dem Öffnen des Fensters einrichten
pub fn init(window: &slint::Window, on_action: impl Fn(&str) + Send + Sync + 'static) {
    let _ = ACTION.set(Box::new(on_action));
    let Some(hwnd) = hwnd(window) else {
        eprintln!("Windows-Anbindung: kein Fenster-Handle");
        return;
    };
    // COM/WinRT für diesen Thread (Mediensteuerung und Taskleiste sind COM-Objekte)
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
    }
    media::init(hwnd);
    taskbar::init(hwnd);
}
