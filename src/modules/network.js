// Network throughput card: current rate text + a rolling sparkline of
// recent up/down speed. Same `sys://stats` payload as system-monitor.js;
// this module just owns formatting and drawing it.

import { drawSparkline } from "./sparkline.js";

const HISTORY_LENGTH = 40; // ~1 minute of history at the 1.5s sample rate

function formatRate(bytesPerSec) {
  if (bytesPerSec >= 1024 * 1024) {
    return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
  }
  return `${(bytesPerSec / 1024).toFixed(0)} KB/s`;
}

export function initNetwork() {
  const upEl = document.querySelector("#net-up");
  const downEl = document.querySelector("#net-down");
  const graphEl = document.querySelector("#net-graph");

  const rxHistory = [];
  const txHistory = [];

  return function onStats(stats) {
    if (upEl) upEl.textContent = formatRate(stats.net_tx_bytes_per_sec);
    if (downEl) downEl.textContent = formatRate(stats.net_rx_bytes_per_sec);

    rxHistory.push(stats.net_rx_bytes_per_sec);
    txHistory.push(stats.net_tx_bytes_per_sec);
    if (rxHistory.length > HISTORY_LENGTH) rxHistory.shift();
    if (txHistory.length > HISTORY_LENGTH) txHistory.shift();

    if (graphEl) {
      drawSparkline(
        graphEl,
        [
          { values: rxHistory, color: "rgba(120, 170, 255, 0.9)" },
          { values: txHistory, color: "rgba(255, 150, 120, 0.9)" },
        ],
        { length: HISTORY_LENGTH },
      );
    }
  };
}
