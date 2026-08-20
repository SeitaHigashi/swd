// No bundler here (plain vanilla template), so we use the injected
// window.__TAURI__ global (enabled via "withGlobalTauri" in tauri.conf.json)
// instead of bare `import "@tauri-apps/api/..."` specifiers, which the
// WebView can't resolve without a bundler.
const { invoke } = window.__TAURI__.core;
const { getCurrentWindow } = window.__TAURI__.window;
const { listen } = window.__TAURI__.event;

import { initClock } from "./modules/clock.js";
import { initSystemMonitor } from "./modules/system-monitor.js";
import { initNetwork } from "./modules/network.js";
import { initMedia } from "./modules/media.js";
import { applySavedPosition, saveCardPosition } from "./modules/layout.js";

const appWindow = getCurrentWindow();

/**
 * Reports the screen-space rectangle of every glass card to the Rust side,
 * so it knows which pixels should keep receiving mouse input while
 * everything else stays click-through. Must be re-run whenever a card
 * moves or the layout otherwise changes.
 */
async function syncHitRegions() {
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

/**
 * Lets the user drag a glass card around the dashboard by its body (but not
 * by text the user might want to select, if selection is ever re-enabled).
 * Purely a CSS-position drag inside the page - the underlying OS window
 * never moves.
 */
function makeDraggable(card) {
  let dragging = false;
  let originX = 0;
  let originY = 0;
  let startLeft = 0;
  let startTop = 0;

  card.addEventListener("pointerdown", (event) => {
    // Let clicks on real controls (e.g. media transport buttons) through
    // instead of hijacking them into a drag.
    if (event.target.closest("button, input, a, select, textarea")) return;

    dragging = true;
    card.setPointerCapture(event.pointerId);
    originX = event.clientX;
    originY = event.clientY;
    const rect = card.getBoundingClientRect();
    startLeft = rect.left;
    startTop = rect.top;
    // Switch from the CSS `right`-anchored default to an explicit
    // left/top as soon as a drag starts, so the two positioning modes
    // never fight each other.
    card.style.right = "auto";
  });

  card.addEventListener("pointermove", (event) => {
    if (!dragging) return;
    const dx = event.clientX - originX;
    const dy = event.clientY - originY;
    card.style.left = `${startLeft + dx}px`;
    card.style.top = `${startTop + dy}px`;
  });

  const stopDragging = (event) => {
    if (!dragging) return;
    dragging = false;
    card.releasePointerCapture(event.pointerId);
    saveCardPosition(card.id, {
      left: parseFloat(card.style.left),
      top: parseFloat(card.style.top),
    });
    syncHitRegions();
  };

  card.addEventListener("pointerup", stopDragging);
  card.addEventListener("pointercancel", stopDragging);
}

window.addEventListener("DOMContentLoaded", () => {
  initClock("#clock", "#date");
  const onSystemStats = initSystemMonitor();
  const onNetworkStats = initNetwork();
  const onNowPlaying = initMedia();

  listen("sys://stats", (event) => {
    onSystemStats(event.payload);
    onNetworkStats(event.payload);
  });
  listen("media://now-playing", (event) => {
    onNowPlaying(event.payload);
  });

  document.querySelectorAll(".glass-card").forEach((card) => {
    applySavedPosition(card);
    makeDraggable(card);
  });

  syncHitRegions();
  window.addEventListener("resize", syncHitRegions);
  new ResizeObserver(syncHitRegions).observe(document.body);
});
