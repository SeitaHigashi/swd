// CPU/memory card. Reads fields off the `sys://stats` payload emitted by
// the Rust backend (see src-tauri/src/system_info.rs) - doesn't fetch
// anything itself, so it stays trivial to swap for a different data
// source later.

import { drawSparkline } from "../../shared/sparkline.js";

const GIB = 1024 ** 3;
const HISTORY_LENGTH = 40; // ~1 minute of history at the 1.5s sample rate

function setMeter(fillEl, percent) {
  const clamped = Math.max(0, Math.min(100, percent));
  fillEl.style.width = `${clamped}%`;
  fillEl.classList.toggle("warn", clamped >= 85);
}

export default {
  id: "system-monitor-card",
  name: "System",
  position: { top: 170, right: 32 },
  styles: ["./style.css"],
  mount(ctx) {
    ctx.root.innerHTML = `
      <h2>System</h2>
      <div class="metric">
        <div class="metric-label">
          <span>CPU</span>
          <span id="cpu-value">--%</span>
        </div>
        <div class="meter"><div class="meter-fill" id="cpu-bar"></div></div>
        <canvas id="cpu-graph" class="metric-graph" width="256" height="36"></canvas>
      </div>
      <div class="metric">
        <div class="metric-label">
          <span>Memory</span>
          <span id="mem-value">-- / -- GB</span>
        </div>
        <div class="meter"><div class="meter-fill" id="mem-bar"></div></div>
        <canvas id="mem-graph" class="metric-graph" width="256" height="36"></canvas>
      </div>
    `;

    const cpuValueEl = ctx.el("#cpu-value");
    const cpuBarEl = ctx.el("#cpu-bar");
    const cpuGraphEl = ctx.el("#cpu-graph");
    const memValueEl = ctx.el("#mem-value");
    const memBarEl = ctx.el("#mem-bar");
    const memGraphEl = ctx.el("#mem-graph");

    const cpuHistory = [];
    const memHistory = [];

    ctx.on("sys://stats", (stats) => {
      if (cpuValueEl) cpuValueEl.textContent = `${stats.cpu_percent.toFixed(0)}%`;
      if (cpuBarEl) setMeter(cpuBarEl, stats.cpu_percent);

      if (memValueEl) {
        const usedGib = stats.mem_used_bytes / GIB;
        const totalGib = stats.mem_total_bytes / GIB;
        memValueEl.textContent = `${usedGib.toFixed(1)} / ${totalGib.toFixed(1)} GB`;
      }
      if (memBarEl) setMeter(memBarEl, stats.mem_percent);

      cpuHistory.push(stats.cpu_percent);
      memHistory.push(stats.mem_percent);
      if (cpuHistory.length > HISTORY_LENGTH) cpuHistory.shift();
      if (memHistory.length > HISTORY_LENGTH) memHistory.shift();

      // Percentages are always 0-100, so a fixed max keeps the graph's
      // scale stable instead of auto-zooming on quiet periods.
      if (cpuGraphEl) {
        drawSparkline(cpuGraphEl, [{ values: cpuHistory, color: "rgba(120, 170, 255, 0.9)" }], {
          max: 100,
          length: HISTORY_LENGTH,
        });
      }
      if (memGraphEl) {
        drawSparkline(memGraphEl, [{ values: memHistory, color: "rgba(190, 150, 255, 0.9)" }], {
          max: 100,
          length: HISTORY_LENGTH,
        });
      }
    });
  },
};
