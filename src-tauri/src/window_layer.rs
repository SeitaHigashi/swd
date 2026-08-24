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

use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Manager};
use windows::core::{w, BOOL};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, TRUE, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, FindWindowW, RegisterWindowMessageW, SendMessageTimeoutW,
    SetWindowPos, SMTO_NORMAL, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WM_DISPLAYCHANGE,
    WM_DPICHANGED,
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

/// Registered id of the `TaskbarCreated` broadcast message (0 until
/// `start_layer_watcher` registers it). Every top-level window receives
/// this message when explorer.exe restarts and recreates the taskbar/
/// desktop window hierarchy — the standard Win32 way to detect that our
/// z-order pin has just been blown away. Stored in a static so the
/// subclass proc (a plain `extern "system" fn`, not a closure) can read it.
static TASKBAR_CREATED_MSG: AtomicU32 = AtomicU32::new(0);

/// `uIdSubclass` passed to `SetWindowSubclass`/`RemoveWindowSubclass`.
/// Arbitrary — it only needs to be unique among subclasses installed on
/// this hwnd, and we only ever install one.
const LAYER_WATCHER_SUBCLASS_ID: usize = 1;

/// Subclass proc installed on the main window. Watches for the messages
/// that indicate the desktop's WorkerW hierarchy may have been rebuilt
/// (explorer restart, display/DPI change) and re-asserts our z-order
/// position when it sees one, then always falls through to
/// `DefSubclassProc` so normal window behavior (input, Tauri's own
/// handling, etc.) is unaffected.
///
/// `dwrefdata` carries a raw pointer to a leaked `AppHandle` clone (set up
/// once in `start_layer_watcher`) since this callback has no closure
/// environment to capture one in.
unsafe extern "system" fn layer_watcher_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uidsubclass: usize,
    dwrefdata: usize,
) -> LRESULT {
    let taskbar_created = TASKBAR_CREATED_MSG.load(Ordering::Relaxed);
    let is_relevant = (taskbar_created != 0 && msg == taskbar_created)
        || msg == WM_DISPLAYCHANGE
        || msg == WM_DPICHANGED;

    if is_relevant {
        let app_ptr = dwrefdata as *const AppHandle;
        if !app_ptr.is_null() {
            let app = (*app_ptr).clone();
            // Hop off the webview's message-loop thread before doing the
            // actual EnumWindows/SendMessageTimeoutW/SetWindowPos work —
            // SendMessageTimeoutW inside pin_to_desktop_layer can block for
            // up to a second if explorer.exe is busy restarting, and
            // stalling the WndProc would stall webview input/paint too.
            std::thread::spawn(move || pin_to_desktop_layer(&app));
        }
    }

    DefSubclassProc(hwnd, msg, wparam, lparam)
}

/// Installs the event-driven z-order watcher: a `SetWindowSubclass` hook on
/// the main window that re-pins on `TaskbarCreated` (explorer.exe restart)
/// and `WM_DISPLAYCHANGE`/`WM_DPICHANGED` (monitor/DPI reconfiguration),
/// which are the actual events that recreate or reshuffle the WorkerW
/// hierarchy and would otherwise let the widget drift back to being a
/// normal top-level window. This replaces the old unconditional
/// poll-every-3-seconds loop, which did a full `EnumWindows` pass on a
/// timer regardless of whether anything had changed.
///
/// `SetWindowSubclass` (comctl32, via `Win32_UI_Shell`) is used instead of
/// clobbering `GWLP_WNDPROC` directly with `SetWindowLongPtrW`, since the
/// latter would silently break if Tauri/WebView2 ever install their own
/// subclass on the same hwnd — subclasses chain via `DefSubclassProc`
/// instead of stomping on each other's WNDPROC pointer. Tauri's webview
/// window already runs a Win32 message loop on the main thread (that's how
/// it processes input/paint at all), so a subclass on that hwnd is
/// guaranteed to actually be pumped rather than sitting dormant.
pub fn start_layer_watcher(app: AppHandle) {
    // Pin immediately so the window is correctly layered right away,
    // rather than waiting for the first TaskbarCreated/display-change
    // event or the first fallback poll tick.
    pin_to_desktop_layer(&app);

    let Some(window) = app.get_webview_window("main") else {
        eprintln!("[window_layer] main window not found; event hook not installed, falling back to poll only");
        start_fallback_poll(app);
        return;
    };
    let Ok(hwnd) = window.hwnd() else {
        eprintln!("[window_layer] failed to get hwnd; event hook not installed, falling back to poll only");
        start_fallback_poll(app);
        return;
    };

    unsafe {
        let taskbar_created = RegisterWindowMessageW(w!("TaskbarCreated"));
        TASKBAR_CREATED_MSG.store(taskbar_created, Ordering::Relaxed);

        // Leaked deliberately: this clone needs to outlive the subclass,
        // which lives as long as the window does, i.e. the whole process.
        let app_ptr = Box::into_raw(Box::new(app.clone())) as usize;

        let installed = SetWindowSubclass(
            hwnd,
            Some(layer_watcher_subclass_proc),
            LAYER_WATCHER_SUBCLASS_ID,
            app_ptr,
        );
        if !installed.as_bool() {
            eprintln!("[window_layer] SetWindowSubclass failed; relying on fallback poll only");
        }
    }

    start_fallback_poll(app);
}

/// Coarse safety-net poll — NOT the primary re-pinning mechanism anymore.
/// The `TaskbarCreated`/`WM_DISPLAYCHANGE`/`WM_DPICHANGED` subclass hook in
/// `start_layer_watcher` handles the real-world triggers; this just guards
/// against an edge case those messages don't cover (e.g. some other tool
/// rebuilding the WorkerW hierarchy without going through the normal
/// explorer-restart path). 60s instead of the old 3s poll interval since
/// it only needs to eventually correct drift, not track it live.
fn start_fallback_poll(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(60));
        pin_to_desktop_layer(&app);
    });
}
