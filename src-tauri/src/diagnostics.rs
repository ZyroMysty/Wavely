//! Diagnose für Admins: Arbeitsspeicher und CPU-Zeit jedes Wavely-Prozesses (auch WebView2 und
//! yt-dlp), WebView2-Version und Größe der Datenordner. Die Oberfläche fragt regelmäßig nach und
//! rechnet aus den CPU-Zeiten die Auslastung aus.

use std::{collections::HashMap, path::Path};

use serde::Serialize;
use tauri::{AppHandle, Manager};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pid: u32,
    name: String,
    /// Wofür der Prozess da ist (Wavely, Oberfläche, Netzwerk, …)
    kind: String,
    /// Privater Speicher – das, was der Prozess wirklich allein belegt
    private_bytes: u64,
    /// Arbeitsseiten im RAM, inkl. geteilter DLLs (zählt bei mehreren Prozessen doppelt)
    working_set: u64,
    peak_working_set: u64,
    cpu_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    label: &'static str,
    path: String,
    bytes: u64,
    files: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    processes: Vec<ProcessInfo>,
    webview_version: Option<String>,
    browser_args: String,
    hardware_acceleration: bool,
    /// Startzeit von Wavely (Unix-Millisekunden)
    started_ms: u64,
    cores: usize,
}

#[tauri::command]
pub async fn diagnostics_snapshot(app: AppHandle) -> Result<Snapshot, String> {
    let (kinds, webview_version) = webview_info(&app).await;
    let processes = tauri::async_runtime::spawn_blocking(move || processes(&kinds)).await.map_err(|e| e.to_string())?;
    Ok(Snapshot {
        processes,
        webview_version,
        browser_args: app.state::<crate::memory::BrowserArgs>().0.clone(),
        hardware_acceleration: crate::memory::hardware_acceleration_get(app.clone()),
        started_ms: started_ms(),
        cores: std::thread::available_parallelism().map_or(1, |n| n.get()),
    })
}

/// Größe der Datenordner – dauert bei vielen Dateien etwas, deshalb ein eigener Aufruf
#[tauri::command]
pub async fn diagnostics_folders(app: AppHandle) -> Result<Vec<Folder>, String> {
    let path = app.path();
    let data = path.app_data_dir().map_err(|e| e.to_string())?;
    // Unter Windows liegt das WebView2-Profil (EBWebView) im selben Ordner wie der Zwischenspeicher
    let local = path.app_local_data_dir().map_err(|e| e.to_string())?;
    let webview = local.join("EBWebView");
    tauri::async_runtime::spawn_blocking(move || {
        let folder = |label, dir: &Path, skip: Option<&Path>| {
            let (bytes, files) = folder_size(dir, skip);
            Folder { label, path: dir.to_string_lossy().into_owned(), bytes, files }
        };
        vec![
            folder("Einstellungen & Daten", &data, None),
            folder("Zwischenspeicher (Cover, Streams)", &local, Some(&webview)),
            folder("WebView2-Profil", &webview, None),
        ]
    })
    .await
    .map_err(|e| e.to_string())
}

fn folder_size(dir: &Path, skip: Option<&Path>) -> (u64, u64) {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_entry(|e| skip != Some(e.path()))
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .fold((0, 0), |(bytes, files), e| (bytes + e.metadata().map_or(0, |m| m.len()), files + 1))
}

/// Prozess-IDs und -Arten, die WebView2 selbst meldet, und die WebView2-Version
async fn webview_info(app: &AppHandle) -> (HashMap<u32, &'static str>, Option<String>) {
    let Some(window) = app.get_webview_window("main") else { return Default::default() };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let sent = window.with_webview(move |webview| {
        let _ = tx.send(unsafe { read_webview(&webview) });
    });
    if sent.is_err() {
        return Default::default();
    }
    rx.await.unwrap_or_default()
}

#[cfg(windows)]
unsafe fn read_webview(webview: &tauri::webview::PlatformWebview) -> (HashMap<u32, &'static str>, Option<String>) {
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use windows::core::{Interface, PWSTR};

    let mut kinds = HashMap::new();
    let Ok(environment) = webview.controller().CoreWebView2().and_then(|c| c.cast::<ICoreWebView2_2>()).and_then(|c| c.Environment()) else {
        return (kinds, None);
    };
    let mut version = PWSTR::null();
    let version = environment.BrowserVersionString(&mut version).ok().and_then(|_| {
        let text = version.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(version.0 as _));
        text
    });
    if let Ok(infos) = environment.cast::<ICoreWebView2Environment8>().and_then(|e| e.GetProcessInfos()) {
        let mut count = 0;
        let _ = infos.Count(&mut count);
        for i in 0..count {
            let Ok(info) = infos.GetValueAtIndex(i) else { continue };
            let (mut pid, mut kind) = (0, COREWEBVIEW2_PROCESS_KIND::default());
            if info.ProcessId(&mut pid).is_ok() && info.Kind(&mut kind).is_ok() {
                let label = match kind {
                    COREWEBVIEW2_PROCESS_KIND_BROWSER => "WebView2-Hauptprozess",
                    COREWEBVIEW2_PROCESS_KIND_RENDERER => "Oberfläche (Renderer)",
                    COREWEBVIEW2_PROCESS_KIND_GPU => "Grafik",
                    COREWEBVIEW2_PROCESS_KIND_UTILITY => "Hilfsdienst",
                    COREWEBVIEW2_PROCESS_KIND_SANDBOX_HELPER => "Sandbox",
                    _ => "WebView2",
                };
                kinds.insert(pid as u32, label);
            }
        }
    }
    (kinds, version)
}

#[cfg(not(windows))]
unsafe fn read_webview(_webview: &tauri::webview::PlatformWebview) -> (HashMap<u32, &'static str>, Option<String>) {
    Default::default()
}

/// Wavely und alle seine Unterprozesse (WebView2, yt-dlp, …)
#[cfg(windows)]
fn processes(kinds: &HashMap<u32, &'static str>) -> Vec<ProcessInfo> {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS},
    };

    let own = std::process::id();
    let mut all = Vec::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return Vec::new() };
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                all.push((entry.th32ProcessID, entry.th32ParentProcessID, String::from_utf16_lossy(&entry.szExeFile[..len])));
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }

    // Nachkommen von Wavely einsammeln
    let mut ids = vec![own];
    let mut i = 0;
    while i < ids.len() {
        let parent = ids[i];
        ids.extend(all.iter().filter(|(pid, ppid, _)| *ppid == parent && *pid != parent && !ids.contains(pid)).map(|(pid, ..)| *pid).collect::<Vec<_>>());
        i += 1;
    }

    let mut result: Vec<ProcessInfo> = ids
        .iter()
        .filter_map(|&pid| {
            let name = all.iter().find(|(p, ..)| *p == pid).map(|(.., n)| n.clone()).unwrap_or_default();
            let (private_bytes, working_set, peak_working_set, cpu_ms, command_line) = measure(pid)?;
            let kind = if pid == own {
                "Wavely (Rust)".to_string()
            } else if let Some(kind) = kinds.get(&pid) {
                match utility_type(&command_line) {
                    Some(sub) if *kind == "Hilfsdienst" => format!("Hilfsdienst: {sub}"),
                    _ => kind.to_string(),
                }
            } else if name.eq_ignore_ascii_case("yt-dlp.exe") {
                "yt-dlp (Stream-Adressen)".to_string()
            } else if command_line.contains("--type=crashpad-handler") {
                "Absturzmelder".to_string()
            } else {
                "Sonstiges".to_string()
            };
            Some(ProcessInfo { pid, name, kind, private_bytes, working_set, peak_working_set, cpu_ms })
        })
        .collect();
    result.sort_by(|a, b| b.private_bytes.cmp(&a.private_bytes));
    result
}

#[cfg(not(windows))]
fn processes(_kinds: &HashMap<u32, &'static str>) -> Vec<ProcessInfo> {
    Vec::new()
}

/// „--utility-sub-type=network.mojom.NetworkService“ → „Netzwerk“
fn utility_type(command_line: &str) -> Option<String> {
    let value = command_line.split("--utility-sub-type=").nth(1)?.split_whitespace().next()?;
    let name = value.rsplit('.').next().unwrap_or(value).trim_matches('"');
    Some(
        match name {
            "NetworkService" => "Netzwerk",
            "StorageService" => "Speicher",
            "AudioService" => "Audio",
            "VideoCaptureService" => "Kamera",
            "CdmServiceBroker" | "MediaFoundationServiceBroker" => "Kopierschutz (Spotify)",
            other => other,
        }
        .to_string(),
    )
}

/// Privater Speicher, Working Set, Spitze, CPU-Zeit (ms) und Befehlszeile eines Prozesses
#[cfg(windows)]
fn measure(pid: u32) -> Option<(u64, u64, u64, u64, String)> {
    use windows::{
        Wdk::System::Threading::{NtQueryInformationProcess, ProcessCommandLineInformation},
        Win32::{
            Foundation::{CloseHandle, FILETIME, UNICODE_STRING},
            System::{
                ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX},
                Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
            },
        },
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut counters = PROCESS_MEMORY_COUNTERS_EX { cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32, ..Default::default() };
        let memory = GetProcessMemoryInfo(handle, &mut counters as *mut _ as *mut PROCESS_MEMORY_COUNTERS, counters.cb).is_ok();

        let (mut created, mut exited, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
        let cpu_ms = if GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user).is_ok() {
            (ticks(kernel) + ticks(user)) / 10_000
        } else {
            0
        };

        // Befehlszeile (zeigt bei WebView2, welcher Hilfsdienst es ist)
        let mut buffer = vec![0u8; 8192];
        let mut length = 0;
        let command_line = if NtQueryInformationProcess(handle, ProcessCommandLineInformation, buffer.as_mut_ptr() as _, buffer.len() as u32, &mut length).is_ok() {
            let text = &*(buffer.as_ptr() as *const UNICODE_STRING);
            let chars = std::slice::from_raw_parts(text.Buffer.0, text.Length as usize / 2);
            String::from_utf16_lossy(chars)
        } else {
            String::new()
        };
        let _ = CloseHandle(handle);

        memory.then(|| {
            (
                counters.PrivateUsage as u64,
                counters.WorkingSetSize as u64,
                counters.PeakWorkingSetSize as u64,
                cpu_ms,
                command_line,
            )
        })
    }
}

/// Startzeit dieses Prozesses als Unix-Millisekunden
#[cfg(windows)]
fn started_ms() -> u64 {
    use windows::Win32::{Foundation::FILETIME, System::Threading::{GetCurrentProcess, GetProcessTimes}};
    let (mut created, mut exited, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe {
        if GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user).is_err() {
            return 0;
        }
    }
    let ticks = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
    // FILETIME zählt 100-ns-Schritte seit 1601
    ticks.saturating_sub(116_444_736_000_000_000) / 10_000
}

#[cfg(not(windows))]
fn started_ms() -> u64 {
    0
}
