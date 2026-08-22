// Mounts plugin modules into the dashboard.
//
// A plugin is a module exporting a default object:
//
//   {
//     id: "system-monitor",              // unique, used for DOM id + storage key
//     name: "System",                    // shown nowhere yet, but kept for a future
//                                         // plugin-list UI
//     position: { top: 32, right: 32 },  // CSS default position (right-anchored,
//                                         // see "why right-anchored" in CLAUDE.md)
//     styles: ["./style.css"],           // resolved relative to the plugin file
//     permissions: { invoke: [...] },    // Tauri commands this plugin may call
//     configSchema: [                    // optional - rendered as a generic
//       { key, label, type, options?, default },  // form by the settings
//     ],                                 // window (see src/settings.js)
//     mount(ctx) { ... },                // build the card's contents; ctx.config
//                                         // holds this plugin's saved config,
//                                         // merged over configSchema defaults
//     unmount?(ctx) { ... },             // optional cleanup beyond what the
//                                         // host already undoes automatically
//   }
//
// The host owns everything generic: creating the `.glass-card` element,
// dragging, position persistence, and hit-region sync. `mount()` only
// needs to fill in `ctx.root` and wire itself up to `ctx`.

import { subscribe } from "./event-bus.js";
import { applySavedPosition } from "./layout.js";
import { makeDraggable } from "./drag.js";
import { syncHitRegions } from "./hit-regions.js";

const { invoke: tauriInvoke } = window.__TAURI__.core;

// No plugin is ever allowed to touch hit-region wiring itself - that's
// host-owned, because a plugin that could set its own hit regions could
// grab mouse input outside its card's bounds.
const FORBIDDEN_COMMANDS = new Set(["set_hit_regions"]);

const loadedStylesheets = new Set();

function loadStylesheet(href) {
  if (loadedStylesheets.has(href)) return;
  loadedStylesheets.add(href);
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = href;
  document.head.appendChild(link);
}

/**
 * Namespaced localStorage wrapper so plugins can't collide on keys or read
 * each other's data.
 */
function makeStorage(pluginId) {
  const prefix = `swd:plugin-data:${pluginId}:`;
  return {
    get(key) {
      try {
        const raw = localStorage.getItem(prefix + key);
        return raw ? JSON.parse(raw) : null;
      } catch {
        return null;
      }
    },
    set(key, value) {
      localStorage.setItem(prefix + key, JSON.stringify(value));
    },
    remove(key) {
      localStorage.removeItem(prefix + key);
    },
  };
}

function makeScopedInvoke(plugin) {
  const allowed = new Set(plugin.permissions?.invoke ?? []);
  return (command, args) => {
    if (FORBIDDEN_COMMANDS.has(command)) {
      return Promise.reject(new Error(`[plugin-host] "${command}" is host-reserved and cannot be invoked by plugins`));
    }
    if (!allowed.has(command)) {
      return Promise.reject(
        new Error(`[plugin-host] plugin "${plugin.id}" did not declare permission to invoke "${command}"`),
      );
    }
    return tauriInvoke(command, args);
  };
}

/**
 * Merges a plugin's declared `configSchema` defaults with whatever was
 * actually saved from the settings window, so `ctx.config` always has
 * every field populated even before the user has touched that plugin's
 * settings.
 */
function resolveConfig(plugin, savedConfig) {
  const defaults = {};
  for (const field of plugin.configSchema ?? []) {
    defaults[field.key] = field.default;
  }
  return { ...defaults, ...(savedConfig ?? {}) };
}

/**
 * Mounts one plugin into `container`. Returns a handle with `unmount()`.
 * @param {object} plugin
 * @param {HTMLElement} container
 * @param {string} [baseUrl] URL to resolve `plugin.styles` against (the
 *   plugin module's own URL - pass `import.meta.url` from the loader).
 * @param {unknown} [savedConfig] Config saved from the settings window,
 *   per `plugin.configSchema` - merged over that schema's defaults and
 *   exposed to the plugin as `ctx.config`.
 */
export function mountPlugin(plugin, container, baseUrl, savedConfig) {
  if (!plugin?.id || typeof plugin.mount !== "function") {
    throw new Error("[plugin-host] plugin must export { id, mount(ctx) }");
  }

  for (const stylesheet of plugin.styles ?? []) {
    loadStylesheet(new URL(stylesheet, baseUrl ?? window.location.href).href);
  }

  const root = document.createElement("section");
  root.className = "glass-card";
  root.id = plugin.id;
  root.dataset.card = "";
  root.dataset.pluginId = plugin.id;
  if (plugin.position?.top != null) root.style.top = `${plugin.position.top}px`;
  if (plugin.position?.right != null) root.style.right = `${plugin.position.right}px`;
  if (plugin.position?.left != null) root.style.left = `${plugin.position.left}px`;

  const unsubscribers = [];
  const destroyCallbacks = [];

  const ctx = {
    root,
    /** Scoped querySelector - looks only inside this plugin's own card. */
    el: (selector) => root.querySelector(selector),
    on: (eventName, handler) => {
      const unsubscribe = subscribe(eventName, handler);
      unsubscribers.push(unsubscribe);
      return unsubscribe;
    },
    invoke: makeScopedInvoke(plugin),
    storage: makeStorage(plugin.id),
    config: resolveConfig(plugin, savedConfig),
    onDestroy: (fn) => destroyCallbacks.push(fn),
  };

  container.appendChild(root);
  applySavedPosition(root);
  makeDraggable(root, syncHitRegions);

  plugin.mount(ctx);

  return {
    plugin,
    ctx,
    unmount() {
      for (const unsubscribe of unsubscribers) unsubscribe();
      for (const fn of destroyCallbacks) {
        try {
          fn();
        } catch (err) {
          console.error(`[plugin-host] onDestroy callback for "${plugin.id}" threw`, err);
        }
      }
      plugin.unmount?.(ctx);
      root.remove();
      syncHitRegions();
    },
  };
}
