// Settings window: lists every discovered plugin (built-in + external,
// including currently-disabled ones) with an on/off switch, plus a
// generic form for whatever `configSchema` fields the plugin declares
// (see the shape documented in src/core/plugin-host.js). No plugin-specific
// UI code lives here - a plugin author who wants configurable settings
// only has to add `configSchema` to their plugin.js export.

import { loadAllPlugins, getAllPluginSettings } from "../core/loader.js";
import { resolveConfig } from "../core/config.js";

const { invoke } = window.__TAURI__.core;

function renderField(field, config, onChange) {
  const row = document.createElement("label");
  row.className = "field-row";

  const labelText = document.createElement("span");
  labelText.className = "field-label";
  labelText.textContent = field.label ?? field.key;
  row.appendChild(labelText);

  let input;
  if (field.type === "boolean") {
    input = document.createElement("input");
    input.type = "checkbox";
    input.checked = !!config[field.key];
    input.addEventListener("change", () => {
      config[field.key] = input.checked;
      onChange();
    });
  } else if (field.type === "select") {
    input = document.createElement("select");
    for (const option of field.options ?? []) {
      const opt = document.createElement("option");
      opt.value = option;
      opt.textContent = option;
      input.appendChild(opt);
    }
    input.value = config[field.key];
    input.addEventListener("change", () => {
      config[field.key] = input.value;
      onChange();
    });
  } else if (field.type === "number") {
    input = document.createElement("input");
    input.type = "number";
    input.value = config[field.key];
    input.addEventListener("change", () => {
      config[field.key] = Number(input.value);
      onChange();
    });
  } else {
    input = document.createElement("input");
    input.type = "text";
    input.value = config[field.key] ?? "";
    input.addEventListener("change", () => {
      config[field.key] = input.value;
      onChange();
    });
  }
  input.className = "field-input";
  row.appendChild(input);
  return row;
}

function renderPluginCard(plugin, saved) {
  const config = resolveConfig(plugin, saved.config);

  const card = document.createElement("section");
  card.className = "widget-card";

  const header = document.createElement("div");
  header.className = "widget-header";

  const titleWrap = document.createElement("div");
  const title = document.createElement("div");
  title.className = "widget-name";
  title.textContent = plugin.name ?? plugin.id;
  const idLabel = document.createElement("div");
  idLabel.className = "widget-id";
  idLabel.textContent = plugin.id;
  titleWrap.append(title, idLabel);

  const toggle = document.createElement("label");
  toggle.className = "switch";
  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  checkbox.checked = saved.enabled !== false;
  const slider = document.createElement("span");
  slider.className = "slider";
  toggle.append(checkbox, slider);
  checkbox.addEventListener("change", () => {
    invoke("set_plugin_enabled", { id: plugin.id, enabled: checkbox.checked }).catch((err) =>
      console.error(`[settings] failed to save enabled state for "${plugin.id}"`, err),
    );
  });

  header.append(titleWrap, toggle);
  card.appendChild(header);

  const fields = plugin.configSchema ?? [];
  if (fields.length > 0) {
    const fieldsWrap = document.createElement("div");
    fieldsWrap.className = "widget-fields";
    for (const field of fields) {
      fieldsWrap.appendChild(
        renderField(field, config, () => {
          invoke("set_plugin_config", { id: plugin.id, config }).catch((err) =>
            console.error(`[settings] failed to save config for "${plugin.id}"`, err),
          );
        }),
      );
    }
    card.appendChild(fieldsWrap);
  }

  return card;
}

async function main() {
  const root = document.getElementById("settings-root");

  let all;
  let settings;
  try {
    [all, settings] = await Promise.all([loadAllPlugins(), getAllPluginSettings()]);
  } catch (err) {
    root.innerHTML = `<h1>Widgets</h1><p class="hint">Failed to load plugins: ${err}</p>`;
    return;
  }

  root.innerHTML = "<h1>Widgets</h1>";

  if (all.length === 0) {
    const hint = document.createElement("p");
    hint.className = "hint";
    hint.textContent = "No widgets found.";
    root.appendChild(hint);
    return;
  }

  for (const { plugin } of all) {
    root.appendChild(renderPluginCard(plugin, settings[plugin.id] ?? { enabled: true, config: undefined }));
  }
}

main();
