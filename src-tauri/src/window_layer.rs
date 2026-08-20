//! Pins the widget window to the desktop's background layer: above the
//! Wallpaper Engine wallpaper, below the desktop icons, below the taskbar
//! and every normal application window.
//!
//! We deliberately do NOT `SetParent` our window into the WorkerW that
//! Explorer creates for the wallpaper (that is what the `tauri-plugin-
//! desktop-underlay` crate does). Reparenting into that hierarchy makes
//! Windows treat the window as part of the desktop itself, which disables
//! all mouse/keyboard interaction with it. Instead we keep our window as an
//! independent top-level window and repeatedly ask Windows to slot it into
//! the z-order directly above that WorkerW (the same trick Rainmeter uses
//! for "Send to bottom" desktop skins). Because it stays a normal top-level
//! window, it keeps receiving input normally, so clickable/draggable panels
//! keep working.
//!
//! Known limitation of this approach (vs. true reparenting): pressing
//! Win+D ("show desktop") will minimize this window like any other regular
//! window, since it is not actually owned by the desktop. Re-showing the
//! desktop icons will not automatically bring it back until it regains
//! focus/is restored. This can be revisited later with a WM_WINDOWPOSCHANGING
//! subclass hook if it turns out to matter in daily use.

use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Manager};
use windows::core::{w, BOOL};
use windows::Win32::Foundation::{HWND, LPARAM, TRUE, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, FindWindowW, SendMessageTimeoutW, SetWindowPos, SMTO_NORMAL,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
};

/// Undocumented message that makes explorer.exe spawn the WorkerW window
/// used to host the desktop wallpaper. Used by Rainmeter and every other
/// "desktop widget" tool for the same purpose.
const SPAWN_WORKERW: u32 = 0x052C;

/// Scratch slot the EnumWindows callback writes its find into. EnumWindows'
/// callback is a plain `extern "system" fn`, not a closure, so it cannot
/// capture state directly.
static FOUND_ICON_OWNER: AtomicIsize = AtomicIsize::new(0);

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, _lparam: LPARAM) -> BOOL {
    // The window (Progman, or a WorkerW on newer Windows builds) that owns
    // the desktop icons has a SHELLDLL_DefView child. The empty
    // wallpaper-hosting WorkerW that Wallpaper Engine renders into already
    // sits immediately below *this* window in the z-order, so pinning our
    // widget to "directly behind the icon owner" (via SetWindowPos'
    // hWndInsertAfter, which places a window BELOW the reference window)
    // slots it exactly between the two: icons > widget > wallpaper.
    if FindWindowExW(Some(hwnd), None, w!("SHELLDLL_DefView"), None).is_ok() {
        FOUND_ICON_OWNER.store(hwnd.0 as isize, Ordering::SeqCst);
        return BOOL(0); // found it, stop enumerating
    }
    TRUE
}

/// Locates the window that currently owns the desktop icon view. Returns
/// `None` if the expected window hierarchy isn't present (e.g. explorer.exe
/// is restarting).
fn locate_icon_owner() -> Option<HWND> {
    unsafe {
        let progman = FindWindowW(w!("Progman"), None).ok()?;

        // Ask explorer to (re)create the WorkerW hierarchy. This is a no-op
        // if it already exists.
        let mut result: usize = 0;
        let _ = SendMessageTimeoutW(
            progman,
            SPAWN_WORKERW,
            WPARAM(0),
            LPARAM(0),
            SMTO_NORMAL,
            1000,
            Some(&mut result),
        );

        FOUND_ICON_OWNER.store(0, Ordering::SeqCst);
        let _ = EnumWindows(Some(enum_windows_proc), LPARAM(0));
        let raw = FOUND_ICON_OWNER.load(Ordering::SeqCst);

        if raw == 0 {
            None
        } else {
            Some(HWND(raw as *mut _))
        }
    }
}

/// Re-asserts the widget's position directly behind (z-order-wise) the
/// desktop icon owner, which in turn sits directly above the wallpaper.
/// Safe to call repeatedly; cheap when nothing has changed.
pub fn pin_to_desktop_layer(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        eprintln!("[window_layer] main window not found");
        return;
    };
    let Ok(hwnd) = window.hwnd() else {
        eprintln!("[window_layer] failed to get hwnd");
        return;
    };
    let Some(icon_owner) = locate_icon_owner() else {
        eprintln!("[window_layer] could not locate desktop icon owner window");
        return;
    };

    unsafe {
        if let Err(err) = SetWindowPos(
            hwnd,
            Some(icon_owner),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        ) {
            eprintln!("[window_layer] SetWindowPos failed: {err:?}");
        }
    }
}

/// Starts a background loop that keeps re-pinning the window. This is
/// necessary because explorer.exe recreates the WorkerW hierarchy on
/// display changes, DPI changes, and explorer restarts, which would
/// otherwise let the widget drift back to being a normal top-level window.
pub fn start_layer_watcher(app: AppHandle) {
    std::thread::spawn(move || loop {
        pin_to_desktop_layer(&app);
        std::thread::sleep(Duration::from_secs(3));
    });
}
