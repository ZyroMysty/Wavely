//! Eigene Wavely-Seiten: Startseite und Wavely-Playlists (wie HomeView.svelte, PlaylistView.svelte).

use std::{collections::HashMap, sync::Arc};

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{
    icon,
    nav::PageKind,
    pages::{CardData, PageData, Target},
    player::Track,
    services::ytm,
    App, Dialog, Nav, NavItem,
};

fn greeting() -> &'static str {
    let hour = host_hour();
    match hour {
        0..=4 => "Gute Nacht",
        5..=10 => "Guten Morgen",
        11..=17 => "Guten Tag",
        _ => "Guten Abend",
    }
}

/// Aktuelle Stunde in Ortszeit
fn host_hour() -> u32 {
    #[cfg(windows)]
    {
        let time = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        time.wHour as u32
    }
    #[cfg(not(windows))]
    {
        12
    }
}

/// Uhrzeit „HH:MM“ in Ortszeit
pub fn clock_time() -> String {
    #[cfg(windows)]
    {
        let time = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        format!("{:02}:{:02}", time.wHour, time.wMinute)
    }
    #[cfg(not(windows))]
    {
        String::new()
    }
}

fn key_of(t: &Track) -> String {
    format!("{}:{}", t.source, t.id)
}

fn first_artist(t: &Track) -> String {
    t.artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| t.artist.clone().unwrap_or_default().split(',').next().unwrap_or("").trim().to_string())
}

/// Cover-Bild einer Wavely-Playlist: erstes Cover ihrer Songs
fn playlist_art(p: &crate::playlists::Playlist) -> Option<String> {
    p.items.iter().find_map(|i| i.track.artwork_url.clone())
}

pub fn playlist_card(p: &crate::playlists::Playlist) -> CardData {
    let n = p.items.len();
    CardData {
        title: p.name.clone(),
        subtitle: format!("Wavely · {n} {}", if n == 1 { "Song" } else { "Songs" }),
        art: playlist_art(p),
        round: false,
        target: Target::Playlist(p.id.clone()),
    }
}

pub fn home_page(app: &Arc<App>) -> PageData {
    let mut p = PageData::default();
    p.title(greeting());
    let history = app.history.lock().unwrap().clone();

    // Weiterhören: zuletzt gehört ohne Wiederholungen
    let mut seen = std::collections::HashSet::new();
    let recent: Vec<Track> =
        history.iter().map(|e| e.track.clone()).filter(|t| seen.insert(key_of(t))).take(8).collect();
    if recent.is_empty() {
        p.message("Hier erscheinen bald deine zuletzt gehörten Songs, deine Lieblingssongs und Mixes, die zu dir passen.");
    } else {
        p.heading("Weiterhören");
        let start = p.tracks.len();
        let cards = recent
            .iter()
            .enumerate()
            .map(|(i, t)| CardData {
                title: t.title.clone(),
                subtitle: t.artist.clone().unwrap_or_default(),
                art: t.artwork_url.clone(),
                round: false,
                target: Target::Track(start + i),
            })
            .collect();
        // Songs gehören zur Seite (für „abspielen ab hier“), aber ohne eigene Zeilen
        p.tracks.extend(recent);
        p.cards(cards);
    }

    // Letzte 30 Tage: Top-Künstler (Mix) und Top-Songs
    let since = crate::store::now_ms().saturating_sub(30 * 24 * 60 * 60 * 1000);
    let mut artists: HashMap<String, (String, usize, Track)> = HashMap::new();
    let mut songs: HashMap<String, (Track, usize)> = HashMap::new();
    for e in history.iter().filter(|e| e.played_at >= since) {
        let name = first_artist(&e.track);
        if !name.is_empty() {
            let entry = artists.entry(name.to_lowercase()).or_insert((name, 0, e.track.clone()));
            entry.1 += 1;
            // Bevorzugt einen YouTube-Song als Startpunkt (Mix kommt von YouTube Music)
            if entry.2.source != "youtube" && e.track.source == "youtube" {
                entry.2 = e.track.clone();
            }
        }
        songs.entry(key_of(&e.track)).or_insert((e.track.clone(), 0)).1 += 1;
    }
    let mut artists: Vec<_> = artists.into_values().collect();
    artists.sort_by(|a, b| b.1.cmp(&a.1));
    if !artists.is_empty() {
        p.heading("Mixes für dich");
        let mut mixes = Vec::new();
        for (name, plays, seed) in artists.into_iter().take(6) {
            let index = p.tracks.len();
            mixes.push(CardData {
                title: format!("{name} Mix"),
                subtitle: format!("{plays}× gehört"),
                art: seed.artwork_url.clone(),
                round: false,
                target: Target::Action(format!("mix:{index}")),
            });
            p.tracks.push(seed);
        }
        p.cards(mixes);
    }
    let mut songs: Vec<_> = songs.into_values().collect();
    songs.sort_by(|a, b| b.1.cmp(&a.1));
    if songs.len() >= 3 {
        p.heading("Deine Top-Songs · letzte 30 Tage");
        p.tracks(songs.into_iter().take(10).map(|(t, _)| t));
    }

    let playlists: Vec<CardData> = app.playlists.lock().unwrap().list.iter().take(12).map(playlist_card).collect();
    if !playlists.is_empty() {
        p.heading("Deine Playlists");
        p.cards(playlists);
    }

    // Charts von YouTube Music (im Hintergrund geladen)
    match app.charts.lock().unwrap().as_ref() {
        Some(charts) => {
            if !charts.top_tracks.is_empty() {
                p.heading("Charts Deutschland");
                p.tracks(charts.top_tracks.iter().cloned().map(Track::from));
            }
            if !charts.new_albums.is_empty() {
                p.heading("Neuerscheinungen");
                p.cards(
                    charts
                        .new_albums
                        .iter()
                        .map(|c| CardData {
                            title: c.title.clone(),
                            subtitle: c.subtitle.clone().unwrap_or_default(),
                            art: c.artwork_url.clone(),
                            round: false,
                            target: Target::Collection { kind: c.kind.clone(), id: c.id.clone(), title: c.title.clone(), art: c.artwork_url.clone() },
                        })
                        .collect(),
                );
            }
            if !charts.top_artists.is_empty() {
                p.heading("Angesagte Künstler");
                p.cards(
                    charts
                        .top_artists
                        .iter()
                        .map(|a| CardData {
                            title: a.name.clone(),
                            subtitle: "Künstler".into(),
                            art: a.image_url.clone(),
                            round: true,
                            target: Target::Artist { id: a.id.clone(), name: a.name.clone() },
                        })
                        .collect(),
                );
            }
        }
        None => {
            p.message("Charts werden geladen…");
            load_charts(app);
        }
    }
    p.spacer();
    p
}

/// Charts höchstens einmal pro Start laden
fn load_charts(app: &Arc<App>) {
    if app.charts_loading.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    app.rt.clone().spawn(async move {
        if let Ok(home) = ytm::ytm_home(&app.ytm).await {
            *app.charts.lock().unwrap() = Some(home);
            let a = app.clone();
            let _ = slint::invoke_from_event_loop(move || a.refresh_if_showing(|p| matches!(p, PageKind::Home)));
        } else {
            app.charts_loading.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    });
}

pub fn playlist_page(app: &Arc<App>, id: &str) -> PageData {
    let mut p = PageData::default();
    let playlists = app.playlists.lock().unwrap();
    let Some(playlist) = playlists.get(id) else {
        p.title("Playlist");
        p.message("Diese Playlist gibt es nicht mehr.");
        return p;
    };
    let n = playlist.items.len();
    p.hero("Wavely-Playlist", &playlist.name, &format!("{n} {}", if n == 1 { "Song" } else { "Songs" }), playlist_art(playlist), false);
    p.buttons(&[
        ("play", "Abspielen", icon::PLAY),
        ("shuffle", "Zufall", icon::SHUFFLE),
        ("rename-playlist", "Umbenennen", icon::EDIT),
        ("delete-playlist", "Löschen", icon::TRASH),
    ]);
    if n == 0 {
        p.message("Noch leer – füge Songs über das Menü (Rechtsklick) hinzu.");
    }
    p.tracks(playlist.items.iter().map(|i| i.track.clone()));
    p.spacer();
    p
}

impl App {
    /// Playlists geändert: speichern, anzeigen und ins Wavely-Konto laden
    pub fn playlists_changed(self: &Arc<Self>) {
        self.playlists_changed_local();
        let Some(session) = self.account.logged_in() else { return };
        let app = self.clone();
        self.rt.spawn(async move {
            let (list, deleted) = {
                let mut p = app.playlists.lock().unwrap();
                (p.list.clone(), std::mem::take(&mut p.deleted))
            };
            if app.account.push_playlists(&session.uid, &list, &deleted).await.is_err() {
                // Beim nächsten Abgleich nochmal versuchen
                app.playlists.lock().unwrap().deleted.extend(deleted);
            }
        });
    }

    /// Playlists in der Seitenleiste anzeigen und speichern (ohne Hochladen)
    pub fn playlists_changed_local(self: &Arc<Self>) {
        let (items, counts): (Vec<NavItem>, Vec<i32>) = {
            let playlists = self.playlists.lock().unwrap();
            playlists.save(&self.store);
            playlists
                .list
                .iter()
                .map(|p| (NavItem { id: p.id.clone().into(), label: p.name.clone().into(), icon: Default::default() }, p.items.len() as i32))
                .unzip()
        };
        self.ui(move |ui| {
            let nav = ui.global::<Nav>();
            nav.set_playlists(ModelRc::from(std::rc::Rc::new(VecModel::from(items))));
            nav.set_playlist_counts(ModelRc::from(std::rc::Rc::new(VecModel::from(counts))));
        });
    }

    pub fn playlist_create(self: &Arc<Self>, name: &str, tracks: Vec<Track>) -> String {
        let id = self.playlists.lock().unwrap().create(name, tracks);
        self.playlists_changed();
        id
    }

    pub fn playlist_add(self: &Arc<Self>, id: &str, tracks: Vec<Track>) {
        let (added, name) = {
            let mut playlists = self.playlists.lock().unwrap();
            let added = playlists.add(id, tracks.clone());
            (added, playlists.get(id).map(|p| p.name.clone()).unwrap_or_default())
        };
        self.playlists_changed();
        match (added, tracks.first()) {
            (0, Some(t)) => self.toast(&format!("„{}“ ist schon in „{name}“", t.title), false),
            _ => self.toast(&format!("Zu „{name}“ hinzugefügt"), false),
        }
        let pid = id.to_string();
        self.refresh_if_showing(|p| matches!(p, PageKind::Playlist(x) if *x == pid));
    }

    pub fn playlist_delete(self: &Arc<Self>, id: &str) {
        self.playlists.lock().unwrap().delete(id);
        self.playlists_changed();
        self.toast("Playlist gelöscht", false);
        self.show_view("home");
    }

    pub fn playlist_remove_item(self: &Arc<Self>, id: &str, index: usize) {
        self.playlists.lock().unwrap().remove_item(id, index);
        self.playlists_changed();
        let pid = id.to_string();
        self.refresh_if_showing(|p| matches!(p, PageKind::Playlist(x) if *x == pid));
    }

    /// Namensdialog: neue Playlist (mit Songs) oder Umbenennen
    pub fn ask_playlist_name(self: &Arc<Self>, rename: Option<String>) {
        let current = rename.as_ref().and_then(|id| self.playlists.lock().unwrap().get(id).map(|p| p.name.clone()));
        *self.dialog_target.lock().unwrap() = rename.clone();
        let title = if rename.is_some() { "Playlist umbenennen" } else { "Neue Wavely-Playlist" };
        let value = current.unwrap_or_else(|| {
            self.dialog_tracks.lock().unwrap().first().map(|t| format!("Mix mit {}", t.title)).unwrap_or_else(|| "Neue Playlist".into())
        });
        let confirm = if rename.is_some() { "Speichern" } else { "Erstellen" };
        self.ui(move |ui| {
            let d = ui.global::<Dialog>();
            d.set_title(title.into());
            d.set_value(value.into());
            d.set_confirm_text(confirm.into());
            d.set_open(true);
        });
    }

    pub fn dialog_confirm(self: &Arc<Self>, value: String) {
        self.ui(|ui| ui.global::<Dialog>().set_open(false));
        match self.dialog_target.lock().unwrap().take() {
            Some(target) if target == "together" => self.together_join(value),
            Some(id) => {
                self.playlists.lock().unwrap().rename(&id, &value);
                self.playlists_changed();
                self.refresh_if_showing(|p| matches!(p, PageKind::Playlist(x) if *x == id));
            }
            None => {
                let tracks = std::mem::take(&mut *self.dialog_tracks.lock().unwrap());
                let id = self.playlist_create(&value, tracks);
                self.open(PageKind::Playlist(id));
            }
        }
    }
}
