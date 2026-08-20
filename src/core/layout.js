// Persists per-card positions across restarts so a manually dragged card
// stays where the user put it. Swap the storage backend here (e.g. for a
// Rust-side config file) without touching main.js.

const STORAGE_PREFIX = "swd:card-position:";

export function loadCardPosition(id) {
  try {
    const raw = localStorage.getItem(STORAGE_PREFIX + id);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

export function saveCardPosition(id, { left, top }) {
  localStorage.setItem(STORAGE_PREFIX + id, JSON.stringify({ left, top }));
}

export function resetCardPosition(id) {
  localStorage.removeItem(STORAGE_PREFIX + id);
}

/**
 * Applies a previously saved position to a card, switching it from its CSS
 * default (right-anchored) to an explicit left/top. No-op if the user has
 * never moved this card.
 */
export function applySavedPosition(card) {
  const saved = loadCardPosition(card.id);
  if (!saved) return;
  card.style.right = "auto";
  card.style.left = `${saved.left}px`;
  card.style.top = `${saved.top}px`;
}
