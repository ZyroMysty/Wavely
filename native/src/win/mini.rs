//! Nativer Mini-Player: eigenes kleines Windows-Fenster, gezeichnet mit Direct2D und DirectWrite.
//! Er spielt selbst nichts ab: Zustand, Cover und Farben kommen vom Hauptprogramm (update, cover,
//! theme), Befehle gehen als JSON an den beim Öffnen übergebenen Empfänger.
//!
//! Alles Fensterbezogene läuft im UI-Thread (die Slint-Ereignisschleife pumpt dort die Windows-Nachrichten).

use std::{cell::RefCell, collections::HashMap, mem::size_of};

use serde_json::{json, Value};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{D2DERR_RECREATE_TARGET, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::*,
            Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DWM_WINDOW_CORNER_PREFERENCE},
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Gdi::{BeginPaint, EndPaint, GetMonitorInfoW, InvalidateRect, MonitorFromWindow, ScreenToClient, MONITORINFO, MONITOR_DEFAULTTOPRIMARY, PAINTSTRUCT},
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT},
            Shell::ExtractIconExW,
            WindowsAndMessaging::*,
        },
    },
};
use windows_numerics::{Matrix3x2, Vector2};

/// Maus hat das Fenster verlassen (steht in Win32_UI_Controls, das Wavely sonst nicht braucht)
const WM_MOUSELEAVE: u32 = 0x02A3;

// --- Zustand vom Hauptfenster ----------------------------------------------------

#[derive(Default, Clone, PartialEq)]
pub struct PlayerState {
    pub key: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub playing: bool,
    pub loading: bool,
    pub position: f64,
    pub duration: f64,
    pub can_like: bool,
    pub liked: bool,
    pub volume: f64,
    pub muted: bool,
}

/// Farben des aktuellen Designs (RGBA 0–1), damit der Mini-Player wie das Hauptfenster aussieht
#[derive(Clone, Copy, PartialEq)]
pub struct Theme {
    pub bg: [f32; 4],
    pub surface: [f32; 4],
    pub hover: [f32; 4],
    pub border: [f32; 4],
    pub text: [f32; 4],
    pub text_dim: [f32; 4],
    pub accent: [f32; 4],
}

pub const fn hex(rgb: u32) -> [f32; 4] {
    [((rgb >> 16) & 255) as f32 / 255.0, ((rgb >> 8) & 255) as f32 / 255.0, (rgb & 255) as f32 / 255.0, 1.0]
}

/// Dunkles Design (wie app.css), bis das Hauptfenster die echten Farben schickt
pub const DARK: Theme = Theme {
    bg: hex(0x0e0e11),
    surface: hex(0x16161b),
    hover: hex(0x26262f),
    border: hex(0x2a2a34),
    text: hex(0xf3f3f6),
    text_dim: hex(0x9b9ba8),
    accent: hex(0xff4f7b),
};

// --- Symbole (gleiche Pfade wie Icon.svelte, 24 × 24) ------------------------------

const ICONS: &[(&str, &str)] = &[
    ("play", "M8 5v14l11-7z"),
    ("pause", "M6 5h4v14H6zm8 0h4v14h-4z"),
    ("next", "M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z"),
    ("previous", "M6 6h2v12H6zm3.5 6l8.5 6V6z"),
    ("volume", "M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z"),
    ("mute", "M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z"),
    ("close", "M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"),
    ("heart", "M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"),
    ("heartOutline", "M16.5 3c-1.74 0-3.41.81-4.5 2.09C10.91 3.81 9.24 3 7.5 3 4.42 3 2 5.42 2 8.5c0 3.78 3.4 6.86 8.55 11.54L12 21.35l1.45-1.32C18.6 15.36 22 12.28 22 8.5 22 5.42 19.58 3 16.5 3zm-4.4 15.55l-.1.1-.1-.1C7.14 14.24 4 11.39 4 8.5 4 6.5 5.5 5 7.5 5c1.54 0 3.04.99 3.57 2.36h1.87C13.46 5.99 14.96 5 16.5 5c2 0 3.5 1.5 3.5 3.5 0 2.89-3.14 5.74-7.9 10.05z"),
    ("pin", "M16 9V4h1c.55 0 1-.45 1-1s-.45-1-1-1H7c-.55 0-1 .45-1 1s.45 1 1 1h1v5c0 1.66-1.34 3-3 3v2h5.97v7l1 1 1-1v-7H19v-2c-1.66 0-3-1.34-3-3z"),
    ("expandWindow", "M19 19H5V5h7V3H5a2 2 0 00-2 2v14a2 2 0 002 2h14c1.1 0 2-.9 2-2v-7h-2v7zM14 3v2h3.59l-9.83 9.83 1.41 1.41L19 6.41V10h2V3h-7z"),
];
/// Wavely-Logo (assets/logo.svg, Ansicht 0..1024, Kachel 16..1008): Welle und die Bögen in den Kreisen
const LOGO_WAVE: &str = "M290 470 C318 512 336 543 358 543 C414 543 436 350 512 350 C588 350 610 543 666 543 C688 543 706 512 734 470 L734 574 C706 646 688 674 649 674 C590 674 576 482 512 482 C448 482 434 674 375 674 C336 674 318 646 290 574 Z";
const LOGO_EYES: &str = "M155 534 C155 489 239 489 239 534 M785 534 C785 489 869 489 869 534";

// --- Fenster-Zustand ----------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Item {
    Pin,
    Expand,
    Close,
    Previous,
    Play,
    Next,
    Like,
    Mute,
    Volume,
    Seek,
}

#[derive(Clone, Copy, Default)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
    fn d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F { left: self.x, top: self.y, right: self.x + self.w, bottom: self.y + self.h }
    }
    fn center(&self) -> Vector2 {
        Vector2 { X: self.x + self.w / 2.0, Y: self.y + self.h / 2.0 }
    }
}

/// Cover als quadratisches Bild (schon zugeschnitten); die Bitmap passt zur aktuellen Größe
struct Cover {
    image: image::RgbaImage,
    bitmap: Option<(u32, ID2D1Bitmap)>,
}

/// Was nach einem Mausklick passieren soll (außerhalb der Zustands-Ausleihe ausgeführt,
/// weil Fensterfunktionen sofort wieder Nachrichten an das Fenster schicken)
enum Effect {
    Command(Value),
    Pin(bool),
    Close { show_main: bool },
}

struct Mini {
    /// Hauptfenster (für „Großes Fenster“) und Empfänger der Befehle
    main: HWND,
    command: Box<dyn Fn(Value)>,
    hwnd: HWND,
    factory: ID2D1Factory,
    dwrite: IDWriteFactory,
    font: PCWSTR,
    target: Option<ID2D1HwndRenderTarget>,
    icons: HashMap<&'static str, ID2D1PathGeometry>,
    wave: ID2D1PathGeometry,
    eyes: ID2D1PathGeometry,
    round: ID2D1StrokeStyle,
    formats: HashMap<(u32, bool, bool), IDWriteTextFormat>,
    state: PlayerState,
    theme: Theme,
    pinned: bool,
    restore_main: bool,
    cover: Option<Cover>,
    /// Klickbare Flächen aus dem letzten Zeichnen (logische Pixel)
    items: Vec<(Rect, Item)>,
    hover: Option<Item>,
    pressed: Option<Item>,
    /// Gezogener Regler und sein Wert (0–1), solange die Maus gedrückt ist
    drag: Option<(Item, f64)>,
}

thread_local! {
    static MINI: RefCell<Option<Box<Mini>>> = const { RefCell::new(None) };
}


fn with_mini<R>(f: impl FnOnce(&mut Mini) -> R) -> Option<R> {
    MINI.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        guard.as_mut().map(|mini| f(mini))
    })
}

fn invalidate(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

// --- Schnittstelle (im UI-Thread aufrufen) -----------------------------------------------

/// Öffnet den Mini-Player oder holt ihn nach vorn. `size` in logischen Pixeln.
pub fn open_player(main: HWND, pinned: bool, size: (f64, f64), restore_main: bool, command: impl Fn(Value) + 'static) {
    if let Err(e) = unsafe { open(main, Box::new(command), pinned, size, restore_main) } {
        eprintln!("Mini-Player: {e}");
    }
}

pub fn is_open() -> bool {
    with_mini(|_| ()).is_some()
}

pub fn update(state: PlayerState) {
    with_mini(|mini| {
        if mini.state != state {
            mini.state = state;
            invalidate(mini.hwnd);
        }
    });
}

pub fn theme(theme: Theme) {
    with_mini(|mini| {
        if mini.theme != theme {
            mini.theme = theme;
            invalidate(mini.hwnd);
        }
    });
}

/// Cover (Bilddaten) setzen; `None` = kein Cover
pub fn cover(bytes: Option<Vec<u8>>, letterboxed: bool) {
    let image = bytes.and_then(|b| square(&b, letterboxed));
    with_mini(|mini| {
        mini.cover = image.map(|image| Cover { image, bitmap: None });
        invalidate(mini.hwnd);
    });
}

pub fn close_player() {
    if let Some(hwnd) = with_mini(|mini| mini.hwnd) {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

/// Mittiges Quadrat wie `object-fit: cover`; alte YouTube-Vorschaubilder (4:3 mit schwarzen
/// Balken) stärker vergrößert, wie im Hauptfenster
fn square(bytes: &[u8], letterboxed: bool) -> Option<image::RgbaImage> {
    let image = image::load_from_memory(bytes).ok()?;
    let side = (image.width().min(image.height()) as f32 / if letterboxed { 1.34 } else { 1.0 }) as u32;
    let side = side.max(1);
    let x = (image.width() - side) / 2;
    let y = (image.height() - side) / 2;
    let image = image.crop_imm(x, y, side, side);
    // Größer als der größte Mini-Player braucht keiner
    let image = if side > 800 { image.thumbnail(800, 800) } else { image };
    Some(image.to_rgba8())
}

// --- Fenster öffnen ----------------------------------------------------------------------

const CLASS: PCWSTR = w!("WavelyMiniPlayer");
const MIN_SIZE: (f64, f64) = (220.0, 72.0);
const MAX_SIZE: (f64, f64) = (640.0, 800.0);

unsafe fn open(main: HWND, command: Box<dyn Fn(Value)>, pinned: bool, size: (f64, f64), restore_main: bool) -> windows::core::Result<()> {
    if let Some(hwnd) = with_mini(|mini| {
        mini.restore_main = restore_main;
        mini.hwnd
    }) {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
        return Ok(());
    }

    let instance = GetModuleHandleW(None)?;
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(|| {
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&class);
    });

    let mut ex_style = WS_EX_APPWINDOW;
    if pinned {
        ex_style |= WS_EX_TOPMOST;
    }
    let hwnd = CreateWindowExW(
        ex_style,
        CLASS,
        w!("Wavely Mini"),
        WS_POPUP | WS_THICKFRAME | WS_MINIMIZEBOX | WS_SYSMENU,
        0,
        0,
        100,
        100,
        None,
        None,
        Some(instance.into()),
        None,
    )?;

    let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
    let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
    let mut icons = HashMap::new();
    for (name, path) in ICONS {
        icons.insert(*name, path_geometry(&factory, path, true)?);
    }
    let wave = path_geometry(&factory, LOGO_WAVE, true)?;
    let eyes = path_geometry(&factory, LOGO_EYES, false)?;
    let round = factory.CreateStrokeStyle(
        &D2D1_STROKE_STYLE_PROPERTIES {
            startCap: D2D1_CAP_STYLE_ROUND,
            endCap: D2D1_CAP_STYLE_ROUND,
            dashCap: D2D1_CAP_STYLE_ROUND,
            lineJoin: D2D1_LINE_JOIN_ROUND,
            miterLimit: 10.0,
            dashStyle: D2D1_DASH_STYLE_SOLID,
            dashOffset: 0.0,
        },
        None,
    )?;
    let font = font_family(&dwrite);

    MINI.with(|cell| {
        *cell.borrow_mut() = Some(Box::new(Mini {
            main,
            command,
            hwnd,
            factory,
            dwrite,
            font,
            target: None,
            icons,
            wave,
            eyes,
            round,
            formats: HashMap::new(),
            state: PlayerState { volume: 0.6, ..Default::default() },
            theme: DARK,
            pinned,
            restore_main,
            cover: None,
            items: Vec::new(),
            hover: None,
            pressed: None,
            drag: None,
        }))
    });

    // Windows 11: abgerundete Ecken wie normale Fenster
    let corners: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
    let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corners as *const _ as _, size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32);
    set_icon(hwnd);

    // Größe nach Bildschirm-Skalierung, Platz unten rechts auf dem Bildschirm des Hauptfensters
    let scale = GetDpiForWindow(hwnd) as f64 / 96.0;
    let width = (size.0.clamp(MIN_SIZE.0, MAX_SIZE.0) * scale) as i32;
    let height = (size.1.clamp(MIN_SIZE.1, MAX_SIZE.1) * scale) as i32;

    let mut monitor = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    let _ = GetMonitorInfoW(MonitorFromWindow(main, MONITOR_DEFAULTTOPRIMARY), &mut monitor);
    let margin = (16.0 * scale) as i32;
    let work = monitor.rcWork;
    let _ = SetWindowPos(
        hwnd,
        None,
        work.right - width - margin,
        work.bottom - height - margin,
        width,
        height,
        SWP_NOZORDER | SWP_FRAMECHANGED,
    );
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    Ok(())
}

/// Gleiche Schrift wie die Oberfläche (Segoe UI Variable unter Windows 11, sonst Segoe UI)
unsafe fn font_family(dwrite: &IDWriteFactory) -> PCWSTR {
    let mut fonts = None;
    if dwrite.GetSystemFontCollection(&mut fonts, false).is_ok() {
        if let Some(fonts) = fonts {
            let (mut index, mut exists) = (0, Default::default());
            if fonts.FindFamilyName(w!("Segoe UI Variable Text"), &mut index, &mut exists).is_ok() && exists.as_bool() {
                return w!("Segoe UI Variable Text");
            }
        }
    }
    w!("Segoe UI")
}

/// Taskleisten-Symbol aus der Wavely.exe
pub unsafe fn set_icon(hwnd: HWND) {
    let Ok(exe) = std::env::current_exe() else { return };
    let path: Vec<u16> = exe.as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    let (mut large, mut small) = (HICON::default(), HICON::default());
    if ExtractIconExW(PCWSTR(path.as_ptr()), 0, Some(&mut large), Some(&mut small), 1) > 0 {
        SendMessageW(hwnd, WM_SETICON, Some(WPARAM(ICON_BIG as usize)), Some(LPARAM(large.0 as isize)));
        SendMessageW(hwnd, WM_SETICON, Some(WPARAM(ICON_SMALL as usize)), Some(LPARAM(small.0 as isize)));
    }
}

// --- SVG-Pfade → Direct2D-Geometrie --------------------------------------------------------

/// Liest die SVG-Pfadbefehle der Symbole (M L H V C S Q A Z, groß und klein)
unsafe fn path_geometry(factory: &ID2D1Factory, path: &str, filled: bool) -> windows::core::Result<ID2D1PathGeometry> {
    let geometry = factory.CreatePathGeometry()?;
    let sink = geometry.Open()?;
    sink.SetFillMode(D2D1_FILL_MODE_WINDING);
    let begin = if filled { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW };

    let mut tokens = Tokens { s: path.as_bytes(), i: 0 };
    let (mut cur, mut start) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
    let mut last_ctrl: Option<(f32, f32)> = None;
    let mut open = false;
    let mut command = b'M';
    let pt = |p: (f32, f32)| Vector2 { X: p.0, Y: p.1 };

    while let Some(next) = tokens.command_or_number() {
        if let Some(c) = next {
            command = c;
            if c == b'z' || c == b'Z' {
                if open {
                    sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                    open = false;
                }
                cur = start;
                last_ctrl = None;
                continue;
            }
        }
        let rel = command.is_ascii_lowercase();
        let base = if rel { cur } else { (0.0, 0.0) };
        let mut num = || tokens.number().unwrap_or(0.0);
        match command.to_ascii_uppercase() {
            b'M' => {
                if open {
                    sink.EndFigure(D2D1_FIGURE_END_OPEN);
                }
                cur = (base.0 + num(), base.1 + num());
                start = cur;
                sink.BeginFigure(pt(cur), begin);
                open = true;
                // Weitere Koordinaten nach M sind Linien
                command = if rel { b'l' } else { b'L' };
                last_ctrl = None;
            }
            b'L' => {
                cur = (base.0 + num(), base.1 + num());
                sink.AddLine(pt(cur));
                last_ctrl = None;
            }
            b'H' => {
                cur.0 = base.0 + num();
                sink.AddLine(pt(cur));
                last_ctrl = None;
            }
            b'V' => {
                cur.1 = base.1 + num();
                sink.AddLine(pt(cur));
                last_ctrl = None;
            }
            b'C' => {
                let c1 = (base.0 + num(), base.1 + num());
                let c2 = (base.0 + num(), base.1 + num());
                let end = (base.0 + num(), base.1 + num());
                sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: pt(c1), point2: pt(c2), point3: pt(end) });
                last_ctrl = Some(c2);
                cur = end;
            }
            b'S' => {
                let c1 = last_ctrl.map_or(cur, |c| (2.0 * cur.0 - c.0, 2.0 * cur.1 - c.1));
                let c2 = (base.0 + num(), base.1 + num());
                let end = (base.0 + num(), base.1 + num());
                sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: pt(c1), point2: pt(c2), point3: pt(end) });
                last_ctrl = Some(c2);
                cur = end;
            }
            b'Q' => {
                let c = (base.0 + num(), base.1 + num());
                let end = (base.0 + num(), base.1 + num());
                sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT { point1: pt(c), point2: pt(end) });
                last_ctrl = None;
                cur = end;
            }
            b'A' => {
                let (rx, ry, rotation) = (num(), num(), num());
                let large = tokens.flag();
                let sweep = tokens.flag();
                let end = (base.0 + tokens.number().unwrap_or(0.0), base.1 + tokens.number().unwrap_or(0.0));
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: pt(end),
                    size: D2D_SIZE_F { width: rx, height: ry },
                    rotationAngle: rotation,
                    sweepDirection: if sweep { D2D1_SWEEP_DIRECTION_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE },
                    arcSize: if large { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
                });
                last_ctrl = None;
                cur = end;
            }
            _ => break,
        }
    }
    if open {
        sink.EndFigure(D2D1_FIGURE_END_OPEN);
    }
    sink.Close()?;
    Ok(geometry)
}

struct Tokens<'a> {
    s: &'a [u8],
    i: usize,
}

impl Tokens<'_> {
    fn skip(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }
    /// Some(Some(Befehl)), Some(None) wenn eine Zahl folgt (Befehl wiederholen), None am Ende
    fn command_or_number(&mut self) -> Option<Option<u8>> {
        self.skip();
        let c = *self.s.get(self.i)?;
        if c.is_ascii_alphabetic() {
            self.i += 1;
            Some(Some(c))
        } else {
            Some(None)
        }
    }
    fn number(&mut self) -> Option<f32> {
        self.skip();
        let start = self.i;
        let s = self.s;
        if self.i < s.len() && (s[self.i] == b'-' || s[self.i] == b'+') {
            self.i += 1;
        }
        let mut dot = false;
        while self.i < s.len() {
            match s[self.i] {
                b'0'..=b'9' => self.i += 1,
                b'.' if !dot => {
                    dot = true;
                    self.i += 1;
                }
                b'e' | b'E' => {
                    self.i += 1;
                    if self.i < s.len() && (s[self.i] == b'-' || s[self.i] == b'+') {
                        self.i += 1;
                    }
                }
                _ => break,
            }
        }
        std::str::from_utf8(&s[start..self.i]).ok()?.parse().ok()
    }
    /// Bogen-Flags stehen oft ohne Trennzeichen („a2 2 0 00-2 2“)
    fn flag(&mut self) -> bool {
        self.skip();
        let on = self.s.get(self.i) == Some(&b'1');
        self.i += 1;
        on
    }
}

// --- Zeichnen ------------------------------------------------------------------------------

fn color(c: [f32; 4], opacity: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c[0], g: c[1], b: c[2], a: c[3] * opacity }
}

fn format_time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "0:00".into();
    }
    let s = seconds as u64;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}

impl Mini {
    fn scale(&self) -> f32 {
        unsafe { GetDpiForWindow(self.hwnd) as f32 / 96.0 }
    }

    /// Größe in logischen Pixeln
    fn size(&self) -> (f32, f32) {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rect);
        }
        let scale = self.scale();
        ((rect.right - rect.left) as f32 / scale, (rect.bottom - rect.top) as f32 / scale)
    }

    unsafe fn ensure_target(&mut self) -> windows::core::Result<ID2D1HwndRenderTarget> {
        if let Some(target) = &self.target {
            return Ok(target.clone());
        }
        let mut rect = RECT::default();
        GetClientRect(self.hwnd, &mut rect)?;
        let dpi = GetDpiForWindow(self.hwnd) as f32;
        // Software-Zeichnen: für ein kleines Fenster schnell genug und ohne Grafikkarten-Speicher
        let target = self.factory.CreateHwndRenderTarget(
            &D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: dpi,
                dpiY: dpi,
                ..Default::default()
            },
            &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd: self.hwnd,
                pixelSize: D2D_SIZE_U { width: rect.right.max(1) as u32, height: rect.bottom.max(1) as u32 },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            },
        )?;
        self.target = Some(target.clone());
        Ok(target)
    }

    fn resize(&mut self) {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rect);
            if let Some(target) = &self.target {
                let _ = target.Resize(&D2D_SIZE_U { width: rect.right.max(1) as u32, height: rect.bottom.max(1) as u32 });
            }
        }
    }

    unsafe fn text_format(&mut self, size: f32, bold: bool, center: bool) -> windows::core::Result<IDWriteTextFormat> {
        let key = ((size * 10.0) as u32, bold, center);
        if let Some(format) = self.formats.get(&key) {
            return Ok(format.clone());
        }
        let weight = if bold { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL };
        let format = self.dwrite.CreateTextFormat(self.font, None, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("de-DE"))?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetTextAlignment(if center { DWRITE_TEXT_ALIGNMENT_CENTER } else { DWRITE_TEXT_ALIGNMENT_LEADING })?;
        let ellipsis = self.dwrite.CreateEllipsisTrimmingSign(&format)?;
        format.SetTrimming(&DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, ..Default::default() }, &ellipsis)?;
        self.formats.insert(key, format.clone());
        Ok(format)
    }

    unsafe fn paint(&mut self) {
        if let Err(e) = self.draw() {
            // Grafikgerät verloren o. Ä.: beim nächsten Mal alles neu anlegen
            if e.code() == D2DERR_RECREATE_TARGET {
                self.target = None;
                if let Some(cover) = &mut self.cover {
                    cover.bitmap = None;
                }
                invalidate(self.hwnd);
            }
        }
    }

    unsafe fn draw(&mut self) -> windows::core::Result<()> {
        let target = self.ensure_target()?;
        let (width, height) = self.size();
        let t = self.theme;
        let s = self.state.clone();
        let brush = target.CreateSolidColorBrush(&color(t.text, 1.0), None)?;
        self.items.clear();

        target.BeginDraw();
        target.SetTransform(&Matrix3x2::identity());
        target.Clear(Some(&color(t.surface, 1.0)));

        let large = height >= 220.0 && width >= 200.0;
        let has_track = s.key.is_some();
        let volume = match self.drag {
            Some((Item::Volume, v)) => v,
            _ if s.muted => 0.0,
            _ => s.volume,
        };
        let position = match self.drag {
            Some((Item::Seek, v)) => v * s.duration,
            _ => s.position,
        };
        let progress = if s.duration > 0.0 { (position / s.duration).clamp(0.0, 1.0) } else { 0.0 };

        if large {
            let cover = (width - 32.0).min(height - 210.0).max(80.0);
            let total = cover + 14.0 + 38.0 + 10.0 + 16.0 + 10.0 + 46.0 + 10.0 + 32.0;
            let mut y = 36.0 + ((height - 52.0 - total) / 2.0).max(0.0);
            self.draw_cover(&target, Rect::new((width - cover) / 2.0, y, cover, cover), 12.0)?;
            y += cover + 14.0;

            let title = s.title.clone().unwrap_or("Nichts läuft".into());
            let artist = if s.loading { "Lädt…".into() } else { s.artist.clone().unwrap_or("Wavely".into()) };
            self.text(&target, &brush, &title, Rect::new(16.0, y, width - 32.0, 21.0), 16.0, true, true, t.text)?;
            self.text(&target, &brush, &artist, Rect::new(16.0, y + 21.0, width - 32.0, 17.0), 13.0, false, true, t.text_dim)?;
            y += 38.0 + 10.0;

            // Fortschritt mit Zeiten
            self.text(&target, &brush, &format_time(position), Rect::new(16.0, y, 34.0, 16.0), 11.0, false, true, t.text_dim)?;
            self.text(&target, &brush, &format_time(s.duration), Rect::new(width - 50.0, y, 34.0, 16.0), 11.0, false, true, t.text_dim)?;
            self.slider(&target, &brush, Item::Seek, Rect::new(58.0, y, width - 116.0, 16.0), progress, has_track)?;
            y += 16.0 + 10.0;

            // Knöpfe mittig
            let like = s.can_like;
            let row = 32.0 + 14.0 + 46.0 + 14.0 + 32.0 + if like { 32.0 + 14.0 } else { 0.0 };
            let mut x = (width - row) / 2.0;
            if like {
                self.like_button(&target, &brush, Rect::new(x, y + 7.0, 32.0, 32.0), 18.0)?;
                x += 32.0 + 14.0;
            }
            self.button(&target, &brush, Item::Previous, "previous", Rect::new(x, y + 7.0, 32.0, 32.0), 24.0, has_track, None)?;
            x += 32.0 + 14.0;
            self.play_button(&target, &brush, Rect::new(x, y, 46.0, 46.0), 26.0, has_track)?;
            x += 46.0 + 14.0;
            self.button(&target, &brush, Item::Next, "next", Rect::new(x, y + 7.0, 32.0, 32.0), 24.0, has_track, None)?;
            y += 46.0 + 10.0;

            // Lautstärke
            let wide = (width - 32.0).min(220.0);
            let x = (width - wide) / 2.0;
            self.button(&target, &brush, Item::Mute, if volume == 0.0 { "mute" } else { "volume" }, Rect::new(x, y, 32.0, 32.0), 17.0, true, None)?;
            self.slider(&target, &brush, Item::Volume, Rect::new(x + 34.0, y + 8.0, wide - 34.0, 16.0), volume, true)?;
        } else {
            let cover = (height - 24.0).min(120.0).min(width * 0.32).max(40.0);
            self.draw_cover(&target, Rect::new(12.0, (height - cover) / 2.0, cover, cover), 8.0)?;

            let body = width - cover - 36.0;
            let show_like = body >= 150.0;
            let show_volume = body >= 190.0;
            let show_slider = body >= 256.0;
            let x0 = 12.0 + cover + 12.0;
            let x1 = width - 10.0;
            let top = (height - 73.0) / 2.0;

            let title = s.title.clone().unwrap_or("Nichts läuft".into());
            let artist = if s.loading { "Lädt…".into() } else { s.artist.clone().unwrap_or("Wavely".into()) };
            // Platz für die Fensterknöpfe oben rechts
            let meta = (x1 - x0 - 78.0).max(0.0);
            self.text(&target, &brush, &title, Rect::new(x0, top, meta, 19.0), 14.0, true, false, t.text)?;
            self.text(&target, &brush, &artist, Rect::new(x0, top + 19.0, meta, 16.0), 12.0, false, false, t.text_dim)?;

            let y = top + 35.0 + 6.0;
            let mut x = x0;
            self.button(&target, &brush, Item::Previous, "previous", Rect::new(x, y, 32.0, 32.0), 20.0, has_track, None)?;
            x += 38.0;
            self.play_button(&target, &brush, Rect::new(x, y, 32.0, 32.0), 20.0, has_track)?;
            x += 38.0;
            self.button(&target, &brush, Item::Next, "next", Rect::new(x, y, 32.0, 32.0), 20.0, has_track, None)?;
            x += 38.0;
            if show_like && s.can_like {
                self.like_button(&target, &brush, Rect::new(x, y, 32.0, 32.0), 18.0)?;
            }
            if show_volume {
                let mut right = x1;
                if show_slider {
                    self.slider(&target, &brush, Item::Volume, Rect::new(right - 64.0, y + 8.0, 64.0, 16.0), volume, true)?;
                    right -= 64.0 + 2.0;
                }
                self.button(&target, &brush, Item::Mute, if volume == 0.0 { "mute" } else { "volume" }, Rect::new(right - 32.0, y, 32.0, 32.0), 17.0, true, None)?;
            }

            // Fortschritt als Linie am unteren Rand
            brush.SetColor(&color(t.accent, 1.0));
            target.FillRectangle(&Rect::new(0.0, height - 3.0, width * progress as f32, 3.0).d2d(), &brush);
        }

        // Fensterknöpfe oben rechts
        let pin_color = if self.pinned { Some(t.accent) } else { None };
        self.window_button(&target, &brush, Item::Close, "close", Rect::new(width - 30.0, 6.0, 24.0, 24.0), None)?;
        self.window_button(&target, &brush, Item::Expand, "expandWindow", Rect::new(width - 56.0, 6.0, 24.0, 24.0), None)?;
        self.window_button(&target, &brush, Item::Pin, "pin", Rect::new(width - 82.0, 6.0, 24.0, 24.0), pin_color)?;

        // Rahmen
        brush.SetColor(&color(t.border, 1.0));
        target.DrawRectangle(&Rect::new(0.5, 0.5, width - 1.0, height - 1.0).d2d(), &brush, 1.0, None);

        target.EndDraw(None, None)
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn text(&mut self, target: &ID2D1HwndRenderTarget, brush: &ID2D1SolidColorBrush, text: &str, rect: Rect, size: f32, bold: bool, center: bool, c: [f32; 4]) -> windows::core::Result<()> {
        if rect.w <= 0.0 {
            return Ok(());
        }
        let format = self.text_format(size, bold, center)?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        brush.SetColor(&color(c, 1.0));
        target.DrawText(&wide, &format, &rect.d2d(), brush, D2D1_DRAW_TEXT_OPTIONS_CLIP, DWRITE_MEASURING_MODE_NATURAL);
        Ok(())
    }

    unsafe fn icon(&self, target: &ID2D1HwndRenderTarget, brush: &ID2D1SolidColorBrush, name: &str, center: Vector2, size: f32) -> windows::core::Result<()> {
        let Some(geometry) = self.icons.get(name) else { return Ok(()) };
        let k = size / 24.0;
        target.SetTransform(&Matrix3x2 { M11: k, M12: 0.0, M21: 0.0, M22: k, M31: center.X - size / 2.0, M32: center.Y - size / 2.0 });
        target.FillGeometry(geometry, brush, None);
        target.SetTransform(&Matrix3x2::identity());
        Ok(())
    }

    /// Runder Knopf mit Symbol; hervorgehoben unter der Maus
    #[allow(clippy::too_many_arguments)]
    unsafe fn button(&mut self, target: &ID2D1HwndRenderTarget, brush: &ID2D1SolidColorBrush, item: Item, icon: &str, rect: Rect, icon_size: f32, enabled: bool, on: Option<[f32; 4]>) -> windows::core::Result<()> {
        let t = self.theme;
        let hot = enabled && self.hover == Some(item);
        if hot {
            brush.SetColor(&color(t.hover, 1.0));
            target.FillEllipse(&D2D1_ELLIPSE { point: rect.center(), radiusX: rect.w / 2.0, radiusY: rect.h / 2.0 }, brush);
        }
        let c = on.unwrap_or(if hot { t.text } else { t.text_dim });
        brush.SetColor(&color(c, if enabled { 1.0 } else { 0.4 }));
        self.icon(target, brush, icon, rect.center(), icon_size)?;
        if enabled {
            self.items.push((rect, item));
        }
        Ok(())
    }

    unsafe fn play_button(&mut self, target: &ID2D1HwndRenderTarget, brush: &ID2D1SolidColorBrush, rect: Rect, icon_size: f32, enabled: bool) -> windows::core::Result<()> {
        let t = self.theme;
        let opacity = if enabled { 1.0 } else { 0.4 };
        // Leicht größer unter der Maus (wie transform: scale(1.06))
        let grow = if enabled && self.hover == Some(Item::Play) { 1.06 } else { 1.0 };
        brush.SetColor(&color(t.text, opacity));
        target.FillEllipse(&D2D1_ELLIPSE { point: rect.center(), radiusX: rect.w / 2.0 * grow, radiusY: rect.h / 2.0 * grow }, brush);
        brush.SetColor(&color(t.bg, opacity));
        self.icon(target, brush, if self.state.playing { "pause" } else { "play" }, rect.center(), icon_size)?;
        if enabled {
            self.items.push((rect, Item::Play));
        }
        Ok(())
    }

    unsafe fn like_button(&mut self, target: &ID2D1HwndRenderTarget, brush: &ID2D1SolidColorBrush, rect: Rect, icon_size: f32) -> windows::core::Result<()> {
        let liked = self.state.liked;
        let on = liked.then_some(self.theme.accent);
        self.button(target, brush, Item::Like, if liked { "heart" } else { "heartOutline" }, rect, icon_size, true, on)
    }

    /// Kleiner eckiger Knopf oben rechts (Anheften, Groß, Schließen)
    unsafe fn window_button(&mut self, target: &ID2D1HwndRenderTarget, brush: &ID2D1SolidColorBrush, item: Item, icon: &str, rect: Rect, on: Option<[f32; 4]>) -> windows::core::Result<()> {
        let t = self.theme;
        let hot = self.hover == Some(item);
        if hot {
            brush.SetColor(&color(t.hover, 1.0));
            target.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect.d2d(), radiusX: 6.0, radiusY: 6.0 }, brush);
        }
        brush.SetColor(&color(on.unwrap_or(if hot { t.text } else { t.text_dim }), 1.0));
        self.icon(target, brush, icon, rect.center(), 15.0)?;
        self.items.push((rect, item));
        Ok(())
    }

    /// Regler wie input[type=range] im Hauptfenster: 4 px Spur, unter der Maus Akzentfarbe und Knopf
    unsafe fn slider(&mut self, target: &ID2D1HwndRenderTarget, brush: &ID2D1SolidColorBrush, item: Item, rect: Rect, value: f64, enabled: bool) -> windows::core::Result<()> {
        if rect.w <= 0.0 {
            return Ok(());
        }
        let t = self.theme;
        let active = enabled && (self.hover == Some(item) || matches!(self.drag, Some((i, _)) if i == item));
        let y = rect.y + rect.h / 2.0;
        let track = |w: f32| D2D1_ROUNDED_RECT { rect: Rect::new(rect.x, y - 2.0, w, 4.0).d2d(), radiusX: 2.0, radiusY: 2.0 };
        let opacity = if enabled { 1.0 } else { 0.4 };
        brush.SetColor(&color(t.hover, opacity));
        target.FillRoundedRectangle(&track(rect.w), brush);
        let filled = rect.w * value.clamp(0.0, 1.0) as f32;
        brush.SetColor(&color(if active { t.accent } else { t.text }, opacity));
        target.FillRoundedRectangle(&track(filled), brush);
        if active {
            brush.SetColor(&color(t.text, 1.0));
            target.FillEllipse(&D2D1_ELLIPSE { point: Vector2 { X: rect.x + filled, Y: y }, radiusX: 6.0, radiusY: 6.0 }, brush);
        }
        if enabled {
            self.items.push((rect, item));
        }
        Ok(())
    }

    unsafe fn draw_cover(&mut self, target: &ID2D1HwndRenderTarget, rect: Rect, radius: f32) -> windows::core::Result<()> {
        let rounded = D2D1_ROUNDED_RECT { rect: rect.d2d(), radiusX: radius, radiusY: radius };
        let scale = self.scale();
        let pixels = (rect.w * scale).round().max(1.0) as u32;

        if let Some(cover) = &mut self.cover {
            // Bitmap genau in Bildschirmgröße – sieht schärfer aus als skalieren beim Zeichnen
            if cover.bitmap.as_ref().is_none_or(|(size, _)| *size != pixels) {
                let resized = image::imageops::resize(&cover.image, pixels, pixels, image::imageops::FilterType::CatmullRom);
                let mut bgra = Vec::with_capacity(resized.len());
                for p in resized.pixels() {
                    let a = p[3] as u32;
                    bgra.extend_from_slice(&[(p[2] as u32 * a / 255) as u8, (p[1] as u32 * a / 255) as u8, (p[0] as u32 * a / 255) as u8, a as u8]);
                }
                let dpi = scale * 96.0;
                let bitmap = target.CreateBitmap(
                    D2D_SIZE_U { width: pixels, height: pixels },
                    Some(bgra.as_ptr() as _),
                    pixels * 4,
                    &D2D1_BITMAP_PROPERTIES {
                        pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                        dpiX: dpi,
                        dpiY: dpi,
                    },
                )?;
                cover.bitmap = Some((pixels, bitmap));
            }
            let (_, bitmap) = cover.bitmap.as_ref().unwrap();
            let image_brush = target.CreateBitmapBrush(bitmap, None, None)?;
            image_brush.SetTransform(&Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: rect.x, M32: rect.y });
            target.FillRoundedRectangle(&rounded, &image_brush);
            return Ok(());
        }

        // Kein Cover: Wavely-Logo (dunkle Kachel, Welle mit zwei Kreisen im Verlauf)
        let k = rect.w / 992.0;
        let dark = target.CreateSolidColorBrush(&color(hex(0x15121f), 1.0), None)?;
        target.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect.d2d(), radiusX: 236.0 * k, radiusY: 236.0 * k }, &dark);
        // Ab hier in Logo-Koordinaten (auch der Verlauf)
        target.SetTransform(&Matrix3x2 { M11: k, M12: 0.0, M21: 0.0, M22: k, M31: rect.x - 16.0 * k, M32: rect.y - 16.0 * k });
        let stops = target.CreateGradientStopCollection(
            &[
                D2D1_GRADIENT_STOP { position: 0.0, color: color(hex(0xff5a8a), 1.0) },
                D2D1_GRADIENT_STOP { position: 0.38, color: color(hex(0xd062db), 1.0) },
                D2D1_GRADIENT_STOP { position: 0.62, color: color(hex(0x9c6cf4), 1.0) },
                D2D1_GRADIENT_STOP { position: 1.0, color: color(hex(0x6378ff), 1.0) },
            ],
            D2D1_GAMMA_2_2,
            D2D1_EXTEND_MODE_CLAMP,
        )?;
        let gradient = target.CreateLinearGradientBrush(
            &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                startPoint: Vector2 { X: 90.0, Y: 560.0 },
                endPoint: Vector2 { X: 940.0, Y: 480.0 },
            },
            None,
            &stops,
        )?;
        target.FillGeometry(&self.wave, &gradient, None);
        for cx in [197.0, 827.0] {
            let circle = D2D1_ELLIPSE { point: Vector2 { X: cx, Y: 522.0 }, radiusX: 113.0, radiusY: 113.0 };
            target.FillEllipse(&circle, &gradient);
        }
        target.DrawGeometry(&self.eyes, &dark, 27.0, &self.round);
        target.SetTransform(&Matrix3x2::identity());
        Ok(())
    }

    fn item_at(&self, x: f32, y: f32) -> Option<Item> {
        self.items.iter().rev().find(|(rect, _)| rect.contains(x, y)).map(|(_, item)| *item)
    }

    fn slider_value(&self, item: Item, x: f32) -> f64 {
        self.items
            .iter()
            .find(|(_, i)| *i == item)
            .map_or(0.0, |(rect, _)| ((x - rect.x) / rect.w).clamp(0.0, 1.0) as f64)
    }

    // --- Maus ---

    fn mouse_move(&mut self, x: f32, y: f32) -> Option<Effect> {
        if let Some((item, _)) = self.drag {
            let value = self.slider_value(item, x);
            self.drag = Some((item, value));
            invalidate(self.hwnd);
            // Lautstärke sofort, Position erst beim Loslassen
            return (item == Item::Volume).then(|| Effect::Command(json!({ "action": "volume", "value": value })));
        }
        let hover = self.item_at(x, y);
        if hover != self.hover {
            self.hover = hover;
            invalidate(self.hwnd);
        }
        None
    }

    fn mouse_down(&mut self, x: f32, y: f32) -> Option<Effect> {
        let item = self.item_at(x, y)?;
        if matches!(item, Item::Volume | Item::Seek) {
            let value = self.slider_value(item, x);
            self.drag = Some((item, value));
            unsafe {
                SetCapture(self.hwnd);
            }
            invalidate(self.hwnd);
            return (item == Item::Volume).then(|| Effect::Command(json!({ "action": "volume", "value": value })));
        }
        self.pressed = Some(item);
        None
    }

    fn mouse_up(&mut self, x: f32, y: f32) -> Option<Effect> {
        if let Some((item, value)) = self.drag.take() {
            unsafe {
                let _ = ReleaseCapture();
            }
            invalidate(self.hwnd);
            return match item {
                Item::Seek => {
                    self.state.position = value * self.state.duration;
                    Some(Effect::Command(json!({ "action": "seek", "position": value * self.state.duration })))
                }
                _ => {
                    self.state.volume = value;
                    None
                }
            };
        }
        let pressed = self.pressed.take()?;
        if self.item_at(x, y) != Some(pressed) {
            return None;
        }
        let command = |action: &str| Some(Effect::Command(json!(action)));
        match pressed {
            Item::Previous => command("previous"),
            Item::Play => command("toggle"),
            Item::Next => command("next"),
            Item::Like => command("like"),
            Item::Mute => command("mute"),
            Item::Pin => {
                self.pinned = !self.pinned;
                invalidate(self.hwnd);
                Some(Effect::Pin(self.pinned))
            }
            Item::Expand => Some(Effect::Close { show_main: true }),
            Item::Close => Some(Effect::Close { show_main: self.restore_main }),
            Item::Volume | Item::Seek => None,
        }
    }
}

// --- Fensternachrichten --------------------------------------------------------------------

fn logical(lparam: LPARAM, hwnd: HWND) -> (f32, f32) {
    let x = (lparam.0 & 0xffff) as i16 as f32;
    let y = ((lparam.0 >> 16) & 0xffff) as i16 as f32;
    let scale = unsafe { GetDpiForWindow(hwnd) } as f32 / 96.0;
    (x / scale, y / scale)
}

/// Befehl an das Hauptprogramm (außerhalb der Zustands-Ausleihe)
fn send(value: Value) {
    MINI.with(|cell| {
        if let Ok(guard) = cell.try_borrow() {
            if let Some(mini) = guard.as_ref() {
                (mini.command)(value);
            }
        }
    });
}

unsafe fn run(hwnd: HWND, effect: Option<Effect>) {
    let Some(effect) = effect else { return };
    match effect {
        Effect::Command(value) => send(value),
        Effect::Pin(pinned) => {
            let after = if pinned { HWND_TOPMOST } else { HWND_NOTOPMOST };
            let _ = SetWindowPos(hwnd, Some(after), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            send(json!({ "action": "miniPinned", "value": pinned }));
        }
        Effect::Close { show_main } => close(hwnd, show_main),
    }
}

unsafe fn close(hwnd: HWND, show_main: bool) {
    let main = with_mini(|mini| mini.main);
    send(json!("closed"));
    let _ = DestroyWindow(hwnd);
    if show_main {
        if let Some(main) = main {
            let _ = ShowWindow(main, SW_RESTORE);
            let _ = SetForegroundWindow(main);
        }
    }
}

/// Rand zum Größeziehen, sonst überall verschiebbar – außer auf Knöpfen und Reglern
unsafe fn hit_test(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    let mut point = POINT { x: (lparam.0 & 0xffff) as i16 as i32, y: ((lparam.0 >> 16) & 0xffff) as i16 as i32 };
    let _ = ScreenToClient(hwnd, &mut point);
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
    let edge = (6.0 * scale) as i32;
    let (left, right) = (point.x < edge, point.x >= rect.right - edge);
    let (top, bottom) = (point.y < edge, point.y >= rect.bottom - edge);
    let code = match (left, right, top, bottom) {
        (true, _, true, _) => HTTOPLEFT,
        (_, true, true, _) => HTTOPRIGHT,
        (true, _, _, true) => HTBOTTOMLEFT,
        (_, true, _, true) => HTBOTTOMRIGHT,
        (true, ..) => HTLEFT,
        (_, true, ..) => HTRIGHT,
        (_, _, true, _) => HTTOP,
        (.., true) => HTBOTTOM,
        _ => {
            let (x, y) = (point.x as f32 / scale, point.y as f32 / scale);
            let on_item = with_mini(|mini| mini.drag.is_some() || mini.item_at(x, y).is_some()).unwrap_or(false);
            if on_item {
                HTCLIENT
            } else {
                HTCAPTION
            }
        }
    };
    LRESULT(code as isize)
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        // Ganzes Fenster ist Inhalt (kein Windows-Rahmen), Größe ziehen geht trotzdem
        WM_NCCALCSIZE if wparam.0 != 0 => return LRESULT(0),
        WM_NCHITTEST => return hit_test(hwnd, lparam),
        WM_GETMINMAXINFO => {
            let info = &mut *(lparam.0 as *mut MINMAXINFO);
            let scale = GetDpiForWindow(hwnd) as f64 / 96.0;
            info.ptMinTrackSize = POINT { x: (MIN_SIZE.0 * scale) as i32, y: (MIN_SIZE.1 * scale) as i32 };
            info.ptMaxTrackSize = POINT { x: (MAX_SIZE.0 * scale) as i32, y: (MAX_SIZE.1 * scale) as i32 };
            return LRESULT(0);
        }
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            BeginPaint(hwnd, &mut paint);
            with_mini(|mini| mini.paint());
            let _ = EndPaint(hwnd, &paint);
            return LRESULT(0);
        }
        WM_SIZE => {
            with_mini(|mini| mini.resize());
            invalidate(hwnd);
            return LRESULT(0);
        }
        WM_DPICHANGED => {
            let r = &*(lparam.0 as *const RECT);
            with_mini(|mini| {
                mini.target = None;
                if let Some(cover) = &mut mini.cover {
                    cover.bitmap = None;
                }
            });
            let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            let mut track = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
            let _ = TrackMouseEvent(&mut track);
            let (x, y) = logical(lparam, hwnd);
            let effect = with_mini(|mini| mini.mouse_move(x, y)).flatten();
            run(hwnd, effect);
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            with_mini(|mini| {
                if mini.drag.is_none() && mini.hover.take().is_some() {
                    invalidate(hwnd);
                }
            });
            return LRESULT(0);
        }
        WM_LBUTTONDOWN => {
            let (x, y) = logical(lparam, hwnd);
            let effect = with_mini(|mini| mini.mouse_down(x, y)).flatten();
            run(hwnd, effect);
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            let (x, y) = logical(lparam, hwnd);
            let effect = with_mini(|mini| mini.mouse_up(x, y)).flatten();
            run(hwnd, effect);
            return LRESULT(0);
        }
        // Mausrad: Lautstärke in 5-%-Schritten
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xffff) as i16;
            let effect = with_mini(|mini| {
                let current = if mini.state.muted { 0.0 } else { mini.state.volume };
                let value = (current + if delta > 0 { 0.05 } else { -0.05 }).clamp(0.0, 1.0);
                mini.state.volume = value;
                mini.state.muted = false;
                invalidate(hwnd);
                Effect::Command(json!({ "action": "volume", "value": value }))
            });
            run(hwnd, effect);
            return LRESULT(0);
        }
        // Größe merken, damit der Mini-Player beim nächsten Öffnen gleich groß ist
        WM_EXITSIZEMOVE => {
            let effect = with_mini(|mini| {
                let (width, height) = mini.size();
                Effect::Command(json!({ "action": "miniSize", "width": width.round(), "height": height.round() }))
            });
            run(hwnd, effect);
            return LRESULT(0);
        }
        // Alt+F4 oder Schließen über die Taskleiste
        WM_CLOSE => {
            if let Some(restore) = with_mini(|mini| mini.restore_main) {
                close(hwnd, restore);
            } else {
                let _ = DestroyWindow(hwnd);
            }
            return LRESULT(0);
        }
        WM_DESTROY => {
            MINI.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if guard.as_ref().is_some_and(|mini| mini.hwnd == hwnd) {
                        *guard = None;
                    }
                }
            });
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
