//! Kontextmenü für Songs (wie TrackMenu.svelte), Warteschlange, Likes und kurze Meldungen.

use std::sync::{atomic::Ordering, Arc};

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{icon, player::Track, services::ytm, youtube::YtPage, App, Menu, MenuItem, Toast};

/// Wofür das Menü gerade offen ist
#[derive(Clone)]
pub struct MenuContext {
    pub track: Track,
    /// Aus der Warteschlange geöffnet (Index)
    pub queue_index: Option<usize>,
    /// Aus einer Wavely-Playlist geöffnet (Playlist-ID, Eintrag)
    pub playlist: Option<(String, usize)>,
}

fn item(id: &str, label: &str, icon: &str, separator: bool, chevron: bool) -> MenuItem {
    MenuItem { id: id.into(), label: label.into(), icon: icon.into(), separator, chevron }
}

impl App {
    pub fn toast(self: &Arc<Self>, text: &str, error: bool) {
        let token = self.toast_token.fetch_add(1, Ordering::SeqCst) + 1;
        let text = text.to_string();
        let app = self.clone();
        self.ui(move |ui| {
            let t = ui.global::<Toast>();
            t.set_text(text.into());
            t.set_error(error);
            t.set_visible(true);
            let weak = ui.as_weak();
            slint::Timer::single_shot(std::time::Duration::from_millis(if error { 5000 } else { 2800 }), move || {
                if app.toast_token.load(Ordering::SeqCst) == token {
                    if let Some(ui) = weak.upgrade() {
                        ui.global::<Toast>().set_visible(false);
                    }
                }
            });
        });
    }

    pub fn open_track_menu(self: &Arc<Self>, context: MenuContext, x: f32, y: f32) {
        *self.menu.lock().unwrap() = Some(context);
        self.show_menu_page("main", x, y);
    }

    fn show_menu_page(self: &Arc<Self>, page: &str, x: f32, y: f32) {
        let Some(context) = self.menu.lock().unwrap().clone() else { return };
        let track = &context.track;
        let mut items = Vec::new();
        match page {
            "youtube" => {
                items.push(item("yt-new", "Neue YouTube-Playlist…", icon::PLUS, false, false));
                for (i, (id, title)) in self.yt_playlists.lock().unwrap().iter().enumerate() {
                    items.push(item(&format!("yt-add:{id}"), title, icon::YOUTUBE, i == 0, false));
                }
            }
            "wavely" => {
                items.push(item("new-playlist", "Neue Playlist…", icon::PLUS, false, false));
                for (i, p) in self.playlists.lock().unwrap().list.iter().enumerate() {
                    items.push(item(&format!("add:{}", p.id), &p.name, icon::PLAYLIST_ADD, i == 0, false));
                }
            }
            _ => {
                items.push(item("play-next", "Als Nächstes spielen", icon::PLAY_NEXT, false, false));
                items.push(item("add-queue", "Zur Warteschlange hinzufügen", icon::ADD_QUEUE, false, false));
                if track.source == "youtube" {
                    items.push(item("radio", "Radio starten", icon::RADIO, false, false));
                }
                items.push(item("wavely", "Zu Wavely-Playlist hinzufügen", icon::PLAYLIST_ADD, true, true));
                if track.source == "youtube" && self.yt_logged_in.load(Ordering::Relaxed) {
                    let liked = self.liked.lock().unwrap().contains(&track.id);
                    items.push(item(
                        "like",
                        if liked { "Gefällt mir nicht mehr" } else { "Gefällt mir" },
                        if liked { icon::HEART } else { icon::HEART_OUTLINE },
                        true,
                        false,
                    ));
                    items.push(item("youtube", "Zu YouTube-Playlist hinzufügen", icon::YOUTUBE, false, true));
                }
                if matches!(track.source.as_str(), "youtube" | "spotify" | "soundcloud") {
                    let saved = self.offline.path_of(&track.id).is_some();
                    items.push(item(
                        if saved { "offline-remove" } else { "offline-save" },
                        if saved { "Offline entfernen" } else { "Offline speichern" },
                        if saved { icon::TRASH } else { icon::DOWNLOAD },
                        true,
                        false,
                    ));
                }
                let artist = track.artists.iter().find(|a| a.id.is_some());
                let mut first_nav = true;
                if track.source == "youtube" {
                    if let Some(a) = artist {
                        items.push(item("artist", &format!("Zu {}", a.name), icon::MUSIC, true, false));
                        first_nav = false;
                    }
                    if track.album_id.is_some() {
                        items.push(item("album", "Zum Album", icon::QUEUE, first_nav, false));
                    }
                }
                if context.queue_index.is_some() {
                    items.push(item("remove-queue", "Aus der Warteschlange entfernen", icon::CLOSE, true, false));
                }
                if context.playlist.is_some() {
                    items.push(item("remove-playlist", "Aus dieser Playlist entfernen", icon::TRASH, true, false));
                }
            }
        }
        self.ui(move |ui| {
            let m = ui.global::<Menu>();
            m.set_items(ModelRc::from(std::rc::Rc::new(VecModel::from(items))));
            if x >= 0.0 {
                m.set_x(x);
                m.set_y(y);
            }
            m.set_open(true);
        });
    }

    /// Discord-Status zum laufenden Song (Verbindung im Hintergrund, Discord kann fehlen)
    pub fn send_presence(self: &Arc<Self>) {
        // Automatische Tests sollen keinen Discord-Status setzen
        if std::env::var_os("WAVELY_TEST_STEPS").is_some() || std::env::var_os("WAVELY_TEST_PLAY").is_some() {
            return;
        }
        let (enabled, display) = {
            let s = self.settings.lock().unwrap();
            (s.discord_enabled, s.discord_display.clone())
        };
        let track = {
            let s = self.state.lock().unwrap();
            s.current().cloned().map(|t| (t, s.position, s.duration, !s.playing))
        };
        let presence = self.presence.clone();
        self.rt.spawn_blocking(move || {
            let mut presence = presence.lock().unwrap();
            match track {
                Some((t, position, duration, paused)) if enabled => presence.set(&crate::services::presence::Track {
                    title: t.title,
                    artist: t.artist.unwrap_or_else(|| "Unbekannter Künstler".into()),
                    album: t.album,
                    artwork: t.artwork_url,
                    url: t.url,
                    position,
                    duration,
                    paused,
                    display: Some(display),
                }),
                _ => presence.clear(),
            }
        });
    }

    /// Medientasten und Taskleisten-Buttons
    pub fn windows_action(self: &Arc<Self>, action: &str) {
        let playing = self.state.lock().unwrap().playing;
        match action {
            "play" if !playing => self.toggle_play(),
            "pause" if playing => self.toggle_play(),
            "toggle" => self.toggle_play(),
            "volume-up" | "volume-down" => {
                let volume = self.state.lock().unwrap().volume;
                self.set_volume((volume + if action == "volume-up" { 0.05 } else { -0.05 }).clamp(0.0, 1.0));
                self.sync_player();
            }
            "next" => self.next(),
            "previous" => self.previous(),
            // Windows hat Fensterinhalt verworfen (Wiederherstellen nach Minimieren, Aufdecken)
            "repaint" => self.force_full_repaint(),
            "restored" => {
                // Sofort und noch einmal nach der Wiederherstellen-Animation
                self.force_full_repaint();
                let app = self.clone();
                slint::Timer::single_shot(std::time::Duration::from_millis(250), move || app.force_full_repaint());
            }
            "like" => {
                let track = self.state.lock().unwrap().current().cloned();
                if let Some(track) = track {
                    self.toggle_like(track);
                }
            }
            _ => {}
        }
    }

    pub fn close_menu(self: &Arc<Self>) {
        self.ui(|ui| ui.global::<Menu>().set_open(false));
    }

    /// Drei-Punkte-Menü der Wiedergabeleiste (seltene Knöpfe). Untermenüs öffnen an derselben Stelle.
    pub fn open_more_menu(self: &Arc<Self>, x: f32, y: f32) {
        *self.more_menu_at.lock().unwrap() = (x, y);
        let together_on = self.settings.lock().unwrap().module_on("together");
        let sleep_active = self.sleep_until.lock().unwrap().is_some() || self.sleep_end_of_track.load(Ordering::Relaxed);
        let mut items = Vec::new();
        if together_on {
            let label = if self.room.lock().unwrap().is_some() { "Zusammen hören (aktiv)" } else { "Zusammen hören" };
            items.push(item("more:together", label, icon::GROUP, false, true));
        }
        items.push(item("more:sleep", if sleep_active { "Schlaf-Timer (läuft)" } else { "Schlaf-Timer" }, icon::BEDTIME, false, true));
        items.push(item("more:sound", "Klang (Equalizer, Tempo)", icon::EQUALIZER, false, false));
        items.push(item("more:mini", "Mini-Player", icon::MINI_PLAYER, false, false));
        *self.menu.lock().unwrap() = None;
        self.ui(move |ui| {
            let m = ui.global::<crate::Menu>();
            m.set_items(slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(items))));
            m.set_x(x);
            m.set_y(y);
            m.set_open(true);
        });
    }

    pub fn menu_pick(self: &Arc<Self>, id: &str) {
        if let Some(arg) = id.strip_prefix("more:") {
            let (x, y) = *self.more_menu_at.lock().unwrap();
            match arg {
                "together" => self.open_together_menu(x, y),
                "sleep" => self.open_sleep_menu(x, y),
                "sound" => {
                    self.close_menu();
                    *self.settings_tab.lock().unwrap() = "sound".into();
                    self.ui(|ui| ui.global::<crate::SettingsTabs>().set_current("sound".into()));
                    self.show_view("settings");
                }
                "mini" => {
                    self.close_menu();
                    #[cfg(windows)]
                    self.open_mini_player();
                }
                _ => self.close_menu(),
            }
            return;
        }
        if let Some(arg) = id.strip_prefix("together:") {
            self.together_action(arg);
            return;
        }
        if let Some(arg) = id.strip_prefix("sleep:") {
            self.sleep_action(arg);
            return;
        }
        if let Some(device) = id.strip_prefix("device:") {
            self.close_menu();
            self.setting_action(&format!("outputDevice={device}"));
            self.toast(if device.is_empty() { "Ausgabe: Windows-Standard" } else { device }, false);
            return;
        }
        match id {
            "account:" => {
                self.close_menu();
                return;
            }
            "account:settings" => {
                self.close_menu();
                self.show_view("settings");
                return;
            }
            "account:logout" => {
                self.close_menu();
                self.youtube_logout();
                return;
            }
            _ => {}
        }
        // Auswahl einer Einstellung (Menü einer Auswahl-Zeile)
        if let Some(action) = id.strip_prefix("set:") {
            self.close_menu();
            self.setting_action(action);
            return;
        }
        let Some(context) = self.menu.lock().unwrap().clone() else { return };
        if id == "wavely" || id == "youtube" {
            // Untermenü an gleicher Stelle
            self.show_menu_page(id, -1.0, -1.0);
            return;
        }
        self.close_menu();
        let track = context.track.clone();
        match id {
            "play-next" => self.play_next(track),
            "add-queue" => self.add_to_queue(track),
            "radio" => self.start_radio(track),
            "like" => self.toggle_like(track),
            "artist" => {
                if let Some(a) = track.artists.iter().find(|a| a.id.is_some()) {
                    self.yt_open(YtPage::Artist { id: a.id.clone().unwrap_or_default(), name: a.name.clone() });
                }
            }
            "album" => {
                if let Some(album_id) = track.album_id.clone() {
                    self.yt_open(YtPage::Collection {
                        kind: "album".into(),
                        id: album_id,
                        title: track.album.clone().unwrap_or_default(),
                        art: track.artwork_url.clone(),
                    });
                }
            }
            "offline-save" => self.offline_save(vec![track], None),
            "offline-remove" => {
                self.offline.remove(None, Some(&track.id));
                self.toast("Offline entfernt", false);
                self.refresh_if_showing(|p| matches!(p, crate::nav::PageKind::Offline | crate::nav::PageKind::OfflineCollection(_)));
            }
            "remove-queue" => {
                if let Some(i) = context.queue_index {
                    self.remove_from_queue(i);
                }
            }
            "remove-playlist" => {
                if let Some((pid, index)) = context.playlist {
                    self.playlist_remove_item(&pid, index);
                }
            }
            "yt-new" => {
                *self.dialog_tracks.lock().unwrap() = vec![track];
                *self.dialog_target.lock().unwrap() = Some("youtube-new".into());
                self.ui(|ui| {
                    let d = ui.global::<crate::Dialog>();
                    d.set_title("Neue YouTube-Playlist (privat)".into());
                    d.set_value("Neue Playlist".into());
                    d.set_confirm_text("Erstellen".into());
                    d.set_open(true);
                });
            }
            "new-playlist" => {
                *self.dialog_tracks.lock().unwrap() = vec![track];
                self.ask_playlist_name(None);
            }
            other => {
                if let Some(pid) = other.strip_prefix("add:") {
                    self.playlist_add(pid, vec![track]);
                } else if let Some(pid) = other.strip_prefix("yt-add:") {
                    let app = self.clone();
                    let pid = pid.to_string();
                    let title = self.yt_playlists.lock().unwrap().iter().find(|(id, _)| *id == pid).map(|(_, t)| t.clone()).unwrap_or_default();
                    self.rt.spawn(async move {
                        match ytm::ytm_add_to_playlist(&app.ytm, pid, vec![track.id.clone()]).await {
                            Ok(()) => app.toast(&format!("Zu „{title}“ hinzugefügt"), false),
                            Err(e) => app.toast(&format!("Hinzufügen fehlgeschlagen: {e}"), true),
                        }
                    });
                }
            }
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
            self.toast(&format!("„{}“ kommt als Nächstes", track.title), false);
        }
    }

    /// Vor den Autoplay-Vorschlägen anhängen
    pub fn add_to_queue(self: &Arc<Self>, track: Track) {
        let empty = {
            let mut s = self.state.lock().unwrap();
            if s.index.is_none() {
                true
            } else {
                let at = s.auto_from.unwrap_or(s.queue.len()).min(s.queue.len());
                s.queue.insert(at, track.clone());
                if let Some(a) = s.auto_from.as_mut() {
                    *a += 1;
                }
                s.original.push(track.clone());
                false
            }
        };
        if empty {
            self.play_list(vec![track], 0);
        } else {
            self.sync_queue();
            self.toast(&format!("„{}“ zur Warteschlange hinzugefügt", track.title), false);
        }
    }

    pub fn remove_from_queue(self: &Arc<Self>, index: usize) {
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
            if let Some(a) = s.auto_from.as_mut() {
                if index < *a {
                    *a -= 1;
                }
            }
        }
        self.sync_queue();
        self.sync_player();
    }

    /// Song plus ähnliche Songs von YouTube Music
    pub fn start_radio(self: &Arc<Self>, seed: Track) {
        self.toast(&format!("Radio zu „{}“ wird gestartet…", seed.title), false);
        let app = self.clone();
        self.rt.spawn(async move {
            match ytm::ytm_radio(&app.ytm, seed.id.clone()).await {
                Ok(radio) => {
                    let list: Vec<Track> = std::iter::once(seed).chain(radio.into_iter().map(Track::from)).collect();
                    let _ = slint::invoke_from_event_loop(move || app.play_list(list, 0));
                }
                Err(e) => app.toast(&format!("Radio konnte nicht gestartet werden: {e}"), true),
            }
        });
    }

    pub fn toggle_like(self: &Arc<Self>, track: Track) {
        let liked = {
            let mut set = self.liked.lock().unwrap();
            let liked = !set.contains(&track.id);
            if liked {
                set.insert(track.id.clone());
            } else {
                set.remove(&track.id);
            }
            liked
        };
        self.sync_player();
        let app = self.clone();
        self.rt.spawn(async move {
            match ytm::ytm_set_like(&app.ytm, track.id.clone(), liked).await {
                Ok(()) => app.toast(if liked { "Zu „Musik, die ich mag“ hinzugefügt" } else { "Aus „Musik, die ich mag“ entfernt" }, false),
                Err(e) => {
                    // Zurücknehmen
                    let mut set = app.liked.lock().unwrap();
                    if liked {
                        set.remove(&track.id);
                    } else {
                        set.insert(track.id.clone());
                    }
                    drop(set);
                    app.sync_player();
                    app.toast(&format!("Like fehlgeschlagen: {e}"), true);
                }
            }
        });
    }

    /// Likes und Kontoname von YouTube Music laden
    /// Kleines Menü anzeigen (Einträge: Kennung, Text, Symbol)
    fn simple_menu(self: &Arc<Self>, entries: Vec<(String, String, &'static str)>, x: f32, y: f32) {
        let items: Vec<crate::MenuItem> = entries
            .into_iter()
            .map(|(id, label, icon)| crate::MenuItem { id: id.into(), label: label.into(), icon: icon.into(), separator: false, chevron: false })
            .collect();
        *self.menu.lock().unwrap() = None;
        self.ui(move |ui| {
            let m = ui.global::<crate::Menu>();
            m.set_items(ModelRc::from(std::rc::Rc::new(VecModel::from(items))));
            m.set_x(x);
            m.set_y(y);
            m.set_open(true);
        });
    }

    /// Konto-Knopf oben rechts (YouTube Music)
    pub fn open_account_menu(self: &Arc<Self>, x: f32, y: f32) {
        let name = self.ui.upgrade().map(|ui| ui.global::<crate::YouTube>().get_account_name().to_string()).unwrap_or_default();
        self.simple_menu(
            vec![
                ("account:".into(), name, crate::icon::YOUTUBE),
                ("account:settings".into(), "Einstellungen".into(), crate::icon::SETTINGS),
                ("account:logout".into(), "Abmelden".into(), crate::icon::LOGOUT),
            ],
            x,
            y,
        );
    }

    /// Ausgabegerät wählen (Player-Leiste)
    pub fn open_device_menu(self: &Arc<Self>, x: f32, y: f32) {
        let current = self.settings.lock().unwrap().output_device.clone();
        let mut entries = vec![(String::new(), "Windows-Standard".to_string())];
        entries.extend(crate::audio::device_names().into_iter().map(|n| (n.clone(), n)));
        let count = entries.len() as f32;
        let entries = entries
            .into_iter()
            .map(|(id, label)| {
                let icon = if id == current { crate::icon::CHECK } else { crate::icon::DEVICES };
                (format!("device:{id}"), label, icon)
            })
            .collect();
        // Menü über dem Knopf (etwa 40 px pro Eintrag)
        self.simple_menu(entries, x, (y - count * 40.0 - 12.0).max(8.0));
    }

    /// Klick auf den Künstler in der Player-Leiste
    pub fn open_current_artist(self: &Arc<Self>) {
        let Some(track) = self.state.lock().unwrap().current().cloned() else { return };
        let artist = track.artists.iter().find(|a| a.id.is_some()).cloned();
        match (track.source.as_str(), artist) {
            ("youtube", Some(a)) => self.yt_open(YtPage::Artist { id: a.id.unwrap_or_default(), name: a.name }),
            ("spotify", Some(a)) => self.open(crate::nav::PageKind::Sp(crate::spotify_view::SpPage::Artist { id: a.id.unwrap_or_default(), name: a.name })),
            _ => {
                // Ohne Künstler-ID: bei YouTube Music suchen
                let name = track.artists.first().map(|a| a.name.clone()).or(track.artist.clone()).unwrap_or_default();
                let name = name.split(',').next().unwrap_or("").trim().to_string();
                if !name.is_empty() {
                    if let Some(ui) = self.ui.upgrade() {
                        ui.global::<crate::YouTube>().set_query(name.clone().into());
                    }
                    self.yt_open(YtPage::Search(name));
                }
            }
        }
    }

    pub fn yt_init(self: &Arc<Self>) {
        self.yt_init_with(true);
    }

    fn yt_init_with(self: &Arc<Self>, may_refresh: bool) {
        if may_refresh {
            self.yt_checking.store(true, Ordering::Relaxed);
        }
        let app = self.clone();
        self.rt.spawn(async move {
            let logged_in = ytm::ytm_status(&app.ytm).await.unwrap_or(false);
            if !logged_in {
                app.yt_checking.store(false, Ordering::Relaxed);
            }
            app.yt_logged_in.store(logged_in, Ordering::Relaxed);
            app.ui(move |ui| ui.global::<crate::YouTube>().set_status(if logged_in { "loggedIn" } else { "loggedOut" }.into()));
            let a = app.clone();
            let _ = slint::invoke_from_event_loop(move || a.refresh_if_showing(|p| matches!(p, crate::nav::PageKind::Yt(YtPage::Library))));
            if !logged_in {
                return;
            }
            if let Ok(library) = ytm::ytm_library(&app.ytm).await {
                *app.yt_playlists.lock().unwrap() =
                    library.playlists.into_iter().filter(|p| p.editable).map(|p| (p.id, p.title)).collect();
            }
            let liked = ytm::ytm_liked_ids(&app.ytm).await;
            let account = ytm::ytm_account(&app.ytm).await.ok();
            let photo = account.as_ref().and_then(|a| a.photo_url.clone());
            let name = account.and_then(|a| a.name);
            // Weder Likes noch Kontoname: Die übernommene Anmeldung gilt nicht mehr → neu anmelden
            if liked.is_err() && name.is_none() {
                // Erst versuchen, die Anmeldung im unsichtbaren Browser aufzufrischen
                if may_refresh {
                    let a = app.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        let b = a.clone();
                        a.youtube_refresh(move |ok| if ok { b.yt_init_with(false) } else { b.yt_expired() });
                    });
                } else {
                    app.yt_expired();
                }
                return;
            }
            if let Ok(ids) = liked {
                *app.liked.lock().unwrap() = ids.into_iter().collect();
                app.sync_player();
            }
            let name = name.unwrap_or_default();
            if let Some(url) = photo {
                let weak = app.ui.clone();
                app.covers.load(&app, url, 96, move |image| {
                    if let Some(ui) = weak.upgrade() {
                        let y = ui.global::<crate::YouTube>();
                        y.set_account_photo(round_image(&image));
                        y.set_has_account_photo(true);
                    }
                });
            }
            let initial = name.trim().chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
            app.ui(move |ui| {
                let y = ui.global::<crate::YouTube>();
                y.set_account_name(name.into());
                y.set_account_initial(initial.into());
            });
            // Prüfung fertig: Bibliothek mit Inhalt zeigen
            app.yt_checking.store(false, Ordering::Relaxed);
            let a = app.clone();
            let _ = slint::invoke_from_event_loop(move || a.refresh_if_showing(|p| matches!(p, crate::nav::PageKind::Yt(YtPage::Library))));
        });
    }

    /// Die Anmeldung gilt nicht mehr → Anmelden-Knopf zeigen
    fn yt_expired(self: &Arc<Self>) {
        self.yt_logged_in.store(false, Ordering::Relaxed);
        self.yt_checking.store(false, Ordering::Relaxed);
        self.ui(|ui| ui.global::<crate::YouTube>().set_status("loggedOut".into()));
        self.toast("Die YouTube-Music-Anmeldung ist abgelaufen – bitte neu anmelden.", true);
        let a = self.clone();
        let _ = slint::invoke_from_event_loop(move || {
            a.refresh_if_showing(|p| matches!(p, crate::nav::PageKind::Yt(YtPage::Library) | crate::nav::PageKind::Settings));
        });
    }
}

/// Bild kreisrund ausschneiden (der Software-Renderer schneidet nicht rund zu)
fn round_image(image: &slint::Image) -> slint::Image {
    let Some(mut buffer) = image.to_rgba8() else { return image.clone() };
    let (w, h) = (buffer.width() as f32, buffer.height() as f32);
    let r = w.min(h) / 2.0;
    let width = buffer.width() as usize;
    for (i, px) in buffer.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = ((i % width) as f32 + 0.5 - w / 2.0, (i / width) as f32 + 0.5 - h / 2.0);
        // Weicher Rand über einen Pixel
        let coverage = (r - (x * x + y * y).sqrt()).clamp(0.0, 1.0);
        px.a = (px.a as f32 * coverage) as u8;
    }
    slint::Image::from_rgba8(buffer)
}
