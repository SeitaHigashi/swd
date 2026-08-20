#[cfg(target_os = "windows")]
mod hit_test;
#[cfg(target_os = "windows")]
mod media;
mod plugins;
mod system_info;
#[cfg(target_os = "windows")]
mod window_layer;

/// Guesses a `Content-Type` for files served over the `swd-plugin://`
/// protocol. Only the extensions an external plugin actually needs are
/// covered; anything else falls back to a generic binary type rather than
/// guessing wrong.
fn content_type_for(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("js" | "mjs") => "text/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// Picks which physical monitor the dashboard should cover. Currently
/// "rightmost of all connected monitors" (matches the 3-monitor desk setup
/// this was built for), but kept as its own function so switching to e.g.
/// "primary monitor" or "monitor at a specific index" later is a one-line
/// change here rather than a hunt through `setup`.
fn select_target_monitor(window: &tauri::WebviewWindow) -> Option<tauri::window::Monitor> {
    let monitors = window.available_monitors().ok()?;
    monitors.into_iter().max_by_key(|m| m.position().x)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        // Serves external plugin files (see src/core/loader.js) to the
        // WebView. `<app data dir>/plugins/<id>/<path>` is exposed as
        // `swd-plugin://<id>/<path>` - a bare filesystem path won't
        // resolve in the WebView, and `frontendDist` is a fixed, bundled
        // directory that can't hold user-installed plugins.
        .register_uri_scheme_protocol("swd-plugin", |ctx, request| {
            let path = request.uri().path().trim_start_matches('/');
            // First segment is the plugin id, the rest is the path within
            // that plugin's own directory.
            let Some((id, relative_path)) = path.split_once('/') else {
                return tauri::http::Response::builder()
                    .status(tauri::http::StatusCode::BAD_REQUEST)
                    .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                    .body(b"expected swd-plugin://<id>/<path>".to_vec())
                    .unwrap();
            };

            // The main page's origin (`http://tauri.localhost` on Windows)
            // differs from this protocol's, so a plain `fetch`/dynamic
            // `import()` of a `swd-plugin://` URL is a cross-origin
            // request - it needs an explicit CORS header or the WebView
            // rejects the response after receiving it. There is no
            // sensitive data behind this protocol (it only ever serves
            // files the user chose to drop in their own plugins
            // directory), so allowing any origin is fine.
            match plugins::resolve_plugin_file(ctx.app_handle(), id, relative_path) {
                Ok(resolved) => match std::fs::read(&resolved) {
                    Ok(data) => tauri::http::Response::builder()
                        .header(tauri::http::header::CONTENT_TYPE, content_type_for(&resolved))
                        .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                        .body(data)
                        .unwrap(),
                    Err(err) => tauri::http::Response::builder()
                        .status(tauri::http::StatusCode::NOT_FOUND)
                        .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                        .body(format!("failed to read file: {err}").into_bytes())
                        .unwrap(),
                },
                Err(err) => tauri::http::Response::builder()
                    .status(tauri::http::StatusCode::FORBIDDEN)
                    .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                    .body(err.into_bytes())
                    .unwrap(),
            }
        });

    #[cfg(target_os = "windows")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        hit_test::set_hit_regions,
        media::media_toggle_play_pause,
        media::media_next,
        media::media_previous,
        plugins::list_plugins,
    ]);
    #[cfg(not(target_os = "windows"))]
    let builder = builder.invoke_handler(tauri::generate_handler![plugins::list_plugins]);

    builder
        .setup(|app| {
            use tauri::Manager;
            use tauri_plugin_autostart::ManagerExt;

            // This is meant to run as an always-on desktop widget, so make
            // sure it's registered to launch at login. Idempotent - safe to
            // call on every startup.
            if let Err(err) = app.autolaunch().enable() {
                eprintln!("[autostart] failed to register for login launch: {err:?}");
            }

            let window = app
                .get_webview_window("main")
                .expect("main window must exist");

            if let Some(monitor) = select_target_monitor(&window) {
                let size = *monitor.size();
                let position = *monitor.position();
                let _ = window.set_size(tauri::Size::Physical(size));
                let _ = window.set_position(tauri::Position::Physical(position));
            }

            // Whole window click-through by default; the frontend flips this
            // per-region so only glass cards intercept the mouse.
            let _ = window.set_ignore_cursor_events(true);

            #[cfg(target_os = "windows")]
            {
                let handle = app.handle().clone();
                window_layer::pin_to_desktop_layer(&handle);
                window_layer::start_layer_watcher(handle.clone());
                hit_test::start_hit_test_loop(handle.clone());
                media::start_media_monitor(handle);
            }

            system_info::start_system_monitor(app.handle().clone());

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
