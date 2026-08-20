// Discovers plugins to mount: the fixed set of built-in plugins, plus
// whatever's installed under `<app data dir>/plugins/` (see
// src-tauri/src/plugins.rs for the manifest format and file-serving side
// of this).

import clockPlugin from "../plugins/clock/plugin.js";
import systemMonitorPlugin from "../plugins/system-monitor/plugin.js";
import networkPlugin from "../plugins/network/plugin.js";
import mediaPlugin from "../plugins/media/plugin.js";

const { invoke } = window.__TAURI__.core;

const BUILT_IN_PLUGINS = [
  { plugin: clockPlugin, baseUrl: new URL("../plugins/clock/plugin.js", import.meta.url).href },
  { plugin: systemMonitorPlugin, baseUrl: new URL("../plugins/system-monitor/plugin.js", import.meta.url).href },
  { plugin: networkPlugin, baseUrl: new URL("../plugins/network/plugin.js", import.meta.url).href },
  { plugin: mediaPlugin, baseUrl: new URL("../plugins/media/plugin.js", import.meta.url).href },
];

/**
 * Builds the URL an external plugin's file is served at. Registered as
 * `swd-plugin` in src-tauri/src/lib.rs via `register_uri_scheme_protocol`.
 *
 * Custom URI schemes resolve to a different Origin per platform (see the
 * "Warning" in Tauri's docs for that API) - Windows uses
 * `http://<scheme>.localhost/<path>`, everywhere else uses
 * `<scheme>://localhost/<path>`. This project only ever runs on Windows
 * (see CLAUDE.md), so only that form is implemented here; a cross-platform
 * build would need to branch on `navigator.userAgent` or similar.
 */
function pluginFileUrl(id, relativePath) {
  return `http://swd-plugin.localhost/${id}/${relativePath.replace(/^\//, "")}`;
}

/**
 * Asks the Rust side what's installed under the plugins directory, then
 * dynamic-imports each one's entry point. A plugin that fails to load
 * (missing file, bad manifest, throwing at import time) is logged and
 * skipped rather than blocking every other plugin from mounting - the
 * same reasoning as `list_plugins` skipping malformed manifests on the
 * Rust side.
 */
async function loadExternalPlugins() {
  let manifests;
  try {
    manifests = await invoke("list_plugins");
  } catch (err) {
    console.error("[loader] list_plugins failed", err);
    return [];
  }

  const loaded = [];
  for (const manifest of manifests) {
    const entryUrl = pluginFileUrl(manifest.id, manifest.entry);
    try {
      const module = await import(entryUrl);
      if (!module.default) {
        throw new Error("module has no default export");
      }
      loaded.push({ plugin: module.default, baseUrl: entryUrl });
    } catch (err) {
      console.error(`[loader] failed to load external plugin "${manifest.id}"`, err);
    }
  }
  return loaded;
}

/**
 * @returns {Promise<{ plugin: object, baseUrl: string }[]>}
 */
export async function loadPlugins() {
  const external = await loadExternalPlugins();
  return [...BUILT_IN_PLUGINS, ...external];
}
