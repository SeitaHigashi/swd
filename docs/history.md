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

## 2026-08-20 — Frontend refactored into a plugin system (stage 1)

**Goal:** make cards extensible without touching core files, while keeping
the built-in cards (clock, system monitor, network, media) as first-class
citizens rather than second-class "example plugins."

### What changed

`src/modules/*.js` (one file per card, each hand-wired into `main.js`) was
replaced with `src/core/*` (generic plugin host) + `src/plugins/*/plugin.js`
(one directory per card, each exporting `{ id, position, styles, mount(ctx)
}`). `index.html` no longer contains any card markup — `core/plugin-host.js`
creates the `.glass-card` element and injects the plugin's stylesheet at
mount time. `main.js` shrank to: ask `core/loader.js` for the plugin list,
mount each one, start the hit-region watcher.

Drag handling, position persistence (`core/layout.js`, unchanged in
behavior from the old `modules/layout.js`), and hit-region sync remained
generic over `.glass-card` — same as before, just relocated.

### Why an event bus instead of `listen()` per plugin

The old code had `main.js` call `listen("sys://stats", ...)` once and fan
the payload out to `system-monitor.js` and `network.js` by hand. Once cards
are independent plugins, nothing else can do that fan-out for them. Rather
than have every plugin subscribing to `sys://stats` open its own Tauri IPC
listener (redundant, and no clean way to unsubscribe on unmount),
`core/event-bus.js` opens exactly one `listen()` per event name and
dispatches to all subscribers. It also caches the last payload per event
and replays it to a plugin that mounts between two ticks, so a
dynamically-added card (stage 2) doesn't sit at "--" for a full sample
interval.

### Why hit-region sync moved from `resize`-only to a `MutationObserver`

The old `syncHitRegions()` was called after the fixed set of cards in
`index.html` had already loaded, then re-run on window `resize` and a
`ResizeObserver` on `<body>`. That's fine when the card set never changes
after startup. Once cards can be mounted/unmounted at runtime (stage 2:
externally-installed plugins, or any future "disable a card" UI), the host
needs to notice `.glass-card` elements appearing or disappearing from the
DOM, not just resizing — hence `core/hit-regions.js` now also watches
`.dashboard` with a `MutationObserver(childList)`.

### Why `ctx.invoke` is permission-scoped per plugin, not a passthrough

`withGlobalTauri: true` means any script in the WebView can already reach
`window.__TAURI__` directly, so `ctx.invoke`'s allowlist (declared as
`permissions.invoke` in each plugin's manifest) is not a real security
boundary against a malicious plugin — a plugin that wants to bypass it can
just call `window.__TAURI__.core.invoke` itself. It exists to catch
*accidents*: a typo'd command name, or a plugin calling a command it forgot
to declare, fails loudly instead of silently doing something the plugin
author didn't intend. `set_hit_regions` is hard-blocked in the host
regardless of what a plugin declares, since letting a card claim its own
hit-test rectangle would let it grab mouse input outside its own bounds.

### Deliberately deferred: loading plugins from outside the repo

`core/loader.js` currently returns a hardcoded list of the four built-in
plugins (statically `import`ed, so no bundler/dynamic-loading concerns).
Loading a plugin from e.g. `%APPDATA%\dev.seita.swd\plugins\` needs a
Rust-side directory listing command and a custom URI scheme protocol to
serve the plugin's JS/CSS to the WebView (a bare filesystem path won't
resolve, and `frontendDist` is a fixed static directory) — deliberately
left for a later change once the plugin contract above has proven itself
against a few more built-in cards.

## 2026-08-20 — External plugin loading (stage 2)

**Goal:** let a plugin live outside the repo entirely — dropped into a
directory on disk — and be discovered and mounted the same way a built-in
one is, closing out the "deliberately deferred" item above.

### What changed

Added `src-tauri/src/plugins.rs`: a `list_plugins` command that scans
`<app data dir>/plugins/*/plugin.json` (skipping any entry with a missing
or malformed manifest, or whose `id` doesn't match its own directory name,
rather than failing the whole scan), plus `resolve_plugin_file` which maps
`swd-plugin://<id>/<path>` to an absolute path *inside that plugin's own
directory only* — canonicalizes and checks `starts_with` the plugin root
so a `../` in a manifest or import can't read another plugin's files or
anything else on disk.

`lib.rs` registers `swd-plugin` via `register_uri_scheme_protocol` to
actually serve those files (content-type guessed from extension), and
`core/loader.js` calls `list_plugins`, then `import()`s each manifest's
`entry` file over that protocol and merges the result into the same list
`mountPlugin` already knew how to handle for built-ins — no changes needed
in `plugin-host.js` at all, confirming the stage-1 contract was the right
shape.

An external plugin directory looks like:

```
<app data dir>/plugins/<id>/
├── plugin.json   # { "id": "<id>", "name": "...", "entry": "index.js" }
├── index.js      # default-exports the same { id, position, styles,
│                 #   permissions, mount(ctx) } shape as a built-in plugin
└── style.css      # (or whatever plugin.js's `styles` array references)
```

`<app data dir>` is `%APPDATA%\dev.seita.swd\` on Windows
(`tauri::path::PathResolver::app_data_dir`).

### The bug that ate most of this session: missing CORS header

The custom protocol worked (files served, correct bytes) but the frontend's
`import()` of `swd-plugin://.../index.js` still silently rejected. Root
cause: the main page's origin is `http://tauri.localhost`, which differs
from a registered custom protocol's origin — per Tauri's own docs for
`register_uri_scheme_protocol`, this makes any `fetch`/dynamic `import()`
of it a cross-origin request, and the WebView drops the response after
receiving it unless the response carries `Access-Control-Allow-Origin`.
The request reached the Rust handler and got a 200 either way, which is
why watching for the request to arrive (easy to check by logging in the
protocol handler) gave a false sense that it was working — the failure
happens client-side, after the response comes back. Fixed by adding
`Access-Control-Allow-Origin: *` to every response from the handler; safe
here since the protocol only ever serves files the user chose to put in
their own plugins directory, nothing sensitive.

Two things made this slow to pin down and are worth remembering for next
time:

- **Debugging a frameless, `focus: false`, click-through-except-cards
  window is hard** — right-click-to-inspect doesn't reach most of the
  window, and there's no titlebar to grab for a devtools shortcut. The
  workaround was a temporary `debug_log` Tauri command that `eprintln!`s
  whatever string the frontend hands it, called from `window.onerror`,
  `unhandledrejection`, and the plugin-host's catch blocks — piping
  browser-side errors into the same terminal already showing the Rust
  logs. Removed once the fix was confirmed; recreate it the same way if
  this needs debugging again rather than fighting the window chrome.
- **A visual "it's not showing up" symptom had a second, unrelated cause
  layered on top**: even after the CORS fix, a screenshot still didn't
  show the card, because the test plugin's position happened to sit
  directly under another already-open foreground window (Spotify). This
  desktop-pinned window is *deliberately* layered below normal app
  windows (see the very first history entry above), so anything on top of
  it will occlude the widget - expected behavior, not a regression. Cost
  real time before being recognized as "logs already proved this works,
  the screenshot is just looking at the wrong thing." When a card's logs
  say it mounted successfully but a screenshot disagrees, check for
  window occlusion before doubting the logs.

## 2026-08-20 — Generic outbound HTTP for plugins (added for a Nature Remo plugin)

**Goal:** let a plugin (specifically: a Nature Remo smart-home control card,
built as an external plugin per stage 2) call a cloud API from the
frontend, without every such plugin having to write its own Rust command.

### Why a WebView `fetch()` wasn't enough

The Nature Remo Cloud API (`api.nature.global`) doesn't send CORS headers.
A plain `fetch()` from the WebView is subject to the same cross-origin
rules a real browser enforces, so - same failure mode as the `swd-plugin://`
CORS issue in the stage-2 entry above, but this time on a server we don't
control and can't add headers to. Two options: write a Rust proxy command
per external API a plugin wants to call, or add one generic capability
that lets *any* plugin make outbound HTTP requests without a browser's
CORS checks applying at all.

Went with the generic option: added the official `tauri-plugin-http`
(registered in `lib.rs`), which implements `fetch()` as a Tauri command
under the hood - the request happens natively in Rust and the response is
handed back to the WebView, so there's no Origin/CORS concept in the way
to begin with. Because `withGlobalTauri: true` is already set, this
automatically becomes available to every plugin (built-in or external) as
`window.__TAURI__.http.fetch`, no wiring needed per plugin.

### Scoped to one domain, not wide open

`capabilities/default.json` grants `http:default` with
`"allow": [{ "url": "https://api.nature.global/*" }]` rather than an
unrestricted allow-list. This is the one place external-plugin work
*does* touch the repo even though the plugin itself lives entirely under
`%APPDATA%\dev.seita.swd\plugins\`: a plugin that needs to call a new
outbound domain needs that domain added to this scope first. Worth
remembering when the next external plugin needs its own API - extend the
`allow` array, don't loosen it to `*`.

The nature-remo plugin itself (`plugin.json` + `index.js` + `style.css`
under the plugins directory, not in this repo) shows a token-entry form on
first run, stores the token in its own `ctx.storage` namespace, and polls
`/1/devices` + `/1/appliances` every 5 minutes; it sends aircon on/off via
`/1/appliances/{id}/aircon_settings` and other appliances' registered IR
signals via `/1/signals/{id}/send`.

## 2026-08-20 — System tray icon

**Goal:** a way to quit or hide the widget without Task Manager. The main
window has `decorations: false` and `skipTaskbar: true` (see "why
right-anchored" era decisions in the first history entry), so there's
never been a titlebar close button or a taskbar entry to right-click.

### What changed

Added `src-tauri/src/tray.rs`, using Tauri's built-in tray APIs (enabled
via the `tray-icon` Cargo feature on the `tauri` dependency - no extra
plugin needed, unlike `tauri-plugin-http` earlier). Built in `setup()`
right after the window is positioned: a two-item context menu (表示/非表示
to toggle the window, 終了 to quit) shown on left or right click, using
`app.default_window_icon()` so it doesn't need a separate icon asset -
same `.ico` already configured in `tauri.conf.json`'s `bundle.icon` for
the taskbar/installer.

### Verifying it worked without a visible taskbar tray

Windows puts a newly-registered tray icon in the hidden/overflow area by
default (behind the `^` chevron) unless the user drags it out - expected,
not a bug. Confirmed the icon actually registered by checking
`HKCU\Control Panel\NotifyIconSettings` for an entry whose
`ExecutablePath` pointed at `swd.exe`, rather than fighting to get a
screenshot of the overflow flyout (which auto-dismisses easily and isn't
straightforward to keep open for a screenshot via automation).

## 2026-08-20 — Fixed the real cause of cards "disappearing": drag saved on any click

**Symptom, recurring:** a card (repeatedly nature-remo-card, the tallest
and most content-dense one) would go missing after a reinstall or after
normal use, even though its plugin default position no longer collided
with anything. Each time, the proximate cause traced back to a saved
`swd:card-position:*` localStorage entry - clearing it fixed the symptom,
but it kept coming back.

**Root cause, finally:** `core/drag.js`'s `pointerdown` handler
unconditionally set `card.style.right = "auto"` and its `pointerup`
handler unconditionally called `saveCardPosition`, with no check for
whether the pointer had actually moved in between. A plain click - on the
card's title, a text row, empty space between buttons, anywhere that
isn't an excluded control element - has a `pointerdown`/`pointerup` pair
with zero or near-zero movement, and was being saved as if it were an
intentional drag to that exact (already-current) position. Harmless in
isolation, except: the saved position is a *snapshot* of
`getBoundingClientRect()` at that instant, and if the card's layout
hadn't fully settled yet (still loading, or content still populating),
that snapshot could freeze in a stale/incorrect spot - and once saved, it
permanently overrides the plugin's own default position on every future
launch, including after any future code fix to that default.

The more content a card has (more appliance rows, more links, more
whitespace between buttons), the more surface area for an incidental
click to trigger this - which is exactly why nature-remo-card kept being
the one affected once its appliance list grew.

**Fix:** added a `DRAG_THRESHOLD_PX` (4px) in `drag.js` - `pointermove`
only treats the gesture as a real drag (switching off right-anchoring,
updating position) once the pointer has moved past that threshold from
`pointerdown`; `pointerup` only saves if that threshold was crossed. A
plain click no longer touches saved position state at all.
