//! Anmeldungen, die einen Browser brauchen (Google/YouTube Music, SoundCloud): kleines
//! WebView2-Fenster mit eigenem Profil. Sobald das Login-Cookie da ist, wird es übernommen und
//! das Fenster geschlossen (wie bisher in ytm.rs/soundcloud.rs).
//!
//! Das Profil bleibt erhalten (auch beim Abmelden in Wavely): Google erkennt so das Gerät wieder
//! („Gerät speichern“, keine Zwei-Faktor-Abfrage jedes Mal) und bietet das Konto zur Auswahl an.
//! Für einen kompletten Neuanfang (z. B. anderes Konto) gibt es `reset_login_windows`.
//! Das Profil enthält Anmeldedaten: nur im App-Ordner, nie in Diagnose oder Berichte aufnehmen.

use std::sync::{atomic::Ordering, Arc};

use crate::{win::web, App};

const YT_LOGIN_URL: &str = "https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fmusic.youtube.com%2F";
const SC_LOGIN_URL: &str = "https://soundcloud.com/signin";

impl App {
    /// Browserprofil der YouTube-Anmeldung. Bleibt erhalten: Google erneuert die Anmelde-Cookies
    /// nur im Browser, ohne ihn gilt die übernommene Anmeldung nach kurzer Zeit nicht mehr.
    fn yt_profile(&self) -> std::path::PathBuf {
        self.paths.cache.join("ytm-webview")
    }

    pub fn youtube_login(self: &Arc<Self>) {
        let data_dir = self.yt_profile();
        // Ordner einer alten Version (wurde früher bei jeder Anmeldung neu angelegt)
        let _ = std::fs::remove_dir_all(self.paths.cache.join("login-webview"));
        self.toast("Anmeldefenster wird geöffnet…", false);
        let app = self.clone();
        let result = web::open(
            "ytm-login",
            web::Options {
                title: "Bei YouTube Music anmelden".into(),
                url: YT_LOGIN_URL.into(),
                size: (500, 720),
                visible: true,
                data_dir,
                html: None,
            },
            |_| true,
            move |url| {
                if !url.starts_with("https://music.youtube.com") {
                    return;
                }
                // Cookies nicht im Ereignis-Handler lesen (WebView2 würde hängen)
                let app = app.clone();
                slint::Timer::single_shot(std::time::Duration::from_millis(300), move || app.youtube_login_check());
            },
            |_| {},
            || {},
        );
        if let Err(e) = result {
            self.toast(&format!("Anmeldefenster konnte nicht geöffnet werden: {e}"), true);
        }
    }

    fn youtube_login_check(self: &Arc<Self>) {
        if web::is_closing("ytm-login") {
            return;
        }
        let cookies = web::cookies("ytm-login", "https://music.youtube.com");
        if !cookies.iter().any(|(name, _)| name == "SAPISID") {
            return; // Noch nicht angemeldet
        }
        let cookie = cookies.iter().map(|(n, v)| format!("{n}={v}")).collect::<Vec<_>>().join("; ");
        // Erst später schließen, damit die Anmeldung im Browser-Ordner gespeichert ist (fürs Auffrischen)
        web::close_later("ytm-login", 45);
        let app = self.clone();
        self.rt.spawn(async move {
            match app.ytm.rp.user_auth_set_cookie(cookie).await {
                Ok(_) => {
                    let _ = std::fs::write(&app.ytm.login_marker, b"");
                    app.toast("Bei YouTube Music angemeldet", false);
                    app.yt_init();
                    app.apply_quality();
                }
                Err(e) => app.toast(&format!("YouTube-Anmeldung fehlgeschlagen: {e}"), true),
            }
        });
    }

    /// Anmeldung im Hintergrund auffrischen: unsichtbares Browserfenster mit dem gespeicherten
    /// Profil öffnet YouTube Music, danach werden die erneuerten Cookies übernommen.
    /// `done(true)`, wenn es geklappt hat. Nur im UI-Thread aufrufen.
    pub fn youtube_refresh(self: &Arc<Self>, done: impl FnOnce(bool) + Send + 'static) {
        let profile = self.yt_profile();
        if !profile.join("EBWebView").exists() || web::is_open("ytm-refresh") || (web::is_open("ytm-login") && !web::is_closing("ytm-login")) {
            done(false);
            return;
        }
        let done = Arc::new(std::sync::Mutex::new(Some(Box::new(done) as Box<dyn FnOnce(bool) + Send>)));
        let finish = move |ok: bool| {
            if let Some(f) = done.lock().unwrap().take() {
                f(ok);
            }
        };
        let app = self.clone();
        let on_load = {
            let finish = finish.clone();
            move |url: String| {
                eprintln!("[yt-refresh] geladen: {}", url.split('?').next().unwrap_or(""));
                if std::env::var_os("WAVELY_TEST_STEPS").is_some() {
                    slint::Timer::single_shot(std::time::Duration::from_millis(500), || {
                        for site in ["https://www.youtube.com", "https://accounts.google.com", "https://www.google.com"] {
                            let names: Vec<String> = web::cookies("ytm-refresh", site).into_iter().map(|(n, _)| n).collect();
                            eprintln!("[yt-refresh] {site}: {names:?}");
                        }
                    });
                }
                // Zustimmungs- oder Anmeldeseite: Im Browser ist niemand mehr angemeldet
                if url.starts_with("https://consent.") || url.starts_with("https://accounts.google.com") {
                    let finish = finish.clone();
                    slint::Timer::single_shot(std::time::Duration::from_millis(100), move || {
                        web::close("ytm-refresh");
                        finish(false);
                    });
                    return;
                }
                if !url.starts_with("https://music.youtube.com") {
                    return;
                }
                let app = app.clone();
                let finish = finish.clone();
                // Kurz warten, bis die Seite ihre Cookies erneuert hat
                slint::Timer::single_shot(std::time::Duration::from_secs(2), move || {
                    if !web::is_open("ytm-refresh") || web::is_closing("ytm-refresh") {
                        return;
                    }
                    let cookies = web::cookies("ytm-refresh", "https://music.youtube.com");
                    // Erneuerte Cookies vom Browser noch speichern lassen
                    web::close_later("ytm-refresh", 45);
                    eprintln!("[yt-refresh] {} Cookies, SAPISID: {}", cookies.len(), cookies.iter().any(|(n, _)| n == "SAPISID"));
                    if !cookies.iter().any(|(name, _)| name == "SAPISID") {
                        finish(false);
                        return;
                    }
                    let cookie = cookies.iter().map(|(n, v)| format!("{n}={v}")).collect::<Vec<_>>().join("; ");
                    let a = app.clone();
                    app.rt.spawn(async move {
                        let result = a.ytm.rp.user_auth_set_cookie(cookie).await;
                        eprintln!("[yt-refresh] übernommen: {result:?}");
                        let ok = result.is_ok();
                        finish(ok);
                    });
                });
            }
        };
        let result = web::open(
            "ytm-refresh",
            web::Options {
                title: "YouTube Music".into(),
                url: "https://music.youtube.com/".into(),
                size: (800, 600),
                visible: false,
                data_dir: profile,
                html: None,
            },
            |_| true,
            on_load,
            |_| {},
            || {},
        );
        if result.is_err() {
            finish(false);
            return;
        }
        // Lädt die Seite nicht, nach 30 Sekunden aufgeben
        slint::Timer::single_shot(std::time::Duration::from_secs(30), move || {
            if web::is_open("ytm-refresh") && !web::is_closing("ytm-refresh") {
                web::close("ytm-refresh");
                finish(false);
            }
        });
    }

    pub fn youtube_logout(self: &Arc<Self>) {
        let app = self.clone();
        self.rt.spawn(async move {
            // Nur die Anmeldung in Wavely entfernen, das Browserprofil bleibt (siehe oben)
            let _ = crate::services::ytm::ytm_logout(&app.ytm).await;
            app.yt_logged_in.store(false, Ordering::Relaxed);
            app.liked.lock().unwrap().clear();
            app.ui(|ui| {
                use slint::ComponentHandle;
                let y = ui.global::<crate::YouTube>();
                y.set_status("loggedOut".into());
                y.set_account_name(Default::default());
            });
            app.toast("Von YouTube Music abgemeldet", false);
            app.settings_changed();
            app.sync_player();
        });
    }

    fn sc_profile(&self) -> std::path::PathBuf {
        self.paths.cache.join("soundcloud-login-webview")
    }

    /// „Anmeldefenster zurücksetzen“: gespeicherte Google- und SoundCloud-Konten im Browserprofil
    /// löschen (auch „Gerät speichern“). Danach fragen die Anmeldefenster wieder alles neu ab.
    pub fn reset_login_windows(self: &Arc<Self>) {
        if web::is_open("ytm-login") || web::is_open("ytm-refresh") || web::is_open("sc-login") {
            self.toast("Bitte erst das Anmeldefenster schließen.", true);
            return;
        }
        let yt = std::fs::remove_dir_all(self.yt_profile());
        let sc = std::fs::remove_dir_all(self.sc_profile());
        let gone = |r: &std::io::Result<()>| r.as_ref().map_or_else(|e| e.kind() == std::io::ErrorKind::NotFound, |_| true);
        if gone(&yt) && gone(&sc) {
            self.toast("Anmeldefenster zurückgesetzt", false);
        } else {
            self.toast("Anmeldefenster konnten nicht ganz zurückgesetzt werden (Datei in Benutzung?)", true);
        }
    }

    pub fn soundcloud_login(self: &Arc<Self>) {
        let data_dir = self.sc_profile();
        let result = web::open(
            "sc-login",
            web::Options {
                title: "Bei SoundCloud anmelden".into(),
                url: SC_LOGIN_URL.into(),
                size: (680, 820),
                visible: true,
                data_dir,
                html: None,
            },
            |_| true,
            |_| {},
            |_| {},
            || {},
        );
        if let Err(e) = result {
            self.toast(&format!("Anmeldefenster konnte nicht geöffnet werden: {e}"), true);
            return;
        }
        // SoundCloud meldet im Overlay an, ohne neu zu laden → jede Sekunde nachsehen (höchstens 15 Minuten)
        self.soundcloud_login_poll(900);
    }

    fn soundcloud_login_poll(self: &Arc<Self>, left: u32) {
        if left == 0 || !web::is_open("sc-login") {
            return;
        }
        let token = web::cookies("sc-login", "https://soundcloud.com")
            .into_iter()
            .find(|(name, value)| name == "oauth_token" && !value.is_empty())
            .map(|(_, value)| value);
        let Some(token) = token else {
            let app = self.clone();
            slint::Timer::single_shot(std::time::Duration::from_secs(1), move || app.soundcloud_login_poll(left - 1));
            return;
        };
        web::close("sc-login");
        let app = self.clone();
        self.rt.spawn(async move {
            app.soundcloud.save_token(Some(token)).await;
            match crate::services::soundcloud::soundcloud_account(&app.soundcloud).await {
                Ok(_) => app.toast("Bei SoundCloud angemeldet", false),
                Err(e) => {
                    app.soundcloud.save_token(None).await;
                    app.toast(&format!("SoundCloud-Anmeldung fehlgeschlagen: {e}"), true);
                }
            }
            app.settings_changed();
            let a = app.clone();
            let _ = slint::invoke_from_event_loop(move || a.refresh_if_showing(|p| matches!(p, crate::nav::PageKind::Sc(_))));
        });
    }
}
