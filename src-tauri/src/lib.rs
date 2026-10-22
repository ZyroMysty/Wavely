mod artwork;
mod library;
mod lyrics;
mod offline;
mod presence;
mod soundcloud;
mod spotify;
#[cfg(windows)]
mod taskbar;
mod ytdlp;
mod ytm;

use std::sync::Mutex;

use library::LocalTrack;
use presence::{Presence, Track};
use serde::Serialize;
use tauri::{AppHandle, Manager, RunEvent, State};

#[derive(Serialize)]
struct LibraryView {
    folders: Vec<String>,
    tracks: Vec<LocalTrack>,
}

#[tauri::command]
async fn update_presence(state: State<'_, Mutex<Presence>>, track: Track) -> Result<(), String> {
    state.lock().unwrap().set(&track);
    Ok(())
}

#[tauri::command]
async fn clear_presence(state: State<'_, Mutex<Presence>>) -> Result<(), String> {
    state.lock().unwrap().clear();
    Ok(())
}

#[tauri::command]
async fn get_library(app: AppHandle) -> Result<LibraryView, String> {
    let folders = library::load_settings(&app).folders;
    library::allow_folders(&app, &folders);

    let to_scan = folders.clone();
    let tracks = tauri::async_runtime::spawn_blocking(move || library::scan(&to_scan))
        .await
        .map_err(|e| e.to_string())?;
    Ok(LibraryView { folders, tracks })
}

#[tauri::command]
async fn add_folder(app: AppHandle, path: String) -> Result<(), String> {
    let mut settings = library::load_settings(&app);
    if !settings.folders.contains(&path) {
        settings.folders.push(path);
        library::save_settings(&app, &settings)?;
    }
    Ok(())
}

#[tauri::command]
async fn remove_folder(app: AppHandle, path: String) -> Result<(), String> {
    let mut settings = library::load_settings(&app);
    settings.folders.retain(|f| f != &path);
    library::save_settings(&app, &settings)
}

#[tauri::command]
async fn get_cover(path: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || library::cover(&path))
        .await
        .map_err(|e| e.to_string())
}

/// Öffnet den Mini-Player (eigenes kleines Fenster ohne Rahmen) oder holt ihn nach vorn.
#[tauri::command]
async fn mini_player_open(app: AppHandle, pinned: bool, width: Option<f64>, height: Option<f64>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("mini") {
        let _ = window.unminimize();
        return window.set_focus().map_err(|e| e.to_string());
    }
    tauri::WebviewWindowBuilder::new(&app, "mini", tauri::WebviewUrl::App("mini.html".into()))
        .title("Wavely Mini")
        .inner_size(width.unwrap_or(360.0), height.unwrap_or(104.0))
        // Klein als Leiste, groß als Player mit großem Cover (siehe MiniPlayer.svelte)
        .resizable(true)
        .min_inner_size(220.0, 72.0)
        .max_inner_size(640.0, 800.0)
        .maximizable(false)
        .decorations(false)
        .always_on_top(pinned)
        .build()
        .map(|_window| {
            #[cfg(windows)]
            taskbar::set_big_icon(&_window);
        })
        .map_err(|e| e.to_string())
}

/// Hauptfenster wieder anzeigen (z. B. aus dem Mini-Player).
#[tauri::command]
async fn show_main_window(app: AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("Hauptfenster fehlt")?;
    let _ = window.unminimize();
    let _ = window.show();
    window.set_focus().map_err(|e| e.to_string())
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(Presence::default()))
        .invoke_handler(tauri::generate_handler![
            update_presence,
            clear_presence,
            get_library,
            add_folder,
            remove_folder,
            get_cover,
            ytm::ytm_status,
            ytm::ytm_login,
            ytm::ytm_logout,
            ytm::ytm_library,
            ytm::ytm_collection,
            ytm::ytm_search,
            ytm::ytm_prefetch,
            ytm::ytm_radio,
            ytm::ytm_account,
            ytm::ytm_liked_ids,
            ytm::ytm_set_like,
            ytm::ytm_create_playlist,
            ytm::ytm_add_to_playlist,
            ytm::ytm_history,
            ytm::ytm_artist,
            spotify::spotify_status,
            spotify::spotify_login,
            spotify::spotify_logout,
            spotify::spotify_account,
            spotify::spotify_library,
            spotify::spotify_collection,
            spotify::spotify_search,
            spotify::spotify_artist,
            spotify::spotify_recent,
            spotify::spotify_resolve,
            spotify::spotify_get_client_id,
            spotify::spotify_set_client_id,
            spotify::spotify_devices,
            spotify::spotify_playback,
            spotify::spotify_play,
            spotify::spotify_resume,
            spotify::spotify_pause,
            spotify::spotify_seek,
            spotify::spotify_volume,
            spotify::spotify_transfer,
            spotify::spotify_access_token,
            soundcloud::soundcloud_status,
            soundcloud::soundcloud_login,
            soundcloud::soundcloud_logout,
            soundcloud::soundcloud_account,
            soundcloud::soundcloud_library,
            soundcloud::soundcloud_collection,
            soundcloud::soundcloud_search,
            soundcloud::soundcloud_artist,
            soundcloud::soundcloud_related,
            soundcloud::soundcloud_recent,
            soundcloud::soundcloud_stream,
            ytm::ytm_report_play,
            mini_player_open,
            show_main_window,
            #[cfg(windows)]
            taskbar::taskbar_update,
            lyrics::get_lyrics,
            offline::offline_list,
            offline::offline_save,
            offline::offline_remove,
            offline::offline_stats,
            offline::offline_open_folder,
            offline::offline_clear,
        ])
        // YouTube-Audio über eigenen Stream-Dienst: holt die Datei in Stücken mit den Headern, die YouTube erwartet
        .register_asynchronous_uri_scheme_protocol("ytstream", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let id = request.uri().path().trim_start_matches('/').to_string();
            let range = request
                .headers()
                .get(tauri::http::header::RANGE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            tauri::async_runtime::spawn(async move {
                let response = app.state::<ytm::Ytm>().serve_stream(&id, range.as_deref()).await;
                responder.respond(response);
            });
        })
        // Cover-Bilder über Rust laden und zwischenspeichern (artwork://<url>)
        .register_asynchronous_uri_scheme_protocol("artwork", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let encoded = request.uri().path().trim_start_matches('/').to_string();
            tauri::async_runtime::spawn(async move {
                responder.respond(app.state::<artwork::Artwork>().serve(&encoded).await);
            });
        })
        // Ohne Hauptfenster läuft keine Musik: Schließen beendet Wavely ganz (auch den Mini-Player)
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { .. } = event {
                    window.app_handle().exit(0);
                }
            }
        })
        .setup(|app| {
            #[cfg(windows)]
            if let Some(main) = app.get_webview_window("main") {
                taskbar::set_big_icon(&main);
                taskbar::init(app.handle(), &main);
            }
            app.manage(artwork::Artwork::new(app.handle())?);
            let ytdlp = ytdlp::YtDlp::init(app.handle())?;
            app.manage(offline::Offline::new(app.handle(), ytdlp.clone())?);
            app.manage(ytm::Ytm::new(app.handle(), ytdlp)?);
            app.manage(spotify::Spotify::new(app.handle())?);
            app.manage(soundcloud::SoundCloud::new(app.handle())?);

            // yt-dlp im Hintergrund aktuell halten (höchstens einmal am Tag)
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                handle.state::<ytm::Ytm>().update_tools().await;
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            app.state::<Mutex<Presence>>().lock().unwrap().shutdown();
        }
    });
}
