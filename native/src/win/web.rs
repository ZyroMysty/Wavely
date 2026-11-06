//! Browserfenster nur bei Bedarf (WebView2 über wry): Anmeldung bei Google/YouTube Music und
//! SoundCloud, unsichtbar für die Spotify-Wiedergabe. Solange keins offen ist, kostet das nichts.
//! Alles läuft im UI-Thread.

use std::{cell::RefCell, collections::HashMap, mem::size_of, num::NonZeroIsize};

use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle};
use windows::{
    core::{w, HSTRING, PCWSTR},
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::*},
    },
};

/// Fenster-Handle für wry
struct Parent(HWND);

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let handle = Win32WindowHandle::new(NonZeroIsize::new(self.0 .0 as isize).ok_or(HandleError::Unavailable)?);
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(handle)) })
    }
}

struct Web {
    hwnd: HWND,
    view: wry::WebView,
    /// Beim Schließen (auch über das X)
    on_close: Option<Box<dyn FnOnce()>>,
    closing: bool,
}

thread_local! {
    static WEBS: RefCell<HashMap<String, Web>> = RefCell::new(HashMap::new());
    static CONTEXTS: RefCell<Vec<Box<wry::WebContext>>> = const { RefCell::new(Vec::new()) };
}

const CLASS: PCWSTR = w!("WavelyWebWindow");

pub struct Options {
    pub title: String,
    pub url: String,
    pub size: (i32, i32),
    pub visible: bool,
    /// Eigener Browser-Ordner (Anmeldungen bleiben getrennt)
    pub data_dir: std::path::PathBuf,
    /// Eigene Seite unter https://<name>.localhost/ (sicherer Ursprung, z. B. für Kopierschutz/EME)
    pub html: Option<(String, String)>,
}

/// Öffnet ein Browserfenster mit Kennung `id` (oder holt es nach vorn)
pub fn open(
    id: &str,
    options: Options,
    on_navigation: impl Fn(&str) -> bool + 'static,
    on_page_load: impl Fn(String) + 'static,
    on_ipc: impl Fn(String) + 'static,
    on_close: impl FnOnce() + 'static,
) -> Result<(), String> {
    // Wird gerade geschlossen: sofort weg, dann neu öffnen
    if is_closing(id) {
        close(id);
    }
    if let Some(hwnd) = WEBS.with(|w| w.borrow().get(id).map(|web| web.hwnd)) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
        return Ok(());
    }
    unsafe {
        let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        static REGISTER: std::sync::Once = std::sync::Once::new();
        REGISTER.call_once(|| {
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: windows::Win32::Graphics::Gdi::GetSysColorBrush(windows::Win32::Graphics::Gdi::COLOR_WINDOW),
                lpszClassName: CLASS,
                ..Default::default()
            };
            RegisterClassExW(&class);
        });
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS,
            &HSTRING::from(options.title.as_str()),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            options.size.0,
            options.size.1,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|e| e.to_string())?;
        // Größe an die Bildschirm-Skalierung anpassen
        let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
        let _ = SetWindowPos(hwnd, None, 0, 0, (options.size.0 as f32 * scale) as i32, (options.size.1 as f32 * scale) as i32, SWP_NOMOVE | SWP_NOZORDER);

        // Fenster gleich zeigen: Der Browser braucht beim ersten Mal ein paar Sekunden
        if options.visible {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
        let _ = std::fs::create_dir_all(&options.data_dir);
        let mut context = Box::new(wry::WebContext::new(Some(options.data_dir.clone())));
        let context_ptr: *mut wry::WebContext = &mut *context;
        CONTEXTS.with(|c| c.borrow_mut().push(context));
        let parent = Parent(hwnd);
        let mut builder = wry::WebViewBuilder::new_with_web_context(&mut *context_ptr)
            .with_navigation_handler(move |url| on_navigation(&url))
            .with_on_page_load_handler(move |event, url| {
                if matches!(event, wry::PageLoadEvent::Finished) {
                    on_page_load(url);
                }
            })
            .with_ipc_handler(move |request| on_ipc(request.body().clone()))
            .with_autoplay(true);
        builder = match options.html.clone() {
            Some((scheme, html)) => {
                use wry::WebViewBuilderExtWindows;
                let url = format!("https://{scheme}.localhost/");
                builder
                    .with_https_scheme(true)
                    .with_custom_protocol(scheme, move |_, _| {
                        wry::http::Response::builder()
                            .header("Content-Type", "text/html; charset=utf-8")
                            .body(std::borrow::Cow::Owned(html.clone().into_bytes()))
                            .unwrap()
                    })
                    .with_url(url)
            }
            None => builder.with_url(options.url.clone()),
        };
        let view = builder.build(&parent).map_err(|e| e.to_string())?;
        resize(hwnd, &view);
        if options.visible {
            let _ = view.focus();
        }
        WEBS.with(|w| w.borrow_mut().insert(id.to_string(), Web { hwnd, view, on_close: Some(Box::new(on_close)), closing: false }));
    }
    Ok(())
}

fn resize(hwnd: HWND, view: &wry::WebView) {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rect);
    }
    let _ = view.set_bounds(wry::Rect {
        position: wry::dpi::PhysicalPosition::new(0, 0).into(),
        size: wry::dpi::PhysicalSize::new((rect.right - rect.left) as u32, (rect.bottom - rect.top) as u32).into(),
    });
}

/// Cookies einer Adresse (nicht aus einem Ereignis-Handler des Browsers aufrufen)
pub fn cookies(id: &str, url: &str) -> Vec<(String, String)> {
    WEBS.with(|w| {
        w.borrow()
            .get(id)
            .and_then(|web| web.view.cookies_for_url(url).ok())
            .map(|list| list.into_iter().map(|c| (c.name().to_string(), c.value().to_string())).collect())
            .unwrap_or_default()
    })
}

pub fn is_open(id: &str) -> bool {
    WEBS.with(|w| w.borrow().contains_key(id))
}

pub fn eval(id: &str, script: &str) {
    WEBS.with(|w| {
        if let Some(web) = w.borrow().get(id) {
            let _ = web.view.evaluate_script(script);
        }
    });
}

/// Fenster verstecken und erst nach `seconds` schließen: Der Browser schreibt Cookies nur alle
/// paar Sekunden auf die Festplatte, sonst wäre eine frische Anmeldung beim nächsten Mal weg.
pub fn close_later(id: &str, seconds: u64) {
    let found = WEBS.with(|w| {
        w.borrow_mut().get_mut(id).map(|web| {
            web.closing = true;
            web.hwnd
        })
    });
    let Some(hwnd) = found else { return };
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
    let id = id.to_string();
    slint::Timer::single_shot(std::time::Duration::from_secs(seconds), move || {
        if WEBS.with(|w| w.borrow().get(&id).is_some_and(|web| web.hwnd == hwnd)) {
            close(&id);
        }
    });
}

/// Wird gerade über `close_later` geschlossen
pub fn is_closing(id: &str) -> bool {
    WEBS.with(|w| w.borrow().get(id).is_some_and(|web| web.closing))
}

/// Fenster schließen (Browser wird beendet, Speicher wieder frei)
pub fn close(id: &str) {
    let web = WEBS.with(|w| w.borrow_mut().remove(id));
    if let Some(mut web) = web {
        if let Some(on_close) = web.on_close.take() {
            on_close();
        }
        let hwnd = web.hwnd;
        drop(web);
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_SIZE => {
            WEBS.with(|w| {
                if let Ok(webs) = w.try_borrow() {
                    if let Some(web) = webs.values().find(|web| web.hwnd == hwnd) {
                        resize(hwnd, &web.view);
                    }
                }
            });
            LRESULT(0)
        }
        // Fenster aktiviert: Fokus in den Browser (sonst lässt sich nichts eintippen)
        WM_SETFOCUS | WM_ACTIVATE => {
            if msg == WM_ACTIVATE && (wparam.0 & 0xffff) == 0 {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            WEBS.with(|w| {
                if let Ok(webs) = w.try_borrow() {
                    if let Some(web) = webs.values().find(|web| web.hwnd == hwnd) {
                        let _ = web.view.focus();
                    }
                }
            });
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CLOSE => {
            let id = WEBS.with(|w| w.try_borrow().ok().and_then(|webs| webs.iter().find(|(_, web)| web.hwnd == hwnd).map(|(id, _)| id.clone())));
            match id {
                Some(id) => {
                    // Nach dem Handler schließen (der Browser ruft evtl. noch zurück)
                    let _ = slint::invoke_from_event_loop(move || close(&id));
                }
                None => {
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
