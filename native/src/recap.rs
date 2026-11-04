//! Rückblick aus der Hörstatistik (wie RecapView.svelte, stats.svelte.js summarize).

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use serde_json::Value;

use crate::{
    pages::{CardData, PageData, RowData, Target},
    player::Track,
    App,
};

pub const RANGES: &[(&str, &str)] = &[("week", "7 Tage"), ("month", "30 Tage"), ("year", "Dieses Jahr"), ("all", "Insgesamt")];
const WEEKDAYS: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
const WEEKDAYS_LONG: [&str; 7] = ["montags", "dienstags", "mittwochs", "donnerstags", "freitags", "samstags", "sonntags"];

fn since(range: &str) -> u64 {
    let now = crate::store::now_ms();
    match range {
        "week" => now.saturating_sub(7 * 86_400_000),
        "month" => now.saturating_sub(30 * 86_400_000),
        "year" => {
            let offset = crate::host::local_offset_ms();
            let today = (now as i64 + offset).div_euclid(86_400_000);
            let (year, _, _) = crate::host::civil_from_days(today);
            (crate::host::days_from_civil(year, 1, 1) * 86_400_000 - offset).max(0) as u64
        }
        _ => 0,
    }
}

fn fmt(n: u64) -> String {
    // Tausenderpunkte wie toLocaleString("de-DE")
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

fn daytime(h: usize) -> &'static str {
    match h {
        0..=4 => "nachts",
        5..=10 => "morgens",
        11..=13 => "mittags",
        14..=17 => "nachmittags",
        18..=21 => "abends",
        _ => "spätabends",
    }
}

fn as_track(p: &Value) -> Track {
    let src = p["src"].as_str().unwrap_or("youtube").to_string();
    let id = p["id"].as_str().map(str::to_string).unwrap_or_else(|| p["id"].to_string());
    Track {
        path: (src == "local").then(|| id.clone()),
        source: src,
        id,
        title: p["title"].as_str().unwrap_or("").into(),
        artist: p["artist"].as_str().map(str::to_string),
        album: p["album"].as_str().map(str::to_string),
        artwork_url: p["art"].as_str().map(str::to_string),
        duration: p["d"].as_f64().unwrap_or(0.0),
        ..Default::default()
    }
}

pub fn page(app: &Arc<App>, range: &str) -> PageData {
    let plays = app.store.load_plays(since(range));
    let mut p = PageData::default();
    p.title("Dein Rückblick");
    let buttons: Vec<(String, &str, &str)> =
        RANGES.iter().map(|(id, label)| (format!("recap={id}"), *label, if *id == range { crate::icon::CHECK } else { "" })).collect();
    let refs: Vec<(&str, &str, &str)> = buttons.iter().map(|(a, b, c)| (a.as_str(), *b, *c)).collect();
    p.buttons(&refs);
    // Gewählten Zeitraum hervorheben
    if let Some(row) = p.rows.last_mut() {
        for (card, (id, _)) in row.cards.iter_mut().zip(RANGES) {
            card.round = *id == range;
            card.subtitle = String::new();
        }
    }
    if plays.is_empty() {
        p.message("Für diesen Zeitraum gibt es noch keine Daten. Wavely zählt jeden Song, den du mindestens 10 Sekunden hörst.");
        return p;
    }

    let offset = crate::host::local_offset_ms();
    let mut seconds = 0u64;
    let mut songs: HashMap<String, (Value, u64, u64)> = HashMap::new();
    let mut artists: HashMap<String, (String, u64, Option<String>, HashSet<String>)> = HashMap::new();
    let mut sources: HashMap<String, u64> = HashMap::new();
    let mut hours = [0u64; 24];
    let mut weekdays = [0u64; 7];
    let mut days: HashSet<i64> = HashSet::new();
    for play in &plays {
        let s = play["s"].as_u64().unwrap_or(0);
        seconds += s;
        let key = format!("{}:{}", play["src"].as_str().unwrap_or(""), play["id"].as_str().unwrap_or(""));
        let song = songs.entry(key.clone()).or_insert((play.clone(), 0, 0));
        song.1 += 1;
        song.2 += s;
        let name = play["artist"].as_str().unwrap_or("").split(',').next().unwrap_or("").trim().to_string();
        if !name.is_empty() {
            let a = artists.entry(name.to_lowercase()).or_insert((name, 0, play["art"].as_str().map(str::to_string), HashSet::new()));
            a.1 += s;
            a.3.insert(key);
        }
        *sources.entry(play["src"].as_str().unwrap_or("youtube").to_string()).or_default() += s;
        let local = play["t"].as_u64().unwrap_or(0) as i64 + offset;
        let day = local.div_euclid(86_400_000);
        hours[(local.rem_euclid(86_400_000) / 3_600_000) as usize] += s;
        // 1970-01-01 war ein Donnerstag; Montag zuerst
        weekdays[((day + 3).rem_euclid(7)) as usize] += s;
        days.insert(day);
    }
    let mut day_list: Vec<i64> = days.iter().copied().collect();
    day_list.sort();
    let (mut streak, mut run) = (0, 0);
    for (i, d) in day_list.iter().enumerate() {
        run = if i > 0 && *d == day_list[i - 1] + 1 { run + 1 } else { 1 };
        streak = streak.max(run);
    }

    // Kennzahlen
    p.rows.push(RowData {
        kind: "stats",
        cards: [
            (fmt(seconds / 60), "Minuten Musik"),
            (fmt(songs.len() as u64), "verschiedene Songs"),
            (fmt(artists.len() as u64), "Künstler"),
            (fmt(days.len() as u64), "Tage mit Musik"),
            (fmt(streak), "Tage am Stück (Rekord)"),
        ]
        .into_iter()
        .map(|(value, label)| CardData { title: value, subtitle: label.into(), art: None, round: false, target: Target::Action(String::new()) })
        .collect(),
        ..Default::default()
    });

    // Top-Songs (Anzahl statt Länge)
    let mut top: Vec<(Value, u64, u64)> = songs.into_values().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)));
    p.heading("Top-Songs");
    let start = p.tracks.len();
    p.tracks(top.iter().take(10).map(|(v, _, _)| as_track(v)));
    for (i, row) in p.rows.iter_mut().filter(|r| r.kind == "track" && r.track.is_some_and(|t| t >= start)).enumerate() {
        row.subtext = format!("{}×", top[i].1);
    }

    // Top-Künstler
    let mut top_artists: Vec<_> = artists.into_values().collect();
    top_artists.sort_by(|a, b| b.1.cmp(&a.1));
    p.heading("Top-Künstler");
    p.cards(
        top_artists
            .iter()
            .take(10)
            .map(|(name, secs, art, songs)| CardData {
                title: name.clone(),
                subtitle: format!("{} Min. · {} {}", fmt(secs / 60), songs.len(), if songs.len() == 1 { "Song" } else { "Songs" }),
                art: art.clone(),
                round: true,
                target: Target::Action(String::new()),
            })
            .collect(),
    );

    // Wann
    let peak_hour = (0..24).max_by_key(|h| hours[*h]).unwrap_or(0);
    let peak_day = (0..7).max_by_key(|d| weekdays[*d]).unwrap_or(0);
    p.heading("Wann du hörst");
    p.message(format!("Am meisten {} (um {peak_hour} Uhr) und am liebsten {}.", daytime(peak_hour), WEEKDAYS_LONG[peak_day]));
    let hour_max = *hours.iter().max().unwrap_or(&1) as f32;
    p.rows.push(RowData {
        kind: "bars",
        values: hours.iter().map(|v| *v as f32 / hour_max.max(1.0)).collect(),
        number: peak_hour as i32,
        cards: (0..24).map(|h| CardData { title: if h % 6 == 0 { format!("{h}") } else { String::new() }, subtitle: String::new(), art: None, round: false, target: Target::Action(String::new()) }).collect(),
        ..Default::default()
    });
    let day_max = *weekdays.iter().max().unwrap_or(&1) as f32;
    p.rows.push(RowData {
        kind: "bars",
        values: weekdays.iter().map(|v| *v as f32 / day_max.max(1.0)).collect(),
        number: peak_day as i32,
        cards: WEEKDAYS.iter().map(|d| CardData { title: d.to_string(), subtitle: String::new(), art: None, round: false, target: Target::Action(String::new()) }).collect(),
        ..Default::default()
    });

    // Woher
    let mut shares: Vec<(String, u64)> = sources.into_iter().collect();
    shares.sort_by(|a, b| b.1.cmp(&a.1));
    p.heading("Woher deine Musik kommt");
    for (src, secs) in shares {
        let name = match src.as_str() {
            "youtube" => "YouTube Music",
            "spotify" => "Spotify",
            "soundcloud" => "SoundCloud",
            "local" => "Lokal",
            other => other,
        };
        p.info_row(name, &format!("{} % · {} Min.", (secs * 100) / seconds.max(1), fmt(secs / 60)));
    }
    p.spacer();
    p
}

impl PageData {
    fn info_row(&mut self, title: &str, description: &str) {
        self.rows.push(RowData { kind: "setting", label: "none".into(), text: title.into(), subtext: description.into(), ..Default::default() });
    }
}

impl App {
    pub fn recap_action(self: &Arc<Self>, action: &str) -> bool {
        let Some(range) = action.strip_prefix("recap=") else { return false };
        *self.recap_range.lock().unwrap() = range.to_string();
        self.stats_flush();
        self.render();
        true
    }
}
