// Digital clock + date card. No backend data - just wall-clock time - so
// this is the simplest possible plugin: it doesn't touch ctx.on/invoke at
// all, only ctx.root.

export default {
  id: "clock-card",
  name: "Clock",
  position: { top: 32, right: 32 },
  styles: ["./style.css"],
  mount(ctx) {
    ctx.root.innerHTML = `
      <p class="clock" id="clock">--:--:--</p>
      <p class="date" id="date">----年--月--日（-）</p>
    `;

    const timeEl = ctx.el("#clock");
    const dateEl = ctx.el("#date");

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
    const intervalId = setInterval(tick, 1000);
    ctx.onDestroy(() => clearInterval(intervalId));
  },
};
