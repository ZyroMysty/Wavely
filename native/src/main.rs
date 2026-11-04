// Kein Konsolenfenster in der fertigen App
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod covers;
mod host;
mod icon;
mod pages;
mod player;
mod services;
mod youtube;

use std::{
    rc::Rc,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
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
    /// Seitenverlauf in YouTube Music (letzte = aktuelle Seite)
    pub yt_stack: Mutex<Vec<youtube::YtPage>>,
    /// Gerade angezeigte Seite (für Klicks)
    pub page: Mutex<pages::PageData>,
    /// Zählt Seitenwechsel: verspätete Ergebnisse alter Seiten werden verworfen
    pub page_token: AtomicU64,
    pub yt_logged_in: AtomicBool,
}

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
            let rows: Vec<TrackRow> = tracks.iter().map(pages::track_row).collect();
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

    // --- Seiten ------------------------------------------------------------

    /// Neue Seite in YouTube Music öffnen (mit „Zurück“)
    pub fn yt_open(self: &Arc<Self>, page: youtube::YtPage, replace_all: bool) {
        {
            let mut stack = self.yt_stack.lock().unwrap();
            if replace_all {
                stack.clear();
            }
            stack.push(page);
        }
        self.yt_render();
    }

    pub fn yt_back(self: &Arc<Self>) {
        {
            let mut stack = self.yt_stack.lock().unwrap();
            if stack.len() > 1 {
                stack.pop();
            }
        }
        self.yt_render();
    }

    /// Aktuelle Seite laden und anzeigen
    pub fn yt_render(self: &Arc<Self>) {
        let (page, depth) = {
            let stack = self.yt_stack.lock().unwrap();
            (stack.last().cloned().unwrap_or(youtube::YtPage::Library), stack.len())
        };
        let token = self.page_token.fetch_add(1, Ordering::SeqCst) + 1;
        self.set_page(youtube::loading(&page), depth > 1, true);
        let app = self.clone();
        self.rt.spawn(async move {
            let logged_in = app.yt_logged_in.load(Ordering::Relaxed);
            let data = youtube::build(&app, &page, logged_in).await;
            if app.page_token.load(Ordering::SeqCst) == token {
                app.set_page(data, depth > 1, false);
            }
        });
    }

    fn set_page(self: &Arc<Self>, data: pages::PageData, can_go_back: bool, loading: bool) {
        *self.page.lock().unwrap() = data.clone();
        let app = self.clone();
        self.ui(move |ui| {
            let page = ui.global::<Page>();
            page.set_can_go_back(can_go_back);
            page.set_loading(loading);
            pages::show(&app, ui, &data);
        });
    }

    fn page_play(self: &Arc<Self>, row: usize) {
        let (tracks, index) = self.page.lock().unwrap().section_tracks(row);
        if !tracks.is_empty() {
            self.play_list(tracks, index);
        }
    }

    fn page_card(self: &Arc<Self>, row: usize, card: usize) {
        let target = {
            let page = self.page.lock().unwrap();
            page.rows.get(row).and_then(|r| r.cards.get(card)).map(|c| c.target.clone())
        };
        match target {
            Some(pages::Target::Collection { kind, id, title, art }) => {
                self.yt_open(youtube::YtPage::Collection { kind, id, title, art }, false)
            }
            Some(pages::Target::Artist { id, name }) => self.yt_open(youtube::YtPage::Artist { id, name }, false),
            Some(pages::Target::Action(id)) => self.page_action(&id),
            None => {}
        }
    }

    fn page_action(self: &Arc<Self>, action: &str) {
        let tracks = self.page.lock().unwrap().tracks.clone();
        match action {
            "play" | "shuffle" if !tracks.is_empty() => {
                let shuffle = action == "shuffle";
                if self.state.lock().unwrap().shuffle != shuffle {
                    self.toggle_shuffle();
                }
                let start = if shuffle { rand::random_range(0..tracks.len()) } else { 0 };
                self.play_list(tracks, start);
            }
            "all-songs" => {
                let current = self.yt_stack.lock().unwrap().last().cloned();
                if let Some(youtube::YtPage::Artist { id, .. }) = current {
                    let app = self.clone();
                    self.rt.spawn(async move {
                        if let Some(page) = youtube::all_songs_target(&app, &id).await {
                            let _ = slint::invoke_from_event_loop(move || app.yt_open(page, false));
                        }
                    });
                }
            }
            _ => {}
        }
    }

    /// Rechtsklick auf einen Song: vorerst „als Nächstes spielen“
    fn page_track_menu(self: &Arc<Self>, row: usize) {
        let track = {
            let page = self.page.lock().unwrap();
            page.rows.get(row).and_then(|r| r.track).and_then(|t| page.tracks.get(t).cloned())
        };
        if let Some(track) = track {
            self.play_next(track);
        }
    }

    pub fn play_next(self: &Arc<Self>, track: Track) {
        let empty = {
            let mut s = self.state.lock().unwrap();
            match s.index {
                Some(i) => {
                    s.queue.insert(i + 1, track.clone());
                    s.original.push(track.clone());
                    false
                }
                None => true,
            }
        };
        if empty {
            self.play_list(vec![track], 0);
        } else {
            self.sync_queue();
        }
    }

    fn remove_from_queue(self: &Arc<Self>, index: usize) {
        {
            let mut s = self.state.lock().unwrap();
            if Some(index) == s.index || index >= s.queue.len() {
                return;
            }
            s.queue.remove(index);
            if let Some(i) = s.index {
                if index < i {
                    s.index = Some(i - 1);
                }
            }
        }
        self.sync_queue();
        self.sync_player();
    }

    fn yt_init(self: &Arc<Self>) {
        let app = self.clone();
        self.rt.spawn(async move {
            let logged_in = services::ytm::ytm_status(&app.ytm).await.unwrap_or(false);
            app.yt_logged_in.store(logged_in, Ordering::Relaxed);
            app.ui(move |ui| ui.global::<YouTube>().set_status(if logged_in { "loggedIn" } else { "loggedOut" }.into()));
            let a = app.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if a.yt_stack.lock().unwrap().is_empty() {
                    a.yt_open(youtube::YtPage::Library, true);
                }
            });
            if logged_in {
                if let Ok(account) = services::ytm::ytm_account(&app.ytm).await {
                    let name = account.name.unwrap_or_default();
                    app.ui(move |ui| ui.global::<YouTube>().set_account_name(name.into()));
                }
            }
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
        yt_stack: Mutex::new(Vec::new()),
        page: Mutex::new(Default::default()),
        page_token: AtomicU64::new(0),
        yt_logged_in: AtomicBool::new(false),
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
    y.on_search(move |q| {
        let q = q.trim().to_string();
        if q.is_empty() {
            return;
        }
        // Neue Suche ersetzt eine angezeigte Suche
        let on_search = matches!(a.yt_stack.lock().unwrap().last(), Some(youtube::YtPage::Search(_)));
        if on_search {
            a.yt_stack.lock().unwrap().pop();
        }
        a.yt_open(youtube::YtPage::Search(q), false);
    });

    let pg = ui.global::<Page>();
    let a = app.clone();
    pg.on_back(move || a.yt_back());
    let a = app.clone();
    pg.on_play(move |row| a.page_play(row as usize));
    let a = app.clone();
    pg.on_card(move |row, card| a.page_card(row as usize, card as usize));
    let a = app.clone();
    pg.on_action(move |id| a.page_action(&id));
    let a = app.clone();
    pg.on_track_menu(move |row| a.page_track_menu(row as usize));

    let q = ui.global::<Queue>();
    let a = app.clone();
    q.on_jump(move |i| a.load(i as usize, 0.0));
    let a = app.clone();
    q.on_remove(move |i| a.remove_from_queue(i as usize));
    app.yt_init();


    // yt-dlp im Hintergrund aktuell halten
    let a = app.clone();
    rt.spawn(async move { a.ytm.update_tools().await });

    if let Ok(query) = std::env::var("WAVELY_TEST_PLAY") {
        ui.global::<YouTube>().set_query(query.clone().into());
        app.yt_open(youtube::YtPage::Search(query), false);
    }

    if let Ok(steps) = std::env::var("WAVELY_TEST_STEPS") {
        run_test_steps(app.clone(), steps);
    }

    ui.run()?;
    app.engine.send(Command::Stop);
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
            "back" => app.yt_back(),
            "view" => {
                if let Some(ui) = app.ui.upgrade() {
                    ui.global::<Nav>().set_view(arg.into());
                }
            }
            "panel" => {
                if let Some(ui) = app.ui.upgrade() {
                    ui.global::<Nav>().set_panel(arg.into());
                }
            }
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
