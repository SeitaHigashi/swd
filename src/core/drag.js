// Card dragging. Purely a CSS-position drag inside the page - the
// underlying OS window never moves.

import { saveCardPosition } from "./layout.js";

// A plain click (pointerdown then pointerup with no real movement) must
// never be treated as a drag. Without this threshold, every click
// anywhere on a card's non-control area - the title, a text row, empty
// space between buttons - silently snapshotted the card's current
// position and saved it, permanently switching it off the plugin's
// right-anchored default. On a card with a lot of clickable-adjacent
// content (many appliance rows, links, sensor readouts), that turned
// into cards randomly "disappearing" days later - not actually gone, just
// pinned to wherever an incidental click happened to catch its layout.
const DRAG_THRESHOLD_PX = 4;

/**
 * Lets the user drag a glass card around the dashboard by its body.
 * @param {HTMLElement} card
 * @param {() => void} onMoved called once per completed drag
 */
export function makeDraggable(card, onMoved) {
  let dragging = false;
  let moved = false;
  let originX = 0;
  let originY = 0;
  let startLeft = 0;
  let startTop = 0;

  card.addEventListener("pointerdown", (event) => {
    // Let clicks on real controls (e.g. media transport buttons) through
    // instead of hijacking them into a drag.
    if (event.target.closest("button, input, a, select, textarea")) return;

    dragging = true;
    moved = false;
    card.setPointerCapture(event.pointerId);
    originX = event.clientX;
    originY = event.clientY;
    const rect = card.getBoundingClientRect();
    startLeft = rect.left;
    startTop = rect.top;
  });

  card.addEventListener("pointermove", (event) => {
    if (!dragging) return;
    const dx = event.clientX - originX;
    const dy = event.clientY - originY;

    if (!moved) {
      if (Math.abs(dx) < DRAG_THRESHOLD_PX && Math.abs(dy) < DRAG_THRESHOLD_PX) return;
      moved = true;
      // Switch from the `right`-anchored default to an explicit left/top
      // only once this is confirmed to be a real drag, not a click.
      card.style.right = "auto";
    }

    card.style.left = `${startLeft + dx}px`;
    card.style.top = `${startTop + dy}px`;
  });

  const stopDragging = (event) => {
    if (!dragging) return;
    dragging = false;
    card.releasePointerCapture(event.pointerId);
    if (!moved) return;
    saveCardPosition(card.id, {
      left: parseFloat(card.style.left),
      top: parseFloat(card.style.top),
    });
    onMoved?.();
  };

  card.addEventListener("pointerup", stopDragging);
  card.addEventListener("pointercancel", stopDragging);
}
