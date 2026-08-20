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

## Architecture

```
src-tauri/src/
├── lib.rs          # setup: window sizing/position, module wiring
├── window_layer.rs # WorkerW z-order pinning (Windows only)
├── hit_test.rs      # click-through region polling (Windows only)
├── system_info.rs  # CPU/memory/network sampling → `sys://stats` event
└── media.rs          # Windows Media Transport Controls → `media://now-playing` (Windows only)

src/
├── index.html
├── style.css        # glass-morphism base styles + one CSS rule per card's default position
├── main.js           # drag handling, hit-region sync, event wiring
└── modules/
    ├── clock.js
    ├── system-monitor.js  # CPU/mem card, uses sparkline.js
    ├── network.js          # network card, uses sparkline.js
    ├── media.js             # now-playing card
    ├── sparkline.js          # shared canvas line-graph renderer
    └── layout.js              # localStorage position persistence
```

### Adding a new card

1. Add a `<section class="glass-card" id="my-card" data-card>` to
   `index.html`.
2. Give it a default position in `style.css` (`#my-card { top: …; right: …
   }` — anchor to the right edge like the others, see "why right-anchored"
   below).
3. Add `src/modules/my-card.js` exporting an `init...()` that returns an
   `onStats`-style callback, following the pattern in `network.js`.
4. Wire it into `main.js`: import, call `init...()` in the
   `DOMContentLoaded` handler, subscribe to whatever event it needs.

Drag handling, click-through hit-testing, and position persistence are all
generic over `.glass-card` + a unique `id` — you get them for free, no
per-card code needed.

If the card needs new data from Rust: prefer emitting a Tauri event from a
background thread (like `system_info.rs` and `media.rs` do) over a
request/response command, unless the frontend is asking for something
one-shot (like `media.rs`'s transport controls, which are plain commands).

## Gotchas (things that will bite you if you don't know them)

- **No bundler in `src/`.** Bare package imports
  (`import x from "@tauri-apps/api/..."`) silently fail to resolve in the
  WebView and kill the entire module script with no visible error unless
  you have devtools open. Use `window.__TAURI__.*` (enabled via
  `withGlobalTauri: true` in `tauri.conf.json`) instead. Plain relative
  imports between files in `src/modules/` are fine.
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
