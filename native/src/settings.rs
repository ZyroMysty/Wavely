//! Einstellungsseite (wie SettingsView.svelte), gebaut aus Zeilen der allgemeinen Seite.

use std::sync::{atomic::Ordering, Arc};

use slint::ComponentHandle;

use crate::{
    nav::PageKind,
    pages::{CardData, PageData, RowData, Target},
    App,
};

/// Versionsverlauf aus CHANGELOG.md (wie changelog.svelte.js): (Version, Datum, Punkte), neueste zuerst
fn changelog() -> Vec<(String, String, Vec<String>)> {
    const TEXT: &str = include_str!("../../CHANGELOG.md");
    let months = ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];
    TEXT.replace('\r', "")
        .split("\n## ")
        .skip(1)
        .filter_map(|part| {
            let mut lines = part.lines();
            let head = lines.next()?;
            let (version, date) = match head.split_once(" – ").or_else(|| head.split_once(" - ")) {
                Some((v, d)) => (v.trim().to_string(), d.trim().to_string()),
                None => (head.trim().to_string(), String::new()),
            };
            if !version.starts_with(|c: char| c.is_ascii_digit()) {
                return None;
            }
            // 2026-11-02 → 2. November 2026
            let date = match date.split('-').map(|n| n.parse::<usize>().ok()).collect::<Option<Vec<_>>>().as_deref() {
                Some([y, m, d]) if (1..=12).contains(m) => format!("{d}. {} {y}", months[m - 1]),
                _ => date,
            };
            let items: Vec<String> = lines.filter_map(|l| l.strip_prefix("- ")).map(|l| l.trim().to_string()).collect();
            (!items.is_empty()).then_some((version, date, items))
        })
        .collect()
}

/// Farbe → (Farbton 0–360, Sättigung 0–1, Helligkeit 0–1)
fn rgb_to_hsv(c: slint::Color) -> (f32, f32, f32) {
    let (r, g, b) = (c.red() as f32 / 255.0, c.green() as f32 / 255.0, c.blue() as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

/// Dienste zum Ein- und Ausschalten (wie modules.svelte.js)
const MODULES: &[(&str, &str, &str)] = &[
    ("youtube", "YouTube Music", "Bibliothek, Suche und Likes (abgespielt wird über YouTube trotzdem)"),
    ("spotify", "Spotify", "Bibliothek, Suche und Premium-Wiedergabe"),
    ("soundcloud", "SoundCloud", "Likes, Playlists, Suche und Künstler"),
    ("local", "Lokale Musik", "Musikordner auf diesem PC"),
    ("lastfm", "Last.fm", "Scrobbeln und „Hört gerade“"),
    ("together", "Zusammen hören", "Knopf in der Player-Leiste"),
];

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

    /// Eingabefeld mit Knopf: `value` steht anfangs im Feld
    fn input(&mut self, key: &str, title: &str, description: &str, placeholder: &str, value: &str, button: &str) {
        self.rows.push(RowData {
            kind: "setting",
            label: "input".into(),
            text: title.into(),
            subtext: description.into(),
            cards: vec![
                CardData { title: placeholder.into(), subtitle: value.into(), art: None, round: false, target: Target::Action(key.into()) },
                option(String::new(), button),
            ],
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
    ("sound", "Audio", &["Audio"]),
    ("services", "Dienste", &["Dienste", "YouTube Music", "Spotify", "SoundCloud", "Last.fm", "Discord"]),
    ("library", "Bibliothek", &["Bibliothek"]),
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
    // Abschnitte einsammeln und in der Reihenfolge des Reiters zeigen
    let mut groups: Vec<(String, Vec<RowData>)> = Vec::new();
    for row in all.rows.into_iter().skip(1) {
        if row.kind == "heading" {
            groups.push((row.text.clone(), vec![row]));
        } else if let Some((_, rows)) = groups.last_mut() {
            rows.push(row);
        }
    }
    for section in sections.iter() {
        if let Some((_, rows)) = groups.iter_mut().find(|(name, _)| name == section) {
            // Überschrift nur, wenn der Reiter mehrere Abschnitte hat
            let skip = if sections.len() > 1 { 0 } else { 1 };
            p.rows.extend(std::mem::take(rows).into_iter().skip(skip));
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
            let ready = !status.is_empty();
            p.rows.push(RowData {
                kind: "account",
                text: session.name.clone(),
                subtext: session.email.clone(),
                label: session.name.trim().chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or("?".into()),
                number: ready as i32,
                cards: vec![option(
                    String::new(),
                    if ready { "Verlauf, Rückblick und Wavely-Playlists sind auf allen Geräten gleich" } else { "Playlists und Verlauf werden abgeglichen…" },
                )],
                ..Default::default()
            });
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
        subtext: "Für Buttons, Hervorhebungen und den Fortschrittsbalken".into(),
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
    p.toggle("sidebarCollapsed", "Seitenleiste einklappen", "Nur Symbole anzeigen, mehr Platz für Musik", s.sidebar_collapsed);
    p.button("appearance-reset", "Darstellung zurücksetzen", "Dunkles Design mit pinker Akzentfarbe", "Zurücksetzen");

    // Wiedergabe
    p.heading("Wiedergabe");
    p.toggle("autoplay", "Autoplay", "Nach dem Ende der Warteschlange passende Songs abspielen", s.autoplay);
    let views: Vec<(String, String)> = [
        ("home", "Startseite"),
        ("youtube", "YouTube Music"),
        ("spotify", "Spotify"),
        ("soundcloud", "SoundCloud"),
        ("local", "Lokale Musik"),
        ("offline", "Offline"),
    ]
    .iter()
    .filter(|(id, _)| s.module_on(id))
    .map(|(id, label)| (id.to_string(), label.to_string()))
    .collect();
    p.select("startView", "Startansicht", "Womit Wavely beim Öffnen startet", &views, &s.start_view);
    let qualities: Vec<(String, String)> = [("low", "Datensparend (~64 kbit)"), ("high", "Hoch (~130 kbit)"), ("premium", "Premium (256 kbit)")]
        .iter()
        .map(|(id, label)| (id.to_string(), label.to_string()))
        .collect();
    p.select(
        "streamQuality",
        "Streaming-Qualität",
        "Für Songs über YouTube. „Premium“ gibt es nur mit YouTube Premium.",
        &qualities,
        &s.stream_quality,
    );

    p.toggle("miniMinimizesMain", "Mini-Player minimiert das Hauptfenster", "Beim Öffnen des Mini-Players (Strg + M)", s.mini_minimizes_main);

    // Klang
    p.heading("Audio");
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
    // Tempo als Regler (0,5× – 2×) mit „Normal“
    p.rows.push(RowData {
        kind: "setting",
        label: "slider".into(),
        text: "Tempo".into(),
        subtext: format!("Schneller oder langsamer abspielen ({}×)", format!("{}", (s.playback_rate * 100.0).round() / 100.0).replace('.', ",")),
        values: vec![((s.playback_rate - 0.5) / 1.5).clamp(0.0, 1.0)],
        number: ((s.playback_rate - 1.0).abs() > 0.001) as i32,
        cards: vec![option("playbackRate".into(), "Normal")],
        ..Default::default()
    });
    p.toggle(
        "preservePitch",
        "Tonhöhe beibehalten",
        "Aus: Schneller klingt höher, langsamer tiefer – wie bei „Sped up“- und „Slowed“-Versionen",
        s.preserve_pitch,
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
        p.toggle(
            "startMinimized",
            "Minimiert starten",
            if autostart { "Beim automatischen Start nur in der Taskleiste, ohne Fenster in den Vordergrund zu holen" } else { "Wirkt, wenn „Mit Windows starten“ an ist" },
            s.start_minimized,
        );
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

    // Dienste ein- und ausschalten
    p.heading("Dienste");
    for (id, name, hint) in MODULES {
        p.toggle(&format!("module:{id}"), name, hint, s.module_on(id));
    }

    // Bibliothek (lokale Musikordner)
    if s.module_on("local") {
        p.heading("Bibliothek");
        let folders = crate::services::library::load_settings(&app.paths).folders;
        if folders.is_empty() {
            p.info("Musikordner", "Noch keine Ordner hinzugefügt (MP3, FLAC, M4A, OGG, OPUS, WAV)");
        }
        for folder in &folders {
            let name = std::path::Path::new(folder).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(folder.clone());
            p.button(&format!("local-remove:{folder}"), &name, folder, "Entfernen");
        }
        p.button("local-add", "Ordner hinzufügen", "Ordner mit lokalen Musikdateien", "Hinzufügen");
    }

    // YouTube Music
    if s.module_on("youtube") {
    p.heading("YouTube Music");
    if app.yt_logged_in.load(Ordering::Relaxed) {
        p.button("youtube-logout", "Angemeldet", "Likes, Bibliothek und Verlauf von YouTube Music", "Abmelden");
        p.toggle("syncYouTubeHistory", "Verlauf mit YouTube Music abgleichen", "In Wavely gehörte Songs auch im YouTube-Music-Verlauf speichern", s.sync_you_tube_history);
    } else {
        p.button("youtube-login", "Nicht angemeldet", "Suchen geht auch ohne Anmeldung", "Mit Google anmelden");
    }
    }

    // Spotify & SoundCloud
    if s.module_on("spotify") {
        p.heading("Spotify");
        if app.spotify_logged_in() {
            p.button("spotify-logout", "Angemeldet", "Bibliothek und Playlists von Spotify", "Abmelden");
        } else {
            p.button("spotify-login", "Nicht angemeldet", "Anmeldung im Browser", "Anmelden");
        }
        let premium = app.spotify_playback.lock().unwrap().premium;
        p.info(
            "Spotify-Wiedergabe",
            match premium {
                Some(true) => "Premium erkannt: Spotify-Songs laufen direkt über Spotify.",
                Some(false) => "Spotify Free: Spotify-Songs laufen über YouTube Music.",
                None => "Mit Premium laufen Spotify-Songs direkt über Spotify, sonst über YouTube Music.",
            },
        );
        let client_id = app.spotify.client_id.lock().unwrap().clone();
        let is_default = client_id == crate::services::spotify::DEFAULT_CLIENT_ID;
        p.input(
            "spotifyClientId",
            "Eigene Spotify Client ID",
            "Für Spotify Premium nötig (siehe unten)",
            "Standard (Wavely)",
            if is_default { "" } else { &client_id },
            "Speichern",
        );
        p.text(
            "Spotify verlangt seit 2026, dass der Inhaber einer Entwickler-App selbst Premium hat (höchstens 5 Nutzer). Lege dazu auf developer.spotify.com eine App an (Redirect URI: http://127.0.0.1:43827/callback, „Web API“ und „Web Playback SDK“) und trage hier ihre Client ID ein.",
        );
        if !is_default {
            p.button("spotify-client-default", "Standard-Client-ID", "Zurück zur Client ID von Wavely", "Zurücksetzen");
        }
    }
    if s.module_on("soundcloud") {
        p.heading("SoundCloud");
        if app.soundcloud_logged_in() {
            p.button("soundcloud-logout", "Angemeldet", "Likes und Playlists von SoundCloud", "Abmelden");
        } else {
            p.button("soundcloud-login", "Nicht angemeldet", "Suchen und Abspielen geht auch ohne Anmeldung", "Anmelden");
        }
    }

    // Last.fm
    if s.module_on("lastfm") {
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
        None => p.button("update-check", "Updates", "Neue Versionen kommen von GitHub und werden vor der Installation geprüft", "Jetzt suchen"),
    }
    p.toggle("autoUpdateCheck", "Automatisch nach Updates suchen", "Beim Start und alle paar Stunden; installiert wird erst, wenn du es bestätigst", s.auto_update_check);
    p.button("changelog", "Versionsverlauf", "Was sich in jeder Wavely-Version geändert hat", "Anzeigen");
    p.info("Verwendete Projekte", "Slint, Symphonia, cpal, wry, RustyPipe (GPL-3.0), yt-dlp (Unlicense), LRCLIB (Liedtexte), lofty, discord-rich-presence");
    if app.account.admin.load(Ordering::Relaxed) {
        p.info("Arbeitsspeicher", &format!("{:.1} MB privat", crate::host::private_bytes() as f64 / 1_048_576.0));
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
        if action == "changelog" {
            self.ui(|ui| {
                let releases: Vec<crate::Release> = changelog()
                    .into_iter()
                    .map(|(version, date, items)| crate::Release {
                        version: version.into(),
                        date: date.into(),
                        items: items.iter().map(|i| format!("•  {i}")).collect::<Vec<_>>().join("\n").into(),
                    })
                    .collect();
                let c = ui.global::<crate::Changelog>();
                c.set_releases(slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(releases))));
                c.set_open(true);
            });
            return true;
        }
        if action == "appearance-reset" {
            {
                let mut s = self.settings.lock().unwrap();
                s.theme = "dark".into();
                s.accent = "#ff4f7b".into();
                s.sidebar_collapsed = false;
            }
            self.ui(|ui| ui.global::<crate::Nav>().set_collapsed(false));
            self.save_settings();
            self.apply_appearance();
            self.settings_changed();
            return true;
        }
        if action == "toggle:sidebarCollapsed" {
            let collapsed = {
                let mut s = self.settings.lock().unwrap();
                s.sidebar_collapsed = !s.sidebar_collapsed;
                s.sidebar_collapsed
            };
            self.ui(move |ui| ui.global::<crate::Nav>().set_collapsed(collapsed));
            self.save_settings();
            self.settings_changed();
            return true;
        }
        if action == "color-picker" {
            let accent = self.settings.lock().unwrap().accent.clone();
            let (h, s, v) = crate::parse_hex(&accent).map(rgb_to_hsv).unwrap_or((343.0, 0.7, 1.0));
            self.ui(move |ui| {
                let c = ui.global::<crate::ColorPick>();
                c.set_hue(h);
                c.set_sat(s);
                c.set_val(v);
                c.set_open(true);
            });
            return true;
        }
        if action == "spotify-client-default" {
            self.set_spotify_client_id(None);
            return true;
        }
        if let Some(id) = action.strip_prefix("toggle:module:") {
            {
                let mut s = self.settings.lock().unwrap();
                let on = s.module_on(id);
                s.modules.insert(id.to_string(), !on);
            }
            self.save_settings();
            self.apply_modules();
            self.settings_changed();
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
                ("autoUpdateCheck", None) => s.auto_update_check = !s.auto_update_check,
                ("preservePitch", None) => s.preserve_pitch = !s.preserve_pitch,
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

    /// Eingabefeld der Einstellungen bestätigt
    pub fn setting_input(self: &Arc<Self>, key: &str, value: &str) {
        let value = value.trim();
        match key {
            "accent" => {
                let hex = if value.starts_with('#') { value.to_string() } else { format!("#{value}") };
                if crate::parse_hex(&hex).is_none() {
                    self.toast("Bitte einen Farbwert wie #ff4f7b eingeben", true);
                    return;
                }
                self.settings.lock().unwrap().accent = hex.to_lowercase();
                self.save_settings();
                self.apply_appearance();
                self.settings_changed();
            }
            "playbackRate" => {
                // Regler 0–1 → 0,5× – 2× in Schritten von 0,05
                let Ok(v) = value.parse::<f32>() else { return };
                let rate = ((0.5 + v.clamp(0.0, 1.0) * 1.5) * 20.0).round() / 20.0;
                self.settings.lock().unwrap().playback_rate = rate;
                self.save_settings();
                self.apply_sound();
                self.settings_changed();
            }
            "accountDelete" => {
                if value.is_empty() {
                    return;
                }
                self.account_delete(value.to_string());
            }
            "accountName" => {
                if value.is_empty() {
                    return;
                }
                let app = self.clone();
                let name = value.to_string();
                self.rt.spawn(async move {
                    match app.account.rename(&name).await {
                        Ok(()) => app.toast("Name geändert", false),
                        Err(e) => app.toast(&e, true),
                    }
                    app.settings_changed();
                });
            }
            "spotifyClientId" => {
                if value.is_empty() {
                    return;
                }
                self.set_spotify_client_id(Some(value.to_string()));
            }
            _ => {}
        }
    }

    /// Eigene Spotify Client ID (oder `None` = Standard). Meldet bei Spotify ab.
    fn set_spotify_client_id(self: &Arc<Self>, id: Option<String>) {
        let app = self.clone();
        self.rt.spawn(async move {
            let reset = id.is_none();
            match crate::services::spotify::spotify_set_client_id(&app.spotify, id).await {
                Ok(()) => {
                    let _ = crate::services::spotify::spotify_logout(&app.spotify).await;
                    app.toast(
                        if reset { "Standard-Client-ID wiederhergestellt – bitte neu anmelden" } else { "Client ID gespeichert – bitte bei Spotify neu anmelden" },
                        false,
                    );
                }
                Err(e) => app.toast(&e, true),
            }
            app.settings_changed();
        });
    }

    /// Ein- und ausgeschaltete Dienste in Seitenleiste und Player-Leiste übernehmen
    pub fn apply_modules(self: &Arc<Self>) {
        let s = self.settings.lock().unwrap().clone();
        let on: Vec<bool> = ["youtube", "spotify", "soundcloud", "local", "together"].iter().map(|id| s.module_on(id)).collect();
        let app = self.clone();
        self.ui(move |ui| {
            let n = ui.global::<crate::Nav>();
            n.set_youtube_on(on[0]);
            n.set_spotify_on(on[1]);
            n.set_soundcloud_on(on[2]);
            n.set_local_on(on[3]);
            n.set_together_on(on[4]);
            // Angezeigter Dienst abgeschaltet → Startseite
            let view = n.get_view().to_string();
            if !n.invoke_shown(view.into()) {
                app.show_view("home");
            }
        });
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
        let preserve = self.settings.lock().unwrap().preserve_pitch;
        // Klang hat sich geändert: Vorrat verwerfen und ab der aktuellen Stelle neu berechnen,
        // sonst hört man die Änderung erst nach dem Puffer (rund 2 Sekunden)
        static LAST: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
        let key = format!("{dsp:?}|{rate}|{preserve}");
        let changed = LAST.lock().unwrap().replace(key.clone()).is_some_and(|old| old != key);
        self.engine.send(crate::audio::Command::Dsp(dsp));
        self.engine.send(crate::audio::Command::Device(device));
        self.engine.set_rate(rate);
        self.engine.set_preserve_pitch(preserve);
        #[cfg(windows)]
        let spotify = self.spotify_active();
        #[cfg(not(windows))]
        let spotify = false;
        if changed && !spotify && self.state.lock().unwrap().current().is_some() {
            self.engine.send(crate::audio::Command::Seek(self.engine.played_seconds()));
        }
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
