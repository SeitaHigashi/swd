// Storage card. Reads the `disks` array off the `sys://stats` payload
// emitted by the Rust backend (see src-tauri/src/system_info.rs) - same
// event the system-monitor and network cards already listen on, so this
// adds no extra sampling cost.

const GIB = 1024 ** 3;

function setMeter(fillEl, percent) {
  const clamped = Math.max(0, Math.min(100, percent));
  fillEl.style.width = `${clamped}%`;
  fillEl.classList.toggle("warn", clamped >= 85);
}

function driveLabel(disk) {
  // Windows mount points come back like "C:\\" - trim to "C:" for a
  // compact label; fall back to the raw mount point for anything else.
  const trimmed = disk.mount_point.replace(/\\$/, "");
  const letter = trimmed || disk.mount_point || "Disk";
  // disk.name is the volume label (e.g. "Windows", "Data") and is empty
  // for unlabeled drives - only append it when there's something to show.
  return disk.name ? `${letter} (${disk.name})` : letter;
}

export default {
  id: "storage-card",
  name: "Storage",
  position: { top: 320, right: 32 },
  styles: ["./style.css"],
  mount(ctx) {
    ctx.root.innerHTML = `
      <h2>Storage</h2>
      <div id="storage-list" class="storage-list">
        <div class="storage-empty">Loading disks...</div>
      </div>
    `;

    const listEl = ctx.el("#storage-list");
    let renderedMountPoints = null;

    ctx.on("sys://stats", (stats) => {
      const disks = stats.disks || [];
      if (!listEl) return;

      const mountPoints = disks.map((disk) => disk.mount_point).join("|");
      if (mountPoints !== renderedMountPoints) {
        renderedMountPoints = mountPoints;
        listEl.innerHTML = disks.length
          ? disks
              .map(
                (disk, i) => `
              <div class="metric">
                <div class="metric-label">
                  <span>${driveLabel(disk)}</span>
                  <span class="storage-value" data-index="${i}">-- / -- GB</span>
                </div>
                <div class="meter"><div class="meter-fill" data-index="${i}"></div></div>
              </div>
            `,
              )
              .join("")
          : `<div class="storage-empty">No disks found</div>`;
      }

      disks.forEach((disk, i) => {
        const valueEl = listEl.querySelector(`.storage-value[data-index="${i}"]`);
        const fillEl = listEl.querySelector(`.meter-fill[data-index="${i}"]`);
        if (valueEl) {
          const usedGib = disk.used_bytes / GIB;
          const totalGib = disk.total_bytes / GIB;
          valueEl.textContent = `${usedGib.toFixed(0)} / ${totalGib.toFixed(0)} GB`;
        }
        if (fillEl) setMeter(fillEl, disk.percent);
      });
    });
  },
};
