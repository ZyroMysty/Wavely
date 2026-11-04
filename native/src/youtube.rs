//! YouTube-Music-Ansicht: Bibliothek, Suche, Playlists/Alben, Künstler (wie YouTubeView.svelte).

use std::sync::Arc;

use crate::{
    pages::{CardData, PageData, Target},
    player::Track,
    services::ytm::{self, YtCollection},
    App,
};

/// Seiten innerhalb von YouTube Music
#[derive(Clone, Debug)]
pub enum YtPage {
    Library,
    Search(String),
    Collection { kind: String, id: String, title: String, art: Option<String> },
    Artist { id: String, name: String },
}

fn card(c: YtCollection) -> CardData {
    CardData {
        title: c.title.clone(),
        subtitle: c.subtitle.clone().unwrap_or_else(|| if c.kind == "album" { "Album".into() } else { "Playlist".into() }),
        art: c.artwork_url.clone(),
        round: false,
        target: Target::Collection { kind: c.kind, id: c.id, title: c.title, art: c.artwork_url },
    }
}

fn artist_card(id: String, name: String, art: Option<String>) -> CardData {
    CardData { title: name.clone(), subtitle: "Künstler".into(), art, round: true, target: Target::Artist { id, name } }
}

/// Platzhalter, solange eine Seite lädt
pub fn loading(page: &YtPage) -> PageData {
    let mut p = PageData::default();
    match page {
        YtPage::Library => {
            p.title("Deine Bibliothek");
            p.message("Bibliothek wird geladen…");
        }
        YtPage::Search(_) => p.message("Suche läuft…"),
        YtPage::Collection { kind, title, art, .. } => {
            p.hero(if kind == "album" { "Album" } else { "Playlist" }, title, "Wird geladen…", art.clone(), false);
        }
        YtPage::Artist { name, .. } => {
            p.hero("Künstler", name, "Wird geladen…", None, true);
        }
    }
    p
}

pub async fn build(app: &Arc<App>, page: &YtPage, logged_in: bool) -> PageData {
    let ytm = &app.ytm;
    let mut p = PageData::default();
    match page {
        YtPage::Library => {
            if !logged_in {
                p.title("YouTube Music");
                p.message("Melde dich an, um deine Playlists, Alben und Lieblingssongs zu sehen. Suchen geht auch ohne Anmeldung.");
                return p;
            }
            p.title("Deine Bibliothek");
            match ytm::ytm_library(ytm).await {
                Ok(library) => {
                    p.heading("Playlists");
                    // Lieblingssongs zuerst
                    let mut playlists: Vec<CardData> = Vec::new();
                    if !library.playlists.iter().any(|c| c.id == "LM") {
                        playlists.push(CardData {
                            title: "Lieblingssongs".into(),
                            subtitle: "Automatische Playlist".into(),
                            art: None,
                            round: false,
                            target: Target::Collection { kind: "playlist".into(), id: "LM".into(), title: "Lieblingssongs".into(), art: None },
                        });
                    }
                    playlists.extend(library.playlists.into_iter().map(card));
                    p.cards(playlists);
                    if !library.albums.is_empty() {
                        p.heading("Alben");
                        p.cards(library.albums.into_iter().map(card).collect());
                    }
                }
                Err(e) => p.error(format!("Bibliothek konnte nicht geladen werden: {e}")),
            }
        }
        YtPage::Search(query) => match ytm::ytm_search(ytm, query.clone()).await {
            Ok(results) => {
                let empty = results.tracks.is_empty() && results.albums.is_empty() && results.playlists.is_empty();
                if !results.tracks.is_empty() {
                    p.heading("Songs");
                    p.tracks(results.tracks.into_iter().map(Track::from));
                }
                if !results.albums.is_empty() {
                    p.heading("Alben");
                    p.cards(results.albums.into_iter().map(card).collect());
                }
                if !results.playlists.is_empty() {
                    p.heading("Playlists");
                    p.cards(results.playlists.into_iter().map(card).collect());
                }
                if empty {
                    p.message("Keine Treffer.");
                }
            }
            Err(e) => p.error(format!("Suche fehlgeschlagen: {e}")),
        },
        YtPage::Collection { kind, id, title, art } => {
            let label = if kind == "album" { "Album" } else { "Playlist" };
            match ytm::ytm_collection(ytm, kind.clone(), id.clone()).await {
                Ok(detail) => {
                    let count = format!("{} Titel", detail.tracks.len());
                    let subtitle = match &detail.info.subtitle {
                        Some(s) => format!("{s} · {count}"),
                        None => count,
                    };
                    let title = if title.is_empty() { detail.info.title.clone() } else { title.clone() };
                    p.hero(label, &title, &subtitle, art.clone().or(detail.info.artwork_url.clone()), false);
                    let saved = app.offline.is_saved_collection(id);
                    p.buttons(&[
                        ("play", "Abspielen", crate::icon::PLAY),
                        ("shuffle", "Zufall", crate::icon::SHUFFLE),
                        ("offline-save", if saved { "Offline gespeichert" } else { "Offline speichern" }, if saved { crate::icon::DOWNLOADED } else { crate::icon::DOWNLOAD }),
                    ]);
                    p.tracks(detail.tracks.into_iter().map(Track::from));
                }
                Err(e) => {
                    p.hero(label, title, "", art.clone(), false);
                    p.error(format!("Konnte nicht geladen werden: {e}"));
                }
            }
        }
        YtPage::Artist { id, name } => match ytm::ytm_artist(ytm, id.clone()).await {
            Ok(artist) => {
                let subs = artist.subscribers.map(format_subscribers).unwrap_or_default();
                p.hero("Künstler", &artist.name, &subs, artist.image_url.clone(), true);
                let mut buttons = vec![("play", "Abspielen", crate::icon::PLAY), ("shuffle", "Zufall", crate::icon::SHUFFLE)];
                if artist.all_songs_playlist_id.is_some() {
                    buttons.push(("all-songs", "Alle Songs", crate::icon::QUEUE));
                }
                p.buttons(&buttons);
                if !artist.tracks.is_empty() {
                    p.heading("Beliebte Songs");
                    p.tracks(artist.tracks.into_iter().map(Track::from));
                }
                for (heading, list) in [("Alben", artist.albums), ("Singles & EPs", artist.singles), ("Playlists", artist.playlists)] {
                    if !list.is_empty() {
                        p.heading(heading);
                        p.cards(list.into_iter().map(card).collect());
                    }
                }
                if !artist.similar.is_empty() {
                    p.heading("Ähnliche Künstler");
                    p.cards(artist.similar.into_iter().map(|a| artist_card(a.id, a.name, a.image_url)).collect());
                }
                if let Some(description) = artist.description.filter(|d| !d.is_empty()) {
                    p.heading(format!("Über {}", artist.name));
                    p.text(description);
                }
                p.spacer();
            }
            Err(e) => {
                p.hero("Künstler", name, "", None, true);
                p.error(format!("Konnte nicht geladen werden: {e}"));
            }
        },
    }
    p.spacer();
    p
}

/// Ziel der Taste „Alle Songs“ auf einer Künstlerseite
pub async fn all_songs_target(app: &Arc<App>, artist_id: &str) -> Option<YtPage> {
    let artist = ytm::ytm_artist(&app.ytm, artist_id.to_string()).await.ok()?;
    Some(YtPage::Collection {
        kind: "playlist".into(),
        id: artist.all_songs_playlist_id?,
        title: format!("{} – alle Songs", artist.name),
        art: artist.image_url,
    })
}

fn format_subscribers(n: u64) -> String {
    let text = if n >= 1_000_000 {
        format!("{} Mio.", format!("{:.1}", n as f64 / 1_000_000.0).replace('.', ",").trim_end_matches(",0"))
    } else if n >= 1_000 {
        format!("{} Tsd.", format!("{:.1}", n as f64 / 1_000.0).replace('.', ",").trim_end_matches(",0"))
    } else {
        n.to_string()
    };
    format!("{text} Abonnenten")
}
