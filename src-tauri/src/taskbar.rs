//! Buttons in der Taskleisten-Vorschau (Zurück, Play/Pause, Weiter, Gefällt mir) – wie bei Spotify.
//!
//! Windows nennt das „Thumbnail Toolbar“ (ITaskbarList3::ThumbBarAddButtons). Klicks kommen als
//! WM_COMMAND-Nachricht am Hauptfenster an; dafür wird das Fenster per SetWindowSubclass „abgehört“
//! und die Aktion als Event `taskbar-action` an die Oberfläche geschickt.
//! Alle Aufrufe laufen im Haupt-Thread (COM-Objekte und Fenster gehören diesem Thread).

use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicU32, Ordering},
        OnceLock,
    },
};

use tauri::{AppHandle, Emitter, WebviewWindow};
use windows::{
    core::w,
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::{
            Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
            Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD},
        },
        UI::{
            Shell::{
                DefSubclassProc, ITaskbarList3, SetWindowSubclass, TaskbarList, THBF_DISABLED, THBF_ENABLED,
                THBF_HIDDEN, THB_FLAGS, THB_ICON, THB_TOOLTIP, THUMBBUTTON, THUMBBUTTONFLAGS,
            },
            WindowsAndMessaging::{
                CreateIcon, DestroyIcon, GetSystemMetrics, RegisterWindowMessageW, HICON, SM_CXSMICON, WM_COMMAND,
            },
        },
    },
};

const THBN_CLICKED: u32 = 0x1800;
const ID_PREVIOUS: u32 = 1;
const ID_PLAY: u32 = 2;
const ID_NEXT: u32 = 3;
const ID_LIKE: u32 = 4;

static APP: OnceLock<AppHandle> = OnceLock::new();
/// Nachrichten-ID von „TaskbarButtonCreated“ (ohne RefCell lesbar, siehe subclass_proc)
static CREATED_MSG: AtomicU32 = AtomicU32::new(0);

/// Was die Buttons gerade zeigen sollen (kommt aus der Oberfläche)
#[derive(Clone, Copy, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskbarState {
    has_track: bool,
    playing: bool,
    /// `None`: Like-Button ausblenden (z. B. lokale Songs oder nicht angemeldet)
    liked: Option<bool>,
}

struct Icons {
    previous: HICON,
    play: HICON,
    pause: HICON,
    next: HICON,
    heart: HICON,
    heart_outline: HICON,
}

struct Taskbar {
    hwnd: HWND,
    list: ITaskbarList3,
    icons: Icons,
    added: bool,
    state: TaskbarState,
}

thread_local! {
    static TASKBAR: RefCell<Option<Taskbar>> = const { RefCell::new(None) };
}

/// Einrichten; muss im Haupt-Thread laufen (z. B. in `setup`).
pub fn init(app: &AppHandle, window: &WebviewWindow) {
    let _ = APP.set(app.clone());
    let Ok(hwnd) = window.hwnd() else { return };
    unsafe {
        let Ok(list) = CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_INPROC_SERVER) else {
            return;
        };
        if list.HrInit().is_err() {
            return;
        }
        let taskbar = Taskbar {
            hwnd,
            list,
            icons: Icons::create(),
            added: false,
            state: TaskbarState::default(),
        };
        CREATED_MSG.store(RegisterWindowMessageW(w!("TaskbarButtonCreated")), Ordering::Relaxed);
        TASKBAR.with(|t| *t.borrow_mut() = Some(taskbar));
        let _ = SetWindowSubclass(hwnd, Some(subclass_proc), 1, 0);
    }
    // Falls der Taskleisten-Button schon existiert, gleich hinzufügen
    add_buttons();
}

/// Neuen Zustand anzeigen (Play/Pause, Like).
pub fn update(app: &AppHandle, state: TaskbarState) {
    let _ = app.run_on_main_thread(move || {
        let Some((list, hwnd, buttons, added)) = TASKBAR.with(|t| {
            let mut t = t.try_borrow_mut().ok()?;
            let taskbar = t.as_mut()?;
            taskbar.state = state;
            Some((taskbar.list.clone(), taskbar.hwnd, taskbar.buttons(), taskbar.added))
        }) else {
            return;
        };
        if added {
            unsafe {
                let _ = list.ThumbBarUpdateButtons(hwnd, &buttons);
            }
        }
    });
}

fn add_buttons() {
    let Some((list, hwnd, buttons, added)) = TASKBAR.with(|t| {
        let t = t.try_borrow().ok()?;
        let taskbar = t.as_ref()?;
        Some((taskbar.list.clone(), taskbar.hwnd, taskbar.buttons(), taskbar.added))
    }) else {
        return;
    };
    // Beim zweiten Mal (z. B. nach Explorer-Neustart) nur aktualisieren
    let result = unsafe {
        if added {
            list.ThumbBarUpdateButtons(hwnd, &buttons)
        } else {
            list.ThumbBarAddButtons(hwnd, &buttons)
        }
    };
    #[cfg(debug_assertions)]
    eprintln!("[taskbar] Buttons {}: {:?}", if added { "aktualisiert" } else { "hinzugefügt" }, result);
    if result.is_ok() {
        TASKBAR.with(|t| {
            if let Ok(mut t) = t.try_borrow_mut() {
                if let Some(taskbar) = t.as_mut() {
                    taskbar.added = true;
                }
            }
        });
    }
}

impl Taskbar {
    fn buttons(&self) -> [THUMBBUTTON; 4] {
        let s = self.state;
        let track_flags = if s.has_track { THBF_ENABLED } else { THBF_DISABLED };
        [
            button(ID_PREVIOUS, self.icons.previous, "Zurück", track_flags),
            button(
                ID_PLAY,
                if s.playing { self.icons.pause } else { self.icons.play },
                if s.playing { "Pause" } else { "Abspielen" },
                track_flags,
            ),
            button(ID_NEXT, self.icons.next, "Weiter", track_flags),
            match s.liked {
                Some(true) => button(ID_LIKE, self.icons.heart, "Gefällt mir nicht mehr", THBF_ENABLED),
                Some(false) => button(ID_LIKE, self.icons.heart_outline, "Gefällt mir", THBF_ENABLED),
                None => button(ID_LIKE, self.icons.heart_outline, "Gefällt mir", THBF_HIDDEN),
            },
        ]
    }
}

fn button(id: u32, icon: HICON, tip: &str, flags: THUMBBUTTONFLAGS) -> THUMBBUTTON {
    let mut sz_tip = [0u16; 260];
    for (i, c) in tip.encode_utf16().take(259).enumerate() {
        sz_tip[i] = c;
    }
    THUMBBUTTON {
        dwMask: THB_ICON | THB_TOOLTIP | THB_FLAGS,
        iId: id,
        iBitmap: 0,
        hIcon: icon,
        szTip: sz_tip,
        dwFlags: flags,
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    let created_msg = CREATED_MSG.load(Ordering::Relaxed);
    if created_msg != 0 && msg == created_msg {
        add_buttons();
    } else if msg == WM_COMMAND && ((wparam.0 >> 16) & 0xffff) as u32 == THBN_CLICKED {
        let action = match (wparam.0 & 0xffff) as u32 {
            ID_PREVIOUS => Some("previous"),
            ID_PLAY => Some("toggle"),
            ID_NEXT => Some("next"),
            ID_LIKE => Some("like"),
            _ => None,
        };
        if let (Some(action), Some(app)) = (action, APP.get()) {
            let _ = app.emit("taskbar-action", action);
        }
        return LRESULT(0);
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

// --- Symbole zeichnen ----------------------------------------------------

impl Icons {
    unsafe fn create() -> Self {
        let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(16, 64);
        // Vorschau-Hintergrund folgt dem Windows-Design: hell → dunkle Symbole
        let color = if windows_uses_light_theme() { [32, 32, 36] } else { [255, 255, 255] };
        let make = |shape: fn(f32, f32) -> bool| unsafe { render_icon(size, color, shape) };
        Icons {
            previous: make(shape_previous),
            play: make(shape_play),
            pause: make(shape_pause),
            next: make(shape_next),
            heart: make(shape_heart),
            heart_outline: make(shape_heart_outline),
        }
    }
}

impl Drop for Icons {
    fn drop(&mut self) {
        for icon in [self.previous, self.play, self.pause, self.next, self.heart, self.heart_outline] {
            unsafe {
                let _ = DestroyIcon(icon);
            }
        }
    }
}

fn windows_uses_light_theme() -> bool {
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    result.is_ok() && value == 1
}

/// Zeichnet eine Form (Koordinaten 0..1) mit 4×4-Kantenglättung als Windows-Icon.
unsafe fn render_icon(size: i32, rgb: [u8; 3], shape: fn(f32, f32) -> bool) -> HICON {
    const SAMPLES: i32 = 4;
    let n = size as usize;
    let mut bgra = vec![0u8; n * n * 4];
    for y in 0..size {
        for x in 0..size {
            let mut hits = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let fx = (x as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / size as f32;
                    let fy = (y as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / size as f32;
                    if shape(fx, fy) {
                        hits += 1;
                    }
                }
            }
            let i = (y as usize * n + x as usize) * 4;
            bgra[i] = rgb[2];
            bgra[i + 1] = rgb[1];
            bgra[i + 2] = rgb[0];
            bgra[i + 3] = (hits * 255 / (SAMPLES * SAMPLES)) as u8;
        }
    }
    // Leere UND-Maske (Zeilen auf 16 Bit ausgerichtet); Transparenz kommt aus dem Alphakanal
    let mask = vec![0u8; ((size + 15) / 16) as usize * 2 * n];
    unsafe { CreateIcon(None, size, size, 1, 32, mask.as_ptr(), bgra.as_ptr()) }.unwrap_or_default()
}

fn in_triangle(px: f32, py: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let sign = |p: (f32, f32), q: (f32, f32), r: (f32, f32)| (p.0 - r.0) * (q.1 - r.1) - (q.0 - r.0) * (p.1 - r.1);
    let p = (px, py);
    let (d1, d2, d3) = (sign(p, a, b), sign(p, b, c), sign(p, c, a));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

fn in_rect(x: f32, y: f32, x0: f32, x1: f32, y0: f32, y1: f32) -> bool {
    x >= x0 && x <= x1 && y >= y0 && y <= y1
}

fn shape_play(x: f32, y: f32) -> bool {
    in_triangle(x, y, (0.28, 0.16), (0.28, 0.84), (0.84, 0.5))
}

fn shape_pause(x: f32, y: f32) -> bool {
    in_rect(x, y, 0.24, 0.42, 0.18, 0.82) || in_rect(x, y, 0.58, 0.76, 0.18, 0.82)
}

fn shape_next(x: f32, y: f32) -> bool {
    in_triangle(x, y, (0.18, 0.18), (0.18, 0.82), (0.66, 0.5)) || in_rect(x, y, 0.68, 0.82, 0.18, 0.82)
}

fn shape_previous(x: f32, y: f32) -> bool {
    shape_next(1.0 - x, y)
}

/// Herz als Formel: (u² + v² − 1)³ − u²·v³ ≤ 0
fn in_heart(u: f32, v: f32) -> bool {
    let a = u * u + v * v - 1.0;
    a * a * a - u * u * v * v * v <= 0.0
}

fn shape_heart(x: f32, y: f32) -> bool {
    in_heart((x - 0.5) * 2.7, (0.56 - y) * 2.7)
}

fn shape_heart_outline(x: f32, y: f32) -> bool {
    let (u, v) = ((x - 0.5) * 2.7, (0.56 - y) * 2.7);
    in_heart(u, v) && !in_heart(u / 0.6, (v - 0.1) / 0.6)
}

#[tauri::command]
pub fn taskbar_update(app: AppHandle, state: TaskbarState) {
    update(&app, state);
}
