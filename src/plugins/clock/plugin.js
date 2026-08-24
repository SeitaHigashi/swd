// Digital clock + date card. No backend data - just wall-clock time - so
// this is the simplest possible plugin: it doesn't touch ctx.on/invoke at
// all, only ctx.root.
//
// It's the one plugin not gated by event-bus's visibility skip (it has no
// backend event to gate), so it pauses its own tick directly instead.

import { onVisibilityChange } from "../../core/visibility.js";

export default {
  id: "clock-card",
  name: "Clock",
  position: { top: 32, right: 32 },
  styles: ["./style.css"],
  mount(ctx) {
    ctx.root.innerHTML = `
      <p class="clock" id="clock">--:--:--</p>
      <p class="date" id="date">---, --- --, ----</p>
    `;

    const timeEl = ctx.el("#clock");
    const dateEl = ctx.el("#date");

    const timeFormatter = new Intl.DateTimeFormat("en-US", {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    });
    const dateFormatter = new Intl.DateTimeFormat("en-US", {
      year: "numeric",
      month: "short",
      day: "2-digit",
      weekday: "short",
    });

    const tick = () => {
      const now = new Date();
      if (timeEl) timeEl.textContent = timeFormatter.format(now);
      if (dateEl) dateEl.textContent = dateFormatter.format(now);
    };

    let intervalId = setInterval(tick, 1000);
    tick();

    // Stop the tick outright while hidden instead of just skipping the DOM
    // write - no point waking up every second for a clock nobody can see.
    const unsubscribeVisibility = onVisibilityChange((hidden) => {
      if (hidden) {
        clearInterval(intervalId);
      } else {
        tick();
        intervalId = setInterval(tick, 1000);
      }
    });
    ctx.onDestroy(() => {
      clearInterval(intervalId);
      unsubscribeVisibility();
    });
  },
};
