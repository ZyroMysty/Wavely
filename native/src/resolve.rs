//! Zuordnung zu YouTube Music: Spotify- und SoundCloud-Songs (und lokale Songs anderer Geräte)
//! werden über den passenden YouTube-Song abgespielt bzw. heruntergeladen.
//! Gefundene Zuordnungen landen in `spotify/matches.json` (wie bisher).

use std::{collections::HashMap, sync::Arc};

use crate::{player::Track, App};

/// Titel vergleichbar machen: Klammerzusätze und „- Remastered“ weg, nur Buchstaben/Ziffern
pub fn normalize(title: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in title.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let out = out.split(" - ").next().unwrap_or(&out);
    out.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()
}

#[derive(Default)]
pub struct Matches {
    file: std::path::PathBuf,
    map: std::sync::Mutex<HashMap<String, String>>,
}

impl Matches {
    pub fn new(paths: &crate::host::Paths) -> Self {
        let dir = paths.data.join("spotify");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("matches.json");
        let map = std::fs::read_to_string(&file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Self { file, map: std::sync::Mutex::new(map) }
    }

    fn get(&self, key: &str) -> Option<String> {
        self.map.lock().unwrap().get(key).cloned()
    }

    fn insert(&self, key: String, video: String) {
        let mut map = self.map.lock().unwrap();
        map.insert(key, video);
        if let Ok(json) = serde_json::to_string(&*map) {
            let _ = std::fs::write(&self.file, json);
        }
    }
}

impl App {
    /// YouTube-Video zum Song (YouTube-Songs direkt)
    pub async fn resolve_video(self: &Arc<Self>, track: &Track) -> Result<String, String> {
        if track.source == "youtube" {
            return Ok(track.id.clone());
        }
        // Gleiches Schlüssel-Format wie bisher (lokale Songs mit Präfix)
        let key = if track.source == "local" { format!("local:{}", track.id) } else { track.id.clone() };
        if let Some(video) = self.matches.get(&key) {
            return Ok(video);
        }
        let artist = track.artist.as_deref().unwrap_or("").split(',').next().unwrap_or("").trim().to_string();
        let query = format!("{artist} {}", track.title);
        let result = self.ytm.rp().query().music_search_tracks(&query).await.map_err(|e| e.to_string())?;
        let wanted_title = normalize(&track.title);
        let wanted_artist = artist.to_lowercase();
        // Bewertung: gleiche Länge (wichtigstes Merkmal), gleicher Titel, gleicher Künstler
        let best = result
            .items
            .items
            .iter()
            .take(10)
            .map(|t| {
                let diff = t.duration.map(|d| (d as f64 - track.duration).abs()).unwrap_or(30.0);
                let mut score = -diff.min(60.0);
                let name = normalize(&t.name);
                if name == wanted_title {
                    score += 20.0;
                } else if name.contains(&wanted_title) || wanted_title.contains(&name) {
                    score += 8.0;
                }
                if t.artists.iter().any(|a| a.name.to_lowercase() == wanted_artist) {
                    score += 15.0;
                }
                (score, t)
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, t)| t.id.clone())
            .ok_or("Kein passender Song bei YouTube Music gefunden")?;
        self.matches.insert(key, best.clone());
        Ok(best)
    }
}
