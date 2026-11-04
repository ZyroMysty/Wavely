// Kein Konsolenfenster in der fertigen App
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod covers;
mod host;
mod player;
mod services;

use std::{
    rc::Rc,
    sync::{Arc, Mutex},
};

use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};

use audio::{Command, Engine, Event};
use player::{Repeat, State, Track};
use services::{ytdlp::YtDlp, ytm::Ytm};

slint::include_modules!();

/// Alles, was die Oberfläche und die Hintergrund-Aufgaben gemeinsam brauchen
pub struct App {
    pub ui: Weak<AppWindow>,
    pub rt: tokio::runtime::Handle,
    pub engine: Arc<Engine>,
    pub ytm: Arc<Ytm>,
    pub state: Mutex<State>,
    pub covers: covers::Covers,
    /// Suchergebnisse (für „abspielen“)
    pub results: Mutex<Vec<Track>>,
}

pub fn format_time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "0:00".into();
    }
    let s = seconds as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn row(track: &Track) -> TrackRow {
    TrackRow {
        id: track.id.clone().into(),
        title: track.title.clone().into(),
        artist: track.artist.clone().unwrap_or_default().into(),
        album: track.album.clone().unwrap_or_default().into(),
        duration: if track.duration > 0.0 { format_time(track.duration).into() } else { SharedString::new() },
        cover: Default::default(),
        has_cover: false,
    }
}

impl App {
    /// Änderung im UI-Thread ausführen
    fn ui(self: &Arc<Self>, f: impl FnOnce(&AppWindow) + Send + 'static) {
        let weak = self.ui.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                f(&ui);
            }
        });
    }

    /// Player-Leiste und Warteschlange aus dem Zustand aktualisieren
    fn sync_player(self: &Arc<Self>) {
        let (title, artist, has_track, playing, loading, error, duration, volume, muted, shuffle, repeat, index, art) = {
            let s = self.state.lock().unwrap();
            let t = s.current();
            (
                t.map(|t| t.title.clone()).unwrap_or_default(),
                t.and_then(|t| t.artist.clone()).unwrap_or_default(),
                t.is_some(),
                s.playing,
                s.loading,
                s.error.clone().unwrap_or_default(),
                s.duration,
                s.volume,
                s.muted,
                s.shuffle,
                s.repeat,
                s.index.map(|i| i as i32).unwrap_or(-1),
                t.and_then(|t| t.artwork_url.clone()),
            )
        };
        let app = self.clone();
        self.ui(move |ui| {
            let p = ui.global::<Player>();
            p.set_title(title.into());
            p.set_artist(artist.into());
            p.set_has_track(has_track);
            p.set_playing(playing);
            p.set_loading(loading);
            p.set_error(error.into());
            p.set_duration(duration as f32);
            p.set_duration_text(format_time(duration).into());
            p.set_volume(volume);
            p.set_muted(muted);
            p.set_shuffle(shuffle);
            p.set_repeat(match repeat {
                Repeat::Off => "off",
                Repeat::All => "all",
                Repeat::One => "one",
            }
            .into());
            p.set_queue_index(index);
            match art {
                Some(url) => {
                    let weak = app.ui.clone();
                    app.covers.load(&app, url, 112, move |image| {
                        if let Some(ui) = weak.upgrade() {
                            ui.global::<Player>().set_cover(image);
                            ui.global::<Player>().set_has_cover(true);
                        }
                    });
                }
                None => p.set_has_cover(false),
            }
        });
    }

    fn sync_queue(self: &Arc<Self>) {
        let tracks: Vec<Track> = self.state.lock().unwrap().queue.clone();
        let app = self.clone();
        self.ui(move |ui| {
            let rows: Vec<TrackRow> = tracks.iter().map(row).collect();
            let model = Rc::new(VecModel::from(rows));
            ui.global::<Queue>().set_tracks(ModelRc::from(model.clone()));
            covers::fill(&app, &model, tracks.iter().map(|t| t.artwork_url.clone()).collect());
        });
    }

    pub fn play_list(self: &Arc<Self>, tracks: Vec<Track>, index: usize) {
        {
            let mut s = self.state.lock().unwrap();
            s.original = tracks.clone();
            if s.shuffle {
                let first = tracks[index].clone();
                let rest: Vec<Track> = tracks.into_iter().enumerate().filter(|(i, _)| *i != index).map(|(_, t)| t).collect();
                s.queue = std::iter::once(first).chain(player::shuffled(rest)).collect();
                s.index = Some(0);
            } else {
                s.queue = tracks;
                s.index = Some(index);
            }
        }
        self.sync_queue();
        let i = self.state.lock().unwrap().index.unwrap_or(0);
        self.load(i, 0.0);
    }

    pub fn load(self: &Arc<Self>, index: usize, start_at: f64) {
        let track = {
            let mut s = self.state.lock().unwrap();
            let Some(track) = s.queue.get(index).cloned() else { return };
            s.index = Some(index);
            s.position = start_at;
            s.duration = track.duration;
            s.loading = true;
            s.playing = false;
            s.error = None;
            track
        };
        self.sync_player();
        let app = self.clone();
        let title = track.title.clone();
        player::start_load(&self.rt, self.engine.clone(), self.ytm.clone(), track, start_at, move |result| {
            let mut s = app.state.lock().unwrap();
            if s.index != Some(index) {
                return;
            }
            match result {
                Ok(generation) => s.generation = generation,
                Err(e) => {
                    s.loading = false;
                    s.error = Some(format!("„{title}“ konnte nicht geladen werden: {e}"));
                    drop(s);
                    app.sync_player();
                }
            }
        });
        self.extend_with_autoplay();
    }

    pub fn toggle_play(self: &Arc<Self>) {
        let (has, playing, generation) = {
            let s = self.state.lock().unwrap();
            (s.current().is_some(), s.playing, s.generation)
        };
        if !has {
            return;
        }
        if generation == 0 {
            // Noch nichts geladen (z. B. nach Neustart)
            let (i, pos) = {
                let s = self.state.lock().unwrap();
                (s.index.unwrap_or(0), s.position)
            };
            self.load(i, pos);
            return;
        }
        self.engine.send(if playing { Command::Pause } else { Command::Play });
    }

    pub fn next(self: &Arc<Self>) {
        let target = {
            let s = self.state.lock().unwrap();
            match s.index {
                Some(i) if i + 1 < s.queue.len() => Some(i + 1),
                _ if s.repeat == Repeat::All && !s.queue.is_empty() => Some(0),
                _ => None,
            }
        };
        match target {
            Some(i) => self.load(i, 0.0),
            None => {
                self.engine.send(Command::Pause);
            }
        }
    }

    pub fn previous(self: &Arc<Self>) {
        let (position, index) = {
            let s = self.state.lock().unwrap();
            (s.position, s.index.unwrap_or(0))
        };
        if position > 3.0 || index == 0 {
            self.seek(0.0);
        } else {
            self.load(index - 1, 0.0);
        }
    }

    pub fn seek(self: &Arc<Self>, seconds: f64) {
        self.state.lock().unwrap().position = seconds;
        self.engine.send(Command::Seek(seconds));
    }

    pub fn set_volume(self: &Arc<Self>, volume: f32) {
        let gain = {
            let mut s = self.state.lock().unwrap();
            s.volume = volume;
            s.muted = false;
            player::gain(s.volume, s.muted)
        };
        self.engine.set_gain(gain);
        let app = self.clone();
        self.ui(move |ui| ui.global::<Player>().set_muted(false));
        drop(app);
    }

    pub fn toggle_mute(self: &Arc<Self>) {
        let gain = {
            let mut s = self.state.lock().unwrap();
            s.muted = !s.muted;
            player::gain(s.volume, s.muted)
        };
        self.engine.set_gain(gain);
        self.sync_player();
    }

    pub fn toggle_shuffle(self: &Arc<Self>) {
        {
            let mut s = self.state.lock().unwrap();
            s.shuffle = !s.shuffle;
            if let Some(i) = s.index {
                let current = s.queue[i].clone();
                if s.shuffle {
                    let rest: Vec<Track> = s.queue.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, t)| t.clone()).collect();
                    s.queue = std::iter::once(current).chain(player::shuffled(rest)).collect();
                    s.index = Some(0);
                } else {
                    let original = s.original.clone();
                    let position = original.iter().position(|t| t.id == current.id).unwrap_or(0);
                    if !original.is_empty() {
                        s.queue = original;
                        s.index = Some(position);
                    }
                }
            }
        }
        self.sync_queue();
        self.sync_player();
    }

    pub fn cycle_repeat(self: &Arc<Self>) {
        {
            let mut s = self.state.lock().unwrap();
            s.repeat = match s.repeat {
                Repeat::Off => Repeat::All,
                Repeat::All => Repeat::One,
                Repeat::One => Repeat::Off,
            };
        }
        self.sync_player();
    }

    /// Nach dem Ende der Warteschlange passende Songs anhängen (YouTube-Radio)
    fn extend_with_autoplay(self: &Arc<Self>) {
        let seed = {
            let s = self.state.lock().unwrap();
            let near_end = s.index.is_some_and(|i| i + 2 >= s.queue.len());
            if !near_end || s.repeat != Repeat::Off {
                return;
            }
            match s.current() {
                Some(t) if t.source == "youtube" => t.id.clone(),
                _ => return,
            }
        };
        let app = self.clone();
        self.rt.spawn(async move {
            let Ok(radio) = services::ytm::ytm_radio(&app.ytm, seed).await else { return };
            {
                let mut s = app.state.lock().unwrap();
                let known: std::collections::HashSet<String> = s.queue.iter().map(|t| t.id.clone()).collect();
                let fresh: Vec<Track> = radio.into_iter().map(Track::from).filter(|t| !known.contains(&t.id)).take(20).collect();
                if fresh.is_empty() {
                    return;
                }
                s.queue.extend(fresh);
            }
            app.sync_queue();
        });
    }

    fn on_audio(self: &Arc<Self>, event: Event) {
        match event {
            Event::Started { generation, duration } => {
                {
                    let mut s = self.state.lock().unwrap();
                    if generation != s.generation {
                        return;
                    }
                    s.loading = false;
                    s.playing = true;
                    s.error_streak = 0;
                    if duration > 0.0 {
                        s.duration = duration;
                    }
                }
                self.sync_player();
            }
            Event::Position { generation, seconds } => {
                let (position, duration) = {
                    let mut s = self.state.lock().unwrap();
                    if generation != s.generation {
                        return;
                    }
                    s.position = seconds;
                    (seconds, s.duration)
                };
                self.ui(move |ui| {
                    let p = ui.global::<Player>();
                    p.set_position(position as f32);
                    p.set_position_text(format_time(position.min(duration.max(position))).into());
                });
            }
            Event::Playing(playing) => {
                self.state.lock().unwrap().playing = playing;
                self.ui(move |ui| ui.global::<Player>().set_playing(playing));
            }
            Event::Ended { generation } => {
                let repeat_one = {
                    let s = self.state.lock().unwrap();
                    if generation != s.generation {
                        return;
                    }
                    s.repeat == Repeat::One
                };
                if repeat_one {
                    let i = self.state.lock().unwrap().index.unwrap_or(0);
                    self.load(i, 0.0);
                } else {
                    self.next();
                }
            }
            Event::Error { generation, message } => {
                let skip = {
                    let mut s = self.state.lock().unwrap();
                    if generation != s.generation {
                        return;
                    }
                    s.loading = false;
                    s.playing = false;
                    s.error_streak += 1;
                    let title = s.current().map(|t| t.title.clone()).unwrap_or_default();
                    s.error = Some(format!("„{title}“ konnte nicht abgespielt werden ({message})"));
                    let more = s.index.is_some_and(|i| i + 1 < s.queue.len());
                    if s.error_streak >= 3 {
                        s.error = Some("Mehrere Songs hintereinander konnten nicht abgespielt werden – Wiedergabe angehalten.".into());
                        s.error_streak = 0;
                        false
                    } else {
                        more
                    }
                };
                self.sync_player();
                if skip {
                    self.next();
                }
            }
        }
    }

    fn search(self: &Arc<Self>, query: String) {
        let query = query.trim().to_string();
        if query.is_empty() {
            return;
        }
        self.ui(|ui| ui.global::<YouTube>().set_searching(true));
        let app = self.clone();
        self.rt.spawn(async move {
            let result = services::ytm::ytm_search(&app.ytm, query).await;
            let tracks: Vec<Track> = match result {
                Ok(r) => r.tracks.into_iter().map(Track::from).collect(),
                Err(e) => {
                    app.ui(move |ui| {
                        let y = ui.global::<YouTube>();
                        y.set_searching(false);
                        y.set_message(format!("Suche fehlgeschlagen: {e}").into());
                        y.set_results(ModelRc::default());
                    });
                    return;
                }
            };
            *app.results.lock().unwrap() = tracks.clone();
            // Test-Hilfe: WAVELY_TEST_PLAY=<Suchbegriff> spielt das erste Ergebnis
            if std::env::var("WAVELY_TEST_PLAY").is_ok() && !tracks.is_empty() {
                let a = app.clone();
                let list = tracks.clone();
                let _ = slint::invoke_from_event_loop(move || a.play_list(list, 0));
            }
            let app2 = app.clone();
            app.ui(move |ui| {
                let rows: Vec<TrackRow> = tracks.iter().map(row).collect();
                let model = Rc::new(VecModel::from(rows));
                let y = ui.global::<YouTube>();
                y.set_searching(false);
                y.set_message(if tracks.is_empty() { "Nichts gefunden".into() } else { SharedString::new() });
                y.set_results(ModelRc::from(model.clone()));
                covers::fill(&app2, &model, tracks.iter().map(|t| t.artwork_url.clone()).collect());
            });
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    let paths = host::Paths::init()?;
    let ytdlp = YtDlp::init(&paths)?;
    let ytm = Arc::new(Ytm::new(&paths, ytdlp)?);

    let ui = AppWindow::new()?;

    let app_slot: Arc<Mutex<Option<Arc<App>>>> = Arc::new(Mutex::new(None));
    let slot = app_slot.clone();
    let engine = Engine::start(move |event| {
        if let Some(app) = slot.lock().unwrap().clone() {
            app.on_audio(event);
        }
    });

    let app = Arc::new(App {
        ui: ui.as_weak(),
        rt: rt.handle().clone(),
        engine,
        ytm,
        state: Mutex::new(State { volume: 0.6, ..Default::default() }),
        covers: covers::Covers::default(),
        results: Mutex::new(Vec::new()),
    });
    *app_slot.lock().unwrap() = Some(app.clone());
    app.engine.set_gain(player::gain(0.6, false));
    // Tests laufen stumm
    if std::env::var("WAVELY_TEST_PLAY").is_ok() {
        app.state.lock().unwrap().muted = true;
        app.engine.set_gain(0.0);
    }

    let p = ui.global::<Player>();
    let a = app.clone();
    p.on_toggle_play(move || a.toggle_play());
    let a = app.clone();
    p.on_next(move || a.next());
    let a = app.clone();
    p.on_previous(move || a.previous());
    let a = app.clone();
    p.on_seek(move |v| a.seek(v as f64));
    let a = app.clone();
    p.on_set_volume(move |v| a.set_volume(v));
    let a = app.clone();
    p.on_toggle_mute(move || a.toggle_mute());
    let a = app.clone();
    p.on_toggle_shuffle(move || a.toggle_shuffle());
    let a = app.clone();
    p.on_cycle_repeat(move || a.cycle_repeat());

    let y = ui.global::<YouTube>();
    let a = app.clone();
    y.on_search(move |q| a.search(q.to_string()));
    let a = app.clone();
    y.on_play(move |i| {
        let tracks = a.results.lock().unwrap().clone();
        if (i as usize) < tracks.len() {
            a.play_list(tracks, i as usize);
        }
    });

    let a = app.clone();
    ui.global::<Queue>().on_jump(move |i| a.load(i as usize, 0.0));

    // yt-dlp im Hintergrund aktuell halten
    let a = app.clone();
    rt.spawn(async move { a.ytm.update_tools().await });

    if let Ok(query) = std::env::var("WAVELY_TEST_PLAY") {
        ui.global::<YouTube>().set_query(query.clone().into());
        app.search(query);
    }

    ui.run()?;
    app.engine.send(Command::Stop);
    Ok(())
}

