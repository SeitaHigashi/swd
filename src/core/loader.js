// Discovers plugins to mount. Stage 1 (current): a fixed list of built-in
// plugins, statically imported. Stage 2 (future): this is the seam where
// externally-installed plugins get discovered - e.g. by asking Rust to
// list a plugins directory and dynamic-`import()`-ing each one's entry
// point over a custom `swd-plugin://` protocol. Nothing above this module
// needs to change when that lands; `loadPlugins()` just starts returning a
// longer list.

import clockPlugin from "../plugins/clock/plugin.js";
import systemMonitorPlugin from "../plugins/system-monitor/plugin.js";
import networkPlugin from "../plugins/network/plugin.js";
import mediaPlugin from "../plugins/media/plugin.js";

const BUILT_IN_PLUGINS = [
  { plugin: clockPlugin, baseUrl: new URL("../plugins/clock/plugin.js", import.meta.url).href },
  { plugin: systemMonitorPlugin, baseUrl: new URL("../plugins/system-monitor/plugin.js", import.meta.url).href },
  { plugin: networkPlugin, baseUrl: new URL("../plugins/network/plugin.js", import.meta.url).href },
  { plugin: mediaPlugin, baseUrl: new URL("../plugins/media/plugin.js", import.meta.url).href },
];

/**
 * @returns {Promise<{ plugin: object, baseUrl: string }[]>}
 */
export async function loadPlugins() {
  return BUILT_IN_PLUGINS;
}
