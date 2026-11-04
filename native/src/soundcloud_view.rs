//! SoundCloud-Ansicht: Bibliothek, Suche, Playlists/Alben, Künstler (wie SoundCloudView.svelte).

use std::sync::Arc;

use crate::{
    icon,
    nav::PageKind,
    pages::{CardData, PageData, Target},
    player::{ArtistRef, Track},
    services::soundcloud::{self, ScArtistCard, ScCollection, ScTrack},
    App,
};

#[derive(Clone, Debug)]
pub enum ScPage {
    Library,
    Search(String),
    Collection { id: String, title: String, art: Option<String> },
    Artist { id: String, name: String },
}

impl From<ScTrack> for Track {
    fn from(t: ScTrack) -> Self {
        Self {
            source: "soundcloud".into(),
            id: t.id,
            title: t.title,
            artist: t.artist,
            artists: t.artists.into_iter().map(|a| ArtistRef { id: a.id, name: a.name }).collect(),
            album: t.album,
            album_id: t.album_id,
            duration: t.duration,
            artwork_url: t.artwork_url,
            url: t.url,
            snipped: t.snipped,
            ..Default::default()
        }
    }
}

fn card(c: ScCollection) -> CardData {
    CardData {
        title: c.title.clone(),
        subtitle: c.subtitle.clone().unwrap_or_else(|| if c.kind == "album" { "Album".into() } else { "Playlist".into() }),
        art: c.artwork_url.clone(),
        round: false,
        target: Target::Open(Box::new(PageKind::Sc(ScPage::Collection { id: c.id, title: c.title, art: c.artwork_url }))),
    }
}

fn artist_card(a: ScArtistCard) -> CardData {
    CardData {
        title: a.name.clone(),
        subtitle: "Künstler".into(),
        art: a.image_url,
        round: true,
        target: Target::Open(Box::new(PageKind::Sc(ScPage::Artist { id: a.id, name: a.name }))),
    }
}

pub fn loading(page: &ScPage) -> PageData {
    let mut p = PageData::default();
    match page {
        ScPage::Library => {
            p.title("SoundCloud");
            p.message("Wird geladen…");
        }
        ScPage::Search(_) => p.message("Suche läuft…"),
        ScPage::Collection { title, art, .. } => p.hero("Playlist", title, "Wird geladen…", art.clone(), false),
        ScPage::Artist { name, .. } => p.hero("Künstler", name, "Wird geladen…", None, true),
    }
    p
}

pub async fn build(app: &Arc<App>, page: &ScPage) -> PageData {
    let sc = &app.soundcloud;
    let mut p = PageData::default();
    match page {
        ScPage::Library => {
            p.title("SoundCloud");
            if !soundcloud::soundcloud_status(sc).await.unwrap_or(false) {
                p.message("Nicht angemeldet. Suchen und Abspielen geht auch ohne Anmeldung.");
                return p;
            }
            match soundcloud::soundcloud_library(sc).await {
                Ok(library) => {
                    p.heading("Playlists");
                    let mut playlists = vec![CardData {
                        title: "Gelikte Songs".into(),
                        subtitle: "SoundCloud".into(),
                        art: None,
                        round: false,
                        target: Target::Open(Box::new(PageKind::Sc(ScPage::Collection { id: "likes".into(), title: "Gelikte Songs".into(), art: None }))),
                    }];
                    playlists.extend(library.playlists.into_iter().map(card));
                    p.cards(playlists);
                    if !library.albums.is_empty() {
                        p.heading("Alben");
                        p.cards(library.albums.into_iter().map(card).collect());
                    }
                    if !library.following.is_empty() {
                        p.heading("Du folgst");
                        p.cards(library.following.into_iter().map(artist_card).collect());
                    }
                }
                Err(e) => p.error(format!("Bibliothek konnte nicht geladen werden: {e}")),
            }
        }
        ScPage::Search(query) => match soundcloud::soundcloud_search(sc, query.clone()).await {
            Ok(r) => {
                let empty = r.tracks.is_empty() && r.albums.is_empty() && r.playlists.is_empty() && r.artists.is_empty();
                if !r.tracks.is_empty() {
                    p.heading("Songs");
                    p.tracks(r.tracks.into_iter().map(Track::from));
                }
                for (heading, list) in [("Alben", r.albums), ("Playlists", r.playlists)] {
                    if !list.is_empty() {
                        p.heading(heading);
                        p.cards(list.into_iter().map(card).collect());
                    }
                }
                if !r.artists.is_empty() {
                    p.heading("Künstler");
                    p.cards(r.artists.into_iter().map(artist_card).collect());
                }
                if empty {
                    p.message("Keine Treffer.");
                }
            }
            Err(e) => p.error(format!("Suche fehlgeschlagen: {e}")),
        },
        ScPage::Collection { id, title, art } => match soundcloud::soundcloud_collection(sc, id.clone()).await {
            Ok(detail) => {
                let count = format!("{} Titel", detail.tracks.len());
                let subtitle = detail.info.subtitle.clone().map(|s| format!("{s} · {count}")).unwrap_or(count);
                let label = if detail.info.kind == "album" { "Album" } else { "Playlist" };
                let title = if title.is_empty() { detail.info.title.clone() } else { title.clone() };
                p.hero(label, &title, &subtitle, art.clone().or(detail.info.artwork_url.clone()), false);
                p.buttons(&[("play", "Abspielen", icon::PLAY), ("shuffle", "Zufall", icon::SHUFFLE)]);
                p.tracks(detail.tracks.into_iter().map(Track::from));
            }
            Err(e) => {
                p.hero("Playlist", title, "", art.clone(), false);
                p.error(format!("Konnte nicht geladen werden: {e}"));
            }
        },
        ScPage::Artist { id, name } => match soundcloud::soundcloud_artist(sc, id.clone()).await {
            Ok(a) => {
                let subs = a.subscribers.map(|n| format!("{n} Follower")).unwrap_or_default();
                p.hero("Künstler", &a.name, &subs, a.image_url.clone(), true);
                p.buttons(&[("play", "Abspielen", icon::PLAY), ("shuffle", "Zufall", icon::SHUFFLE)]);
                if !a.tracks.is_empty() {
                    p.heading("Beliebte Songs");
                    p.tracks(a.tracks.into_iter().map(Track::from));
                }
                for (heading, list) in [("Alben", a.albums), ("Playlists", a.playlists)] {
                    if !list.is_empty() {
                        p.heading(heading);
                        p.cards(list.into_iter().map(card).collect());
                    }
                }
                if !a.similar.is_empty() {
                    p.heading("Ähnliche Künstler");
                    p.cards(a.similar.into_iter().map(artist_card).collect());
                }
                if let Some(d) = a.description {
                    p.heading(format!("Über {}", a.name));
                    p.text(d);
                }
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

impl App {
    /// SoundCloud direkt abspielen (ganze Datei wird geladen); `None` → über YouTube Music
    pub async fn soundcloud_source(self: &Arc<Self>, track: &Track) -> Option<(Box<dyn symphonia::core::io::MediaSource>, String)> {
        if track.source != "soundcloud" || track.snipped {
            return None;
        }
        let (path, ext) = self.soundcloud.prepared_file(&track.id).await.ok()?;
        let file = std::fs::File::open(path).ok()?;
        Some((Box::new(file), ext.to_string()))
    }
}
