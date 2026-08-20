# Writing a Plugin

A plugin is one card on the dashboard. This doc covers how to write one,
whether it lives in this repo (a **built-in** plugin) or on disk outside
it (an **external** plugin). See `CLAUDE.md`'s "Installing an external
plugin" section for the short version; this is the long version plus the
`ctx` API reference.

## Built-in vs. external

|                    | Built-in                          | External                                    |
| ------------------ | ---------------------------------- | -------------------------------------------- |
| Lives in            | `src/plugins/<id>/`                | `%APPDATA%\dev.seita.swd\plugins\<id>\`      |
| Registered in       | `src/core/loader.js`'s `BUILT_IN_PLUGINS` array | Discovered automatically via `list_plugins` |
| Ships via            | Committed to the repo, built into the installer | Copied onto the machine directly, no reinstall needed |
| Use for              | Cards that are part of SWD itself (clock, system monitor, network, media) | Anything personal/optional (smart-home controls, a stock ticker, whatever) |

Both use the **exact same plugin contract** below — `core/plugin-host.js`
doesn't know or care which kind it's mounting. Start external unless
you're specifically contributing a card back to SWD itself; it's zero
build/install friction to iterate on.

## Quick start (external plugin)

```
%APPDATA%\dev.seita.swd\plugins\my-plugin\
├── plugin.json
├── index.js
└── style.css        (optional, referenced from index.js if used)
```

**`plugin.json`** — just enough for the Rust side to discover the file:

```json
{
  "id": "my-plugin",
  "name": "My Plugin",
  "entry": "index.js"
}
```

`id` **must exactly match the directory name** — a mismatch gets the
plugin silently skipped (see `src-tauri/src/plugins.rs`). `entry` is the
JS file to load, relative to this directory.

**`index.js`** — the actual card:

```js
export default {
  id: "my-plugin-card",       // becomes the card's DOM id - keep it
                               // distinct from the manifest's `id` above
                               // if you like, they don't have to match
  name: "My Plugin",
  position: { top: 32, right: 750 },
  styles: ["./style.css"],
  mount(ctx) {
    ctx.root.innerHTML = `<h2>My Plugin</h2><p>Hello!</p>`;
  },
};
```

Restart SWD. `core/loader.js` calls `list_plugins` on startup and
`import()`s this file over a `swd-plugin://` protocol registered in
`src-tauri/src/lib.rs`.

## The default export

```js
export default {
  id: "unique-card-id",       // required. Also the localStorage key for
                               // this card's dragged position.
  name: "Display Name",       // required. Not shown in the UI yet, but
                               // kept for a future plugin-list view.
  position: { top, right },   // required-ish. CSS pixel offsets from the
                               // dashboard's top/right edge - see
                               // "Position and dragging" below. Omit a
                               // field to leave that axis unset.
  styles: ["./style.css"],    // optional. Paths resolved relative to
                               // this file (via import.meta.url under
                               // the hood), injected as <link> tags once.
  permissions: {
    invoke: ["some_command"], // optional. Tauri commands this plugin is
  },                           // allowed to call via ctx.invoke - see
                               // "Permissions" below.
  mount(ctx) { /* ... */ },   // required. Build the card's contents.
  unmount(ctx) { /* ... */ }, // optional. Extra cleanup beyond what the
                               // host already undoes automatically.
};
```

`core/plugin-host.js` (`mountPlugin`) does everything generic: creates the
`.glass-card` `<section>`, sets `id`/position from the fields above, wires
up dragging, restores a previously-saved drag position, injects your
stylesheets, and calls `mount(ctx)`. You only ever touch `ctx`.

## The `ctx` object

Passed to both `mount(ctx)` and `unmount(ctx)`.

### `ctx.root`

The card's own `<section class="glass-card">` element. Build your UI
inside it however you like - `innerHTML`, DOM APIs, whatever:

```js
mount(ctx) {
  ctx.root.innerHTML = `<h2>My Plugin</h2><div class="body"></div>`;
}
```

### `ctx.el(selector)`

`querySelector` scoped to this card only - never accidentally reaches
into another plugin's markup:

```js
const body = ctx.el(".body");
```

### `ctx.on(eventName, handler)`

Subscribes to a Tauri event, routed through a shared event bus
(`core/event-bus.js`) so N plugins listening to the same event only cost
one underlying `listen()` call. Returns an unsubscribe function, but you
don't need to call it yourself - the host unsubscribes automatically on
unmount.

```js
ctx.on("sys://stats", (stats) => {
  cpuLabel.textContent = `${stats.cpu_percent.toFixed(0)}%`;
});
```

A plugin mounted after an event has already fired once gets the **last
payload replayed immediately** - you don't sit at placeholder text for a
full sample interval just because your card mounted between two ticks.

**Events already available:**

| Event               | Payload shape                                                                                          | Emitted by |
| -------------------- | -------------------------------------------------------------------------------------------------------- | ---------- |
| `sys://stats`         | `{ cpu_percent, mem_used_bytes, mem_total_bytes, mem_percent, net_rx_bytes_per_sec, net_tx_bytes_per_sec }` (all numbers) | `system_info.rs`, every ~1.5s |
| `media://now-playing` | `{ title, artist, status, thumbnail_data_url }` (status is `"playing"`/`"paused"`/etc; thumbnail is a data: URL or `null`) | `media.rs`, Windows only |

If you need new backend data, prefer adding a new Tauri event over a
request/response command (see `system_info.rs`/`media.rs` for the
pattern) unless it's a one-shot action, not a data feed.

### `ctx.invoke(command, args)`

Calls a Tauri command, but only if it's listed in this plugin's own
`permissions.invoke` array. This is **not a real security boundary** -
`withGlobalTauri: true` means any plugin can already reach
`window.__TAURI__.core.invoke` directly and bypass it - it exists purely
to catch accidents (a typo'd command name, forgetting to declare one you
meant to use) with a clear error instead of silent wrong behavior.

`set_hit_regions` is hard-blocked for every plugin regardless of what it
declares - letting a card claim its own hit-test rectangle would let it
grab mouse input outside its own bounds.

### `ctx.storage`

A namespaced `localStorage` wrapper - keys are automatically prefixed
with `swd:plugin-data:<your-id>:`, so plugins can't collide with or read
each other's data:

```js
ctx.storage.set("token", value);      // JSON.stringify'd under the hood
const token = ctx.storage.get("token"); // JSON.parse'd back, or null
ctx.storage.remove("token");
```

This is plain-text `localStorage`, not an OS credential vault. Fine for a
personal single-user desktop widget; don't treat it as secure storage for
anything more sensitive than what you'd already accept living on this
machine.

### `ctx.onDestroy(fn)`

Registers a cleanup callback run when the card is unmounted (not
currently triggered by anything in the running app, but exists for a
future "disable a plugin" UI, and matters if you register your own timers
or listeners outside of `ctx.on`):

```js
mount(ctx) {
  const intervalId = setInterval(refresh, 60_000);
  ctx.onDestroy(() => clearInterval(intervalId));
}
```

## Styling

Every stylesheet you list gets injected globally into the page, so **scope
every rule under your card's `id`** or it'll leak into other cards:

```css
/* good */
#my-plugin-card .body { font-size: 13px; }

/* bad - leaks into every card that also has a ".body" */
.body { font-size: 13px; }
```

Reuse the base design tokens already defined on `:root` in `src/style.css`
rather than hardcoding colors, so your card matches the others:

```css
#my-plugin-card .value {
  color: var(--swd-accent);        /* accent blue */
}
#my-plugin-card .warn {
  color: var(--swd-accent-warn);   /* warm orange */
}
```

(`--swd-text`, `--swd-text-dim`, `--swd-card-bg`, `--swd-card-border`,
`--swd-meter-track` are also available.) The base `.glass-card` class
(background blur, border, padding, width) and `.glass-card h2` (section
heading style) are already applied - you don't need to redeclare them.

## Position and dragging

`position: { top, right }` (or `left` instead of `right`) is only the
**default** - dragging a card by anything that isn't a
button/input/a/select/textarea switches it to an explicit `left`/`top`
and remembers that in `ctx.storage`-adjacent localStorage
(`swd:card-position:<id>`) permanently, overriding the default on every
future launch.

Card height is **not fixed** - it grows with your content. Two
consequences:

- **Don't stack a new card directly below another one at a guessed pixel
  offset.** If either card's content grows, they'll start overlapping,
  and the overlap can fully hide the lower card with no visible error.
  Prefer putting a new plugin in its **own column** (a very different
  `right` value from the built-in cards' shared `right: 32` column) so
  vertical growth in one column can't collide with another.
- A card that's fully hidden behind another one is still clickable-area
  live underneath it - a user innocently clicking near/through where they
  think a control is can trigger an accidental micro-drag, which
  permanently pins the hidden card to a bad position. (This used to be a
  real bug - `core/drag.js` requires the pointer to move past a small
  threshold before anything counts as a drag - but a bad default position
  is still worth avoiding on its own merits.)

## Calling an outbound HTTP API

Use `window.__TAURI__.http.fetch` (from `tauri-plugin-http`), **not** the
WebView's native `fetch()`. A plain `fetch()` is subject to the same CORS
rules a real browser enforces - most cloud APIs don't send CORS headers
(they're not designed to be called from a browser), so a native `fetch()`
fails silently after the request has already gone out. The Tauri HTTP
plugin makes the request natively in Rust and hands back the response,
sidestepping CORS entirely.

```js
const { fetch } = window.__TAURI__.http;
const res = await fetch("https://api.example.com/data", {
  headers: { Authorization: `Bearer ${token}` },
});
```

`capabilities/default.json`'s `http:default` permission is scoped to
`{ "url": "*" }` (any domain, user-requested trade-off - see
`docs/history.md`), so no repo change is needed to call a new API from a
plugin. If that scope is ever narrowed back down, calling a new domain
would need an entry added there and a rebuild + reinstall (see
`CLAUDE.md`). See `plugins/nature-remo/index.js` (external, not in this
repo, but described in `docs/history.md`) for a full real-world example
of the outbound-HTTP pattern itself.

## Permissions

```js
permissions: {
  invoke: ["media_toggle_play_pause"],
}
```

Only needed if you call `ctx.invoke(...)`. Omit entirely if your plugin
only uses `ctx.on`, `ctx.storage`, and/or `window.__TAURI__.http.fetch`.

## Testing your plugin

- **Built-in**: add the import + array entry in `src/core/loader.js`, then
  `npm run dev` - hot-restarts on `src-tauri/` changes; frontend files
  under `src/` are served live from disk with no build step.
- **External**: drop the files under
  `%APPDATA%\dev.seita.swd\plugins\<id>\` and restart SWD (`npm run dev`
  or the installed exe both work - it's the same discovery path either
  way). No rebuild needed for JS/CSS-only changes.

There's no devtools-friendly way to inspect this window (frameless,
mostly click-through, transparent) - if something isn't rendering and you
can't tell why, a quick temporary technique that's worked well:

1. Add a `debug_log` Tauri command that appends a string to a plain file
   (`eprintln!` alone goes nowhere in a release build - it's a
   GUI-subsystem app with no attached console; a file works in both dev
   and release).
2. Call it from `window.onerror`, `window.onunhandledrejection`, and your
   plugin's own catch blocks.
3. Remove all of it once you've found the issue - it's diagnostic
   scaffolding, not something to ship.

See `docs/history.md`'s entries under "External plugin loading (stage 2)"
and later for real examples of this in action, including a CORS bug and a
first-launch race condition it caught.

## If you're working with an AI coding agent in a sandboxed environment

Worth knowing if you (or whoever's helping you) are using something like
Claude Code to write or edit a plugin: some agent environments run inside
an OS-level sandbox (Windows AppContainer, in Claude Code's case) that
transparently redirects reads/writes under paths like `%APPDATA%\Roaming`
to a private virtualized copy - the agent's own file-existence checks,
`Test-Path` calls, even a test run of the built exe **can all report
success while writing to a copy the real, normally-launched app never
sees**. This is exactly what happened once already (see `docs/history.md`
under "再インストールもしました" for the full story) - hours of "it works
when I test it, but not for you" before the actual cause turned up.

The reliable fix: have the agent hand you the plugin files as an actual
file transfer (a zip, not just "I wrote it to X path") and place them
under `%APPDATA%\dev.seita.swd\plugins\` **yourself**, in your own normal
session - never trust "I verified the path exists" from inside a
sandboxed tool for anything under a redirected known folder.
