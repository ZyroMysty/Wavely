//! Navigation: jede Ansicht der Seitenleiste hat ihren eigenen Seitenverlauf („Zurück“).

use std::sync::{atomic::Ordering, Arc};

use slint::ComponentHandle;

use crate::{pages, player::Track, wavely, youtube, App, Nav, Page};

#[derive(Clone, Debug)]
pub enum PageKind {
    Yt(youtube::YtPage),
    Sc(crate::soundcloud_view::ScPage),
    Sp(crate::spotify_view::SpPage),
    /// Wavely-Playlist
    Playlist(String),
    Home,
    Settings,
    Local,
    Recap,
    Offline,
    OfflineCollection(String),
    /// Noch nicht nachgebaute Ansicht
    Placeholder(&'static str),
}

/// Scrollposition je Seite: Alle Seiten teilen sich eine Liste in der Oberfläche. Ohne das hier
/// bliebe beim Wechsel (andere Seite, anderer Einstellungs-Reiter) die alte Position stehen.
#[derive(Default)]
pub struct ScrollMemory {
    /// Seite, die gerade angezeigt wird (siehe `App::page_key`)
    key: String,
    /// Gemerkte Positionen anderer Seiten (für „Zurück“ und Ansichtswechsel)
    saved: std::collections::HashMap<String, f32>,
    /// Wiederherzustellende Position, sobald die Seite fertig geladen ist
    pending: Option<f32>,
}

/// Startseite einer Ansicht
fn root_of(view: &str) -> PageKind {
    match view {
        "youtube" => PageKind::Yt(youtube::YtPage::Library),
        "home" => PageKind::Home,
        "local" => PageKind::Local,
        "spotify" => PageKind::Sp(crate::spotify_view::SpPage::Library),
        "soundcloud" => PageKind::Sc(crate::soundcloud_view::ScPage::Library),
        "offline" => PageKind::Offline,
        "recap" => PageKind::Recap,
        "settings" => PageKind::Settings,
        _ => PageKind::Placeholder("Wavely"),
    }
}

impl App {
    pub fn current_view(&self) -> String {
        self.view.lock().unwrap().clone()
    }

    /// Ansicht in der Seitenleiste gewählt. Erneuter Klick führt zur Startseite der Ansicht.
    pub fn show_view(self: &Arc<Self>, view: &str) {
        let same = self.current_view() == view;
        *self.view.lock().unwrap() = view.to_string();
        {
            let mut stacks = self.stacks.lock().unwrap();
            let stack = stacks.entry(view.to_string()).or_default();
            if stack.is_empty() || same {
                stack.clear();
                stack.push(root_of(view));
            }
        }
        if same {
            self.forget_scroll();
        }
        let v = view.to_string();
        self.ui(move |ui| ui.global::<Nav>().set_view(v.into()));
        self.render();
    }

    /// Neue Seite in der aktuellen Ansicht öffnen (mit „Zurück“)
    pub fn open(self: &Arc<Self>, page: PageKind) {
        // Wavely-Playlists haben eine eigene Ansicht
        if let PageKind::Playlist(id) = &page {
            *self.view.lock().unwrap() = "playlist".into();
            let id = id.clone();
            self.stacks.lock().unwrap().insert("playlist".into(), vec![page]);
            self.ui(move |ui| {
                let nav = ui.global::<Nav>();
                nav.set_current_playlist(id.into());
                nav.set_view("playlist".into());
            });
            self.forget_scroll();
            self.render();
            return;
        }
        let view = self.current_view();
        // YouTube-Seiten immer in der YouTube-Ansicht
        let view = match (&page, view.as_str()) {
            (PageKind::Sp(_), "spotify") => view,
            (PageKind::Sp(_), _) => {
                *self.view.lock().unwrap() = "spotify".into();
                self.ui(|ui| ui.global::<Nav>().set_view("spotify".into()));
                "spotify".to_string()
            }
            (PageKind::Sc(_), "soundcloud") => view,
            (PageKind::Sc(_), _) => {
                *self.view.lock().unwrap() = "soundcloud".into();
                self.ui(|ui| ui.global::<Nav>().set_view("soundcloud".into()));
                "soundcloud".to_string()
            }
            (PageKind::Yt(_), "youtube") => view,
            (PageKind::Yt(_), _) => {
                *self.view.lock().unwrap() = "youtube".into();
                self.ui(|ui| ui.global::<Nav>().set_view("youtube".into()));
                "youtube".to_string()
            }
            _ => view,
        };
        {
            let mut stacks = self.stacks.lock().unwrap();
            let stack = stacks.entry(view.clone()).or_default();
            if stack.is_empty() {
                stack.push(root_of(&view));
            }
            stack.push(page);
        }
        self.forget_scroll();
        self.render();
    }

    pub fn yt_open(self: &Arc<Self>, page: youtube::YtPage) {
        self.open(PageKind::Yt(page));
    }

    pub fn back(self: &Arc<Self>) {
        {
            let view = self.current_view();
            let mut stacks = self.stacks.lock().unwrap();
            if let Some(stack) = stacks.get_mut(&view) {
                if stack.len() > 1 {
                    stack.pop();
                }
            }
        }
        self.render();
    }

    pub fn current_page(&self) -> Option<PageKind> {
        let view = self.current_view();
        self.stacks.lock().unwrap().get(&view).and_then(|s| s.last().cloned())
    }

    /// Ersetzt die oberste Seite (z. B. neue Suche statt alter Suche)
    pub fn replace_top(self: &Arc<Self>, page: PageKind) {
        {
            let view = self.current_view();
            let mut stacks = self.stacks.lock().unwrap();
            let stack = stacks.entry(view).or_default();
            stack.pop();
            stack.push(page);
        }
        self.forget_scroll();
        self.render();
    }

    /// Aktuelle Seite laden und anzeigen
    pub fn render(self: &Arc<Self>) {
        let view = self.current_view();
        let (page, depth) = {
            let stacks = self.stacks.lock().unwrap();
            let stack = stacks.get(&view);
            (stack.and_then(|s| s.last().cloned()).unwrap_or_else(|| root_of(&view)), stack.map(|s| s.len()).unwrap_or(1))
        };
        let token = self.page_token.fetch_add(1, Ordering::SeqCst) + 1;
        let back = depth > 1;
        match page {
            PageKind::Yt(yt) => {
                let loading = youtube::loading(&yt);
                self.load_remote(token, back, loading, move |app| async move {
                    let logged_in = app.yt_logged_in.load(Ordering::Relaxed);
                    youtube::build(&app, &yt, logged_in).await
                });
            }
            PageKind::Sc(sc) => {
                let loading = crate::soundcloud_view::loading(&sc);
                self.load_remote(token, back, loading, move |app| async move {
                    crate::soundcloud_view::build(&app, &sc).await
                });
            }
            PageKind::Sp(sp) => {
                let loading = crate::spotify_view::loading(&sp);
                self.load_remote(token, back, loading, move |app| async move {
                    crate::spotify_view::build(&app, &sp).await
                });
            }
            PageKind::Playlist(id) => {
                let data = wavely::playlist_page(self, &id);
                self.set_page(data, back, false);
            }
            PageKind::Home => {
                let data = wavely::home_page(self);
                self.set_page(data, back, false);
            }
            PageKind::Offline => {
                let data = crate::offline::page(self);
                self.set_page(data, back, false);
            }
            PageKind::OfflineCollection(id) => {
                let data = crate::offline::collection_page(self, &id);
                self.set_page(data, back, false);
            }
            PageKind::Recap => {
                let range = self.recap_range.lock().unwrap().clone();
                let data = crate::recap::page(self, &range);
                self.set_page(data, back, false);
            }
            PageKind::Local => {
                let data = crate::local::page(self);
                self.set_page(data, back, false);
            }
            PageKind::Settings => {
                let data = crate::settings::page(self);
                self.set_page(data, back, false);
            }
            PageKind::Placeholder(title) => {
                let mut data = pages::PageData::default();
                data.title(title);
                data.message("Kommt in einer der nächsten Versionen.");
                self.set_page(data, back, false);
            }
        }
    }

    /// Seite neu aufbauen, falls sie gerade angezeigt wird (z. B. nach Änderung einer Playlist)
    pub fn refresh_if_showing(self: &Arc<Self>, check: impl Fn(&PageKind) -> bool) {
        if self.current_page().is_some_and(|p| check(&p)) {
            self.render();
        }
    }

    /// Seite aus dem Netz laden. War sie schon einmal offen, erscheint sofort der letzte Stand
    /// (statt „wird geladen“) und wird nur ersetzt, wenn sich wirklich etwas geändert hat – sonst
    /// würde jeder Ansichtswechsel die Liste neu aufbauen und die Scrollposition verlieren.
    fn load_remote<F, Fut>(self: &Arc<Self>, token: u64, back: bool, loading: pages::PageData, build: F)
    where
        F: FnOnce(Arc<App>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = pages::PageData> + Send + 'static,
    {
        let key = self.page_key();
        let cached = self.page_cache.lock().unwrap().get(&key).cloned();
        let shown = match cached {
            Some((data, fingerprint)) => {
                self.set_page(data, back, false);
                Some(fingerprint)
            }
            None => {
                self.set_page(loading, back, true);
                None
            }
        };
        let app = self.clone();
        self.rt.spawn(async move {
            let data = build(app.clone()).await;
            if app.page_token.load(Ordering::SeqCst) != token {
                return;
            }
            let fingerprint = format!("{:?}", data.rows);
            if shown.as_ref() != Some(&fingerprint) {
                app.set_page(data.clone(), back, false);
            }
            let mut cache = app.page_cache.lock().unwrap();
            if cache.len() > 50 {
                cache.clear();
            }
            cache.insert(key, (data, fingerprint));
        });
    }

    fn set_page(self: &Arc<Self>, data: pages::PageData, can_go_back: bool, loading: bool) {
        *self.page.lock().unwrap() = data.clone();
        let key = self.page_key();
        let app = self.clone();
        self.ui(move |ui| {
            let page = ui.global::<Page>();
            page.set_can_go_back(can_go_back);
            page.set_loading(loading);
            // Andere Seite: oben anfangen bzw. gemerkte Position wiederherstellen. Dieselbe Seite
            // neu aufgebaut (z. B. Einstellung geändert, fertig geladen): Position behalten.
            let restore = {
                let mut scroll = app.scroll.lock().unwrap();
                if scroll.key != key {
                    let old = std::mem::replace(&mut scroll.key, key.clone());
                    if !old.is_empty() {
                        scroll.saved.insert(old, page.get_scroll_y());
                    }
                    scroll.pending = scroll.saved.get(&key).copied().filter(|y| *y < 0.0);
                    page.set_scroll_y(0.0);
                }
                if loading { None } else { scroll.pending.take() }
            };
            pages::show(&app, ui, &data);
            if let Some(y) = restore {
                page.set_scroll_y(y);
                // Die Liste schätzt die Höhe noch nicht gebauter Zeilen und verschiebt die Position
                // nach dem ersten Bild etwas – danach noch einmal genau setzen
                let app = app.clone();
                slint::Timer::single_shot(std::time::Duration::from_millis(30), move || {
                    let Some(ui) = app.ui.upgrade() else { return };
                    if app.scroll.lock().unwrap().key == key {
                        ui.global::<Page>().set_scroll_y(y);
                    }
                });
            }
        });
    }

    /// Knopf „Neu laden“: Seite frisch holen (ohne gespeicherten Stand), Scrollposition bleibt
    pub fn refresh_page(self: &Arc<Self>) {
        let view = self.current_view();
        let page = self.current_page().unwrap_or_else(|| root_of(&view));
        if matches!(page, PageKind::Local) {
            // Lokale Musik: Ordner neu einlesen
            self.page_action("local-refresh");
            return;
        }
        let key = self.page_key();
        self.page_cache.lock().unwrap().remove(&key);
        self.render();
    }

    /// Kennung der angezeigten Seite für die Scrollposition
    fn page_key(&self) -> String {
        let view = self.current_view();
        let (page, depth) = {
            let stacks = self.stacks.lock().unwrap();
            let stack = stacks.get(&view);
            (stack.and_then(|s| s.last().cloned()).unwrap_or_else(|| root_of(&view)), stack.map_or(1, |s| s.len()))
        };
        let tab = if matches!(page, PageKind::Settings) { self.settings_tab.lock().unwrap().clone() } else { String::new() };
        format!("{view}|{depth}|{page:?}|{tab}")
    }

    /// Neu geöffnete Seite beginnt oben (keine alte Position von früher wiederherstellen)
    fn forget_scroll(&self) {
        let key = self.page_key();
        let mut scroll = self.scroll.lock().unwrap();
        scroll.saved.remove(&key);
        if scroll.saved.len() > 200 {
            scroll.saved.clear();
        }
    }

    pub fn page_play(self: &Arc<Self>, row: usize) {
        let (tracks, index) = self.page.lock().unwrap().section_tracks(row);
        if !tracks.is_empty() {
            self.play_list(tracks, index);
        }
    }

    pub fn page_card(self: &Arc<Self>, row: usize, card: usize) {
        let target = {
            let page = self.page.lock().unwrap();
            page.rows.get(row).and_then(|r| r.cards.get(card)).map(|c| c.target.clone())
        };
        match target {
            Some(pages::Target::Collection { kind, id, title, art }) => {
                self.yt_open(youtube::YtPage::Collection { kind, id, title, art })
            }
            Some(pages::Target::Artist { id, name }) => self.yt_open(youtube::YtPage::Artist { id, name }),
            Some(pages::Target::Playlist(id)) => self.open(PageKind::Playlist(id)),
            Some(pages::Target::Open(page)) => self.open(*page),
            Some(pages::Target::Track(index)) => {
                let tracks = self.page.lock().unwrap().tracks.clone();
                if index < tracks.len() {
                    self.play_list(tracks, index);
                }
            }
            Some(pages::Target::Action(id)) => self.page_action(&id),
            None => {}
        }
    }

    pub fn page_action(self: &Arc<Self>, action: &str) {
        if self.setting_action(action)
            || self.local_action(action)
            || self.offline_action(action)
            || self.service_action(action)
            || self.recap_action(action)
        {
            return;
        }
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
                if let Some(PageKind::Yt(youtube::YtPage::Artist { id, .. })) = self.current_page() {
                    let app = self.clone();
                    self.rt.spawn(async move {
                        if let Some(page) = youtube::all_songs_target(&app, &id).await {
                            let _ = slint::invoke_from_event_loop(move || app.yt_open(page));
                        }
                    });
                }
            }
            "offline-save" => {
                if let Some(PageKind::Yt(youtube::YtPage::Collection { kind, id, title, art })) = self.current_page() {
                    self.offline_toggle_collection(&id, &kind, &title, art, tracks);
                    self.render();
                }
            }
            "delete-playlist" => {
                if let Some(PageKind::Playlist(id)) = self.current_page() {
                    self.playlist_delete(&id);
                }
            }
            "rename-playlist" => {
                if let Some(PageKind::Playlist(id)) = self.current_page() {
                    self.ask_playlist_name(Some(id));
                }
            }
            other => {
                // Startseite: Mix zu einem Künstler (Index des Start-Songs)
                if let Some(i) = other.strip_prefix("mix:").and_then(|i| i.parse::<usize>().ok()) {
                    if let Some(seed) = tracks.get(i).cloned() {
                        self.start_radio(seed);
                    }
                }
            }
        }
    }

    /// Song einer Zeile der aktuellen Seite
    pub fn page_track(&self, row: usize) -> Option<(Track, usize)> {
        let page = self.page.lock().unwrap();
        let index = page.rows.get(row).and_then(|r| r.track)?;
        page.tracks.get(index).cloned().map(|t| (t, index))
    }
}
