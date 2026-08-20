// Reports the screen-space rectangle of every glass card to the Rust side,
// so it knows which pixels should keep receiving mouse input while
// everything else stays click-through (see src-tauri/src/hit_test.rs).
//
// Must be re-run whenever a card moves, resizes, or is added/removed -
// which under the plugin system can happen at any time, not just at
// startup, so this also watches the DOM instead of only wiring `resize`.

const { invoke } = window.__TAURI__.core;
const { getCurrentWindow } = window.__TAURI__.window;

const appWindow = getCurrentWindow();

async function computeAndSend() {
  const scale = window.devicePixelRatio || 1;
  const winPos = await appWindow.outerPosition();

  const rects = Array.from(document.querySelectorAll(".glass-card")).map((el) => {
    const r = el.getBoundingClientRect();
    return {
      x: winPos.x + r.left * scale,
      y: winPos.y + r.top * scale,
      width: r.width * scale,
      height: r.height * scale,
    };
  });

  try {
    await invoke("set_hit_regions", { rects });
  } catch (err) {
    console.error("set_hit_regions failed", err);
  }
}

// Coalesce bursts (e.g. several plugins mounting back to back, or a drag
// producing many pointermove-driven layout changes) into one IPC call.
let pending = false;
export function syncHitRegions() {
  if (pending) return;
  pending = true;
  queueMicrotask(() => {
    pending = false;
    computeAndSend();
  });
}

/**
 * Starts watching `container` for anything that could change a card's
 * on-screen rectangle: cards being added/removed by the plugin loader,
 * cards resizing, or the whole window resizing.
 */
export function startHitRegionWatcher(container) {
  syncHitRegions();

  window.addEventListener("resize", syncHitRegions);

  new MutationObserver(syncHitRegions).observe(container, {
    childList: true,
    subtree: false,
  });

  new ResizeObserver(syncHitRegions).observe(document.body);
}
