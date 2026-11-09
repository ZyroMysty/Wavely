//! Mini-Player anbinden: öffnen, Zustand/Farben/Cover schicken, Befehle ausführen.

use std::sync::Arc;

use serde_json::Value;

use crate::{covers::cover_url, win::mini, App};

fn rgba(c: slint::Color) -> [f32; 4] {
    [c.red() as f32 / 255.0, c.green() as f32 / 255.0, c.blue() as f32 / 255.0, c.alpha() as f32 / 255.0]
}

impl App {
    pub fn open_mini_player(self: &Arc<Self>) {
        let Some(ui) = self.ui.upgrade() else { return };
        let Some(main) = crate::win::hwnd(ui.window()) else {
            eprintln!("Mini-Player: kein Hauptfenster");
            return;
        };
        let (pinned, size, minimize) = {
            let s = self.settings.lock().unwrap();
            (s.mini_pinned, (s.mini_width, s.mini_height), s.mini_minimizes_main)
        };
        let app = self.clone();
        mini::open_player(main, pinned, size, minimize, move |command| {
            let a = app.clone();
            let _ = slint::invoke_from_event_loop(move || a.mini_command(command));
        });
        *self.mini_cover_key.lock().unwrap() = None;
        self.sync_mini_theme();
        self.sync_mini();
        if minimize {
            ui.window().set_minimized(true);
        }
    }

    fn mini_command(self: &Arc<Self>, command: Value) {
        match &command {
            Value::String(action) => match action.as_str() {
                "previous" => self.previous(),
                "toggle" => self.toggle_play(),
                "next" => self.next(),
                "mute" => self.toggle_mute(),
                "like" => self.windows_action("like"),
                // Das Neuzeichnen beim Zurückholen des Hauptfensters übernimmt win::taskbar
                // (WM_SIZE nach dem Minimieren bzw. WM_PAINT); hier nur zur Sicherheit einmal
                "closed" => self.force_full_repaint(),
                _ => {}
            },
            Value::Object(obj) => match obj.get("action").and_then(Value::as_str) {
                Some("volume") => {
                    if let Some(v) = obj.get("value").and_then(Value::as_f64) {
                        self.set_volume(v as f32);
                        self.sync_player();
                    }
                }
                Some("seek") => {
                    if let Some(p) = obj.get("position").and_then(Value::as_f64) {
                        self.seek(p);
                    }
                }
                Some("miniPinned") => {
                    self.settings.lock().unwrap().mini_pinned = obj.get("value").and_then(Value::as_bool).unwrap_or(true);
                    self.save_settings();
                }
                Some("miniSize") => {
                    {
                        let mut s = self.settings.lock().unwrap();
                        s.mini_width = obj.get("width").and_then(Value::as_f64).unwrap_or(360.0);
                        s.mini_height = obj.get("height").and_then(Value::as_f64).unwrap_or(104.0);
                    }
                    self.save_settings();
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// Zustand an den Mini-Player (im UI-Thread; ohne offenen Mini-Player passiert nichts)
    pub fn sync_mini(self: &Arc<Self>) {
        if !mini::is_open() {
            return;
        }
        let (state, art) = {
            let s = self.state.lock().unwrap();
            let t = s.current();
            let can_like = t.is_some_and(|t| t.source == "youtube") && self.yt_logged_in.load(std::sync::atomic::Ordering::Relaxed);
            (
                mini::PlayerState {
                    key: t.map(|t| format!("{}:{}", t.source, t.id)),
                    title: t.map(|t| t.title.clone()),
                    artist: t.and_then(|t| t.artist.clone()),
                    playing: s.playing,
                    loading: s.loading,
                    position: s.position,
                    duration: s.duration,
                    can_like,
                    liked: t.is_some_and(|t| self.liked.lock().unwrap().contains(&t.id)),
                    volume: s.volume as f64,
                    muted: s.muted,
                },
                t.and_then(cover_url),
            )
        };
        // Cover nur bei neuem Song laden
        if *self.mini_cover_key.lock().unwrap() != state.key {
            *self.mini_cover_key.lock().unwrap() = state.key.clone();
            let key = state.key.clone();
            let app = self.clone();
            self.rt.spawn(async move {
                let letterboxed = art.as_deref().is_some_and(|u| u.contains("ytimg.com") && ["hqdefault", "mqdefault", "sddefault"].iter().any(|n| u.contains(n)));
                let bytes = match art {
                    Some(url) => app.covers.raw(&url).await,
                    None => None,
                };
                let _ = slint::invoke_from_event_loop(move || {
                    if *app.mini_cover_key.lock().unwrap() == key {
                        mini::cover(bytes, letterboxed);
                    }
                });
            });
        }
        mini::update(state);
    }

    pub fn sync_mini_theme(self: &Arc<Self>) {
        let Some(ui) = self.ui.upgrade() else { return };
        let t = ui.global::<crate::Theme>();
        mini::theme(mini::Theme {
            bg: rgba(t.get_bg()),
            surface: rgba(t.get_surface()),
            hover: rgba(t.get_hover()),
            border: rgba(t.get_border()),
            text: rgba(t.get_text()),
            text_dim: rgba(t.get_text_dim()),
            accent: rgba(t.get_accent()),
        });
    }
}

use slint::ComponentHandle;
