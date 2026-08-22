// Digital clock + date card. No backend data - just wall-clock time - so
// this is the simplest possible plugin: it doesn't touch ctx.on/invoke at
// all, only ctx.root.

export default {
  id: "clock-card",
  name: "Clock",
  position: { top: 32, right: 32 },
  styles: ["./style.css"],
  configSchema: [
    {
      key: "hourFormat",
      label: "Hour format",
      type: "select",
      options: ["24h", "12h"],
      default: "24h",
    },
  ],
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
      hour12: ctx.config.hourFormat === "12h",
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

    tick();
    const intervalId = setInterval(tick, 1000);
    ctx.onDestroy(() => clearInterval(intervalId));
  },
};
