// Digital clock + date module. Self-contained: pass it selectors and it
// owns updating those elements from then on.
export function initClock(timeSelector, dateSelector) {
  const timeEl = document.querySelector(timeSelector);
  const dateEl = dateSelector ? document.querySelector(dateSelector) : null;

  const timeFormatter = new Intl.DateTimeFormat("ja-JP", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
  const dateFormatter = new Intl.DateTimeFormat("ja-JP", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    weekday: "short",
  });

  const tick = () => {
    const now = new Date();
    if (timeEl) timeEl.textContent = timeFormatter.format(now);
    if (dateEl) dateEl.textContent = dateFormatter.format(now);
  };

  tick();
  setInterval(tick, 1000);
}
