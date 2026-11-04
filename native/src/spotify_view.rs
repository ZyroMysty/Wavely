//! Spotify-Ansicht: Bibliothek, Suche, Playlists/Alben, Künstler (wie SpotifyView.svelte).
//! Abgespielt wird über den passenden YouTube-Music-Song (siehe resolve.rs).

use std::sync::Arc;

use crate::{
    icon,
    nav::PageKind,
    pages::{CardData, PageData, Target},
    player::{ArtistRef, Track},
    services::spotify::{self, SpArtistCard, SpCollection, SpTrack},
    App,
};

#[derive(Clone, Debug)]
pub enum SpPage {
    Library,
    Search(String),
    Collection { kind: String, id: String, title: String, art: Option<String> },
    Artist { id: String, name: String },
}

impl From<SpTrack> for Track {
    fn from(t: SpTrack) -> Self {
        Self {
            source: "spotify".into(),
            id: t.id,
            title: t.title,
            artist: t.artist,
            artists: t.artists.into_iter().map(|a| ArtistRef { id: a.id, name: a.name }).collect(),
            album: t.album,
            album_id: t.album_id,
            duration: t.duration,
            artwork_url: t.artwork_url,
            url: t.url,
            ..Default::default()
        }
    }
}

fn card(c: SpCollection) -> CardData {
    CardData {
        title: c.title.clone(),
        subtitle: c.subtitle.clone().unwrap_or_else(|| if c.kind == "album" { "Album".into() } else { "Playlist".into() }),
        art: c.artwork_url.clone(),
        round: false,
        target: Target::Open(Box::new(PageKind::Sp(SpPage::Collection { kind: c.kind, id: c.id, title: c.title, art: c.artwork_url }))),
    }
}

fn artist_card(a: SpArtistCard) -> CardData {
    CardData {
        title: a.name.clone(),
        subtitle: "Künstler".into(),
        art: a.image_url,
        round: true,
        target: Target::Open(Box::new(PageKind::Sp(SpPage::Artist { id: a.id, name: a.name }))),
    }
}

pub fn loading(page: &SpPage) -> PageData {
    let mut p = PageData::default();
    match page {
        SpPage::Library => {
            p.title("Spotify");
            p.message("Wird geladen…");
        }
        SpPage::Search(_) => p.message("Suche läuft…"),
        SpPage::Collection { kind, title, art, .. } => {
            p.hero(if kind == "album" { "Album" } else { "Playlist" }, title, "Wird geladen…", art.clone(), false)
        }
        SpPage::Artist { name, .. } => p.hero("Künstler", name, "Wird geladen…", None, true),
    }
    p
}

pub async fn build(app: &Arc<App>, page: &SpPage) -> PageData {
    let sp = &app.spotify;
    let mut p = PageData::default();
    let logged_in = spotify::spotify_status(sp).await.unwrap_or(false);
    match page {
        SpPage::Library => {
            p.title("Spotify");
            if !logged_in {
                p.message("Melde dich an, um deine Spotify-Playlists, Alben und Lieblingssongs in Wavely zu hören. Abgespielt wird über YouTube Music.");
                p.buttons(&[("spotify-login", "Mit Spotify anmelden", icon::SPOTIFY)]);
                return p;
            }
            match spotify::spotify_library(sp).await {
                Ok(library) => {
                    p.heading("Playlists");
                    let mut playlists = vec![CardData {
                        title: "Lieblingssongs".into(),
                        subtitle: "Spotify".into(),
                        art: None,
                        round: false,
                        target: Target::Open(Box::new(PageKind::Sp(SpPage::Collection {
                            kind: "playlist".into(),
                            id: "liked".into(),
                            title: "Lieblingssongs".into(),
                            art: None,
                        }))),
                    }];
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
        SpPage::Search(query) => {
            if !logged_in {
                p.message("Für die Spotify-Suche bitte zuerst anmelden.");
                p.buttons(&[("spotify-login", "Mit Spotify anmelden", icon::SPOTIFY)]);
                return p;
            }
            match spotify::spotify_search(sp, query.clone()).await {
                Ok(r) => {
                    let empty = r.tracks.is_empty() && r.albums.is_empty() && r.playlists.is_empty() && r.artists.is_empty();
                    if !r.tracks.is_empty() {
                        p.heading("Songs");
                        p.tracks(r.tracks.into_iter().map(Track::from));
                    }
                    if !r.artists.is_empty() {
                        p.heading("Künstler");
                        p.cards(r.artists.into_iter().map(artist_card).collect());
                    }
                    for (heading, list) in [("Alben", r.albums), ("Playlists", r.playlists)] {
                        if !list.is_empty() {
                            p.heading(heading);
                            p.cards(list.into_iter().map(card).collect());
                        }
                    }
                    if empty {
                        p.message("Keine Treffer.");
                    }
                }
                Err(e) => p.error(format!("Suche fehlgeschlagen: {e}")),
            }
        }
        SpPage::Collection { kind, id, title, art } => match spotify::spotify_collection(sp, kind.clone(), id.clone()).await {
            Ok(detail) => {
                let count = format!("{} Titel", detail.tracks.len());
                let subtitle = detail.info.subtitle.clone().map(|s| format!("{s} · {count}")).unwrap_or(count);
                let label = if kind == "album" { "Album" } else { "Playlist" };
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
        SpPage::Artist { id, name } => match spotify::spotify_artist(sp, id.clone()).await {
            Ok(a) => {
                let subs = a.subscribers.map(|n| format!("{n} Follower")).unwrap_or_default();
                p.hero("Künstler", &a.name, &subs, a.image_url.clone(), true);
                p.buttons(&[("play", "Abspielen", icon::PLAY), ("shuffle", "Zufall", icon::SHUFFLE)]);
                if !a.tracks.is_empty() {
                    p.heading("Beliebte Songs");
                    p.tracks(a.tracks.into_iter().map(Track::from));
                }
                for (heading, list) in [("Alben", a.albums), ("Singles & EPs", a.singles), ("Playlists", a.playlists)] {
                    if !list.is_empty() {
                        p.heading(heading);
                        p.cards(list.into_iter().map(card).collect());
                    }
                }
                if !a.similar.is_empty() {
                    p.heading("Ähnliche Künstler");
                    p.cards(a.similar.into_iter().map(artist_card).collect());
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
    pub fn spotify_logged_in(&self) -> bool {
        self.spotify.dir.join("token.json").exists()
    }

    pub fn soundcloud_logged_in(&self) -> bool {
        self.soundcloud.dir.join("token.txt").exists()
    }

    pub fn service_action(self: &Arc<Self>, action: &str) -> bool {
        match action {
            "spotify-login" => {
                self.toast("Spotify-Anmeldung im Browser geöffnet…", false);
                let app = self.clone();
                self.rt.spawn(async move {
                    match spotify::spotify_login(&app.spotify).await {
                        Ok(()) => {
                            app.toast("Bei Spotify angemeldet", false);
                            let a = app.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                a.refresh_if_showing(|p| matches!(p, PageKind::Sp(_) | PageKind::Settings));
                            });
                        }
                        Err(e) => app.toast(&e, true),
                    }
                });
                true
            }
            "spotify-logout" => {
                let app = self.clone();
                self.rt.spawn(async move {
                    let _ = spotify::spotify_logout(&app.spotify).await;
                    app.toast("Von Spotify abgemeldet", false);
                    app.settings_changed();
                });
                true
            }
            "lastfm-login" => {
                self.toast("Last.fm-Anmeldung im Browser geöffnet…", false);
                let app = self.clone();
                self.rt.spawn(async move {
                    match crate::services::lastfm::lastfm_login(&app.lastfm).await {
                        Ok(()) => app.toast("Mit Last.fm verbunden", false),
                        Err(e) => app.toast(&e, true),
                    }
                    app.settings_changed();
                });
                true
            }
            "lastfm-logout" => {
                let app = self.clone();
                self.rt.spawn(async move {
                    let _ = crate::services::lastfm::lastfm_logout(&app.lastfm).await;
                    app.settings_changed();
                });
                true
            }
            "soundcloud-logout" => {
                let app = self.clone();
                self.rt.spawn(async move {
                    let _ = crate::services::soundcloud::soundcloud_logout(&app.soundcloud).await;
                    app.toast("Von SoundCloud abgemeldet", false);
                    app.settings_changed();
                });
                true
            }
            _ => false,
        }
    }
}
