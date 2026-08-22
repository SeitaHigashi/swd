// System tray icon: the main window has no titlebar or taskbar entry
// (decorations: false, skipTaskbar: true - see tauri.conf.json), so
// without this there's no UI way to quit the app short of Task Manager.
// The tray icon also lets the window be hidden/shown on demand, e.g. to
// get it out of the way of a full-screen app temporarily.

use crate::settings::open_settings_window;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};

const TOGGLE_VISIBILITY_ID: &str = "toggle-visibility";
const SETTINGS_ID: &str = "settings";
const QUIT_ID: &str = "quit";

fn toggle_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let is_visible = window.is_visible().unwrap_or(true);
    if is_visible {
        let _ = window.hide();
    } else {
        let _ = window.show();
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let toggle_item = MenuItem::with_id(app, TOGGLE_VISIBILITY_ID, "Show/Hide", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, SETTINGS_ID, "Settings...", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, QUIT_ID, "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[&toggle_item, &settings_item, &separator, &quit_item],
    )?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().cloned().expect("bundle icon must be configured"))
        .tooltip("SWD")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TOGGLE_VISIBILITY_ID => toggle_main_window(app),
            SETTINGS_ID => {
                if let Err(err) = open_settings_window(app) {
                    eprintln!("[tray] failed to open settings window: {err:?}");
                }
            }
            QUIT_ID => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}
