// Network throughput card: current rate text + a rolling sparkline of
// recent up/down speed. Same `sys://stats` payload as system-monitor's
// plugin; this one just owns formatting and drawing it.

import { drawSparkline } from "../../shared/sparkline.js";

const HISTORY_LENGTH = 40; // ~1 minute of history at the 1.5s sample rate

function formatRate(bytesPerSec) {
  if (bytesPerSec >= 1024 * 1024) {
    return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
  }
  return `${(bytesPerSec / 1024).toFixed(0)} KB/s`;
}

export default {
  id: "network-card",
  name: "Network",
  position: { top: 460, right: 32 },
  styles: ["./style.css"],
  mount(ctx) {
    ctx.root.innerHTML = `
      <h2>Network</h2>
      <div class="net-row">
        <span class="net-arrow up">&#x25B2;</span>
        <span id="net-up">-- KB/s</span>
      </div>
      <div class="net-row">
        <span class="net-arrow down">&#x25BC;</span>
        <span id="net-down">-- KB/s</span>
      </div>
      <canvas id="net-graph" class="net-graph" width="256" height="56"></canvas>
    `;

    const upEl = ctx.el("#net-up");
    const downEl = ctx.el("#net-down");
    const graphEl = ctx.el("#net-graph");

    const rxHistory = [];
    const txHistory = [];

    ctx.on("sys://stats", (stats) => {
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
    });
  },
};
