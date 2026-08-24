// Multiplexes Tauri events for plugins.
//
// Without this each plugin would call `listen()` itself, which means N
// separate IPC subscriptions for the same event and no way for the host to
// clean them up when a plugin is unmounted. Here the bus keeps exactly one
// `listen()` per event name and fans the payload out to every subscriber.
//
// It also remembers the most recent payload per event and replays it to
// late subscribers, so a plugin mounted between two `sys://stats` ticks
// renders real data immediately instead of showing "--" for a full sample
// interval.
//
// While the window is hidden (tray "Hide" or fully occluded by another
// window - see visibility.js) handler dispatch is skipped entirely, so
// idle cards stop doing DOM writes and canvas redraws for data nobody can
// see. `last` still gets updated so the moment the window is visible
// again every channel replays its latest payload and the UI catches up
// in one frame instead of trickling in over the next few sample ticks.

import { isHidden, onVisibilityChange } from "./visibility.js";

const { listen } = window.__TAURI__.event;

/** @type {Map<string, { handlers: Set<Function>, last: unknown, hasLast: boolean }>} */
const channels = new Map();

function channel(name) {
  let entry = channels.get(name);
  if (entry) return entry;

  entry = { handlers: new Set(), last: undefined, hasLast: false };
  // Never unlistened: the bus lives as long as the window does, and keeping
  // the channel open is what lets `last` stay warm for the next subscriber.
  listen(name, (event) => {
    entry.last = event.payload;
    entry.hasLast = true;
    if (isHidden()) return;
    dispatch(name, entry, event.payload);
  });
  channels.set(name, entry);
  return entry;
}

function dispatch(name, entry, payload) {
  for (const handler of entry.handlers) {
    try {
      handler(payload);
    } catch (err) {
      // One misbehaving plugin must not stop the others from updating.
      console.error(`[event-bus] handler for "${name}" threw`, err);
    }
  }
}

// Catch every card up in one shot the moment the window becomes visible
// again, instead of leaving it stale until the next backend tick.
onVisibilityChange((hidden) => {
  if (hidden) return;
  for (const [name, entry] of channels) {
    if (entry.hasLast) dispatch(name, entry, entry.last);
  }
});

/**
 * Subscribes to a backend event. Returns an unsubscribe function.
 * @param {string} name e.g. "sys://stats"
 * @param {(payload: unknown) => void} handler
 */
export function subscribe(name, handler) {
  const entry = channel(name);
  entry.handlers.add(handler);

  if (entry.hasLast) {
    // Deliver asynchronously so subscribe() never re-enters the caller
    // before it has finished setting itself up.
    queueMicrotask(() => {
      if (entry.handlers.has(handler)) handler(entry.last);
    });
  }

  return () => entry.handlers.delete(handler);
}
