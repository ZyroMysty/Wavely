mod presence;

use std::sync::Mutex;

use presence::{Presence, Track};
use tauri::{Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};

/// Wird in jede Seite injiziert und meldet den aktuellen Titel an Rust.
const INJECT_SCRIPT: &str = include_str!("inject.js");

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

pub fn run() {
    let app = tauri::Builder::default()
        .manage(Mutex::new(Presence::default()))
        .invoke_handler(tauri::generate_handler![update_presence, clear_presence])
        .setup(|app| {
            WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External("https://music.youtube.com".parse()?),
            )
            .title("YouTube Music")
            .inner_size(1200.0, 800.0)
            .min_inner_size(400.0, 300.0)
            .initialization_script(INJECT_SCRIPT)
            .build()?;
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
