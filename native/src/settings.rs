//! Einstellungsseite (wie SettingsView.svelte), gebaut aus Zeilen der allgemeinen Seite.

use std::sync::{atomic::Ordering, Arc};

use slint::ComponentHandle;

use crate::{
    nav::PageKind,
    pages::{CardData, PageData, RowData, Target},
    App,
};

/// Equalizer-Voreinstellungen (wie sound.svelte.js)
pub const EQ_PRESETS: &[(&str, &str, [f32; 10])] = &[
    ("flat", "Aus (neutral)", [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("bass", "Bass Boost", [6.0, 5.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("treble", "Höhen", [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 4.0, 5.0, 6.0]),
    ("vocal", "Gesang", [-2.0, -2.0, -1.0, 1.0, 3.0, 4.0, 3.0, 1.0, 0.0, -1.0]),
    ("pop", "Pop", [-1.0, 1.0, 3.0, 4.0, 2.0, 0.0, -1.0, -1.0, 1.0, 2.0]),
    ("rock", "Rock", [4.0, 3.0, 1.0, -1.0, -2.0, -1.0, 1.0, 3.0, 4.0, 4.0]),
    ("electronic", "Elektronisch", [5.0, 4.0, 1.0, 0.0, -2.0, 1.0, 0.0, 2.0, 4.0, 5.0]),
    ("acoustic", "Akustik", [3.0, 2.0, 1.0, 1.0, 2.0, 2.0, 3.0, 2.0, 2.0, 1.0]),
    ("speech", "Sprache / Podcast", [-6.0, -4.0, -2.0, 0.0, 2.0, 4.0, 4.0, 3.0, 1.0, -2.0]),
    ("night", "Nachtmodus (leise Stellen lauter)", [-3.0, -2.0, 0.0, 1.0, 2.0, 2.0, 2.0, 1.0, 0.0, -1.0]),
];

pub const ACCENTS: &[(&str, &str)] = &[
    ("Pink", "#ff4f7b"),
    ("Violett", "#8b5cf6"),
    ("Blau", "#3b82f6"),
    ("Türkis", "#14b8a6"),
    ("Grün", "#22c55e"),
    ("Gelb", "#eab308"),
    ("Orange", "#f97316"),
    ("Rot", "#ef4444"),
];

fn option(id: String, title: &str) -> CardData {
    CardData { title: title.into(), subtitle: String::new(), art: None, round: false, target: Target::Action(id) }
}

impl PageData {
    /// Einstellung mit Schalter
    fn toggle(&mut self, key: &str, title: &str, description: &str, on: bool) {
        self.rows.push(RowData {
            kind: "setting",
            label: "toggle".into(),
            text: title.into(),
            subtext: description.into(),
            number: on as i32,
            cards: vec![option(format!("toggle:{key}"), "")],
            ..Default::default()
        });
    }

    /// Einstellung mit Auswahl (Segmente)
    fn choice(&mut self, key: &str, title: &str, description: &str, options: &[(&str, &str)], current: &str) {
        self.rows.push(RowData {
            kind: "setting",
            label: "choice".into(),
            text: title.into(),
            subtext: description.into(),
            number: options.iter().position(|(v, _)| *v == current).map(|i| i as i32).unwrap_or(-1),
            cards: options.iter().map(|(value, label)| option(format!("{key}={value}"), label)).collect(),
            ..Default::default()
        });
    }

    /// Einstellung mit Knopf
    fn button(&mut self, id: &str, title: &str, description: &str, button: &str) {
        self.rows.push(RowData {
            kind: "setting",
            label: "button".into(),
            text: title.into(),
            subtext: description.into(),
            cards: vec![option(id.into(), button)],
            ..Default::default()
        });
    }

    /// Auswahl über ein Menü: Optionen (Wert, Beschriftung)
    fn select(&mut self, key: &str, title: &str, description: &str, options: &[(String, String)], current: &str) {
        let label = options.iter().find(|(v, _)| v == current).map(|(_, l)| l.clone()).unwrap_or_else(|| current.to_string());
        self.rows.push(RowData {
            kind: "select",
            label,
            text: title.into(),
            subtext: description.into(),
            cards: options.iter().map(|(value, l)| option(format!("{key}={value}"), l)).collect(),
            ..Default::default()
        });
    }

    /// Nur Text (Titel + Beschreibung)
    fn info(&mut self, title: &str, description: &str) {
        self.rows.push(RowData { kind: "setting", label: "none".into(), text: title.into(), subtext: description.into(), ..Default::default() });
    }
}

/// Reiter der Einstellungen: (Kennung, Name, Abschnitte)
pub const TABS: &[(&str, &str, &[&str])] = &[
    ("account", "Wavely-Konto", &["Wavely-Konto"]),
    ("appearance", "Darstellung", &["Darstellung"]),
    ("playback", "Wiedergabe", &["Wiedergabe"]),
    ("sound", "Klang", &["Klang"]),
    ("services", "Dienste", &["YouTube Music", "Spotify", "SoundCloud", "Last.fm", "Discord"]),
    ("offline", "Offline", &["Offline"]),
    ("windows", "Windows", &["Windows", "Tastenkürzel"]),
    ("about", "Über Wavely", &["Über Wavely"]),
];

/// Nur die Abschnitte des gewählten Reiters
pub fn page(app: &Arc<App>) -> PageData {
    let tab = app.settings_tab.lock().unwrap().clone();
    let (_, label, sections) = TABS.iter().find(|(id, _, _)| *id == tab).copied().unwrap_or(TABS[0]);
    let all = full_page(app);
    let mut p = PageData { tracks: all.tracks, ..Default::default() };
    p.title(label);
    let mut keep = false;
    for row in all.rows.into_iter().skip(1) {
        if row.kind == "heading" {
            keep = sections.contains(&row.text.as_str());
            // Überschrift nur, wenn der Reiter mehrere Abschnitte hat
            if keep && sections.len() > 1 {
                p.rows.push(row);
            }
            continue;
        }
        if keep {
            p.rows.push(row);
        }
    }
    p.spacer();
    p
}

fn full_page(app: &Arc<App>) -> PageData {
    let s = app.settings.lock().unwrap().clone();
    let mut p = PageData::default();
    p.title("Einstellungen");

    // Wavely-Konto
    p.heading("Wavely-Konto");
    match app.account.logged_in() {
        Some(session) => {
            let status = app.sync_status.lock().unwrap().clone();
            let admin = if app.account.admin.load(Ordering::Relaxed) { " · Admin" } else { "" };
            p.info(&session.name, &format!("{}{admin}", session.email));
            p.info("Abgleich", if status.is_empty() { "Playlists und Verlauf werden abgeglichen…" } else { &status });
            p.button("account-logout", "Abmelden", "Deine Daten bleiben auf diesem PC und im Konto erhalten.", "Abmelden");
        }
        None => {
            let busy = *app.account_busy.lock().unwrap();
            p.rows.push(RowData {
                kind: "login",
                text: "Melde dich an, um Playlists und Verlauf zwischen deinen Geräten abzugleichen.".into(),
                subtext: app.account_error.lock().unwrap().clone().unwrap_or_default(),
                number: busy as i32,
                ..Default::default()
            });
        }
    }

    // Darstellung
    p.heading("Darstellung");
    p.choice("theme", "Design", "Hell, dunkel oder wie Windows", &[("system", "System"), ("dark", "Dunkel"), ("light", "Hell")], &s.theme);
    p.rows.push(RowData {
        kind: "setting",
        label: "swatches".into(),
        text: "Akzentfarbe".into(),
        subtext: "Farbe für Hervorhebungen".into(),
        number: ACCENTS.iter().position(|(_, hex)| hex.eq_ignore_ascii_case(&s.accent)).map(|i| i as i32).unwrap_or(-1),
        cards: ACCENTS.iter().map(|(name, hex)| CardData {
            title: name.to_string(),
            subtitle: hex.to_string(),
            art: None,
            round: true,
            target: Target::Action(format!("accent={hex}")),
        }).collect(),
        ..Default::default()
    });

    // Wiedergabe
    p.heading("Wiedergabe");
    p.toggle("autoplay", "Autoplay", "Nach dem Ende der Warteschlange passende Songs abspielen", s.autoplay);
    p.choice(
        "startView",
        "Startansicht",
        "Was beim Öffnen von Wavely angezeigt wird",
        &[("home", "Startseite"), ("youtube", "YouTube Music"), ("local", "Lokale Musik")],
        &s.start_view,
    );
    p.choice(
        "streamQuality",
        "Streaming-Qualität",
        "„Premium“ braucht YouTube Premium (256 kbit)",
        &[("low", "Niedrig"), ("high", "Hoch"), ("premium", "Premium")],
        &s.stream_quality,
    );

    p.toggle("miniMinimizesMain", "Mini-Player minimiert das Hauptfenster", "Beim Öffnen des Mini-Players (Strg + M)", s.mini_minimizes_main);

    // Klang
    p.heading("Klang");
    p.toggle("normalize", "Lautstärke angleichen", "Leise und laute Songs gleich laut abspielen", s.normalize);
    let presets: Vec<(String, String)> = EQ_PRESETS
        .iter()
        .map(|(id, name, _)| (id.to_string(), name.to_string()))
        .chain(std::iter::once(("custom".to_string(), "Eigene Einstellung".to_string())))
        .collect();
    p.select("eqPreset", "Equalizer", if s.eq_enabled { "An" } else { "Aus" }, &presets, &s.eq_preset);
    if s.eq_enabled {
        p.rows.push(RowData {
            kind: "eq",
            cards: crate::audio::dsp::EQ_BANDS
                .iter()
                .map(|f| option(String::new(), &if *f >= 1000.0 { format!("{}k", f / 1000.0) } else { format!("{f}") }))
                .collect(),
            ..Default::default()
        });
    }
    p.choice(
        "playbackRate",
        "Tempo",
        "Die Tonhöhe ändert sich mit",
        &[("0.75", "0,75×"), ("0.9", "0,9×"), ("1", "1×"), ("1.1", "1,1×"), ("1.25", "1,25×"), ("1.5", "1,5×")],
        &format!("{}", s.playback_rate),
    );
    let devices: Vec<(String, String)> = std::iter::once((String::new(), "Windows-Standard".to_string()))
        .chain(crate::audio::device_names().into_iter().map(|n| (n.clone(), n)))
        .collect();
    p.select("outputDevice", "Ausgabegerät", "Lautsprecher oder Kopfhörer für Wavely", &devices, &s.output_device);

    // Windows
    #[cfg(windows)]
    {
        p.heading("Windows");
        let autostart = crate::instance::autostart_enabled();
        p.toggle("autostart", "Mit Windows starten", "Wavely startet automatisch nach der Anmeldung", autostart);
        if autostart {
            p.toggle("startMinimized", "Minimiert starten", "Beim automatischen Start nur in der Taskleiste", s.start_minimized);
        }
    }

    // Tastenkürzel
    p.heading("Tastenkürzel");
    p.toggle("globalShortcuts", "Systemweite Tastenkürzel", "Strg + Alt + Leertaste / ← / → / ↑ / ↓ – auch wenn Wavely im Hintergrund ist", s.global_shortcuts);
    for (keys, what) in crate::keys::SHORTCUT_LIST {
        p.info(keys, what);
    }

    // Discord
    p.heading("Discord");
    p.toggle("discordEnabled", "Im Discord-Status anzeigen", "Zeigt deinen Freunden, was du gerade hörst", s.discord_enabled);
    p.choice(
        "discordDisplay",
        "Anzeige in der Mitgliederliste",
        "Was hinter „Hört“ steht",
        &[("details", "Songtitel"), ("state", "Künstler"), ("name", "Wavely")],
        &s.discord_display,
    );

    // YouTube Music
    p.heading("YouTube Music");
    if app.yt_logged_in.load(Ordering::Relaxed) {
        p.button("youtube-logout", "Angemeldet", "Likes, Bibliothek und Verlauf von YouTube Music", "Abmelden");
        p.toggle("syncYouTubeHistory", "Verlauf mit YouTube Music abgleichen", "In Wavely gehörte Songs auch im YouTube-Music-Verlauf speichern", s.sync_you_tube_history);
    } else {
        p.button("youtube-login", "Nicht angemeldet", "Suchen geht auch ohne Anmeldung", "Mit Google anmelden");
    }

    // Spotify & SoundCloud
    p.heading("Spotify");
    if app.spotify_logged_in() {
        p.button("spotify-logout", "Angemeldet", "Gespielt wird über den passenden Song bei YouTube Music", "Abmelden");
    } else {
        p.button("spotify-login", "Nicht angemeldet", "Anmeldung im Browser", "Anmelden");
    }
    p.heading("SoundCloud");
    if app.soundcloud_logged_in() {
        p.button("soundcloud-logout", "Angemeldet", "Likes und Playlists von SoundCloud", "Abmelden");
    } else {
        p.button("soundcloud-login", "Nicht angemeldet", "Suchen und Abspielen geht auch ohne Anmeldung", "Anmelden");
    }

    // Last.fm
    p.heading("Last.fm");
    match crate::services::lastfm::lastfm_status_now(&app.lastfm) {
        status if !status.available => p.info("Nicht verfügbar", "Last.fm ist in dieser Version nicht eingerichtet."),
        status => match status.user {
            Some(user) => {
                let pending = if status.pending > 0 { format!(" · {} warten aufs Senden", status.pending) } else { String::new() };
                p.button("lastfm-logout", &user, &format!("Verbunden{pending}"), "Trennen");
                p.toggle("lastfmScrobble", "Gehörte Songs scrobbeln", "Songs ab der Hälfte (höchstens 4 Minuten) eintragen", s.lastfm_scrobble);
            }
            None => p.button("lastfm-login", "Nicht verbunden", "Gehörte Songs in deinem Last.fm-Profil eintragen", "Verbinden"),
        },
    }

    // Offline
    p.heading("Offline");
    p.choice(
        "downloadQuality",
        "Download-Qualität",
        "Niedrig spart etwa die Hälfte an Speicherplatz",
        &[("low", "Niedrig"), ("high", "Hoch"), ("premium", "Premium")],
        &s.download_quality,
    );
    let (songs, bytes) = app.offline.stats();
    p.button("offline-folder", "Gespeicherte Songs", &format!("{songs} Songs · {:.0} MB", bytes as f64 / 1_048_576.0), "Ordner öffnen");
    p.button("offline-clear", "Alle Offline-Songs löschen", "Gespeicherte Playlists und Songs entfernen", "Löschen");

    // Über
    p.heading("Über Wavely");
    p.info(&format!("Wavely {} (native Testversion)", env!("CARGO_PKG_VERSION")), "Ohne Browser-Technik: Oberfläche mit Slint, eigene Audio-Engine");
    match app.update.lock().unwrap().as_ref() {
        Some(update) => p.button("update-install", &format!("Update {} verfügbar", update.version), update.notes.as_deref().unwrap_or(""), "Installieren"),
        None => p.button("update-check", "Updates", "Neue native Testversionen werden automatisch gesucht", "Jetzt suchen"),
    }
    p.info("Arbeitsspeicher", &format!("{:.1} MB privat", crate::host::private_bytes() as f64 / 1_048_576.0));
    if app.account.admin.load(Ordering::Relaxed) {
        let queue = app.state.lock().unwrap().queue.len();
        p.info(
            "Diagnose",
            &format!(
                "Warteschlange {queue} · Verlauf {} · Likes {} · Playlists {} · Lokal {} · Offline {} · Cover im Speicher {}",
                app.history.lock().unwrap().len(),
                app.liked.lock().unwrap().len(),
                app.playlists.lock().unwrap().list.len(),
                app.local_tracks.lock().unwrap().len(),
                app.offline.stats().0,
                app.covers.count(),
            ),
        );
    }
    p.spacer();
    p
}

impl App {
    /// Einstellungsseite neu aufbauen, falls sie angezeigt wird
    pub fn settings_changed(self: &Arc<Self>) {
        let a = self.clone();
        let _ = slint::invoke_from_event_loop(move || a.refresh_if_showing(|p| matches!(p, PageKind::Settings)));
    }

    /// Klick auf einen Schalter, eine Auswahl oder einen Knopf der Einstellungsseite
    pub fn setting_action(self: &Arc<Self>, action: &str) -> bool {
        if action == "account-logout" {
            self.account_logout();
            return true;
        }
        if action == "update-check" {
            self.check_update(true);
            return true;
        }
        if action == "update-install" {
            self.install_update();
            return true;
        }
        #[cfg(windows)]
        if action == "toggle:autostart" {
            crate::instance::set_autostart(!crate::instance::autostart_enabled());
            self.settings_changed();
            return true;
        }
        let (key, value) = match (action.strip_prefix("toggle:"), action.split_once('=')) {
            (Some(key), _) => (key.to_string(), None),
            (None, Some((k, v))) => (k.to_string(), Some(v.to_string())),
            _ => return false,
        };
        {
            let mut s = self.settings.lock().unwrap();
            match (key.as_str(), value) {
                ("autoplay", None) => s.autoplay = !s.autoplay,
                ("syncYouTubeHistory", None) => s.sync_you_tube_history = !s.sync_you_tube_history,
                ("discordEnabled", None) => s.discord_enabled = !s.discord_enabled,
                ("lastfmScrobble", None) => s.lastfm_scrobble = !s.lastfm_scrobble,
                ("normalize", None) => s.normalize = !s.normalize,
                ("globalShortcuts", None) => s.global_shortcuts = !s.global_shortcuts,
                ("miniMinimizesMain", None) => s.mini_minimizes_main = !s.mini_minimizes_main,
                ("startMinimized", None) => s.start_minimized = !s.start_minimized,
                ("playbackRate", Some(v)) => s.playback_rate = v.parse().unwrap_or(1.0),
                ("outputDevice", Some(v)) => s.output_device = v,
                ("eqPreset", Some(v)) => {
                    if let Some((_, _, gains)) = EQ_PRESETS.iter().find(|(id, _, _)| *id == v) {
                        s.eq_gains = gains.to_vec();
                        s.eq_enabled = v != "flat";
                    } else {
                        s.eq_enabled = true;
                    }
                    s.eq_preset = v;
                }
                ("discordDisplay", Some(v)) => s.discord_display = v,
                ("theme", Some(v)) => s.theme = v,
                ("accent", Some(v)) => s.accent = v,
                ("startView", Some(v)) => s.start_view = v,
                ("streamQuality", Some(v)) => s.stream_quality = v,
                ("downloadQuality", Some(v)) => s.download_quality = v,
                _ => return false,
            }
        }
        self.save_settings();
        self.apply_sound();
        if key == "globalShortcuts" {
            self.apply_global_shortcuts();
        }
        self.send_presence();
        self.apply_appearance();
        self.apply_quality();
        self.settings_changed();
        true
    }

    /// Klang-Einstellungen an die Audio-Engine geben
    pub fn apply_sound(self: &Arc<Self>) {
        let (dsp, rate, device, gains) = {
            let s = self.settings.lock().unwrap();
            let mut gains = [0.0f32; 10];
            for (g, v) in gains.iter_mut().zip(s.eq_gains.iter()) {
                *g = *v;
            }
            (
                crate::audio::dsp::Settings { eq_enabled: s.eq_enabled, eq_gains: gains, normalize: s.normalize },
                s.playback_rate,
                s.output_device.clone(),
                gains,
            )
        };
        self.engine.send(crate::audio::Command::Dsp(dsp));
        self.engine.send(crate::audio::Command::Device(device));
        self.engine.set_rate(rate);
        self.ui(move |ui| {
            let model: Vec<f32> = gains.to_vec();
            ui.global::<crate::Page>().set_eq_gains(slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(model))));
        });
    }

    /// Ein Equalizer-Band ziehen
    pub fn set_eq_band(self: &Arc<Self>, band: usize, db: f32) {
        {
            let mut s = self.settings.lock().unwrap();
            if s.eq_gains.len() < 10 {
                s.eq_gains.resize(10, 0.0);
            }
            if s.eq_gains[band] == db {
                return;
            }
            s.eq_gains[band] = db;
            s.eq_preset = "custom".into();
            s.eq_enabled = true;
        }
        self.save_settings();
        self.apply_sound();
    }

    /// Klick auf eine Auswahl-Zeile: Optionen als Menü
    pub fn setting_select(self: &Arc<Self>, row: usize, x: f32, y: f32) {
        let options = self.page.lock().unwrap().rows.get(row).map(|r| r.cards.clone()).unwrap_or_default();
        let items: Vec<crate::MenuItem> = options
            .iter()
            .map(|c| crate::MenuItem {
                id: match &c.target {
                    Target::Action(id) => format!("set:{id}").into(),
                    _ => Default::default(),
                },
                label: c.title.clone().into(),
                icon: Default::default(),
                separator: false,
                chevron: false,
            })
            .collect();
        *self.menu.lock().unwrap() = None;
        self.ui(move |ui| {
            let m = ui.global::<crate::Menu>();
            m.set_items(slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(items))));
            m.set_x(x);
            m.set_y(y);
            m.set_open(true);
        });
    }

    /// Audioqualität an yt-dlp weitergeben
    pub fn apply_quality(self: &Arc<Self>) {
        let (stream, download) = {
            let s = self.settings.lock().unwrap();
            (s.stream_quality.clone(), s.download_quality.clone())
        };
        let parse = |q: &str| -> crate::services::ytdlp::Quality {
            serde_json::from_value(serde_json::Value::String(q.to_string())).unwrap_or_default()
        };
        let app = self.clone();
        self.rt.spawn(async move {
            let _ = crate::services::ytm::set_audio_quality(&app.ytm, parse(&stream), parse(&download)).await;
        });
    }
}
