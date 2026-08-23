// Per-plugin settings: enabled/disabled + a free-form config blob each
// plugin interprets itself (see `configSchema` in a plugin.js/index.js
// default export). Persisted as one JSON file so both the main dashboard
// window and the settings window (see `tray.rs`) agree on the same state
// without relying on localStorage being shared across webviews.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// Guards the load-modify-save sequence in `set_plugin_enabled` and
/// `set_plugin_config` so two settings changes fired in quick succession
/// (e.g. toggling two different widgets) can't race and silently drop one
/// of them - see the `Mutex` pattern already used the same way in
/// hit_test.rs and media.rs.
static SETTINGS_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PluginSettings {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub config: serde_json::Value,
}

fn default_enabled() -> bool {
    true
}

impl Default for PluginSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            config: serde_json::Value::Null,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
struct SettingsFile {
    #[serde(default)]
    plugins: HashMap<String, PluginSettings>,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|err| format!("could not resolve app config dir: {err}"))?;
    std::fs::create_dir_all(&dir).map_err(|err| format!("could not create app config dir: {err}"))?;
    Ok(dir.join("settings.json"))
}

/// A missing or unparseable file just means "nothing customized yet" -
/// every plugin defaults to enabled with no config, same as a fresh
/// install would behave without this file existing at all.
fn load(app: &AppHandle) -> SettingsFile {
    let Ok(path) = settings_path(app) else {
        return SettingsFile::default();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return SettingsFile::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save(app: &AppHandle, file: &SettingsFile) -> Result<(), String> {
    let path = settings_path(app)?;
    let raw = serde_json::to_string_pretty(file).map_err(|err| format!("failed to serialize settings: {err}"))?;
    std::fs::write(&path, raw).map_err(|err| format!("failed to write settings file: {err}"))
}

/// Payload broadcast to every window whenever one plugin's settings
/// change, so the dashboard can remount just that plugin instead of
/// requiring a full app restart.
#[derive(Debug, Serialize, Clone)]
struct SettingsChanged {
    id: String,
    settings: PluginSettings,
}

const SETTINGS_CHANGED_EVENT: &str = "settings://changed";

#[tauri::command]
pub fn get_all_plugin_settings(app: AppHandle) -> Result<HashMap<String, PluginSettings>, String> {
    Ok(load(&app).plugins)
}

#[tauri::command]
pub fn set_plugin_enabled(app: AppHandle, id: String, enabled: bool) -> Result<(), String> {
    let _guard = SETTINGS_LOCK.lock().unwrap();
    let mut file = load(&app);
    file.plugins.entry(id.clone()).or_default().enabled = enabled;
    save(&app, &file)?;

    let settings = file.plugins.get(&id).cloned().unwrap_or_default();
    drop(_guard);
    let _ = app.emit(SETTINGS_CHANGED_EVENT, SettingsChanged { id, settings });
    Ok(())
}

#[tauri::command]
pub fn set_plugin_config(app: AppHandle, id: String, config: serde_json::Value) -> Result<(), String> {
    let _guard = SETTINGS_LOCK.lock().unwrap();
    let mut file = load(&app);
    file.plugins.entry(id.clone()).or_default().config = config;
    save(&app, &file)?;

    let settings = file.plugins.get(&id).cloned().unwrap_or_default();
    drop(_guard);
    let _ = app.emit(SETTINGS_CHANGED_EVENT, SettingsChanged { id, settings });
    Ok(())
}

/// Opens the settings window, creating it on first use and focusing the
/// existing one on subsequent calls (only one instance ever makes sense).
/// Unlike the "main" window, this is a normal decorated/taskbar window -
/// it doesn't need any of the desktop-pinning tricks in `window_layer.rs`.
pub fn open_settings_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("settings") {
        window.show()?;
        window.set_focus()?;
        return Ok(());
    }

    tauri::WebviewWindowBuilder::new(app, "settings", tauri::WebviewUrl::App("settings.html".into()))
        .title("SWD Settings")
        .inner_size(440.0, 640.0)
        .resizable(true)
        .decorations(true)
        .skip_taskbar(false)
        .focused(true)
        .build()?;

    Ok(())
}
