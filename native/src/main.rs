// Kein Konsolenfenster in der fertigen App
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod account;
mod audio;
mod covers;
mod host;
mod icon;
mod keys;
mod local;
mod menu;
#[cfg(windows)]
mod mini_ui;
mod nav;
mod playlists;
mod recap;
mod resolve;
mod offline;
mod pages;
mod panel;
mod player;
mod services;
mod settings;
mod soundcloud_view;
mod spotify_view;
mod store;
mod together;
mod instance;
mod wavely;
mod youtube;
#[cfg(windows)]
mod win;

use std::{
    rc::Rc,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use slint::{ComponentHandle, ModelRc, VecModel, Weak};

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
    /// Gewählte Ansicht der Seitenleiste
    pub view: Mutex<String>,
    /// Seitenverlauf je Ansicht (letzte = aktuelle Seite)
    pub stacks: Mutex<std::collections::HashMap<String, Vec<nav::PageKind>>>,
    /// Gerade angezeigte Seite (für Klicks)
    pub page: Mutex<pages::PageData>,
    /// Zählt Seitenwechsel: verspätete Ergebnisse alter Seiten werden verworfen
    pub page_token: AtomicU64,
    pub yt_logged_in: AtomicBool,
    /// Gelikte YouTube-Songs (Video-IDs)
    pub liked: Mutex<std::collections::HashSet<String>>,
    pub playlists: Mutex<playlists::Playlists>,
    /// Charts für die Startseite
    pub charts: Mutex<Option<services::ytm::YtHome>>,
    pub charts_loading: AtomicBool,
    pub menu: Mutex<Option<menu::MenuContext>>,
    pub toast_token: AtomicU64,
    /// Namensdialog: umzubenennende Playlist (None = neue) und Songs für eine neue Playlist
    pub dialog_target: Mutex<Option<String>>,
    pub dialog_tracks: Mutex<Vec<Track>>,
    pub store: store::Store,
    pub paths: host::Paths,
    pub local_tracks: Mutex<Vec<Track>>,
    pub offline: offline::Offline,
    pub soundcloud: Arc<services::soundcloud::SoundCloud>,
    pub spotify: Arc<services::spotify::Spotify>,
    pub lastfm: Arc<services::lastfm::LastFm>,
    /// Schlaf-Timer: Ende, „am Ende des Songs“, Zähler (ältere Timer verfallen)
    pub sleep_until: Mutex<Option<std::time::Instant>>,
    /// Zeitraum im Rückblick
    pub recap_range: Mutex<String>,
    /// Zusammen hören
    pub room: Mutex<Option<together::Room>>,
    pub room_generation: AtomicU64,
    /// Beim Mithören bestimmt der Gastgeber, was als Nächstes kommt
    pub autoplay_blocked: AtomicBool,
    /// Song, dessen Cover der Mini-Player gerade hat
    pub mini_cover_key: Mutex<Option<String>>,
    pub sleep_end_of_track: AtomicBool,
    pub sleep_token: AtomicU64,
    /// Gemessene Lautheit je Song (für die Angleichung)
    pub loudness: Mutex<std::collections::HashMap<String, f32>>,
    /// Zuordnungen Spotify/SoundCloud → YouTube
    pub matches: resolve::Matches,
    /// Discord-Status (Verbindung zum Discord-Client)
    pub presence: Arc<Mutex<services::presence::Presence>>,
    /// Offenes Seitenfeld rechts und dessen Songs/Zeilen
    pub panel: Mutex<String>,
    pub panel_tracks: Mutex<Vec<Track>>,
    pub panel_rows: Mutex<Vec<Option<usize>>>,
    /// Liedtext: Song-Schlüssel, Ergebnis, Zeilen mit Zeit, lädt gerade
    pub lyrics: Mutex<(String, Option<services::lyrics::Lyrics>, Vec<(f64, String)>, bool)>,
    pub local_loading: Mutex<bool>,
    pub local_watcher: services::library::Watcher,
    pub account: account::Account,
    /// Zählt Anmeldungen: ein alter Abgleich hört auf, wenn sich jemand neu anmeldet
    pub sync_generation: AtomicU64,
    pub sync_status: Mutex<String>,
    pub account_busy: Mutex<bool>,
    pub account_error: Mutex<Option<String>>,
    /// Neue Hör-Einträge, die noch ins Konto müssen
    pub plays_to_upload: Mutex<Vec<serde_json::Value>>,
    pub settings: Mutex<store::Settings>,
    pub history: Mutex<Vec<store::HistoryEntry>>,
    /// Laufender Song für die Hörstatistik
    pub listening: Mutex<Option<Listening>>,
    /// Zeitpunkt der letzten Sicherung der Warteschlange
    pub last_resume_save: Mutex<std::time::Instant>,
}

/// Gerade gehörter Song: nur zusammenhängend gehörte Zeit zählt (wie stats.svelte.js)
pub struct Listening {
    pub track: Track,
    pub started_at: u64,
    pub listened: f64,
    pub last_pos: f64,
    /// Schon an Last.fm gemeldet
    pub scrobbled: bool,
}

/// Mindestens so lange gehört, damit ein Song in die Statistik kommt
const MIN_LISTENED: f64 = 10.0;

pub fn format_time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "0:00".into();
    }
    let s = seconds as u64;
    format!("{}:{:02}", s / 60, s % 60)
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
                t.and_then(covers::cover_url),
            )
        };
        let (can_like, liked) = {
            let s = self.state.lock().unwrap();
            match s.current() {
                Some(t) if t.source == "youtube" && self.yt_logged_in.load(Ordering::Relaxed) => {
                    (true, self.liked.lock().unwrap().contains(&t.id))
                }
                _ => (false, false),
            }
        };
        let app = self.clone();
        let art_url = art.clone();
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
            // Windows: Medientasten-Overlay und Taskleisten-Buttons
            #[cfg(windows)]
            {
                win::media::update(win::media::MediaInfo {
                    has_track,
                    playing,
                    title: has_track.then(|| p.get_title().to_string()),
                    artist: has_track.then(|| p.get_artist().to_string()),
                    album: None,
                    artwork_url: art_url.clone().filter(|u| u.starts_with("http")),
                });
                win::taskbar::update(win::taskbar::TaskbarState { has_track, playing, liked: can_like.then_some(liked) });
                app.sync_mini();
            }
            p.set_can_like(can_like);
            p.set_liked(liked);
            match art {
                Some(url) if !url.is_empty() => {
                    p.set_has_cover(false);
                    let weak = app.ui.clone();
                    app.covers.load(&app, url, 112, move |image| {
                        if let Some(ui) = weak.upgrade() {
                            ui.global::<Player>().set_cover(image);
                            ui.global::<Player>().set_has_cover(true);
                        }
                    });
                }
                _ => p.set_has_cover(false),
            }
        });
    }

    fn sync_queue(self: &Arc<Self>) {
        self.save_resume();
        let tracks: Vec<Track> = self.state.lock().unwrap().queue.clone();
        let app = self.clone();
        self.ui(move |ui| {
            let rows: Vec<TrackRow> = tracks.iter().map(pages::track_row).collect();
            let model = Rc::new(VecModel::from(rows));
            ui.global::<Queue>().set_tracks(ModelRc::from(model.clone()));
            covers::fill(&app, &model, tracks.iter().map(covers::cover_url).collect());
        });
    }

    pub fn play_list(self: &Arc<Self>, tracks: Vec<Track>, index: usize) {
        {
            let mut s = self.state.lock().unwrap();
            s.original = tracks.clone();
            s.auto_from = None;
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
        self.stats_flush();
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
        player::start_load(self, track, start_at, move |result| {
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
        self.send_presence();
    }

    pub fn set_volume(self: &Arc<Self>, volume: f32) {
        let gain = {
            let mut s = self.state.lock().unwrap();
            s.volume = volume;
            s.muted = false;
            player::gain(s.volume, s.muted)
        };
        self.settings.lock().unwrap().volume = volume;
        self.save_settings();
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
        if !self.settings.lock().unwrap().autoplay || self.autoplay_blocked.load(Ordering::Relaxed) {
            return;
        }
        let seed = {
            let s = self.state.lock().unwrap();
            let near_end = s.index.is_some_and(|i| i + 2 >= s.queue.len());
            if !near_end || s.repeat != Repeat::Off {
                return;
            }
            match s.current() {
                Some(t) if t.source != "local" => t.clone(),
                _ => return,
            }
        };
        let app = self.clone();
        self.rt.spawn(async move {
            // SoundCloud: ähnliche SoundCloud-Songs, sonst YouTube-Radio
            let soundcloud = if seed.source == "soundcloud" {
                services::soundcloud::soundcloud_related(&app.soundcloud, seed.id.clone()).await.ok()
            } else {
                None
            };
            let radio: Vec<Track> = match soundcloud {
                Some(list) if !list.is_empty() => list.into_iter().map(Track::from).collect(),
                _ => {
                    let Ok(video) = app.resolve_video(&seed).await else { return };
                    let Ok(radio) = services::ytm::ytm_radio(&app.ytm, video).await else { return };
                    radio.into_iter().map(Track::from).collect()
                }
            };
            {
                let mut s = app.state.lock().unwrap();
                let known: std::collections::HashSet<String> = s.queue.iter().map(|t| t.id.clone()).collect();
                let fresh: Vec<Track> = radio.into_iter().filter(|t| !known.contains(&t.id)).take(20).collect();
                if fresh.is_empty() {
                    return;
                }
                if s.auto_from.is_none() {
                    s.auto_from = Some(s.queue.len());
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
                self.started_playing();
                self.sync_player();
                self.send_presence();
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
                self.stats_progress(position);
                self.lyrics_progress(position);
                if self.last_resume_save.lock().unwrap().elapsed().as_secs() >= 5 {
                    self.save_resume();
                }
                let app = self.clone();
                self.ui(move |ui| {
                    #[cfg(windows)]
                    app.sync_mini();
                    let p = ui.global::<Player>();
                    p.set_position(position as f32);
                    p.set_position_text(format_time(position.min(duration.max(position))).into());
                });
            }
            Event::Playing(playing) => {
                self.state.lock().unwrap().playing = playing;
                self.send_presence();
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
                if self.consume_end_of_track() {
                    self.state.lock().unwrap().playing = false;
                    self.sync_player();
                    return;
                }
                if repeat_one {
                    let i = self.state.lock().unwrap().index.unwrap_or(0);
                    self.load(i, 0.0);
                } else {
                    self.next();
                }
            }
            Event::Measured { generation, lufs } => {
                let key = {
                    let s = self.state.lock().unwrap();
                    if generation != s.generation {
                        return;
                    }
                    s.current().map(|t| format!("{}:{}", t.source, t.id))
                };
                if let Some(key) = key {
                    let map = {
                        let mut map = self.loudness.lock().unwrap();
                        map.insert(key, (lufs * 10.0).round() / 10.0);
                        // Höchstens 3000 Songs merken
                        if map.len() > 3000 {
                            if let Some(k) = map.keys().next().cloned() {
                                map.remove(&k);
                            }
                        }
                        map.clone()
                    };
                    self.store.save_loudness(&map);
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

    // --- Speichern -----------------------------------------------------------

    pub fn save_resume(self: &Arc<Self>) {
        let resume = {
            let s = self.state.lock().unwrap();
            let index = s.index.unwrap_or(0);
            // Höchstens 300 Songs rund um den aktuellen
            let start = index.saturating_sub(50);
            store::Resume {
                queue: s.queue.iter().skip(start).take(300).cloned().collect(),
                index: index - start,
                position: s.position.floor(),
                shuffle: s.shuffle,
                repeat: s.repeat,
            }
        };
        *self.last_resume_save.lock().unwrap() = std::time::Instant::now();
        self.store.save_resume(&resume);
    }

    fn restore_resume(self: &Arc<Self>) {
        let resume = self.store.resume();
        if resume.queue.is_empty() {
            return;
        }
        {
            let mut s = self.state.lock().unwrap();
            s.index = Some(resume.index.min(resume.queue.len() - 1));
            s.original = resume.queue.clone();
            s.queue = resume.queue;
            s.position = resume.position;
            s.duration = s.current().map(|t| t.duration).unwrap_or(0.0);
            s.shuffle = resume.shuffle;
            s.repeat = resume.repeat;
        }
        self.sync_queue();
        self.sync_player();
        let position = resume.position;
        self.ui(move |ui| {
            let p = ui.global::<Player>();
            p.set_position(position as f32);
            p.set_position_text(format_time(position).into());
        });
    }

    pub fn save_settings(self: &Arc<Self>) {
        let settings = self.settings.lock().unwrap().clone();
        self.store.save_settings(&settings);
    }

    /// Design und Akzentfarbe anwenden
    pub fn apply_appearance(self: &Arc<Self>) {
        let (theme, accent) = {
            let s = self.settings.lock().unwrap();
            (s.theme.clone(), s.accent.clone())
        };
        let dark = match theme.as_str() {
            "light" => false,
            "system" => host::system_dark(),
            _ => true,
        };
        let color = parse_hex(&accent).unwrap_or(slint::Color::from_rgb_u8(0xff, 0x4f, 0x7b));
        let app = self.clone();
        self.ui(move |ui| {
            let t = ui.global::<Theme>();
            t.set_dark(dark);
            t.set_accent(color);
            #[cfg(windows)]
            app.sync_mini_theme();
        });
    }

    /// Hörstatistik: laufenden Song abschließen
    fn stats_flush(self: &Arc<Self>) {
        let Some(l) = self.listening.lock().unwrap().take() else { return };
        if l.listened < MIN_LISTENED {
            return;
        }
        let entry = serde_json::json!({
            "t": l.started_at,
            "s": l.listened.round() as u64,
            "d": l.track.duration.round() as u64,
            "src": l.track.source,
            "id": l.track.id,
            "title": l.track.title,
            "artist": l.track.artist,
            "album": l.track.album,
            "art": l.track.artwork_url,
        });
        self.store.append_plays(&[entry.clone()]);
        if self.account.logged_in().is_some() {
            self.plays_to_upload.lock().unwrap().push(entry);
        }
    }

    fn stats_progress(self: &Arc<Self>, position: f64) {
        let duration = self.state.lock().unwrap().duration;
        let scrobble = {
            let mut listening = self.listening.lock().unwrap();
            let Some(l) = listening.as_mut() else { return };
            let delta = position - l.last_pos;
            if delta > 0.0 && delta < 5.0 {
                l.listened += delta;
            }
            l.last_pos = position;
            // Last.fm-Regel: länger als 30 s, gehört bis zur Hälfte oder 4 Minuten
            let length = if duration > 0.0 { duration } else { l.track.duration };
            if !l.scrobbled && length >= 30.0 && l.listened >= (length / 2.0).min(240.0) {
                l.scrobbled = true;
                Some((l.track.clone(), l.started_at))
            } else {
                None
            }
        };
        if let Some((track, started_at)) = scrobble {
            self.lastfm_scrobble(&track, started_at);
        }
    }

    fn lastfm_data(&self, track: &Track, started_at: u64) -> Option<services::lastfm::Scrobble> {
        if !self.settings.lock().unwrap().lastfm_scrobble || std::env::var_os("WAVELY_TEST_STEPS").is_some() {
            return None;
        }
        let artist = track
            .artists
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| track.artist.clone().unwrap_or_default().split(',').next().unwrap_or("").trim().to_string());
        (!artist.is_empty()).then(|| services::lastfm::Scrobble {
            artist,
            title: track.title.clone(),
            album: track.album.clone(),
            duration: (track.duration > 0.0).then(|| track.duration.round() as u64),
            timestamp: started_at / 1000,
        })
    }

    fn lastfm_now_playing(self: &Arc<Self>, track: &Track) {
        let Some(data) = self.lastfm_data(track, store::now_ms()) else { return };
        let lastfm = self.lastfm.clone();
        self.rt.spawn(async move {
            let _ = services::lastfm::lastfm_now_playing(&lastfm, data).await;
        });
    }

    fn lastfm_scrobble(self: &Arc<Self>, track: &Track, started_at: u64) {
        let Some(data) = self.lastfm_data(track, started_at) else { return };
        let lastfm = self.lastfm.clone();
        self.rt.spawn(async move {
            let _ = services::lastfm::lastfm_scrobble(&lastfm, data).await;
        });
    }

    /// Song läuft wirklich: Verlauf und Statistik
    fn started_playing(self: &Arc<Self>) {
        let Some(track) = self.state.lock().unwrap().current().cloned() else { return };
        self.stats_flush();
        // Echte YouTube-Music-Songs auch im YouTube-Verlauf speichern
        if track.source == "youtube" && self.yt_logged_in.load(Ordering::Relaxed) && self.settings.lock().unwrap().sync_you_tube_history {
            let app = self.clone();
            let id = track.id.clone();
            self.rt.spawn(async move {
                let _ = services::ytm::ytm_report_play(&app.ytm, id).await;
            });
        }
        let now = store::now_ms();
        *self.listening.lock().unwrap() = Some(Listening { track: track.clone(), started_at: now, listened: 0.0, last_pos: 0.0, scrobbled: false });
        self.lastfm_now_playing(&track);
        let history = {
            let mut h = self.history.lock().unwrap();
            // Derselbe Song direkt nochmal innerhalb einer Minute: nicht doppelt eintragen
            let repeat = h.first().is_some_and(|e| e.track.id == track.id && now - e.played_at < 60_000);
            if !repeat {
                h.insert(0, store::HistoryEntry { track, played_at: now });
                h.truncate(store::MAX_HISTORY);
            }
            h.clone()
        };
        self.store.save_history(&history);
        // Offene Seitenfelder zum neuen Song aktualisieren
        let panel = self.panel.lock().unwrap().clone();
        let app = self.clone();
        let _ = slint::invoke_from_event_loop(move || match panel.as_str() {
            "lyrics" => app.show_lyrics(),
            "history" => app.show_history(),
            _ => {}
        });
    }

}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let testing = std::env::var_os("WAVELY_TEST_STEPS").is_some() || std::env::var_os("WAVELY_TEST_PLAY").is_some();
    // Zweiter Start: an die laufende Wavely weitergeben (z. B. Einladungslink) und beenden
    if !testing && instance::forward_to_running() {
        return Ok(());
    }
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    let paths = host::Paths::init()?;
    let ytdlp = YtDlp::init(&paths)?;
    let ytm = Arc::new(Ytm::new(&paths, ytdlp)?);

    let store = store::Store::new(&paths);
    let settings = store.settings();

    let ui = AppWindow::new()?;
    ui.global::<Nav>().set_collapsed(settings.sidebar_collapsed);

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
        state: Mutex::new(State { volume: settings.volume, ..Default::default() }),
        covers: covers::Covers::default(),
        view: Mutex::new("home".into()),
        stacks: Mutex::new(Default::default()),
        liked: Mutex::new(Default::default()),
        playlists: Mutex::new(playlists::Playlists::load(&store)),
        charts: Mutex::new(None),
        charts_loading: AtomicBool::new(false),
        menu: Mutex::new(None),
        toast_token: AtomicU64::new(0),
        dialog_target: Mutex::new(None),
        dialog_tracks: Mutex::new(Vec::new()),
        page: Mutex::new(Default::default()),
        page_token: AtomicU64::new(0),
        yt_logged_in: AtomicBool::new(false),
        history: Mutex::new(load_history(&store)),
        settings: Mutex::new(settings.clone()),
        account: account::Account::new(&paths),
        paths: paths.clone(),
        local_tracks: Mutex::new(Vec::new()),
        offline: offline::Offline::new(&paths),
        soundcloud: Arc::new(services::soundcloud::SoundCloud::new(&paths)?),
        spotify: Arc::new(services::spotify::Spotify::new(&paths)?),
        lastfm: Arc::new(services::lastfm::LastFm::new(&paths)?),
        sleep_until: Mutex::new(None),
        recap_range: Mutex::new("month".into()),
        room: Mutex::new(None),
        room_generation: AtomicU64::new(0),
        autoplay_blocked: AtomicBool::new(false),
        mini_cover_key: Mutex::new(None),
        sleep_end_of_track: AtomicBool::new(false),
        sleep_token: AtomicU64::new(0),
        loudness: Mutex::new(store.loudness()),
        matches: resolve::Matches::new(&paths),
        presence: Default::default(),
        panel: Mutex::new(String::new()),
        panel_tracks: Mutex::new(Vec::new()),
        panel_rows: Mutex::new(Vec::new()),
        lyrics: Mutex::new(Default::default()),
        local_loading: Mutex::new(false),
        local_watcher: Default::default(),
        sync_generation: AtomicU64::new(0),
        sync_status: Mutex::new(String::new()),
        account_busy: Mutex::new(false),
        account_error: Mutex::new(None),
        plays_to_upload: Mutex::new(Vec::new()),
        store,
        listening: Mutex::new(None),
        last_resume_save: Mutex::new(std::time::Instant::now()),
    });
    *app_slot.lock().unwrap() = Some(app.clone());
    app.engine.set_gain(player::gain(settings.volume, false));
    app.apply_appearance();
    app.restore_resume();
    // Tests laufen stumm und außerhalb des sichtbaren Bildschirms (stören niemanden)
    if std::env::var("WAVELY_TEST_PLAY").is_ok() || std::env::var("WAVELY_TEST_STEPS").is_ok() {
        app.state.lock().unwrap().muted = true;
        app.engine.set_gain(0.0);
        ui.window().set_position(slint::PhysicalPosition::new(-20000, 0));
        let weak = ui.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(1), move || {
            if let Some(ui) = weak.upgrade() {
                ui.window().set_position(slint::PhysicalPosition::new(-20000, 0));
            }
        });
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
    y.on_search(move |q| {
        let q = q.trim().to_string();
        if q.is_empty() {
            return;
        }
        // Neue Suche ersetzt eine angezeigte Suche
        let page = match a.current_view().as_str() {
            "soundcloud" => nav::PageKind::Sc(soundcloud_view::ScPage::Search(q)),
            "spotify" => nav::PageKind::Sp(spotify_view::SpPage::Search(q)),
            _ => nav::PageKind::Yt(youtube::YtPage::Search(q)),
        };
        let searching = matches!(
            a.current_page(),
            Some(nav::PageKind::Yt(youtube::YtPage::Search(_)))
                | Some(nav::PageKind::Sc(soundcloud_view::ScPage::Search(_)))
                | Some(nav::PageKind::Sp(spotify_view::SpPage::Search(_)))
        );
        if searching {
            a.replace_top(page);
        } else {
            a.open(page);
        }
    });

    let pg = ui.global::<Page>();
    let a = app.clone();
    pg.on_back(move || a.back());
    let a = app.clone();
    pg.on_play(move |row| a.page_play(row as usize));
    let a = app.clone();
    pg.on_card(move |row, card| a.page_card(row as usize, card as usize));
    let a = app.clone();
    pg.on_action(move |id| a.page_action(&id));
    let a = app.clone();
    pg.on_track_menu(move |row, x, y| {
        let Some((track, index)) = a.page_track(row as usize) else { return };
        let playlist = match a.current_page() {
            Some(nav::PageKind::Playlist(id)) => Some((id, index)),
            _ => None,
        };
        a.open_track_menu(menu::MenuContext { track, queue_index: None, playlist }, x, y);
    });

    let q = ui.global::<Queue>();
    let a = app.clone();
    q.on_jump(move |i| a.load(i as usize, 0.0));
    let a = app.clone();
    q.on_menu(move |i, x, y| {
        let track = a.state.lock().unwrap().queue.get(i as usize).cloned();
        if let Some(track) = track {
            a.open_track_menu(menu::MenuContext { track, queue_index: Some(i as usize), playlist: None }, x, y);
        }
    });

    let m = ui.global::<Menu>();
    let a = app.clone();
    m.on_pick(move |id| a.menu_pick(&id));
    let a = app.clone();
    m.on_close(move || a.close_menu());

    let d = ui.global::<Dialog>();
    let a = app.clone();
    d.on_confirm(move |value| a.dialog_confirm(value.to_string()));
    let a = app.clone();
    d.on_cancel(move || a.ui(|ui| ui.global::<Dialog>().set_open(false)));

    let side = ui.global::<Side>();
    let a = app.clone();
    side.on_play(move |row| a.panel_play(row as usize));
    let a = app.clone();
    side.on_menu(move |row, x, y| a.panel_menu(row as usize, x, y));
    let a = app.clone();
    side.on_lyric_clicked(move |line| a.lyrics_seek(line as usize));

    let fb = ui.global::<Feedback>();
    let a = app.clone();
    fb.on_send(move |kind, text, contact, info| a.send_feedback(kind.to_string(), text.to_string(), contact.to_string(), info));

    let n = ui.global::<Nav>();
    let a = app.clone();
    n.on_together_menu(move |x, y| a.open_together_menu(x, y));
    let a = app.clone();
    n.on_toggle_panel(move |panel| a.toggle_panel(&panel));
    let a = app.clone();
    n.on_sleep_menu(move |x, y| a.open_sleep_menu(x, y));
    let a = app.clone();
    n.on_mini_player(move || {
        #[cfg(windows)]
        a.open_mini_player();
    });
    let a = app.clone();
    n.on_key(move |key, ctrl, shift| a.key_pressed(&key, ctrl, shift));
    let a = app.clone();
    n.on_select(move |view| a.show_view(&view));
    let a = app.clone();
    n.on_open_playlist(move |id| a.open(nav::PageKind::Playlist(id.to_string())));
    let a = app.clone();
    n.on_new_playlist(move || {
        a.dialog_tracks.lock().unwrap().clear();
        a.ask_playlist_name(None);
    });

    let a = app.clone();
    p.on_toggle_like(move || {
        let track = a.state.lock().unwrap().current().cloned();
        if let Some(track) = track {
            a.toggle_like(track);
        }
    });

    let a = app.clone();
    pg.on_login(move |name, email, password, mode| {
        let name = (mode == "register").then(|| name.to_string());
        a.account_login(email.to_string(), password.to_string(), name);
    });
    let a = app.clone();
    pg.on_reset_password(move |email| a.account_reset(email.to_string()));
    let a = app.clone();
    pg.on_select(move |row, x, y| a.setting_select(row as usize, x, y));
    let a = app.clone();
    pg.on_eq_band(move |band, db| a.set_eq_band(band as usize, db));

    app.playlists_changed_local();
    app.apply_sound();
    app.apply_global_shortcuts();
    app.local_scan();
    app.apply_quality();
    app.start_sync();
    let start_view = app.settings.lock().unwrap().start_view.clone();
    app.show_view(if start_view.is_empty() { "home" } else { &start_view });
    app.yt_init();


    // yt-dlp aktuell halten, liegengebliebene Scrobbles nachreichen
    let a = app.clone();
    rt.spawn(async move {
        services::lastfm::flush_pending(&a.lastfm).await;
        a.ytm.update_tools().await;
    });

    if let Ok(query) = std::env::var("WAVELY_TEST_PLAY") {
        ui.global::<YouTube>().set_query(query.clone().into());
        app.yt_open(youtube::YtPage::Search(query));
    }

    if let Ok(steps) = std::env::var("WAVELY_TEST_STEPS") {
        run_test_steps(app.clone(), steps);
    }

    // Windows-Anbindung, sobald das Fenster existiert
    ui.show()?;
    // Zweite Starts: Fenster nach vorn, Einladungslink übernehmen
    if !testing {
        let a = app.clone();
        instance::listen(move |args| {
            let a = a.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = a.ui.upgrade() {
                    ui.window().set_minimized(false);
                    let _ = ui.show();
                }
                if let Some(code) = args.iter().find_map(|arg| instance::code_from_link(arg)) {
                    a.together_join(code);
                }
            });
        });
    }
    if let Some(code) = std::env::args().find_map(|arg| instance::code_from_link(&arg)) {
        let a = app.clone();
        slint::Timer::single_shot(std::time::Duration::from_secs(2), move || a.together_join(code));
    }
    // Automatischer Start mit Windows: minimiert, wenn so eingestellt
    let autostarted = std::env::args().any(|a| a == "--autostart");
    if autostarted && app.settings.lock().unwrap().start_minimized {
        let weak = ui.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(100), move || {
            if let Some(ui) = weak.upgrade() {
                ui.window().set_minimized(true);
            }
        });
    }

    // Das Fenster entsteht erst, wenn die Ereignisschleife läuft
    #[cfg(windows)]
    {
        let a = app.clone();
        let weak = ui.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(50), move || {
            let Some(ui) = weak.upgrade() else { return };
            let handler = a.clone();
            win::init(ui.window(), move |action| {
                let a = handler.clone();
                let action = action.to_string();
                let _ = slint::invoke_from_event_loop(move || a.windows_action(&action));
            });
            a.sync_player();
        });
    }
    slint::run_event_loop()?;
    #[cfg(windows)]
    win::mini::close_player();
    app.stats_flush();
    app.save_resume();
    // Raum verlassen (Gäste informieren)
    if app.room.lock().unwrap().is_some() {
        let a = app.clone();
        let _ = rt.block_on(async move { tokio::time::timeout(std::time::Duration::from_secs(3), a.together_leave_async()).await });
    }
    {
        let mut settings = app.settings.lock().unwrap();
        settings.sidebar_collapsed = ui.global::<Nav>().get_collapsed();
    }
    app.save_settings();
    app.engine.send(Command::Stop);
    app.presence.lock().unwrap().shutdown();
    Ok(())
}


/// Test-Hilfe: WAVELY_TEST_STEPS="search=Daft Punk|4|card=3,0|5|back"
/// führt Schritte nacheinander aus (Zahl = Sekunden warten). Nur für automatische Tests.
fn run_test_steps(app: Arc<App>, steps: String) {
    let mut queue: std::collections::VecDeque<String> = steps.split('|').map(|s| s.trim().to_string()).collect();
    fn next(app: Arc<App>, mut queue: std::collections::VecDeque<String>) {
        let Some(step) = queue.pop_front() else { return };
        if let Ok(seconds) = step.parse::<f64>() {
            slint::Timer::single_shot(std::time::Duration::from_secs_f64(seconds), move || next(app, queue));
            return;
        }
        let (name, arg) = step.split_once('=').unwrap_or((step.as_str(), ""));
        let nums: Vec<usize> = arg.split(',').filter_map(|n| n.trim().parse().ok()).collect();
        match name {
            "search" => {
                if let Some(ui) = app.ui.upgrade() {
                    ui.global::<YouTube>().invoke_search(arg.into());
                }
            }
            "card" if nums.len() == 2 => app.page_card(nums[0], nums[1]),
            "play" if !nums.is_empty() => app.page_play(nums[0]),
            "action" => app.page_action(arg),
            "back" => app.back(),
            "view" => app.show_view(arg),
            "menu" if !nums.is_empty() => {
                if let Some((track, _)) = app.page_track(nums[0]) {
                    app.open_track_menu(menu::MenuContext { track, queue_index: None, playlist: None }, 600.0, 300.0);
                }
            }
            "pick" => app.menu_pick(arg),
            // Bild der Oberfläche speichern (geht auch, wenn das Fenster außerhalb des Bildschirms liegt)
            "shot" => {
                if let Some(ui) = app.ui.upgrade() {
                    match ui.window().take_snapshot() {
                        Ok(pixels) => {
                            let _ = image::RgbaImage::from_raw(pixels.width(), pixels.height(), pixels.as_bytes().to_vec())
                                .map(|img| img.save(arg));
                        }
                        Err(e) => eprintln!("[test] Bild fehlgeschlagen: {e}"),
                    }
                }
            }
            "confirm" => app.dialog_confirm(arg.to_string()),
            "feedback" => {
                if let Some(ui) = app.ui.upgrade() {
                    ui.global::<Feedback>().set_open(true);
                }
            }
            "together" => app.open_together_menu(900.0, 560.0),
            #[cfg(windows)]
            "mini" => app.open_mini_player(),
            #[cfg(windows)]
            "mini-close" => win::mini::close_player(),
            // Fenstergröße (für Bilder langer Seiten)
            "size" if nums.len() == 2 => {
                if let Some(ui) = app.ui.upgrade() {
                    ui.window().set_size(slint::LogicalSize::new(nums[0] as f32, nums[1] as f32));
                }
            }
            "panel" => app.toggle_panel(arg),
            "dump" => {
                for (i, r) in app.page.lock().unwrap().rows.iter().enumerate() {
                    eprintln!("[test] Zeile {i}: {} {:?} ({} Kacheln)", r.kind, r.text, r.cards.len());
                }
            }
            _ => eprintln!("[test] Unbekannter Schritt: {step}"),
        }
        next(app, queue);
    }
    queue.retain(|s| !s.is_empty());
    next(app, queue);
}

/// "#ff4f7b" → Farbe
pub fn parse_hex(hex: &str) -> Option<slint::Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some(slint::Color::from_rgb_u8((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

/// Verlauf laden; beim ersten Start aus der Hörstatistik der bisherigen Wavely aufbauen
fn load_history(store: &store::Store) -> Vec<store::HistoryEntry> {
    let history = store.history();
    if !history.is_empty() {
        return history;
    }
    let mut plays = store.load_plays(0);
    plays.sort_by_key(|p| std::cmp::Reverse(p["t"].as_u64().unwrap_or(0)));
    let history: Vec<store::HistoryEntry> = plays
        .into_iter()
        .take(store::MAX_HISTORY)
        .map(|p| store::HistoryEntry {
            played_at: p["t"].as_u64().unwrap_or(0),
            track: Track {
                source: p["src"].as_str().unwrap_or("youtube").into(),
                id: p["id"].as_str().map(str::to_string).unwrap_or_else(|| p["id"].to_string()),
                title: p["title"].as_str().unwrap_or("").into(),
                artist: p["artist"].as_str().map(str::to_string),
                album: p["album"].as_str().map(str::to_string),
                artwork_url: p["art"].as_str().map(str::to_string),
                duration: p["d"].as_f64().unwrap_or(0.0),
                ..Default::default()
            },
        })
        .collect();
    store.save_history(&history);
    history
}
