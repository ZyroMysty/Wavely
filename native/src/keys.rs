//! Tastenkürzel im Fenster (wie shortcuts.svelte.js) und Schlaf-Timer (wie sleepTimer.svelte.js).

use std::sync::{atomic::Ordering, Arc};

use slint::ComponentHandle;

use crate::{icon, App, Menu, MenuItem, Nav};

/// Für die Übersicht in den Einstellungen: (Tasten, Beschreibung)
pub const SHORTCUT_LIST: &[(&str, &str)] = &[
    ("Leertaste", "Abspielen / Pause"),
    ("Strg + → / ←", "Nächster / vorheriger Song"),
    ("→ / ←", "5 Sekunden vor / zurück (mit Umschalt: 30 Sekunden)"),
    ("↑ / ↓", "Lauter / leiser"),
    ("M · S · R · L", "Stumm · Zufall · Wiederholen · Gefällt mir"),
    ("Strg + L · Q · H", "Liedtext · Warteschlange · Verlauf"),
    ("Strg + ,", "Einstellungen"),
];

/// Schlaf-Timer: Minuten zur Auswahl
const SLEEP_OPTIONS: [u64; 8] = [5, 10, 15, 30, 45, 60, 90, 120];
/// So lange wird am Ende ausgeblendet
const FADE_SECONDS: u64 = 8;

impl App {
    /// Taste im Fenster (nicht in einem Textfeld). `key`: Zeichen oder „left“, „right“, „up“, „down“, „space“
    pub fn key_pressed(self: &Arc<Self>, key: &str, ctrl: bool, shift: bool) -> bool {
        let position = self.state.lock().unwrap().position;
        let volume = self.state.lock().unwrap().volume;
        match (key, ctrl) {
            ("space", false) => self.toggle_play(),
            ("right", true) => self.next(),
            ("left", true) => self.previous(),
            ("right", false) => self.seek(position + if shift { 30.0 } else { 5.0 }),
            ("left", false) => self.seek((position - if shift { 30.0 } else { 5.0 }).max(0.0)),
            ("up", false) => self.set_volume((volume + 0.05).min(1.0)),
            ("down", false) => self.set_volume((volume - 0.05).max(0.0)),
            ("l", true) => self.toggle_panel("lyrics"),
            ("q", true) => self.toggle_panel("queue"),
            ("h", true) => self.toggle_panel("history"),
            (",", true) => self.show_view("settings"),
            #[cfg(windows)]
            ("m", true) => self.open_mini_player(),
            ("m", false) => self.toggle_mute(),
            ("s", false) => self.toggle_shuffle(),
            ("r", false) => self.cycle_repeat(),
            ("l", false) => {
                let track = self.state.lock().unwrap().current().cloned();
                if let Some(track) = track.filter(|t| t.source == "youtube") {
                    self.toggle_like(track);
                }
            }
            _ => return false,
        }
        if matches!(key, "up" | "down") {
            self.sync_player();
        }
        true
    }

    /// Systemweite Kürzel nach der Einstellung an- oder abschalten
    pub fn apply_global_shortcuts(self: &Arc<Self>) {
        #[cfg(windows)]
        {
            let enabled = self.settings.lock().unwrap().global_shortcuts;
            let failed = crate::win::hotkeys::set(enabled);
            if !failed.is_empty() {
                self.toast(&format!("Schon von einem anderen Programm belegt: {}", failed.join(", ")), true);
            }
        }
    }

    // --- Schlaf-Timer -------------------------------------------------------

    pub fn open_sleep_menu(self: &Arc<Self>, x: f32, y: f32) {
        let active = self.sleep_until.lock().unwrap().is_some() || self.sleep_end_of_track.load(Ordering::Relaxed);
        let mut items: Vec<MenuItem> = SLEEP_OPTIONS
            .iter()
            .map(|m| MenuItem {
                id: format!("sleep:{m}").into(),
                label: format!("{m} Minuten").into(),
                icon: icon::BEDTIME.into(),
                separator: false,
                chevron: false,
            })
            .collect();
        items.push(MenuItem { id: "sleep:end".into(), label: "Am Ende des Songs".into(), icon: icon::MUSIC.into(), separator: true, chevron: false });
        if active {
            items.push(MenuItem { id: "sleep:off".into(), label: "Schlaf-Timer aus".into(), icon: icon::CLOSE.into(), separator: true, chevron: false });
        }
        *self.menu.lock().unwrap() = None;
        self.ui(move |ui| {
            let m = ui.global::<Menu>();
            m.set_items(slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(items))));
            m.set_x(x);
            m.set_y(y);
            m.set_open(true);
        });
    }

    pub fn sleep_action(self: &Arc<Self>, arg: &str) {
        self.close_menu();
        let token = self.sleep_token.fetch_add(1, Ordering::SeqCst) + 1;
        *self.sleep_until.lock().unwrap() = None;
        self.sleep_end_of_track.store(false, Ordering::Relaxed);
        match arg {
            "off" => self.toast("Schlaf-Timer aus", false),
            "end" => {
                self.sleep_end_of_track.store(true, Ordering::Relaxed);
                self.toast("Schlaf-Timer: Musik stoppt am Ende des Songs", false);
            }
            minutes => {
                let Ok(minutes) = minutes.parse::<u64>() else { return };
                *self.sleep_until.lock().unwrap() = Some(std::time::Instant::now() + std::time::Duration::from_secs(minutes * 60));
                self.toast(&format!("Schlaf-Timer: Musik stoppt in {minutes} Minuten"), false);
                let app = self.clone();
                self.rt.spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs((minutes * 60).saturating_sub(FADE_SECONDS))).await;
                    if app.sleep_token.load(Ordering::SeqCst) != token {
                        return;
                    }
                    // Sanft ausblenden, dann anhalten
                    app.engine.fade(0.0, FADE_SECONDS as f32);
                    tokio::time::sleep(std::time::Duration::from_secs(FADE_SECONDS)).await;
                    if app.sleep_token.load(Ordering::SeqCst) != token {
                        app.engine.fade(1.0, 0.05);
                        return;
                    }
                    *app.sleep_until.lock().unwrap() = None;
                    let playing = app.state.lock().unwrap().playing;
                    if playing {
                        app.engine.send(crate::audio::Command::Pause);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    app.engine.fade(1.0, 0.05);
                    app.sync_sleep();
                });
            }
        }
        self.sync_sleep();
    }

    /// Ende eines Songs: true = nicht weiterspielen (Schlaf-Timer „Ende des Songs“)
    pub fn consume_end_of_track(self: &Arc<Self>) -> bool {
        let stop = self.sleep_end_of_track.swap(false, Ordering::SeqCst);
        if stop {
            self.sync_sleep();
        }
        stop
    }

    /// Symbol in der Player-Leiste hervorheben, solange der Timer läuft
    pub fn sync_sleep(self: &Arc<Self>) {
        let active = self.sleep_until.lock().unwrap().is_some() || self.sleep_end_of_track.load(Ordering::Relaxed);
        self.ui(move |ui| ui.global::<Nav>().set_sleep_active(active));
    }
}
