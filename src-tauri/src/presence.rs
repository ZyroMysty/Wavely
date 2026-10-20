use std::time::{SystemTime, UNIX_EPOCH};

use discord_rich_presence::{
    activity::{Activity, ActivityType, Assets, Button, StatusDisplayType, Timestamps},
    DiscordIpc, DiscordIpcClient,
};
use serde::Deserialize;

/// Application ID aus dem Discord Developer Portal (https://discord.com/developers/applications).
/// Der Name der Discord-App erscheint im Status als „Hört <Name>“.
const DISCORD_CLIENT_ID: &str = "1562117922736971816";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub artwork: Option<String>,
    pub url: Option<String>,
    /// Aktuelle Position in Sekunden
    pub position: f64,
    /// Gesamtlänge in Sekunden (0 bei Livestreams)
    pub duration: f64,
    pub paused: bool,
}

/// Hält die Verbindung zum lokalen Discord-Client und baut sie bei Bedarf neu auf.
#[derive(Default)]
pub struct Presence {
    client: Option<DiscordIpcClient>,
}

impl Presence {
    fn client(&mut self) -> Option<&mut DiscordIpcClient> {
        if DISCORD_CLIENT_ID.starts_with("DEINE_") {
            return None; // Noch keine Discord-App eingetragen
        }
        if self.client.is_none() {
            let mut client = DiscordIpcClient::new(DISCORD_CLIENT_ID);
            // Schlägt fehl, wenn Discord nicht läuft; beim nächsten Update wird es erneut versucht.
            if client.connect().is_ok() {
                self.client = Some(client);
            }
        }
        self.client.as_mut()
    }

    pub fn set(&mut self, track: &Track) {
        let details = discord_text(&track.title);
        let state = if track.paused {
            discord_text(&format!("Pausiert · {}", track.artist))
        } else {
            discord_text(&track.artist)
        };
        let large_text = track.album.as_deref().map(discord_text);

        let mut assets = Assets::new();
        if let Some(artwork) = &track.artwork {
            assets = assets.large_image(artwork);
        }
        if let Some(text) = &large_text {
            assets = assets.large_text(text);
        }

        let mut activity = Activity::new()
            .activity_type(ActivityType::Listening)
            // In der Mitgliederliste „Hört <Songtitel>“ statt „Hört Wavely“ anzeigen
            .status_display_type(StatusDisplayType::Details)
            .details(&details)
            .state(&state)
            .assets(assets);

        if !track.paused && track.duration > 0.0 {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or_default();
            let start = now - track.position;
            let end = start + track.duration;
            activity = activity.timestamps(Timestamps::new().start(start as i64).end(end as i64));
        }

        if let Some(url) = &track.url {
            activity = activity.buttons(vec![Button::new("Auf YouTube Music anhören", url)]);
        }

        let Some(client) = self.client() else { return };
        if client.set_activity(activity).is_err() {
            // Verbindung verloren (z. B. Discord neu gestartet) – beim nächsten Mal neu verbinden.
            self.client = None;
        }
    }

    pub fn clear(&mut self) {
        if let Some(client) = self.client.as_mut() {
            if client.clear_activity().is_err() {
                self.client = None;
            }
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(mut client) = self.client.take() {
            let _ = client.clear_activity();
            let _ = client.close();
        }
    }
}

/// Discord verlangt 2–128 Zeichen pro Textfeld.
fn discord_text(s: &str) -> String {
    let mut out: String = s.chars().take(128).collect();
    while out.chars().count() < 2 {
        out.push(' ');
    }
    out
}
