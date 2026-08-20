# SWD (Seita Windows Dashboard)

An always-on Windows 11 desktop widget. Renders a glass-morphism system
dashboard (clock, CPU/memory, network, now-playing media, plus whatever
plugins are installed) pinned above the desktop wallpaper (Wallpaper
Engine) and below the desktop icons.

The reasoning behind non-obvious decisions lives in
[docs/history.md](docs/history.md); a quick-reference for anyone (human
or AI) working in this codebase is in [CLAUDE.md](CLAUDE.md); a guide to
writing a plugin (built-in or external) is in
[docs/plugin-authoring.md](docs/plugin-authoring.md).

## Setup

### Requirements

| Tool | Version | Notes |
| --- | --- | --- |
| Node.js | 20+ | developed against the v26 line |
| npm | 10+ | |
| Rust (stable, MSVC toolchain) | 1.75+ | `rustup default stable-x86_64-pc-windows-msvc` |
| Tauri CLI | `@tauri-apps/cli` v2 | installed as a devDependency via `npm install` |

Requires Windows 11 and the WebView2 runtime (bundled with Windows 11).
**Windows-only** — it depends directly on Win32 APIs, so on macOS/Linux
the `window_layer` and `hit_test` modules are excluded from the build and
desktop pinning / click-through don't work.

### Install

```bash
npm install
```

## Run / build

```bash
# Dev mode - not hot reload, but auto-recompiles/restarts on src-tauri/ changes
npm run dev

# Release bundle (installers under src-tauri/target/release/bundle/)
npm run build
```

`npm run dev` auto-recompiles and restarts on any `src-tauri/` change.
The frontend (`src/`) has no bundler — plain HTML/CSS/JS served as-is —
so a `src/` change may need a window reload (or an app restart) to show
up, since there's no dev server watching those files.

## Architecture overview

```
src-tauri/src/
├── lib.rs          # app bootstrap: monitor selection, window init, module wiring
├── window_layer.rs # desktop-pinning z-order trick (Windows only)
├── hit_test.rs      # click-through region polling (Windows only)
├── system_info.rs  # CPU/memory/network sampling, cross-platform
├── media.rs           # now-playing info + transport controls (Windows only)
├── plugins.rs           # external plugin discovery + safe file resolution
└── tray.rs                 # system tray icon + context menu

src/
├── index.html         # empty <div class="dashboard"> - cards mount at runtime
├── style.css          # glass-morphism base styles only
├── main.js            # bootstrap: load plugins, mount them, start hit-region sync
├── core/               # generic plugin host (not card-specific)
├── shared/              # code shared across plugins (e.g. the sparkline renderer)
└── plugins/               # built-in cards: clock, system-monitor, network, media
```

Adding a card is adding a plugin — see
[docs/plugin-authoring.md](docs/plugin-authoring.md) for the full guide.
Drag handling, click-through hit-testing, and position persistence are
all generic over `.glass-card` + a unique `id`, so a new plugin gets them
for free.

### How desktop pinning works

Two existing Tauri plugins were evaluated and rejected: one places the
window in the same slot the wallpaper itself renders into (fights with
Wallpaper Engine for the same surface), the other correctly targets
"above wallpaper, below icons" but disables all mouse/keyboard input to
get there. Neither meets the requirement of a pinned window that's also
interactive.

Instead, this project talks to the Win32 WorkerW hierarchy directly (the
same idea Rainmeter's "Send to Desktop" uses):

- The window is never `SetParent`'d into WorkerW — that would disable
  input entirely. It stays an independent top-level window; a background
  thread just reorders its z-position, right behind the window that owns
  the desktop icons and just in front of the wallpaper's WorkerW
  (`window_layer.rs`).
- That z-order fix-up reruns every few seconds, since `explorer.exe`
  periodically recreates WorkerW (e.g. on a display change or an
  `explorer.exe` restart), which would otherwise silently undo it.
- Click-through can't be implemented as "the window ignores the cursor
  until its own mousemove tells it to stop" — once a window ignores the
  cursor, it stops receiving mouse events at all, including the one that
  would undo it. Instead the frontend reports each card's screen-space
  rectangle to Rust (`set_hit_regions`), and an independent thread polls
  `GetCursorPos` to decide whether the window should currently be
  click-through or not (`hit_test.rs`).

### Now-playing card

Reads the current title/artist/thumbnail/playback state from Windows'
System Media Transport Controls (the same API behind the Win+G media
overlay and lock-screen media controls) and can send
play/pause/next/previous (`media.rs`). WinRT calls require COM
initialization on the calling thread, so both the polling loop and the
transport commands run on one dedicated thread, with commands routed to
it over an `mpsc` channel.

### Known limitations

- **Win+D ("show desktop") minimizes the widget.** A trade-off of not
  using `SetParent` — like any normal app window, it isn't exempt from
  Win+D. Fixable with a `WM_WINDOWPOSCHANGING` subclass hook that
  intercepts the move request; not implemented.
- **No GPU metric.** The `sysinfo` crate doesn't expose GPU usage — only
  CPU, memory, and network are covered. Would need PDH's `GPU Engine`
  performance counters or a vendor-specific SDK (e.g. NVML).
- **Multi-monitor targeting is "rightmost of all connected monitors."**
  Centralized in `select_target_monitor` in `src-tauri/src/lib.rs`, so
  switching to "primary monitor" or a specific index is a one-function
  change.
- **Card positions persist in the WebView's `localStorage`,** tied to the
  app's user-data directory — clearing the WebView profile loses them.
  Swapping this for a config file later only means changing
  `src/core/layout.js`.
