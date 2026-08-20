//! Makes the transparent parts of the dashboard click-through while keeping
//! glass cards clickable/draggable, without ever letting the window stop
//! receiving input entirely (which is what makes a naive "toggle
//! ignore_cursor_events from the webview's own mousemove" approach
//! deadlock: once the window ignores the cursor, it stops receiving the
//! mousemove events needed to notice the cursor came back).
//!
//! Instead, the frontend reports the screen-space rectangles of its cards
//! whenever layout changes, and a background thread polls the OS cursor
//! position independently of the webview's event loop, flipping
//! `ignore_cursor_events` only when the cursor crosses a card boundary.

use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

#[derive(Clone, Copy, Deserialize)]
pub struct HitRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl HitRect {
    fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px <= self.x + self.width && py >= self.y && py <= self.y + self.height
    }
}

static HIT_REGIONS: Mutex<Vec<HitRect>> = Mutex::new(Vec::new());

#[tauri::command]
pub fn set_hit_regions(rects: Vec<HitRect>) {
    *HIT_REGIONS.lock().unwrap() = rects;
}

/// Polls the cursor position ~30 times per second and flips
/// `ignore_cursor_events` only on transitions, so it costs nothing while
/// the cursor stays on one side of a card boundary.
pub fn start_hit_test_loop(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last_ignore = true;
        loop {
            std::thread::sleep(Duration::from_millis(30));

            let mut point = POINT::default();
            if unsafe { GetCursorPos(&mut point) }.is_err() {
                continue;
            }

            let over_card = {
                let regions = HIT_REGIONS.lock().unwrap();
                regions
                    .iter()
                    .any(|r| r.contains(point.x as f64, point.y as f64))
            };
            let should_ignore = !over_card;

            if should_ignore != last_ignore {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_ignore_cursor_events(should_ignore);
                }
                last_ignore = should_ignore;
            }
        }
    });
}
