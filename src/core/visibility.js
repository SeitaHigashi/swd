// Tracks whether the window is currently worth doing per-tick work for.
//
// `document.visibilityState` already covers both cases we care about on
// Windows without any Rust-side plumbing:
//   - tray.rs's `window.hide()` (Win32 SW_HIDE) makes the WebView report
//     hidden immediately.
//   - WebView2 is Chromium-based, and Chromium enables native window
//     occlusion tracking on Windows by default - when another window's
//     opaque rect fully covers this one, the OS tells Chromium and it
//     flips `document.visibilityState` to "hidden" the same way a
//     minimized or backgrounded tab would, then back to "visible" the
//     moment any part becomes uncovered again. No extra wiring needed.
//
// Event-driven work (event-bus, hit-region sync) should skip its per-tick
// cost while hidden and catch up once instead of trickling in late.

const listeners = new Set();

export function isHidden() {
  return document.hidden;
}

/**
 * @param {(hidden: boolean) => void} handler
 * @returns {() => void} unsubscribe
 */
export function onVisibilityChange(handler) {
  listeners.add(handler);
  return () => listeners.delete(handler);
}

document.addEventListener("visibilitychange", () => {
  for (const handler of listeners) handler(document.hidden);
});
