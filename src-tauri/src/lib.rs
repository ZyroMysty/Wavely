mod artwork;
mod library;
mod lyrics;
mod offline;
mod presence;
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
            lyrics::get_lyrics,
            offline::offline_list,
            offline::offline_save,
            offline::offline_remove,
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
        .setup(|app| {
            app.manage(artwork::Artwork::new(app.handle())?);
            let ytdlp = ytdlp::YtDlp::init(app.handle())?;
            app.manage(offline::Offline::new(app.handle(), ytdlp.clone())?);
            app.manage(ytm::Ytm::new(app.handle(), ytdlp)?);

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
