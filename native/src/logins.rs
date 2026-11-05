//! Anmeldungen, die einen Browser brauchen (Google/YouTube Music, SoundCloud): kleines
//! WebView2-Fenster mit eigenem, frischem Profil. Sobald das Login-Cookie da ist, wird es
//! übernommen und das Fenster geschlossen (wie bisher in ytm.rs/soundcloud.rs).

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
        let _ = std::fs::remove_dir_all(&data_dir);
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
        let cookies = web::cookies("ytm-login", "https://music.youtube.com");
        if !cookies.iter().any(|(name, _)| name == "SAPISID") {
            return; // Noch nicht angemeldet
        }
        let cookie = cookies.iter().map(|(n, v)| format!("{n}={v}")).collect::<Vec<_>>().join("; ");
        web::close("ytm-login");
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
        if !profile.join("EBWebView").exists() || web::is_open("ytm-refresh") || web::is_open("ytm-login") {
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
                if !url.starts_with("https://music.youtube.com") {
                    return;
                }
                let app = app.clone();
                let finish = finish.clone();
                // Kurz warten, bis die Seite ihre Cookies erneuert hat
                slint::Timer::single_shot(std::time::Duration::from_secs(2), move || {
                    if !web::is_open("ytm-refresh") {
                        return;
                    }
                    let cookies = web::cookies("ytm-refresh", "https://music.youtube.com");
                    web::close("ytm-refresh");
                    if !cookies.iter().any(|(name, _)| name == "SAPISID") {
                        finish(false);
                        return;
                    }
                    let cookie = cookies.iter().map(|(n, v)| format!("{n}={v}")).collect::<Vec<_>>().join("; ");
                    let a = app.clone();
                    app.rt.spawn(async move {
                        let ok = a.ytm.rp.user_auth_set_cookie(cookie).await.is_ok();
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
            if web::is_open("ytm-refresh") {
                web::close("ytm-refresh");
                finish(false);
            }
        });
    }

    pub fn youtube_logout(self: &Arc<Self>) {
        let app = self.clone();
        self.rt.spawn(async move {
            let _ = crate::services::ytm::ytm_logout(&app.ytm).await;
            let _ = std::fs::remove_dir_all(app.yt_profile());
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

    pub fn soundcloud_login(self: &Arc<Self>) {
        let data_dir = self.paths.cache.join("soundcloud-login-webview");
        let _ = std::fs::remove_dir_all(&data_dir);
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
            let _ = std::fs::remove_dir_all(app.paths.cache.join("soundcloud-login-webview"));
            app.settings_changed();
            let a = app.clone();
            let _ = slint::invoke_from_event_loop(move || a.refresh_if_showing(|p| matches!(p, crate::nav::PageKind::Sc(_))));
        });
    }
}
