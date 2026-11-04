//! Systemweite Tastenkürzel (Strg+Alt+…), auch wenn Wavely im Hintergrund ist.
//! Eigener Thread mit Nachrichtenschleife; RegisterHotKey meldet WM_HOTKEY an diesen Thread.

use std::sync::{atomic::{AtomicU32, Ordering}, Mutex};

use windows::Win32::{
    System::Threading::GetCurrentThreadId,
    UI::{
        Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, VK_DOWN, VK_LEFT, VK_RIGHT, VK_SPACE, VK_UP},
        WindowsAndMessaging::{GetMessageW, PostThreadMessageW, MSG, WM_HOTKEY, WM_QUIT},
    },
};

/// (Kennung, Taste, Aktion)
const KEYS: [(i32, u16, &str); 5] = [
    (1, VK_SPACE.0, "toggle"),
    (2, VK_RIGHT.0, "next"),
    (3, VK_LEFT.0, "previous"),
    (4, VK_UP.0, "volume-up"),
    (5, VK_DOWN.0, "volume-down"),
];

static THREAD: AtomicU32 = AtomicU32::new(0);
static LOCK: Mutex<()> = Mutex::new(());

/// Ein- oder ausschalten; gibt die Kürzel zurück, die schon ein anderes Programm belegt
pub fn set(enabled: bool) -> Vec<&'static str> {
    let _guard = LOCK.lock().unwrap();
    // Laufenden Thread beenden
    let old = THREAD.swap(0, Ordering::SeqCst);
    if old != 0 {
        unsafe {
            let _ = PostThreadMessageW(old, WM_QUIT, Default::default(), Default::default());
        }
    }
    if !enabled {
        return Vec::new();
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut failed = Vec::new();
        let modifiers = HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_ALT.0 | MOD_NOREPEAT.0);
        for (id, vk, _) in KEYS {
            if unsafe { RegisterHotKey(None, id, modifiers, vk as u32) }.is_err() {
                failed.push(match id {
                    1 => "Strg+Alt+Leertaste",
                    2 => "Strg+Alt+→",
                    3 => "Strg+Alt+←",
                    4 => "Strg+Alt+↑",
                    _ => "Strg+Alt+↓",
                });
            }
        }
        THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
        let _ = tx.send(failed);
        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            if msg.message == WM_HOTKEY {
                if let Some((_, _, action)) = KEYS.iter().find(|(id, _, _)| *id as usize == msg.wParam.0) {
                    super::action(action);
                }
            }
        }
        for (id, _, _) in KEYS {
            unsafe {
                let _ = UnregisterHotKey(None, id);
            }
        }
    });
    rx.recv().unwrap_or_default()
}
