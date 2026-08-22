# CLAUDE.md

Guidance for Claude Code (or any coding agent) working in this repository.

## What this is

SWD (Seita Windows Dashboard): a Windows 11 always-on desktop widget built
with Tauri v2. It renders a transparent, glass-morphism dashboard (clock,
CPU/memory, network, now-playing media) pinned above the desktop wallpaper
(Wallpaper Engine) and below the desktop icons, using a self-implemented
Win32 WorkerW z-order trick rather than any existing "desktop wallpaper
window" plugin. See [docs/history.md](docs/history.md) for the full
reasoning behind that choice and the bugs hit along the way — read it
before touching `window_layer.rs` or `hit_test.rs`, it will save you from
re-making mistakes already made and fixed once.

**Windows-only.** Everything platform-specific is behind
`#[cfg(target_os = "windows")]` module gates in `src-tauri/src/lib.rs`, so
the crate still compiles (with reduced functionality) on other platforms,
but it has only ever been run on Windows 11.

## Build / run

```bash
npm install
npm run dev     # tauri dev — auto-recompiles on src-tauri/ changes
npm run build   # tauri build — release bundle
```

`npm run dev` restarts the whole app on any `src-tauri/` change (there's no
hot-reload for Rust). Frontend files under `src/` are served as-is with no
bundler — see the "no bundler" gotcha below before editing `src/main.js` or
any new frontend module.

### Delivering a change to the installed app

The user runs an installed build (`%LOCALAPPDATA%\swd\swd.exe`, launched
via autostart), not `npm run dev`'s debug build — it only picks up
`src-tauri/` or `src/` changes when reinstalled. **After finishing a unit
of work that touches `src-tauri/` or `src/` (a feature, a fix — not every
intermediate edit), rebuild and hand over a fresh installer as part of
calling that work done**, rather than leaving the user on a stale build:

```bash
npm run build   # produces both installers under src-tauri/target/release/bundle/
```

Send both `bundle/nsis/swd_<version>_x64-setup.exe` and
`bundle/msi/swd_<version>_x64_en-US.msi` to the user. Remind them to close
the running instance first — Windows installers can't overwrite a running
exe.

Exception: a fix that lives entirely in an *external* plugin under
`%APPDATA%\dev.seita.swd\plugins\<id>\` (see "Installing an external
plugin" below) never touches this repo, so no rebuild or reinstall is
needed — editing that plugin's files and restarting the app is enough.

## Architecture

```
src-tauri/src/
├── lib.rs          # setup: window sizing/position, module wiring, swd-plugin:// protocol
├── window_layer.rs # WorkerW z-order pinning (Windows only)
├── hit_test.rs      # click-through region polling (Windows only)
├── system_info.rs  # CPU/memory/network sampling → `sys://stats` event
├── media.rs          # Windows Media Transport Controls → `media://now-playing` (Windows only)
├── plugins.rs          # external plugin discovery (list_plugins) + safe file resolution
├── settings.rs           # per-plugin enabled/config persistence + settings window
└── tray.rs                 # system tray icon + context menu (show/hide, settings, quit)

src/
├── index.html         # empty <div class="dashboard"> — cards are mounted at runtime
├── settings.html        # settings window shell (tray → Settings...)
├── style.css          # glass-morphism base styles only, no per-card rules
├── main.js            # bootstrap: loadAllPlugins() → filter by settings → mountPlugin()
├── settings/
│   ├── main.js           # renders every plugin's on/off toggle + configSchema form
│   └── style.css           # normal opaque window styling, not glass-morphism
├── core/               # generic plugin host, not card-specific
│   ├── plugin-host.js  # mountPlugin(): builds the .glass-card element, wires ctx (incl. ctx.config)
│   ├── loader.js         # discovers plugins (built-ins + external) and their saved settings
│   ├── event-bus.js       # one listen() per Tauri event, fanned out to subscribers
│   ├── drag.js              # pointer-drag handling
│   ├── hit-regions.js         # click-through region sync, MutationObserver-driven
│   └── layout.js                # localStorage position persistence
├── shared/
│   └── sparkline.js    # shared canvas line-graph renderer
└── plugins/
    ├── clock/{plugin.js, style.css}
    ├── system-monitor/{plugin.js, style.css}  # CPU/mem card
    ├── network/{plugin.js, style.css}          # uses shared/sparkline.js
    └── media/{plugin.js, style.css}             # now-playing card
```

### Per-widget on/off + settings

Right-click the tray icon → **Settings...** opens a separate, normal
(decorated, taskbar) window listing every plugin — built-in and external
— with an enable/disable toggle and, if the plugin declares a
`configSchema`, a generic settings form for it. See
[docs/plugin-authoring.md](docs/plugin-authoring.md)'s "Configurable
settings" section for the plugin-authoring side of this, and
`src-tauri/src/settings.rs` for how state is persisted
(`<app config dir>/settings.json`) and broadcast live to the dashboard via
a `settings://changed` event.

### Adding a new card

Each card is a self-contained plugin — see `docs/history.md` under
"Frontend refactored into a plugin system" for why it's shaped this way
before changing `core/*`.

1. Create `src/plugins/my-card/plugin.js` exporting a default object:
   `{ id, name, position: { top, right }, styles: ["./style.css"],
   permissions: { invoke: [...] }, mount(ctx) { ... } }`. `permissions` is
   only needed if the plugin calls `ctx.invoke(...)`.
2. Create `src/plugins/my-card/style.css`, scoping every rule under
   `#my-card` (the plugin's `id`) so it can't leak into other cards. Anchor
   the default `position` to the right edge like the others — see "why
   right-anchored" below.
3. In `mount(ctx)`, build the card's contents with `ctx.root.innerHTML =`
   (or DOM APIs), read elements back with `ctx.el(selector)` (scoped to
   this card only), and subscribe to data with `ctx.on("sys://stats", cb)`.
4. Register it in `src/core/loader.js`'s `BUILT_IN_PLUGINS` list (import +
   one array entry). Nothing in `main.js` needs to change.

Drag handling, click-through hit-testing, and position persistence are all
generic over `.glass-card` — handled by `core/plugin-host.js` — so you get
them for free, no per-card code needed.

If the card needs new data from Rust: prefer emitting a Tauri event from a
background thread (like `system_info.rs` and `media.rs` do) over a
request/response command, unless the frontend is asking for something
one-shot (like `media.rs`'s transport controls, which are plain commands).

### Installing an external plugin

See [docs/plugin-authoring.md](docs/plugin-authoring.md) for the full
guide to writing a plugin (the `ctx` API, styling, events, outbound HTTP,
positioning pitfalls). Short version below.

A plugin doesn't have to live in this repo. Drop a directory at
`%APPDATA%\dev.seita.swd\plugins\<id>\` (the same `<id>` used both as the
directory name and inside the manifest — a mismatch gets the plugin
skipped, see `plugins.rs`):

```
plugins/<id>/
├── plugin.json   # { "id": "<id>", "name": "...", "entry": "index.js" }
├── index.js      # default export: same { id, position, styles,
│                 #   permissions, mount(ctx) } shape as a built-in plugin
└── style.css      # whatever plugin.json's `entry` module references
```

Restart the app (or wherever a future "reload plugins" affordance lands);
`core/loader.js` calls `list_plugins` on startup and `import()`s each
manifest's `entry` file over the `swd-plugin://<id>/<path>` protocol
registered in `lib.rs`. See `docs/history.md` under "External plugin
loading (stage 2)" for why that protocol's responses need a CORS header
and how path traversal is blocked.

If the plugin needs to call an outbound HTTPS API from the frontend, use
`window.__TAURI__.http.fetch` (from `tauri-plugin-http`, registered in
`lib.rs`) instead of the WebView's native `fetch()` - a plain `fetch()` is
subject to the same CORS rules a browser enforces, so it silently fails
against any API that doesn't send CORS headers (most cloud APIs don't,
since they're not designed to be called from a browser). The Rust-side
plugin makes the request natively, sidestepping CORS entirely.
`http:default`'s `allow` list in `capabilities/default.json` is scoped to
`{ "url": "*://*/*" }` (user-requested; originally per-domain, see
`docs/history.md` under "Generic outbound HTTP for plugins" and its
follow-up for the reasoning both ways), so no capability change is
needed to call a new domain.

## Gotchas (things that will bite you if you don't know them)

- **No bundler in `src/`.** Bare package imports
  (`import x from "@tauri-apps/api/..."`) silently fail to resolve in the
  WebView and kill the entire module script with no visible error unless
  you have devtools open. Use `window.__TAURI__.*` (enabled via
  `withGlobalTauri: true` in `tauri.conf.json`) instead. Plain relative
  imports between files under `src/core/`, `src/shared/`, and
  `src/plugins/` are fine.
- **`SetWindowPos`'s `hWndInsertAfter` places the window *behind* (below)
  the reference window**, not in front of it. Getting this backwards is
  the exact bug that shipped once already (see history.md) — the symptom
  was desktop icons rendering *under* the widget instead of on top.
- **Never `SetParent` the main window into a WorkerW.** It looks like the
  "more correct" way to pin behind desktop icons, and it's what
  `tauri-plugin-desktop-underlay` does, but it makes Windows treat the
  window as desktop-owned and disables all mouse/keyboard input to it.
  The whole point of the z-order-only approach in `window_layer.rs` is to
  keep the window interactive.
- **Don't toggle `ignore_cursor_events` from the webview's own
  `mousemove`.** Once a window ignores the cursor it stops receiving mouse
  events entirely, including the one that would tell it to stop ignoring —
  that's a deadlock, not a bug to patch around. `hit_test.rs` avoids it by
  polling `GetCursorPos` from an independent Rust thread instead.
- **WinRT (used by `media.rs`) requires COM initialized on the calling
  thread.** Don't call WinRT APIs from an arbitrary Tauri command handler —
  apartment state on Tauri's async command threads isn't guaranteed
  stable. `media.rs` runs everything (polling *and* transport commands,
  routed over an `mpsc` channel) on one dedicated thread that calls
  `CoInitializeEx` once at startup.
- **Card default positions are right-anchored** (`right: 32px`, not
  `left`) because the dashboard is meant to sit on the right side of
  whichever monitor `select_target_monitor()` (in `lib.rs`) picks —
  currently the rightmost connected monitor. If you change that function,
  double check whether right-anchoring still makes sense.
- **A full disk produces a confusing Rust build error.** `os error 112`
  ("not enough space on disk") looks like a linker/codegen problem at
  first glance; it isn't. Check `df -h` before debugging further if a
  build fails inexplicably on a machine that was working before.

## Known limitations (not bugs, just unfinished)

- Win+D ("show desktop") minimizes the widget like a normal window, since
  it isn't actually desktop-owned. Fix would be a `WM_WINDOWPOSCHANGING`
  subclass hook; not implemented.
- No GPU usage metric (`sysinfo` doesn't expose one; would need PDH's
  `GPU Engine` counters or a vendor SDK).

Full narrative history, including things that were tried and rejected, is
in [docs/history.md](docs/history.md) — append to it (don't rewrite past
entries) when you make another non-obvious call future-you will want the
reasoning for.
