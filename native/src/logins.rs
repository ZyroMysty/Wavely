//! Anmeldungen, die einen Browser brauchen (Google/YouTube Music, SoundCloud): kleines
//! WebView2-Fenster mit eigenem, frischem Profil. Sobald das Login-Cookie da ist, wird es
//! übernommen und das Fenster geschlossen (wie bisher in ytm.rs/soundcloud.rs).

use std::sync::{atomic::Ordering, Arc};

use crate::{win::web, App};

const YT_LOGIN_URL: &str = "https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fmusic.youtube.com%2F";
const SC_LOGIN_URL: &str = "https://soundcloud.com/signin";

impl App {
    pub fn youtube_login(self: &Arc<Self>) {
        let data_dir = self.paths.cache.join("login-webview");
        let _ = std::fs::remove_dir_all(&data_dir);
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
            let _ = std::fs::remove_dir_all(app.paths.cache.join("login-webview"));
        });
    }

    pub fn youtube_logout(self: &Arc<Self>) {
        let app = self.clone();
        self.rt.spawn(async move {
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
