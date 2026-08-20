// Card dragging. Purely a CSS-position drag inside the page - the
// underlying OS window never moves.

import { saveCardPosition } from "./layout.js";

/**
 * Lets the user drag a glass card around the dashboard by its body.
 * @param {HTMLElement} card
 * @param {() => void} onMoved called once per completed drag
 */
export function makeDraggable(card, onMoved) {
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
    // Switch from the `right`-anchored default to an explicit left/top as
    // soon as a drag starts, so the two positioning modes never fight.
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
    onMoved?.();
  };

  card.addEventListener("pointerup", stopDragging);
  card.addEventListener("pointercancel", stopDragging);
}
