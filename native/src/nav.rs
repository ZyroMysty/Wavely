//! Navigation: jede Ansicht der Seitenleiste hat ihren eigenen Seitenverlauf („Zurück“).

use std::sync::{atomic::Ordering, Arc};

use slint::ComponentHandle;

use crate::{pages, player::Track, wavely, youtube, App, Nav, Page};

#[derive(Clone, Debug)]
pub enum PageKind {
    Yt(youtube::YtPage),
    /// Wavely-Playlist
    Playlist(String),
    Home,
    Settings,
    Local,
    /// Noch nicht nachgebaute Ansicht
    Placeholder(&'static str),
}

/// Startseite einer Ansicht
fn root_of(view: &str) -> PageKind {
    match view {
        "youtube" => PageKind::Yt(youtube::YtPage::Library),
        "home" => PageKind::Home,
        "local" => PageKind::Local,
        "spotify" => PageKind::Placeholder("Spotify"),
        "soundcloud" => PageKind::Placeholder("SoundCloud"),
        "offline" => PageKind::Placeholder("Offline"),
        "recap" => PageKind::Placeholder("Rückblick"),
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
            self.render();
            return;
        }
        let view = self.current_view();
        // YouTube-Seiten immer in der YouTube-Ansicht
        let view = match (&page, view.as_str()) {
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
                self.set_page(youtube::loading(&yt), back, true);
                let app = self.clone();
                self.rt.spawn(async move {
                    let logged_in = app.yt_logged_in.load(Ordering::Relaxed);
                    let data = youtube::build(&app, &yt, logged_in).await;
                    if app.page_token.load(Ordering::SeqCst) == token {
                        app.set_page(data, back, false);
                    }
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
                data.message("Kommt in einer der nächsten nativen Testversionen.");
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
        if self.setting_action(action) || self.local_action(action) {
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
