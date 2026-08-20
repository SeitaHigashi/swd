#[cfg(target_os = "windows")]
mod hit_test;
#[cfg(target_os = "windows")]
mod media;
mod system_info;
#[cfg(target_os = "windows")]
mod window_layer;

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
        ));

    #[cfg(target_os = "windows")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        hit_test::set_hit_regions,
        media::media_toggle_play_pause,
        media::media_next,
        media::media_previous,
    ]);

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
