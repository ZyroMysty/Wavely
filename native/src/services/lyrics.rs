//! Liedtexte: zuerst LRCLIB (frei, oft mit Zeitstempeln), sonst YouTube Music (nur Text).

use serde::{Deserialize, Serialize};
use crate::services::ytm::Ytm;

const LRCLIB: &str = "https://lrclib.net/api";
/// LRCLIB bittet darum, die App im User-Agent zu nennen
const USER_AGENT: &str = concat!("Wavely/", env!("CARGO_PKG_VERSION"), " (Desktop-Musikplayer)");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsQuery {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: f64,
    /// YouTube-Video-ID für den Ersatz über YouTube Music
    pub video_id: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    /// LRC-Text mit Zeitstempeln (`[mm:ss.xx] Zeile`)
    pub synced: Option<String>,
    pub plain: Option<String>,
    pub source: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrcLibEntry {
    duration: Option<f64>,
    #[serde(default)]
    instrumental: bool,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

impl LrcLibEntry {
    fn into_lyrics(self) -> Option<Lyrics> {
        if self.instrumental {
            return Some(Lyrics {
                synced: None,
                plain: Some("♪ Instrumental ♪".into()),
                source: "LRCLIB".into(),
            });
        }
        let synced = self.synced_lyrics.filter(|s| !s.trim().is_empty());
        let plain = self.plain_lyrics.filter(|s| !s.trim().is_empty());
        (synced.is_some() || plain.is_some()).then(|| Lyrics {
            synced,
            plain,
            source: "LRCLIB".into(),
        })
    }
}

/// Entfernt Zusätze wie „(feat. …)“ oder „[Official Video]“, die die Suche stören.
fn clean_title(title: &str) -> String {
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
    let out = out.split(" - ").next().unwrap_or(&out).trim().to_string();
    if out.is_empty() { title.to_string() } else { out }
}

fn first_artist(artist: &str) -> &str {
    artist.split([',', '&']).next().unwrap_or(artist).trim()
}

async fn from_lrclib(http: &reqwest::Client, q: &LyricsQuery) -> Option<Lyrics> {
    let artist = q.artist.as_deref().map(first_artist).unwrap_or("");
    let title = clean_title(&q.title);

    // 1. Exakter Treffer (Titel, Künstler, Album, Länge)
    let mut params = vec![
        ("track_name", title.clone()),
        ("artist_name", artist.to_string()),
        ("duration", (q.duration.round() as u64).to_string()),
    ];
    if let Some(album) = &q.album {
        params.push(("album_name", album.clone()));
    }
    if let Ok(response) = http.get(format!("{LRCLIB}/get")).query(&params).send().await {
        if response.status().is_success() {
            if let Some(lyrics) = response.json::<LrcLibEntry>().await.ok().and_then(LrcLibEntry::into_lyrics) {
                return Some(lyrics);
            }
        }
    }

    // 2. Suche; Treffer mit passender Länge bevorzugen, synchronisierte zuerst
    let entries: Vec<LrcLibEntry> = http
        .get(format!("{LRCLIB}/search"))
        .query(&[("track_name", title.as_str()), ("artist_name", artist)])
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let fits = |e: &LrcLibEntry| q.duration <= 0.0 || e.duration.is_none_or(|d| (d - q.duration).abs() < 8.0);
    let mut candidates: Vec<LrcLibEntry> = entries.into_iter().filter(fits).collect();
    candidates.sort_by_key(|e| e.synced_lyrics.is_none());
    candidates.into_iter().find_map(LrcLibEntry::into_lyrics)
}

async fn from_youtube(ytm: &Ytm, video_id: &str) -> Option<Lyrics> {
    let query = ytm.rp().query();
    let details = query.music_details(video_id).await.ok()?;
    let lyrics = query.music_lyrics(details.lyrics_id?).await.ok()?;
    let source = lyrics.footer.trim().trim_start_matches("Source:").trim().to_string();
    Some(Lyrics {
        synced: None,
        plain: Some(lyrics.body),
        source: if source.is_empty() { "YouTube Music".into() } else { source },
    })
}

pub async fn get_lyrics(ytm: &Ytm, query: LyricsQuery) -> Result<Option<Lyrics>, String> {
    let http = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    if let Some(lyrics) = from_lrclib(&http, &query).await {
        return Ok(Some(lyrics));
    }
    match &query.video_id {
        Some(id) => Ok(from_youtube(ytm, id).await),
        None => Ok(None),
    }
}

/// LRC-Text („[01:23.45] Zeile“) → (Zeit in Sekunden, Text), nach Zeit sortiert
pub fn parse_lrc(lrc: &str) -> Vec<(f64, String)> {
    let mut lines = Vec::new();
    for raw in lrc.lines() {
        let mut rest = raw;
        let mut stamps = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            let stamp = &stripped[..end];
            if let Some((min, sec)) = stamp.split_once(':') {
                if let (Ok(m), Ok(s)) = (min.parse::<f64>(), sec.parse::<f64>()) {
                    stamps.push(m * 60.0 + s);
                }
            }
            rest = &stripped[end + 1..];
        }
        let text = rest.trim().to_string();
        for t in stamps {
            lines.push((t, text.clone()));
        }
    }
    lines.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    lines
}
