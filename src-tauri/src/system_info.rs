//! Periodically samples CPU/memory/network stats and pushes them to the
//! frontend as a Tauri event. Kept independent from `window_layer` and
//! `hit_test` so it works the same on every platform.

use std::time::Duration;

use serde::Serialize;
use sysinfo::{Networks, System};
use tauri::{AppHandle, Emitter};

const REFRESH_INTERVAL: Duration = Duration::from_millis(1500);
const STATS_EVENT: &str = "sys://stats";

#[derive(Clone, Serialize)]
pub struct SystemStats {
    cpu_percent: f32,
    mem_used_bytes: u64,
    mem_total_bytes: u64,
    mem_percent: f32,
    net_rx_bytes_per_sec: f64,
    net_tx_bytes_per_sec: f64,
}

/// Starts a background thread that emits `sys://stats` roughly every
/// `REFRESH_INTERVAL`. The frontend's system-monitor and network modules
/// both listen on this one event so we only pay for one sampling loop.
pub fn start_system_monitor(app: AppHandle) {
    std::thread::spawn(move || {
        let mut sys = System::new_all();
        let mut networks = Networks::new_with_refreshed_list();

        loop {
            std::thread::sleep(REFRESH_INTERVAL);

            sys.refresh_cpu_usage();
            sys.refresh_memory();
            networks.refresh();

            let (rx_bytes, tx_bytes) = networks
                .iter()
                .fold((0u64, 0u64), |(rx, tx), (_name, data)| {
                    (rx + data.received(), tx + data.transmitted())
                });

            let secs = REFRESH_INTERVAL.as_secs_f64();
            let total_memory = sys.total_memory();
            let used_memory = sys.used_memory();

            let stats = SystemStats {
                cpu_percent: sys.global_cpu_usage(),
                mem_used_bytes: used_memory,
                mem_total_bytes: total_memory,
                mem_percent: if total_memory > 0 {
                    used_memory as f32 / total_memory as f32 * 100.0
                } else {
                    0.0
                },
                net_rx_bytes_per_sec: rx_bytes as f64 / secs,
                net_tx_bytes_per_sec: tx_bytes as f64 / secs,
            };

            let _ = app.emit(STATS_EVENT, stats);
        }
    });
}
