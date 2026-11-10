//! Zusammen hören (wie together.svelte.js): Der Gastgeber schreibt Song, Stelle und Play/Pause in
//! rooms/{CODE}; Gäste folgen (hier: Abfrage alle 2 Sekunden statt Live-Verbindung).
//! Gäste melden sich unter rooms/{CODE}/members/{uid}.

use std::sync::{atomic::Ordering, Arc};

use serde_json::{json, Value};

use crate::{player::Track, store::now_ms, App};

const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(Clone, Default)]
pub struct Room {
    /// "host" | "guest"
    pub role: String,
    pub code: String,
    pub host_name: String,
    pub members: Vec<String>,
}

fn new_code() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    (0..6).map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char).collect()
}

/// Song für andere: lokale Pfade bleiben privat
fn shareable(track: Option<&Track>) -> Value {
    match track {
        Some(t) => {
            let mut t = t.clone();
            t.path = None;
            serde_json::to_value(t).unwrap_or(Value::Null)
        }
        None => Value::Null,
    }
}

impl App {
    fn room_state(&self) -> Value {
        let s = self.state.lock().unwrap();
        json!({
            "track": shareable(s.current()),
            "playing": s.playing,
            "position": s.position,
            "at": now_ms(),
        })
    }

    pub fn together_start(self: &Arc<Self>) {
        let Some(session) = self.account.logged_in() else {
            self.toast("Bitte zuerst im Wavely-Konto anmelden (Einstellungen).", true);
            return;
        };
        let app = self.clone();
        self.rt.spawn(async move {
            app.together_leave_now().await;
            let code = new_code();
            let mut data = app.room_state();
            data["host"] = json!(session.uid);
            data["hostName"] = json!(session.name);
            if let Err(e) = app.account.set_doc(&format!("rooms/{code}"), &data, false).await {
                app.toast(&format!("Raum konnte nicht geöffnet werden: {e}"), true);
                return;
            }
            *app.room.lock().unwrap() = Some(Room { role: "host".into(), code: code.clone(), host_name: session.name, members: Vec::new() });
            let generation = app.room_generation.fetch_add(1, Ordering::SeqCst) + 1;
            app.toast(&format!("Zusammen hören: Code {code} – Einladungslink wavely://join/{code}"), false);
            app.sync_together();
            // Stand alle 2 Sekunden weitergeben, Gäste alle 15 Sekunden zählen
            let mut tick = 0u32;
            let mut last = Value::Null;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                if app.room_generation.load(Ordering::SeqCst) != generation {
                    return;
                }
                let state = app.room_state();
                let changed = state["track"] != last["track"]
                    || state["playing"] != last["playing"]
                    || (state["position"].as_f64().unwrap_or(0.0) - last["position"].as_f64().unwrap_or(0.0) - 2.0).abs() > 3.0
                    || tick % 5 == 0;
                if changed {
                    let _ = app.account.set_doc(&format!("rooms/{code}"), &state, true).await;
                    last = state;
                }
                if tick % 8 == 0 {
                    if let Ok(members) = app.account.list_docs(&format!("rooms/{code}/members")).await {
                        let now = now_ms();
                        let names: Vec<String> = members
                            .into_iter()
                            .filter(|m| now.saturating_sub(m["lastSeen"].as_u64().unwrap_or(0)) < 90_000)
                            .map(|m| m["name"].as_str().unwrap_or("Jemand").to_string())
                            .collect();
                        if let Some(room) = app.room.lock().unwrap().as_mut() {
                            room.members = names;
                        }
                        app.sync_together();
                    }
                }
                tick += 1;
            }
        });
    }

    pub fn together_join(self: &Arc<Self>, code: String) {
        let Some(session) = self.account.logged_in() else {
            self.toast("Zum Mithören bitte zuerst im Wavely-Konto anmelden (Einstellungen).", true);
            return;
        };
        let code: String = code.to_uppercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        if code.len() != 6 {
            self.toast("Der Code hat 6 Zeichen.", true);
            return;
        }
        let app = self.clone();
        self.rt.spawn(async move {
            app.together_leave_now().await;
            let room = match app.account.get_doc(&format!("rooms/{code}")).await {
                Ok(Some(room)) => room,
                Ok(None) => return app.toast("Diesen Raum gibt es nicht (mehr).", true),
                Err(e) => return app.toast(&e, true),
            };
            if room["host"].as_str() == Some(session.uid.as_str()) {
                return app.toast("Das ist dein eigener Raum.", true);
            }
            let host_name = room["hostName"].as_str().unwrap_or("Jemand").to_string();
            *app.room.lock().unwrap() = Some(Room { role: "guest".into(), code: code.clone(), host_name: host_name.clone(), members: Vec::new() });
            app.autoplay_blocked.store(true, Ordering::Relaxed);
            let generation = app.room_generation.fetch_add(1, Ordering::SeqCst) + 1;
            app.toast(&format!("Du hörst jetzt mit {host_name} zusammen"), false);
            app.sync_together();
            let member = format!("rooms/{code}/members/{}", session.uid);
            let mut current_key = String::new();
            // Uhr-Abstand zum Gastgeber (siehe `follow`)
            let mut clock: (u64, Option<i64>) = (0, None);
            let mut tick = 0u32;
            loop {
                if app.room_generation.load(Ordering::SeqCst) != generation {
                    return;
                }
                if tick % 15 == 0 {
                    let _ = app.account.set_doc(&member, &json!({ "name": session.name, "lastSeen": now_ms() }), false).await;
                }
                match app.account.get_doc(&format!("rooms/{code}")).await {
                    Ok(Some(room)) if !room["ended"].as_bool().unwrap_or(false) => {
                        let a = app.clone();
                        a.follow(&room, &mut current_key, &mut clock);
                    }
                    Ok(_) => {
                        app.toast(&format!("{host_name} hat „Zusammen hören“ beendet"), false);
                        app.together_leave_now().await;
                        return;
                    }
                    Err(_) => {}
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                tick += 2;
            }
        });
    }

    /// Gast: Song, Stelle und Play/Pause des Gastgebers übernehmen
    ///
    /// `at` ist die Uhrzeit des Gastgeber-PCs. Gehen die Uhren beider PCs auseinander, wäre die
    /// erwartete Stelle dauerhaft falsch und der Gast würde alle 2 Sekunden spulen (springende
    /// Wiedergabeleiste). Deshalb wird der Abstand geschätzt: der kleinste Wert von
    /// „hier empfangen − dort geschrieben“ ist Uhr-Abstand plus kürzeste Übertragungszeit.
    /// Gezählt wird nur ein frisch geänderter Stand (der erste beim Beitreten kann alt sein).
    /// `clock` = (zuletzt gesehenes `at`, geschätzter Abstand in ms).
    fn follow(self: &Arc<Self>, room: &Value, current_key: &mut String, clock: &mut (u64, Option<i64>)) {
        let Ok(track) = serde_json::from_value::<Track>(room["track"].clone()) else { return };
        if track.id.is_empty() {
            return;
        }
        let key = format!("{}:{}", track.source, track.id);
        let playing = room["playing"].as_bool().unwrap_or(false);
        let at = room["at"].as_u64().unwrap_or(0);
        let now = now_ms();
        let seen = now as i64 - at as i64;
        if clock.0 != 0 && clock.0 != at {
            clock.1 = Some(clock.1.map_or(seen, |o| o.min(seen)));
        }
        clock.0 = at;
        let offset = clock.1.unwrap_or(0);
        let elapsed = if playing { ((seen - offset).max(0) as f64) / 1000.0 } else { 0.0 };
        let expected = room["position"].as_f64().unwrap_or(0.0) + elapsed;
        let app = self.clone();
        if key != *current_key {
            *current_key = key;
            let _ = slint::invoke_from_event_loop(move || {
                app.play_list(vec![track], 0);
                let a = app.clone();
                slint::Timer::single_shot(std::time::Duration::from_secs(2), move || a.seek(expected + 2.0));
            });
            return;
        }
        let _ = slint::invoke_from_event_loop(move || {
            let (mine, position, loading) = {
                let s = app.state.lock().unwrap();
                (s.playing, s.position, s.loading)
            };
            if loading {
                return;
            }
            if mine != playing {
                app.toggle_play();
            }
            if (position - expected).abs() > 2.5 {
                app.seek(expected);
            }
        });
    }

    async fn together_leave_now(self: &Arc<Self>) {
        self.room_generation.fetch_add(1, Ordering::SeqCst);
        self.autoplay_blocked.store(false, Ordering::Relaxed);
        let room = self.room.lock().unwrap().take();
        self.sync_together();
        let (Some(room), Some(session)) = (room, self.account.logged_in()) else { return };
        if room.role == "host" {
            let _ = self.account.set_doc(&format!("rooms/{}", room.code), &json!({ "ended": true }), true).await;
            if let Ok(members) = self.account.list_names(&format!("rooms/{}/members", room.code)).await {
                for id in members {
                    let _ = self.account.delete_doc(&format!("rooms/{}/members/{id}", room.code)).await;
                }
            }
            let _ = self.account.delete_doc(&format!("rooms/{}", room.code)).await;
        } else {
            let _ = self.account.delete_doc(&format!("rooms/{}/members/{}", room.code, session.uid)).await;
        }
    }

    pub async fn together_leave_async(self: &Arc<Self>) {
        self.together_leave_now().await;
    }

    /// Feedback aus dem Dialog schicken
    pub fn send_feedback(self: &Arc<Self>, kind: String, text: String, contact: String, with_info: bool) {
        if text.trim().is_empty() {
            self.toast("Bitte schreib kurz, worum es geht.", true);
            return;
        }
        let app = self.clone();
        self.rt.spawn(async move {
            match app.account.send_feedback(&kind, &text, &contact, with_info).await {
                Ok(()) => {
                    app.toast("Danke für dein Feedback!", false);
                    app.ui(|ui| {
                        use slint::ComponentHandle;
                        let f = ui.global::<crate::Feedback>();
                        f.set_open(false);
                        f.set_text(Default::default());
                    });
                }
                Err(e) => app.toast(&e, true),
            }
        });
    }

    pub fn together_leave(self: &Arc<Self>) {
        let app = self.clone();
        self.rt.spawn(async move {
            app.together_leave_now().await;
            app.toast("„Zusammen hören“ beendet", false);
        });
    }

    /// Menü am Knopf in der Player-Leiste
    pub fn open_together_menu(self: &Arc<Self>, x: f32, y: f32) {
        let room = self.room.lock().unwrap().clone();
        let item = |id: &str, label: String, icon: &str| crate::MenuItem { id: id.into(), label: label.into(), icon: icon.into(), separator: false, chevron: false };
        let items = match room {
            Some(r) if r.role == "host" => vec![
                item("together:copy", format!("Code {} · Link kopieren", r.code), crate::icon::COPY),
                item(
                    "together:none",
                    if r.members.is_empty() { "Noch niemand dabei".into() } else { format!("Dabei: {}", r.members.join(", ")) },
                    crate::icon::GROUP,
                ),
                item("together:leave", "Beenden".into(), crate::icon::CLOSE),
            ],
            Some(r) => vec![
                item("together:none", format!("Du hörst mit {}", r.host_name), crate::icon::GROUP),
                item("together:leave", "Verlassen".into(), crate::icon::CLOSE),
            ],
            None => vec![
                item("together:start", "Raum öffnen (andere hören mit)".into(), crate::icon::GROUP),
                item("together:join", "Mit Code beitreten…".into(), crate::icon::ADD_QUEUE),
            ],
        };
        *self.menu.lock().unwrap() = None;
        self.ui(move |ui| {
            use slint::ComponentHandle;
            let m = ui.global::<crate::Menu>();
            m.set_items(slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(items))));
            m.set_x(x);
            m.set_y(y);
            m.set_open(true);
        });
    }

    pub fn together_action(self: &Arc<Self>, action: &str) {
        self.close_menu();
        match action {
            "start" => self.together_start(),
            "leave" => self.together_leave(),
            "join" => {
                *self.dialog_target.lock().unwrap() = Some("together".into());
                self.ui(|ui| {
                    use slint::ComponentHandle;
                    let d = ui.global::<crate::Dialog>();
                    d.set_title("Mit Code beitreten".into());
                    d.set_value(Default::default());
                    d.set_confirm_text("Beitreten".into());
                    d.set_open(true);
                });
            }
            "copy" => {
                if let Some(code) = self.room.lock().unwrap().as_ref().map(|r| r.code.clone()) {
                    crate::host::copy_text(&format!("wavely://join/{code}"));
                    self.toast("Einladungslink kopiert", false);
                }
            }
            _ => {}
        }
    }

    pub fn sync_together(self: &Arc<Self>) {
        let active = self.room.lock().unwrap().is_some();
        self.ui(move |ui| {
            use slint::ComponentHandle;
            ui.global::<crate::Nav>().set_together_active(active);
        });
    }
}
