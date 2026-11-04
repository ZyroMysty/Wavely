//! Lokale Musik: Ordner einlesen, beobachten und anzeigen (wie LocalView.svelte).

use std::sync::Arc;

use crate::{
    icon,
    nav::PageKind,
    pages::{CardData, PageData, RowData, Target},
    player::Track,
    services::library,
    App,
};

impl From<library::LocalTrack> for Track {
    fn from(t: library::LocalTrack) -> Self {
        Self {
            source: "local".into(),
            id: t.id,
            title: t.title,
            artist: t.artist,
            album: t.album,
            duration: t.duration,
            path: Some(t.path),
            ..Default::default()
        }
    }
}

pub fn page(app: &Arc<App>) -> PageData {
    let folders = library::load_settings(&app.paths).folders;
    let tracks = app.local_tracks.lock().unwrap().clone();
    let loading = *app.local_loading.lock().unwrap();
    let mut p = PageData::default();
    p.title("Lokale Musik");
    if folders.is_empty() {
        p.message("Wähle einen Ordner mit MP3-, FLAC- oder M4A-Dateien aus.");
        p.buttons(&[("local-add", "Ordner hinzufügen", icon::PLUS)]);
        return p;
    }
    p.message(if loading { "Wird eingelesen…".to_string() } else { format!("{} Titel", tracks.len()) });
    p.buttons(&[
        ("play", "Abspielen", icon::PLAY),
        ("shuffle", "Zufall", icon::SHUFFLE),
        ("local-add", "Ordner hinzufügen", icon::PLUS),
        ("local-refresh", "Neu einlesen", icon::REFRESH),
    ]);
    p.heading("Ordner");
    for folder in &folders {
        let name = std::path::Path::new(folder).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(folder.clone());
        p.rows.push(RowData {
            kind: "setting",
            label: "button".into(),
            text: name,
            subtext: folder.clone(),
            cards: vec![CardData {
                title: "Entfernen".into(),
                subtitle: String::new(),
                art: None,
                round: false,
                target: Target::Action(format!("local-remove:{folder}")),
            }],
            ..Default::default()
        });
    }
    if !tracks.is_empty() {
        p.heading("Titel");
        p.tracks(tracks);
    }
    p.spacer();
    p
}

impl App {
    /// Ordner einlesen (im Hintergrund) und beobachten
    pub fn local_scan(self: &Arc<Self>) {
        let folders = library::load_settings(&self.paths).folders;
        *self.local_loading.lock().unwrap() = true;
        let app = self.clone();
        self.local_watcher.watch(&folders, move || {
            let a = app.clone();
            let _ = slint::invoke_from_event_loop(move || a.local_scan());
        });
        let app = self.clone();
        self.rt.spawn_blocking(move || {
            let tracks: Vec<Track> = library::scan(&folders).into_iter().map(Track::from).collect();
            *app.local_tracks.lock().unwrap() = tracks;
            *app.local_loading.lock().unwrap() = false;
            let a = app.clone();
            let _ = slint::invoke_from_event_loop(move || a.refresh_if_showing(|p| matches!(p, PageKind::Local)));
        });
        self.refresh_if_showing(|p| matches!(p, PageKind::Local));
    }

    pub fn local_action(self: &Arc<Self>, action: &str) -> bool {
        match action {
            "local-add" => {
                let app = self.clone();
                // Ordnerauswahl blockiert: eigener Thread
                std::thread::spawn(move || {
                    let Some(folder) = rfd::FileDialog::new().set_title("Musikordner auswählen").pick_folder() else { return };
                    let path = folder.to_string_lossy().into_owned();
                    let mut settings = library::load_settings(&app.paths);
                    if !settings.folders.contains(&path) {
                        settings.folders.push(path);
                        let _ = library::save_settings(&app.paths, &settings);
                    }
                    let a = app.clone();
                    let _ = slint::invoke_from_event_loop(move || a.local_scan());
                });
                true
            }
            "local-refresh" => {
                self.local_scan();
                true
            }
            other => match other.strip_prefix("local-remove:") {
                Some(folder) => {
                    let mut settings = library::load_settings(&self.paths);
                    settings.folders.retain(|f| f != folder);
                    let _ = library::save_settings(&self.paths, &settings);
                    self.local_scan();
                    true
                }
                None => false,
            },
        }
    }
}
