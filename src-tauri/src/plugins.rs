// External plugin discovery and file serving.
//
// A plugin lives at `<app data dir>/plugins/<id>/`, containing a
// `plugin.json` manifest (id, name, entry point) plus whatever JS/CSS the
// entry point needs. `list_plugins` lets the frontend discover what's
// installed; the `swd-plugin` URI scheme (registered in `lib.rs`) is what
// actually serves those files to the WebView, since a bare filesystem path
// doesn't resolve there and `frontendDist` is a fixed, bundled directory.
//
// Not Windows-specific - this only touches the filesystem - so it isn't
// behind the `target_os = "windows"` gate the way hit_test/media/
// window_layer are.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PluginManifest {
    id: String,
    name: String,
    /// Path to the plugin's JS entry point, relative to the plugin's own
    /// directory (e.g. "index.js"). Resolved and fetched through the
    /// `swd-plugin://` protocol by the frontend loader.
    entry: String,
}

pub fn plugins_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("plugins"))
        .map_err(|err| format!("could not resolve app data dir: {err}"))
}

/// Lists installed external plugins by reading `plugins_dir()`. A missing
/// directory (nothing installed yet) is not an error - just no plugins.
/// A subdirectory with a missing/malformed `plugin.json`, or whose `id`
/// doesn't match its own directory name, is skipped with a warning rather
/// than failing the whole scan - one broken plugin shouldn't take down
/// every other one.
#[tauri::command]
pub fn list_plugins(app: AppHandle) -> Result<Vec<PluginManifest>, String> {
    let dir = plugins_dir(&app)?;

    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("failed to read plugins dir: {err}")),
    };

    let mut manifests = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        match read_manifest(&path) {
            Ok(manifest) if manifest.id == dir_name => manifests.push(manifest),
            Ok(manifest) => eprintln!(
                "[plugins] skipping \"{dir_name}\": manifest id \"{}\" does not match its directory name",
                manifest.id
            ),
            Err(err) => eprintln!("[plugins] skipping \"{dir_name}\": {err}"),
        }
    }

    Ok(manifests)
}

fn read_manifest(plugin_dir: &Path) -> Result<PluginManifest, String> {
    let raw = std::fs::read_to_string(plugin_dir.join("plugin.json"))
        .map_err(|err| format!("could not read plugin.json: {err}"))?;
    serde_json::from_str(&raw).map_err(|err| format!("invalid plugin.json: {err}"))
}

/// Resolves a `swd-plugin://<id>/<relative path>` request to an absolute
/// path inside that plugin's own directory, rejecting anything that would
/// escape it (e.g. `..` segments) so one plugin can't read another
/// plugin's files or arbitrary files elsewhere on disk.
pub fn resolve_plugin_file(app: &AppHandle, id: &str, relative_path: &str) -> Result<PathBuf, String> {
    let dir = plugins_dir(app)?;
    let plugin_root = dir
        .join(id)
        .canonicalize()
        .map_err(|err| format!("unknown plugin \"{id}\": {err}"))?;

    resolve_within_plugin_root(&plugin_root, relative_path)
}

/// The traversal-guarding half of `resolve_plugin_file`, split out so it
/// can be unit tested against a plain temp directory instead of a live
/// `AppHandle`.
fn resolve_within_plugin_root(plugin_root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let candidate = plugin_root.join(relative_path.trim_start_matches('/'));
    let resolved = candidate
        .canonicalize()
        .map_err(|err| format!("file not found: {err}"))?;

    if !resolved.starts_with(plugin_root) {
        return Err(format!("path escapes plugin directory: {relative_path}"));
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn resolves_a_file_inside_the_plugin_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        write(&root.join("index.js"), "export default {};");

        let resolved = resolve_within_plugin_root(&root, "index.js").unwrap();

        assert_eq!(resolved, root.join("index.js"));
    }

    #[test]
    fn resolves_a_nested_file_and_strips_a_leading_slash() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        write(&root.join("assets/icon.svg"), "<svg></svg>");

        let resolved = resolve_within_plugin_root(&root, "/assets/icon.svg").unwrap();

        assert_eq!(resolved, root.join("assets/icon.svg"));
    }

    #[test]
    fn rejects_a_path_that_escapes_the_plugin_root_via_dot_dot() {
        let tmp = tempfile::tempdir().unwrap();
        let root_dir = tmp.path().join("plugin-a");
        let secret = tmp.path().join("secret.txt");
        write(&secret, "top secret");
        fs::create_dir_all(&root_dir).unwrap();
        let root = root_dir.canonicalize().unwrap();

        let err = resolve_within_plugin_root(&root, "../secret.txt").unwrap_err();

        assert!(err.contains("escapes plugin directory"));
    }

    #[test]
    fn reports_missing_files_as_not_found_rather_than_escaped() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();

        let err = resolve_within_plugin_root(&root, "does-not-exist.js").unwrap_err();

        assert!(err.contains("file not found"));
    }

    #[test]
    fn reads_a_valid_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("plugin.json"),
            r#"{"id":"clock","name":"Clock","entry":"index.js"}"#,
        );

        let manifest = read_manifest(tmp.path()).unwrap();

        assert_eq!(manifest.id, "clock");
        assert_eq!(manifest.name, "Clock");
        assert_eq!(manifest.entry, "index.js");
    }

    #[test]
    fn rejects_a_missing_manifest() {
        let tmp = tempfile::tempdir().unwrap();

        let err = read_manifest(tmp.path()).unwrap_err();

        assert!(err.contains("could not read plugin.json"));
    }

    #[test]
    fn rejects_a_malformed_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("plugin.json"), "{ not json");

        let err = read_manifest(tmp.path()).unwrap_err();

        assert!(err.contains("invalid plugin.json"));
    }
}
