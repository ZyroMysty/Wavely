//! Einstellungsseite (wie SettingsView.svelte), gebaut aus Zeilen der allgemeinen Seite.

use std::sync::{atomic::Ordering, Arc};

use crate::{
    nav::PageKind,
    pages::{CardData, PageData, RowData, Target},
    App,
};

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

    /// Nur Text (Titel + Beschreibung)
    fn info(&mut self, title: &str, description: &str) {
        self.rows.push(RowData { kind: "setting", label: "none".into(), text: title.into(), subtext: description.into(), ..Default::default() });
    }
}

pub fn page(app: &Arc<App>) -> PageData {
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
        p.info("Angemeldet", "Die Anmeldung wurde aus der bisherigen Wavely übernommen.");
        p.toggle("syncYouTubeHistory", "Verlauf mit YouTube Music abgleichen", "In Wavely gehörte Songs auch im YouTube-Music-Verlauf speichern", s.sync_you_tube_history);
    } else {
        p.info("Nicht angemeldet", "Die Anmeldung bei Google kommt in einer der nächsten nativen Testversionen. Suchen geht auch ohne.");
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
        p.button("soundcloud-logout", "Angemeldet", "Die Anmeldung wurde aus der bisherigen Wavely übernommen.", "Abmelden");
    } else {
        p.info("Nicht angemeldet", "Die SoundCloud-Anmeldung kommt in einer der nächsten nativen Testversionen. Suchen und Abspielen geht auch ohne.");
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
    p.info("Arbeitsspeicher", &format!("{:.1} MB privat", crate::host::private_bytes() as f64 / 1_048_576.0));
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
        self.send_presence();
        self.apply_appearance();
        self.apply_quality();
        self.settings_changed();
        true
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
