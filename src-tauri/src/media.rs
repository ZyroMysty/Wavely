//! Windows-Mediensteuerung („System Media Transport Controls“): Medientasten der Tastatur,
//! das Lautstärke-Overlay von Windows und die Medienkachel in den Schnelleinstellungen.
//!
//! WebView2 meldet sich nicht selbst bei Windows an, deshalb übernimmt das hier das Hauptfenster.
//! Tastendrücke gehen wie die Taskleisten-Buttons als Event `taskbar-action` an die Oberfläche –
//! das funktioniert unabhängig davon, ob gerade über YouTube, SoundCloud oder das Spotify-SDK gespielt wird.

use std::cell::RefCell;

use tauri::{AppHandle, Emitter, WebviewWindow};
use windows::{
    core::{factory, HSTRING},
    Foundation::{TypedEventHandler, Uri},
    Media::{
        MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls, SystemMediaTransportControlsButton,
        SystemMediaTransportControlsButtonPressedEventArgs,
    },
    Storage::Streams::RandomAccessStreamReference,
    Win32::System::WinRT::ISystemMediaTransportControlsInterop,
};

/// Was Windows anzeigen soll (kommt aus der Oberfläche)
#[derive(Clone, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    has_track: bool,
    playing: bool,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    /// Nur http(s)-Adressen; Windows lädt das Bild selbst
    artwork_url: Option<String>,
}

struct Media {
    controls: SystemMediaTransportControls,
    info: MediaInfo,
}

thread_local! {
    static MEDIA: RefCell<Option<Media>> = const { RefCell::new(None) };
}

/// Einrichten; muss im Haupt-Thread laufen (z. B. in `setup`).
pub fn init(app: &AppHandle, window: &WebviewWindow) {
    let Ok(hwnd) = window.hwnd() else { return };
    let controls = (|| -> windows::core::Result<SystemMediaTransportControls> {
        let interop = factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>()?;
        let controls: SystemMediaTransportControls = unsafe { interop.GetForWindow(hwnd)? };
        controls.SetIsEnabled(true)?;
        controls.SetIsPlayEnabled(true)?;
        controls.SetIsPauseEnabled(true)?;
        controls.SetIsNextEnabled(true)?;
        controls.SetIsPreviousEnabled(true)?;
        controls.SetPlaybackStatus(MediaPlaybackStatus::Closed)?;

        let app = app.clone();
        controls.ButtonPressed(&TypedEventHandler::<
            SystemMediaTransportControls,
            SystemMediaTransportControlsButtonPressedEventArgs,
        >::new(move |_, args| {
            // Play und Pause getrennt melden, damit „Pause“ nie versehentlich startet
            let action = match args.ok()?.Button()? {
                SystemMediaTransportControlsButton::Play => "play",
                SystemMediaTransportControlsButton::Pause | SystemMediaTransportControlsButton::Stop => "pause",
                SystemMediaTransportControlsButton::Next => "next",
                SystemMediaTransportControlsButton::Previous => "previous",
                _ => return Ok(()),
            };
            let _ = app.emit("taskbar-action", action);
            Ok(())
        }))?;
        Ok(controls)
    })();
    if let Ok(controls) = controls {
        MEDIA.with(|m| {
            *m.borrow_mut() = Some(Media {
                controls,
                info: MediaInfo::default(),
            })
        });
    }
}

#[tauri::command]
pub fn media_update(app: AppHandle, info: MediaInfo) {
    let _ = app.run_on_main_thread(move || {
        MEDIA.with(|m| {
            let mut m = m.borrow_mut();
            let Some(media) = m.as_mut() else { return };
            if media.info == info {
                return;
            }
            let metadata_changed = media.info.title != info.title
                || media.info.artist != info.artist
                || media.info.album != info.album
                || media.info.artwork_url != info.artwork_url
                || media.info.has_track != info.has_track;
            let _ = apply(&media.controls, &info, metadata_changed);
            media.info = info;
        });
    });
}

fn apply(controls: &SystemMediaTransportControls, info: &MediaInfo, metadata_changed: bool) -> windows::core::Result<()> {
    controls.SetPlaybackStatus(match (info.has_track, info.playing) {
        (false, _) => MediaPlaybackStatus::Closed,
        (true, true) => MediaPlaybackStatus::Playing,
        (true, false) => MediaPlaybackStatus::Paused,
    })?;
    if !metadata_changed {
        return Ok(());
    }
    let updater = controls.DisplayUpdater()?;
    if !info.has_track {
        updater.ClearAll()?;
        return updater.Update();
    }
    updater.SetType(MediaPlaybackType::Music)?;
    let music = updater.MusicProperties()?;
    music.SetTitle(&HSTRING::from(info.title.as_deref().unwrap_or("")))?;
    music.SetArtist(&HSTRING::from(info.artist.as_deref().unwrap_or("")))?;
    music.SetAlbumTitle(&HSTRING::from(info.album.as_deref().unwrap_or("")))?;
    let thumbnail = info
        .artwork_url
        .as_deref()
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
        .and_then(|url| Uri::CreateUri(&HSTRING::from(url)).ok())
        .and_then(|uri| RandomAccessStreamReference::CreateFromUri(&uri).ok());
    updater.SetThumbnail(thumbnail.as_ref())?;
    updater.Update()
}
