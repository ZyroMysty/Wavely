//! Seiten aus Zeilen (siehe ui/page.slint): Inhalt wird im Hintergrund geladen, als
//! Send-fähige Daten gebaut und dann im UI-Thread in Slint-Modelle umgewandelt.

use std::{rc::Rc, sync::Arc};

use slint::{ModelRc, SharedString, VecModel};

use crate::{covers, player::Track, App, Card, Page, PageRow, TrackRow};

/// Wohin ein Klick auf eine Kachel führt
#[derive(Clone, Debug)]
pub enum Target {
    Collection { kind: String, id: String, title: String, art: Option<String> },
    Artist { id: String, name: String },
    /// Wavely-Playlist
    Playlist(String),
    /// Beliebige Seite öffnen
    Open(Box<crate::nav::PageKind>),
    /// Song der Seite abspielen (Index in `PageData::tracks`)
    Track(usize),
    /// Knopf mit Kennung (siehe `App::page_action`)
    Action(String),
}

#[derive(Clone, Debug)]
pub struct CardData {
    pub title: String,
    pub subtitle: String,
    pub art: Option<String>,
    pub round: bool,
    pub target: Target,
}

#[derive(Clone, Debug, Default)]
pub struct RowData {
    pub kind: &'static str,
    pub text: String,
    pub subtext: String,
    pub label: String,
    /// Index in `PageData::tracks`
    pub track: Option<usize>,
    pub number: i32,
    pub cards: Vec<CardData>,
    pub art: Option<String>,
    pub round: bool,
    /// Zahlen für Diagramme (0–1)
    pub values: Vec<f32>,
}

/// Eine fertige Seite
#[derive(Clone, Debug, Default)]
pub struct PageData {
    pub rows: Vec<RowData>,
    pub tracks: Vec<Track>,
}

impl PageData {
    pub fn heading(&mut self, text: impl Into<String>) {
        self.rows.push(RowData { kind: "heading", text: text.into(), ..Default::default() });
    }

    /// Große Überschrift einer Seite
    pub fn title(&mut self, text: impl Into<String>) {
        self.rows.push(RowData { kind: "heading", text: text.into(), number: 1, ..Default::default() });
    }

    pub fn message(&mut self, text: impl Into<String>) {
        self.rows.push(RowData { kind: "message", text: text.into(), ..Default::default() });
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.rows.push(RowData { kind: "message", text: text.into(), number: 1, ..Default::default() });
    }

    pub fn text(&mut self, text: impl Into<String>) {
        self.rows.push(RowData { kind: "text", text: text.into(), ..Default::default() });
    }

    pub fn hero(&mut self, label: &str, title: &str, subtitle: &str, art: Option<String>, round: bool) {
        self.rows.push(RowData {
            kind: "hero",
            label: label.to_uppercase(),
            text: title.into(),
            subtext: subtitle.into(),
            art,
            round,
            ..Default::default()
        });
    }

    /// Knöpfe: (Kennung, Text, Symbol)
    pub fn buttons(&mut self, buttons: &[(&str, &str, &str)]) {
        let cards = buttons
            .iter()
            .enumerate()
            .map(|(i, (id, text, icon))| CardData {
                title: text.to_string(),
                subtitle: icon.to_string(),
                art: None,
                // Hervorgehobener Knopf (standardmäßig der erste)
                round: i == 0,
                target: Target::Action(id.to_string()),
            })
            .collect();
        self.rows.push(RowData { kind: "buttons", cards, ..Default::default() });
    }

    pub fn tracks(&mut self, tracks: impl IntoIterator<Item = Track>) {
        let start = self.tracks.len();
        for (n, track) in tracks.into_iter().enumerate() {
            self.rows.push(RowData { kind: "track", track: Some(start + n), number: n as i32, ..Default::default() });
            self.tracks.push(track);
        }
    }

    pub fn cards(&mut self, cards: Vec<CardData>) {
        if !cards.is_empty() {
            self.rows.push(RowData { kind: "cards", cards, ..Default::default() });
        }
    }

    pub fn spacer(&mut self) {
        self.rows.push(RowData { kind: "spacer", ..Default::default() });
    }

    /// Songs eines Abschnitts (Zeile `row` und folgende) – zum Abspielen ab dieser Zeile
    pub fn section_tracks(&self, row: usize) -> (Vec<Track>, usize) {
        let Some(index) = self.rows.get(row).and_then(|r| r.track) else { return (Vec::new(), 0) };
        let mut first = row;
        while first > 0 && self.rows[first - 1].kind == "track" {
            first -= 1;
        }
        let mut last = row;
        while last + 1 < self.rows.len() && self.rows[last + 1].kind == "track" {
            last += 1;
        }
        let from = self.rows[first].track.unwrap_or(0);
        let to = self.rows[last].track.unwrap_or(0);
        (self.tracks[from..=to].to_vec(), index - from)
    }
}

pub fn track_row(track: &Track) -> TrackRow {
    TrackRow {
        id: track.id.clone().into(),
        title: track.title.clone().into(),
        artist: track.artist.clone().unwrap_or_default().into(),
        album: track.album.clone().unwrap_or_default().into(),
        duration: if track.duration > 0.0 { crate::format_time(track.duration).into() } else { SharedString::new() },
        cover: Default::default(),
        has_cover: false,
    }
}

/// Zeigt eine Seite an (im UI-Thread)
pub fn show(app: &Arc<App>, ui: &crate::AppWindow, page: &PageData) {
    let mut row_covers = Vec::new();
    let mut card_models = Vec::new();
    let rows: Vec<PageRow> = page
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let track = r.track.map(|t| {
                if let Some(url) = covers::cover_url(&page.tracks[t]) {
                    row_covers.push((i, url, covers::LIST_SIZE));
                }
                let mut row = track_row(&page.tracks[t]);
                // Eigener Text statt der Länge (z. B. „12×“ im Rückblick)
                if !r.subtext.is_empty() {
                    row.duration = r.subtext.clone().into();
                }
                row
            });
            if let Some(url) = &r.art {
                row_covers.push((i, url.clone(), 400));
            }
            let cards: Vec<Card> = r
                .cards
                .iter()
                .map(|c| Card {
                    id: match &c.target {
                        Target::Action(id) => id.clone().into(),
                        Target::Collection { id, .. } | Target::Artist { id, .. } | Target::Playlist(id) => id.clone().into(),
                        Target::Track(i) => format!("track:{i}").into(),
                        Target::Open(_) => c.title.clone().into(),
                    },
                    title: c.title.clone().into(),
                    subtitle: c.subtitle.clone().into(),
                    cover: Default::default(),
                    has_cover: false,
                    round: c.round,
                })
                .collect();
            let card_model = Rc::new(VecModel::from(cards));
            if r.kind == "cards" {
                let urls: Vec<(usize, String)> =
                    r.cards.iter().enumerate().filter_map(|(ci, c)| c.art.clone().map(|u| (ci, u))).collect();
                card_models.push((card_model.clone(), urls));
            }
            PageRow {
                kind: r.kind.into(),
                text: r.text.clone().into(),
                subtext: r.subtext.clone().into(),
                label: r.label.clone().into(),
                track: track.unwrap_or_default(),
                number: r.number,
                cards: ModelRc::from(card_model),
                cover: Default::default(),
                has_cover: false,
                round: r.round,
                values: ModelRc::from(Rc::new(VecModel::from(r.values.clone()))),
            }
        })
        .collect();
    let model = Rc::new(VecModel::from(rows));
    ui.global::<Page>().set_rows(ModelRc::from(model.clone()));

    // Cover: Songzeilen klein, Kopfbild groß
    let (small, big): (Vec<_>, Vec<_>) = row_covers.into_iter().partition(|(_, _, size)| *size == covers::LIST_SIZE);
    covers::fill_with(app, &model, small.into_iter().map(|(i, u, _)| (i, u)).collect(), covers::LIST_SIZE, |r| r.track.id.clone(), |r, image| {
        r.track.cover = image;
        r.track.has_cover = true;
    });
    covers::fill_with(app, &model, big.into_iter().map(|(i, u, _)| (i, u)).collect(), 400, |r| r.text.clone(), |r, image| {
        r.cover = image;
        r.has_cover = true;
    });
    for (cards, urls) in card_models {
        covers::fill_with(app, &cards, urls, 340, |c| c.id.clone(), |c, image| {
            c.cover = image;
            c.has_cover = true;
        });
    }
}

use slint::ComponentHandle;
