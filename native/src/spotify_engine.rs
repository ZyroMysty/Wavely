//! Echte Spotify-Wiedergabe für Premium-Konten (wie spotifyPlayer.svelte.js): Ein unsichtbares
//! Browserfenster lädt den Spotify Web Playback SDK und meldet sich als Gerät „Wavely“ an.
//! Es entsteht erst beim ersten Spotify-Song und schließt sich, wenn wieder etwas anderes läuft.
//! Free-Konten meldet der SDK mit „account_error“ – dann geht es über YouTube Music weiter.

use std::sync::{atomic::Ordering, Arc};

use serde_json::Value;

use crate::{services::spotify, win::web, App};

const WEB_ID: &str = "spotify-player";

/// Seite mit dem SDK; Zustand geht jede Sekunde per IPC an Wavely
const PAGE: &str = r#"<!doctype html><meta charset="utf-8"><title>Wavely Spotify</title>
<script>
let player = null;
let tokenCallbacks = [];
const send = (msg) => window.ipc.postMessage(JSON.stringify(msg));
window.__token = (token) => { const cbs = tokenCallbacks; tokenCallbacks = []; cbs.forEach((cb) => cb(token)); };
window.__cmd = async (cmd, value) => {
  if (!player) return;
  if (cmd === "pause") player.pause();
  if (cmd === "resume") player.resume();
  if (cmd === "seek") player.seek(value);
  if (cmd === "volume") player.setVolume(value);
};
window.onSpotifyWebPlaybackSDKReady = () => {
  player = new Spotify.Player({
    name: "Wavely",
    getOAuthToken: (cb) => { tokenCallbacks.push(cb); send({ type: "token" }); },
    volume: 0.6,
  });
  player.addListener("ready", ({ device_id }) => send({ type: "ready", deviceId: device_id }));
  player.addListener("not_ready", () => send({ type: "not_ready" }));
  player.addListener("account_error", () => send({ type: "account_error" }));
  player.addListener("authentication_error", ({ message }) => send({ type: "auth_error", message }));
  player.addListener("initialization_error", ({ message }) => send({ type: "init_error", message }));
  player.connect();
  setInterval(async () => {
    const state = await player.getCurrentState().catch(() => null);
    if (!state) return;
    const t = state.track_window && state.track_window.current_track;
    send({ type: "state", playing: !state.paused, position: state.position / 1000, duration: state.duration / 1000,
           ids: [t && t.id, t && t.linked_from && t.linked_from.id].filter(Boolean) });
  }, 1000);
};
</script>
<script src="https://sdk.scdn.co/spotify-player.js"></script>"#;

/// Zustand der Spotify-Wiedergabe
#[derive(Default)]
pub struct SpotifyPlayback {
    /// Gerät „Wavely“ bereit
    pub device_id: Option<String>,
    /// None = unbekannt, Some(false) = Free (→ YouTube Music)
    pub premium: Option<bool>,
    /// Läuft gerade ein Song über Spotify? Dann dessen ID
    pub active: Option<String>,
    pub started: bool,
    pub near_end: bool,
    pub finished: bool,
}

impl App {
    /// Kann dieser Song echt über Spotify laufen?
    pub fn can_play_on_spotify(&self, track: &crate::player::Track) -> bool {
        track.source == "spotify" && self.spotify_logged_in() && self.spotify_playback.lock().unwrap().premium != Some(false)
    }

    /// Unsichtbares Spotify-Fenster öffnen (falls nötig) und auf das Gerät warten
    async fn spotify_device(self: &Arc<Self>) -> Option<String> {
        if let Some(id) = self.spotify_playback.lock().unwrap().device_id.clone() {
            if web::is_open(WEB_ID) {
                return Some(id);
            }
        }
        let app = self.clone();
        let _ = slint::invoke_from_event_loop(move || app.open_spotify_web());
        for _ in 0..60 {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            let p = self.spotify_playback.lock().unwrap();
            if p.premium == Some(false) {
                return None;
            }
            if let Some(id) = p.device_id.clone() {
                return Some(id);
            }
        }
        None
    }

    fn open_spotify_web(self: &Arc<Self>) {
        if web::is_open(WEB_ID) {
            return;
        }
        let app = self.clone();
        let closed = self.clone();
        let result = web::open(
            WEB_ID,
            web::Options {
                title: "Wavely Spotify".into(),
                url: String::new(),
                size: (400, 300),
                visible: false,
                data_dir: self.paths.cache.join("spotify-webview"),
                html: Some(("wavelyspotify".into(), PAGE.into())),
            },
            |_| true,
            |_| {},
            move |message| {
                let app = app.clone();
                let _ = slint::invoke_from_event_loop(move || app.spotify_message(&message));
            },
            move || {
                let mut p = closed.spotify_playback.lock().unwrap();
                p.device_id = None;
                p.active = None;
            },
        );
        if let Err(e) = result {
            eprintln!("Spotify-Fenster: {e}");
            self.spotify_playback.lock().unwrap().premium = Some(false);
        }
    }

    fn spotify_message(self: &Arc<Self>, message: &str) {
        let Ok(msg) = serde_json::from_str::<Value>(message) else { return };
        match msg["type"].as_str() {
            Some("token") => {
                let app = self.clone();
                self.rt.spawn(async move {
                    let token = spotify::spotify_access_token(&app.spotify).await.unwrap_or_default();
                    let script = format!("window.__token({})", serde_json::to_string(&token).unwrap_or_default());
                    let _ = slint::invoke_from_event_loop(move || web::eval(WEB_ID, &script));
                });
            }
            Some("ready") => {
                self.spotify_playback.lock().unwrap().device_id = msg["deviceId"].as_str().map(str::to_string);
            }
            Some("not_ready") => self.spotify_playback.lock().unwrap().device_id = None,
            Some("account_error") => {
                // Kein Premium: Fenster wieder schließen, ab jetzt über YouTube Music
                self.spotify_playback.lock().unwrap().premium = Some(false);
                web::close(WEB_ID);
            }
            Some("auth_error") => {
                self.toast("Für die Spotify-Wiedergabe bitte einmal bei Spotify ab- und wieder anmelden.", true);
                self.spotify_playback.lock().unwrap().premium = Some(false);
                web::close(WEB_ID);
            }
            Some("state") => self.spotify_state(&msg),
            _ => {}
        }
    }

    /// Song über Spotify starten; `Err`, wenn es nur über YouTube Music geht
    pub async fn play_on_spotify(self: &Arc<Self>, track_id: &str, start_at: f64) -> Result<(), ()> {
        let device = self.spotify_device().await.ok_or(())?;
        match spotify::spotify_play(&self.spotify, Some(device), track_id.to_string(), Some((start_at * 1000.0) as u64)).await {
            Ok(()) => {
                let mut p = self.spotify_playback.lock().unwrap();
                p.premium = Some(true);
                p.active = Some(track_id.to_string());
                p.started = false;
                p.near_end = false;
                p.finished = false;
                drop(p);
                // Lautstärke wie im normalen Player
                let gain = {
                    let s = self.state.lock().unwrap();
                    crate::player::gain(s.volume, s.muted)
                };
                let _ = slint::invoke_from_event_loop(move || web::eval(WEB_ID, &format!("window.__cmd('volume', {gain})")));
                Ok(())
            }
            Err(e) => {
                if e == "PREMIUM_REQUIRED" {
                    self.spotify_playback.lock().unwrap().premium = Some(false);
                }
                Err(())
            }
        }
    }

    /// Läuft gerade ein Song über Spotify?
    pub fn spotify_active(&self) -> bool {
        self.spotify_playback.lock().unwrap().active.is_some()
    }

    /// Andere Quelle spielt: Spotify anhalten und Fenster schließen (Speicher frei)
    pub fn spotify_stop(self: &Arc<Self>) {
        let active = self.spotify_playback.lock().unwrap().active.take();
        if active.is_some() {
            web::eval(WEB_ID, "window.__cmd('pause')");
            slint::Timer::single_shot(std::time::Duration::from_millis(500), || web::close(WEB_ID));
        }
    }

    pub fn spotify_command(self: &Arc<Self>, command: &str, value: f64) {
        web::eval(WEB_ID, &format!("window.__cmd('{command}', {value})"));
    }

    /// Zustand vom SDK übernehmen (Fortschritt, Play/Pause, Song-Ende)
    fn spotify_state(self: &Arc<Self>, msg: &Value) {
        let Some(active) = self.spotify_playback.lock().unwrap().active.clone() else { return };
        let playing = msg["playing"].as_bool().unwrap_or(false);
        let position = msg["position"].as_f64().unwrap_or(0.0);
        let duration = msg["duration"].as_f64().unwrap_or(0.0);
        let ours = msg["ids"].as_array().is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(active.as_str())));
        let (started_now, finished) = {
            let mut p = self.spotify_playback.lock().unwrap();
            let mut started_now = false;
            let mut finished = false;
            if ours {
                if playing && !p.started {
                    p.started = true;
                    started_now = true;
                }
                if duration > 0.0 && position >= duration - 2.5 {
                    p.near_end = true;
                }
                if p.started && p.near_end && (!playing || position < 2.0) && !p.finished {
                    p.finished = true;
                    finished = true;
                }
            } else if p.started && !p.finished {
                // Spotify spielt etwas anderes (eigenes Autoplay) → unser Song ist vorbei
                p.finished = true;
                finished = true;
            }
            (started_now, finished)
        };
        if ours {
            {
                let mut s = self.state.lock().unwrap();
                s.playing = playing;
                s.position = position;
                s.loading = false;
                if duration > 0.0 {
                    s.duration = duration;
                }
            }
            self.stats_progress(position);
            self.lyrics_progress(position);
            self.ui(move |ui| {
                use slint::ComponentHandle;
                let p = ui.global::<crate::Player>();
                p.set_playing(playing);
                p.set_position(position as f32);
                p.set_position_text(crate::format_time(position).into());
                p.set_duration(duration as f32);
                p.set_duration_text(crate::format_time(duration).into());
            });
            if self.last_resume_save.lock().unwrap().elapsed().as_secs() >= 5 {
                self.save_resume();
            }
        }
        if started_now {
            self.started_playing();
            self.sync_player();
            self.send_presence();
        }
        if finished {
            if self.consume_end_of_track() {
                self.spotify_command("pause", 0.0);
                return;
            }
            let repeat_one = self.state.lock().unwrap().repeat == crate::player::Repeat::One;
            if repeat_one {
                let i = self.state.lock().unwrap().index.unwrap_or(0);
                self.load(i, 0.0);
            } else {
                self.next();
            }
        }
        let _ = Ordering::Relaxed;
    }
}
