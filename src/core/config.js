// Shared by plugin-host.js (to build `ctx.config` for a mounted card) and
// settings/main.js (to build the form's initial values) so both windows
// agree on what an unconfigured field defaults to.

/**
 * Merges a plugin's declared `configSchema` defaults with whatever was
 * actually saved from the settings window, so the result always has every
 * field populated even before the user has touched that plugin's settings.
 * @param {{ configSchema?: { key: string, default: unknown }[] }} plugin
 * @param {unknown} savedConfig
 */
export function resolveConfig(plugin, savedConfig) {
  const defaults = {};
  for (const field of plugin.configSchema ?? []) {
    defaults[field.key] = field.default;
  }
  return { ...defaults, ...(savedConfig ?? {}) };
}
