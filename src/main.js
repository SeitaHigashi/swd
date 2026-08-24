// Host bootstrap. This file used to import every card module directly and
// wire it up by hand; now it just asks the plugin loader what to mount and
// hands each one to the plugin host. Adding, removing, or externally
// installing a card no longer touches this file at all.

import { loadAllPlugins, getAllPluginSettings } from "./core/loader.js";
import { mountPlugin } from "./core/plugin-host.js";
import { startHitRegionWatcher } from "./core/hit-regions.js";
import { subscribe } from "./core/event-bus.js";

window.addEventListener("DOMContentLoaded", async () => {
  const dashboard = document.querySelector(".dashboard");

  // Keep the full plugin list (including disabled ones) around, not just
  // the initially-mounted subset, so a plugin toggled on later from the
  // settings window can be mounted without reloading the whole app.
  const byId = new Map((await loadAllPlugins()).map((entry) => [entry.plugin.id, entry]));
  const mounted = new Map();

  function mount(id, config) {
    const entry = byId.get(id);
    if (!entry) return;
    try {
      mounted.set(id, mountPlugin(entry.plugin, dashboard, entry.baseUrl, config));
    } catch (err) {
      console.error(`[main] failed to mount plugin "${id}"`, err);
    }
  }

  function unmount(id) {
    const handle = mounted.get(id);
    if (!handle) return;
    handle.unmount();
    mounted.delete(id);
  }

  const settings = await getAllPluginSettings();
  for (const id of byId.keys()) {
    if (settings[id]?.enabled === false) continue;
    mount(id, settings[id]?.config);
  }

  // Hit-region sync is driven by a MutationObserver on `dashboard` (see
  // core/hit-regions.js), so it must start watching *after* the initial
  // mount pass above, or it would resync once per card as they're added
  // during startup instead of once at the end.
  startHitRegionWatcher(dashboard);

  // The settings window (src/settings.js) emits this after every
  // enable/disable/config change, so a toggle there takes effect
  // immediately instead of requiring a restart.
  subscribe("settings://changed", ({ id, settings: next }) => {
    unmount(id);
    if (next?.enabled !== false) mount(id, next?.config);
  });
});
