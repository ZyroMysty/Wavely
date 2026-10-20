mod library;
mod presence;
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
            ytm::ytm_stream,
        ])
        .setup(|app| {
            app.manage(ytm::Ytm::new(app.handle())?);
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
