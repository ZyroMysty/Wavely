//! Seitenfelder rechts: Verlauf und Liedtext (wie HistoryPanel.svelte, LyricsPanel.svelte).

use std::{rc::Rc, sync::Arc};

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{
    covers,
    menu::MenuContext,
    pages::track_row,
    player::Track,
    services::lyrics::{self, LyricsQuery},
    App, PageRow, Side,
};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const WEEKDAYS: [&str; 7] = ["Donnerstag", "Freitag", "Samstag", "Sonntag", "Montag", "Dienstag", "Mittwoch"];
const MONTHS: [&str; 12] =
    ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];

/// Tag (seit 1970, Ortszeit) eines Zeitpunkts
fn local_day(ms: u64) -> i64 {
    (ms as i64 + crate::host::local_offset_ms()).div_euclid(DAY_MS)
}

/// „Heute“, „Gestern“ oder z. B. „Montag, 3. November“
fn day_label(day: i64, today: i64) -> String {
    match today - day {
        0 => "Heute".into(),
        1 => "Gestern".into(),
        _ => {
            let (_, month, d) = crate::host::civil_from_days(day);
            format!("{}, {}. {}", WEEKDAYS[day.rem_euclid(7) as usize], d, MONTHS[(month - 1) as usize])
        }
    }
}

fn clock(ms: u64) -> String {
    let local = (ms as i64 + crate::host::local_offset_ms()).rem_euclid(DAY_MS) / 1000;
    format!("{:02}:{:02}", local / 3600, (local % 3600) / 60)
}

impl App {
    /// Seitenfeld öffnen/schließen („queue“, „history“, „lyrics“)
    pub fn toggle_panel(self: &Arc<Self>, panel: &str) {
        let current = self.panel.lock().unwrap().clone();
        let next = if current == panel { String::new() } else { panel.to_string() };
        *self.panel.lock().unwrap() = next.clone();
        let n = next.clone();
        self.ui(move |ui| ui.global::<crate::Nav>().set_panel(n.into()));
        match next.as_str() {
            "history" => self.show_history(),
            "lyrics" => self.show_lyrics(),
            _ => {}
        }
    }

    /// Verlauf nach Tagen gruppiert
    pub fn show_history(self: &Arc<Self>) {
        let history = self.history.lock().unwrap().clone();
        let today = local_day(crate::store::now_ms());
        let mut tracks: Vec<Track> = Vec::new();
        let mut rows: Vec<(String, Option<usize>, String)> = Vec::new();
        let mut last_day = None;
        for e in &history {
            let day = local_day(e.played_at);
            if last_day != Some(day) {
                rows.push((day_label(day, today), None, String::new()));
                last_day = Some(day);
            }
            rows.push((String::new(), Some(tracks.len()), clock(e.played_at)));
            tracks.push(e.track.clone());
        }
        *self.panel_tracks.lock().unwrap() = tracks.clone();
        let index: Vec<Option<usize>> = rows.iter().map(|r| r.1).collect();
        *self.panel_rows.lock().unwrap() = index;
        let app = self.clone();
        self.ui(move |ui| {
            let page_rows: Vec<PageRow> = rows
                .iter()
                .map(|(label, track, time)| match track {
                    None => PageRow { kind: "heading".into(), text: label.into(), ..Default::default() },
                    Some(t) => {
                        let mut row = track_row(&tracks[*t]);
                        // Uhrzeit statt Länge
                        row.duration = time.into();
                        PageRow { kind: "track".into(), track: row, number: *t as i32, ..Default::default() }
                    }
                })
                .collect();
            let model = Rc::new(VecModel::from(page_rows));
            let side = ui.global::<Side>();
            side.set_title("Verlauf".into());
            side.set_message(if tracks.is_empty() { "Noch nichts gehört.".into() } else { Default::default() });
            side.set_rows(ModelRc::from(model.clone()));
            let items = rows
                .iter()
                .enumerate()
                .filter_map(|(i, (_, t, _))| t.and_then(|t| covers::cover_url(&tracks[t]).map(|u| (i, u))))
                .collect();
            covers::fill_with(&app, &model, items, covers::LIST_SIZE, |r| r.track.id.clone(), |r, image| {
                r.track.cover = image;
                r.track.has_cover = true;
            });
        });
    }

    pub fn panel_play(self: &Arc<Self>, row: usize) {
        let index = self.panel_rows.lock().unwrap().get(row).copied().flatten();
        let tracks = self.panel_tracks.lock().unwrap().clone();
        if let Some(i) = index {
            // Ab diesem Song weiter in Verlaufs-Reihenfolge
            self.play_list(tracks, i);
        }
    }

    pub fn panel_menu(self: &Arc<Self>, row: usize, x: f32, y: f32) {
        let index = self.panel_rows.lock().unwrap().get(row).copied().flatten();
        let track = index.and_then(|i| self.panel_tracks.lock().unwrap().get(i).cloned());
        if let Some(track) = track {
            self.open_track_menu(MenuContext { track, queue_index: None, playlist: None }, x, y);
        }
    }

    // --- Liedtext ---------------------------------------------------------

    /// Liedtext zum laufenden Song laden (einmal pro Song)
    pub fn show_lyrics(self: &Arc<Self>) {
        let Some(track) = self.state.lock().unwrap().current().cloned() else {
            self.set_lyrics_message("Gerade läuft nichts.");
            return;
        };
        let key = format!("{}:{}", track.source, track.id);
        if self.lyrics.lock().unwrap().0 == key {
            self.publish_lyrics();
            return;
        }
        *self.lyrics.lock().unwrap() = (key.clone(), None, Vec::new(), true);
        self.set_lyrics_message("Liedtext wird gesucht…");
        let app = self.clone();
        self.rt.spawn(async move {
            let query = LyricsQuery {
                title: track.title.clone(),
                artist: track.artist.clone(),
                album: track.album.clone(),
                duration: track.duration,
                video_id: (track.source == "youtube").then(|| track.id.clone()),
            };
            let result = lyrics::get_lyrics(&app.ytm, query).await.ok().flatten();
            {
                let mut l = app.lyrics.lock().unwrap();
                if l.0 != key {
                    return;
                }
                let lines = result.as_ref().and_then(|r| r.synced.as_deref()).map(lyrics::parse_lrc).unwrap_or_default();
                *l = (key, result, lines, false);
            }
            app.publish_lyrics();
        });
    }

    fn set_lyrics_message(self: &Arc<Self>, text: &str) {
        let text = text.to_string();
        self.ui(move |ui| {
            let side = ui.global::<Side>();
            side.set_title("Liedtext".into());
            side.set_lyric_lines(ModelRc::default());
            side.set_lyric_source(Default::default());
            side.set_message(text.into());
        });
    }

    fn publish_lyrics(self: &Arc<Self>) {
        let (result, lines, loading) = {
            let l = self.lyrics.lock().unwrap();
            (l.1.clone(), l.2.clone(), l.3)
        };
        if loading {
            return self.set_lyrics_message("Liedtext wird gesucht…");
        }
        let Some(result) = result else {
            return self.set_lyrics_message("Für diesen Song wurde kein Liedtext gefunden.");
        };
        let synced = !lines.is_empty();
        let texts: Vec<slint::SharedString> = if synced {
            lines.iter().map(|(_, t)| if t.is_empty() { "♪".into() } else { t.into() }).collect()
        } else {
            result.plain.clone().unwrap_or_default().lines().map(Into::into).collect()
        };
        let source = format!("Quelle: {}", result.source);
        self.ui(move |ui| {
            let side = ui.global::<Side>();
            side.set_title("Liedtext".into());
            side.set_message(Default::default());
            side.set_lyric_synced(synced);
            side.set_lyric_current(-1);
            side.set_lyric_source(source.into());
            side.set_lyric_lines(ModelRc::from(Rc::new(VecModel::from(texts))));
        });
    }

    /// Aktuelle Zeile beim Abspielen hervorheben
    pub fn lyrics_progress(self: &Arc<Self>, position: f64) {
        if self.panel.lock().unwrap().as_str() != "lyrics" {
            return;
        }
        let current = {
            let l = self.lyrics.lock().unwrap();
            if l.2.is_empty() {
                return;
            }
            l.2.iter().rposition(|(t, _)| *t <= position + 0.3).map(|i| i as i32).unwrap_or(-1)
        };
        self.ui(move |ui| ui.global::<Side>().set_lyric_current(current));
    }

    /// Klick auf eine Liedtextzeile: dorthin springen
    pub fn lyrics_seek(self: &Arc<Self>, line: usize) {
        let time = self.lyrics.lock().unwrap().2.get(line).map(|(t, _)| *t);
        if let Some(t) = time {
            self.seek(t);
        }
    }
}
