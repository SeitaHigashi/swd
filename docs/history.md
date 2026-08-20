# Development History

A chronological log of how SWD was built and why, kept for future
contributors (human or AI) who need the reasoning behind non-obvious
decisions without re-deriving it from the diff.

## 2026-08-20 — Project bootstrap and desktop layering spike

**Goal:** a Windows 11 desktop widget that sits above the Wallpaper Engine
wallpaper and below desktop icons, with clickable/draggable glass-morphism
cards, without disturbing the wallpaper itself.

### Layering approach: rejected two existing plugins, built our own

Before writing any window code, evaluated two existing Tauri plugins:

- **`tauri-plugin-wallpaper` (meslzy)** — its `attach()` places the window
  in the *same* WorkerW slot the wallpaper itself renders into. That's the
  right tool for replacing Wallpaper Engine, not for sitting on top of it;
  running both would fight over the same surface. Rejected.
- **`tauri-plugin-desktop-underlay` (Charlie-XIAO)** — correctly targets
  "above wallpaper, below icons," but its own FAQ states that a window set
  as a desktop underlay has **all user interaction disabled** (it becomes a
  true child of the WorkerW hierarchy, so Windows treats it as part of the
  desktop rather than an interactive app window). That conflicts directly
  with the requirement for draggable/clickable cards. Rejected.

Decision (user-confirmed): implement the Win32 WorkerW technique directly,
the same way Rainmeter's "Send to Desktop" skins work — keep the window as
an **independent top-level window** (never `SetParent` it into WorkerW) and
just reinsert it into the *z-order* between the icon-owning window and the
wallpaper's WorkerW. This preserves normal input handling because the
window is never actually owned by the desktop.

Implementation landed in `src-tauri/src/window_layer.rs`.

### Bug: z-order reference was backwards

First implementation located the *wallpaper* WorkerW and called
`SetWindowPos(hwnd, insertAfter = wallpaper_workerw, ...)`. That's backwards
— `hWndInsertAfter` places a window **behind** the reference window, so this
put the widget *below* the wallpaper (or, since the call silently failed to
find the right window in practice, left it in its default position: above
the icons). Visual symptom: desktop icons rendered *under* the widget
instead of on top of it.

Fix: locate the **icon-owning** window (the one with the `SHELLDLL_DefView`
child) and insert directly behind *that* instead. Because the wallpaper's
WorkerW already sits immediately below the icon owner in the existing
z-order, this slots the widget exactly in between: `icons > widget >
wallpaper`. Verified working against a live Wallpaper Engine wallpaper.

### Bug: click-through/drag were dead on arrival

The `vanilla` Tauri template ships without a bundler — `src/` is served
as-is, no Vite/esbuild step. `main.js` was written with
`import { invoke } from "@tauri-apps/api/core"`, a bare package specifier
the browser's module loader can't resolve without a bundler. The whole
module script failed to load, so nothing past that line ran: no clock tick,
no drag handling, no hit-region sync. `withGlobalTauri: true` is already
set in `tauri.conf.json` specifically so `window.__TAURI__` is available —
switched to using that instead of ES module imports for anything
Tauri-provided (plain relative imports between our own `src/modules/*.js`
files are fine, since those don't need bundler resolution).

### Click-through design: cursor polling, not webview mousemove

A naive "toggle `ignore_cursor_events` from the webview's own `mousemove`"
approach deadlocks: the instant the window ignores the cursor, WebView2
stops receiving mouse events *at all*, including the mousemove that would
have told it to stop ignoring. Solved by decoupling: the frontend reports
card rectangles to Rust (`set_hit_regions` command) whenever layout
changes, and a background thread polls `GetCursorPos` independently of the
webview's event loop, flipping `ignore_cursor_events` only on actual
boundary crossings (`src-tauri/src/hit_test.rs`).

### Environment note: build failed on a full disk

Mid-session, `cargo check` failed with `os error 112` (not enough space on
disk) — the dev machine's C: drive had 11 MB free. Not a code issue; the
user freed space (down to 28 GB used before the fix) and the build
proceeded normally. Worth knowing if a fresh clone's first build fails with
a disk error that looks unrelated to the code.

### Multi-monitor: switched from "primary only" to "rightmost of N"

Initial scope (per the original spec) was primary-monitor-only, by design,
to de-risk the first checkpoint. Once the layering/click-through/input
loop was verified working, the user's actual desk setup (3 monitors, widget
wanted on the right side of the rightmost one) came up, and the window
placement logic was generalized: `select_target_monitor` in
`src-tauri/src/lib.rs` enumerates `available_monitors()` and currently picks
the one with the largest `x` position. Kept as a single small function
specifically so retargeting (primary monitor, a specific index, etc.) is a
one-line change instead of a hunt through `setup()`.

### Card layout persistence: plain `localStorage`, not a config file

Considered a Rust-side JSON config file for remembering dragged card
positions, but WebView2's `localStorage` is already persisted per-app (tied
to the app's user data directory, survives restarts) and needs zero backend
code. Went with `src/modules/layout.js` wrapping `localStorage` under a
small `loadCardPosition`/`saveCardPosition`/`applySavedPosition` API, so
swapping the backend later (e.g. to a Rust-owned config file, if
`localStorage` ever proves insufficient) only touches that one file.

### CPU/memory/network graphs

Added a shared `src/modules/sparkline.js` canvas renderer used by both
`system-monitor.js` (CPU%, memory%, fixed 0–100 scale) and `network.js`
(rx/tx bytes/sec, auto-scaled to the current window's max) rather than
duplicating the drawing code, since the two cards only differ in what data
and scaling they feed in.

### Now-playing card via Windows System Media Transport Controls

Implemented in `src-tauri/src/media.rs` using the `windows` crate's
`Media::Control` WinRT projection (the same API backing the Win+G /
lock-screen media overlay), covering title/artist/artwork/playback state
plus play-pause/next/previous transport commands.

Two implementation details worth remembering:

- **WinRT needs COM initialized on the calling thread.** Rather than
  calling `CoInitializeEx` from whichever tokio worker thread happens to
  run a Tauri command (apartment state isn't guaranteed stable across
  Tauri's async command pool), polling and transport commands both run on
  one dedicated background thread. Commands arrive over an `mpsc` channel
  that the same thread drains between polls (`recv_timeout`), so only that
  one thread ever touches the WinRT session objects.
- **Album art comes back as a WinRT stream, not bytes.** Extracting it
  requires an extra async hop (`Thumbnail().OpenReadAsync()`) plus a
  `DataReader` to read the buffer synchronously, which is then base64-
  encoded into a `data:` URL so the frontend can drop it straight into an
  `<img src>` with no extra IPC round-trip for the image bytes.

## Known gaps (tracked, not yet done)

- **Win+D minimizes the widget.** The WorkerW z-order trick doesn't make
  Windows treat the window as desktop-owned, so "Show Desktop" hides it
  like any other app window. Fixable with a `WM_WINDOWPOSCHANGING`
  subclass hook that intercepts the off-screen move Win+D issues, same
  idea `tauri-plugin-wallpaper` uses — not implemented yet.
- **No GPU usage.** `sysinfo` doesn't expose GPU metrics; would need PDH's
  `GPU Engine` performance counters or a vendor SDK (NVML etc.).
