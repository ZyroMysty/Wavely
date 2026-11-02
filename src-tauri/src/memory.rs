//! Arbeitsspeicher sparen: Hardwarebeschleunigung abschaltbar (Standard: aus) und weniger
//! Speicher für minimierte Fenster.

use std::path::PathBuf;

use tauri::{AppHandle, Manager, Window, WindowEvent};

/// Standard-Argumente von wry (Mini-Menü und SmartScreen aus) – gehen verloren, sobald man eigene setzt
const DEFAULT_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";
/// Ohne Grafikkarte zeichnet Chromium in Software. Bei Wavelys schlichter Oberfläche ist das nicht
/// spürbar (gleiche CPU-Last beim Abspielen), spart aber den GPU-Prozess mit rund 70 MB.
const NO_GPU_ARGS: &str = " --disable-gpu --disable-software-rasterizer";

/// Browser-Argumente beim Start – alle Fenster brauchen dieselben (gemeinsamer WebView2-Ordner)
pub struct BrowserArgs(pub String);

/// Datei statt Einstellung, weil die Argumente feststehen müssen, bevor die Oberfläche lädt
fn gpu_marker(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("gpu"))
}

pub fn browser_args(app: &AppHandle) -> String {
    if gpu_marker(app).is_ok_and(|p| p.exists()) {
        DEFAULT_ARGS.into()
    } else {
        format!("{DEFAULT_ARGS}{NO_GPU_ARGS}")
    }
}

#[tauri::command]
pub fn hardware_acceleration_get(app: AppHandle) -> bool {
    gpu_marker(&app).is_ok_and(|p| p.exists())
}

/// Wirkt erst nach einem Neustart von Wavely
#[tauri::command]
pub fn hardware_acceleration_set(app: AppHandle, enabled: bool) -> Result<(), String> {
    let path = gpu_marker(&app)?;
    if enabled {
        std::fs::write(path, "1").map_err(|e| e.to_string())
    } else {
        let _ = std::fs::remove_file(path);
        Ok(())
    }
}

/// Minimierte Fenster: WebView2 bekommt das Ziel „wenig Speicher“ und gibt Zwischenspeicher frei.
/// Die Musik läuft dabei normal weiter.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if !matches!(event, WindowEvent::Resized(_)) {
        return;
    }
    let low = window.is_minimized().unwrap_or(false);
    if let Some(webview) = window.app_handle().get_webview_window(window.label()) {
        set_low_memory(&webview, low);
    }
}

#[cfg(windows)]
fn set_low_memory(webview: &tauri::WebviewWindow, low: bool) {
    let _ = webview.with_webview(move |w| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
        };
        use windows::core::Interface;

        let Ok(core) = w.controller().CoreWebView2().and_then(|c| c.cast::<ICoreWebView2_19>()) else {
            return;
        };
        let level = if low { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW } else { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL };
        // Beim Ziehen am Fensterrand kommen viele Resized-Ereignisse – nur bei Änderung setzen
        let mut current = COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL::default();
        if core.MemoryUsageTargetLevel(&mut current).is_ok() && current == level {
            return;
        }
        let _ = core.SetMemoryUsageTargetLevel(level);
    });
}

#[cfg(not(windows))]
fn set_low_memory(_webview: &tauri::WebviewWindow, _low: bool) {}
