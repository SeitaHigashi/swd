// Host bootstrap. This file used to import every card module directly and
// wire it up by hand; now it just asks the plugin loader what to mount and
// hands each one to the plugin host. Adding, removing, or externally
// installing a card no longer touches this file at all.

import { loadPlugins } from "./core/loader.js";
import { mountPlugin } from "./core/plugin-host.js";
import { startHitRegionWatcher } from "./core/hit-regions.js";

window.addEventListener("DOMContentLoaded", async () => {
  const dashboard = document.querySelector(".dashboard");

  const plugins = await loadPlugins();
  for (const { plugin, baseUrl } of plugins) {
    try {
      mountPlugin(plugin, dashboard, baseUrl);
    } catch (err) {
      console.error(`[main] failed to mount plugin "${plugin?.id}"`, err);
    }
  }

  // Hit-region sync is driven by a MutationObserver on `dashboard` (see
  // core/hit-regions.js), so it must start watching *after* the initial
  // mount pass above, or it would resync once per card as they're added
  // during startup instead of once at the end.
  startHitRegionWatcher(dashboard);
});
