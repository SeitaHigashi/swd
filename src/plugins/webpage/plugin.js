// Embeds an arbitrary web page as a card via <iframe>.
//
// With "Transparent background" on (the default) the glass chrome is
// stripped and the iframe is left unpainted, so the wallpaper shows through
// wherever the *page itself* has no opaque background. Sites that set an
// opaque <body> background (or that send X-Frame-Options / frame-ancestors
// and refuse to be framed at all) can't be fixed from here - see
// docs/history.md "Webpage widget".

export default {
  id: "webpage-card",
  name: "Web page",
  position: { top: 32, right: 360 },
  styles: ["./style.css"],
  configSchema: [
    { key: "url", label: "URL (http/https)", type: "string", default: "" },
    { key: "width", label: "Width (px)", type: "number", default: 480 },
    { key: "height", label: "Height (px)", type: "number", default: 320 },
    { key: "transparent", label: "Transparent background", type: "boolean", default: true },
    // Must match the embedded page's own `color-scheme` (its `:root`
    // declaration, or `prefers-color-scheme` handling): when they differ,
    // the browser paints the iframe an opaque canvas and transparency is
    // lost. "dark" = page declares `color-scheme: dark`; "light" = page
    // declares nothing / `light`.
    {
      key: "pageColorScheme",
      label: "Page color scheme",
      type: "select",
      options: ["dark", "light"],
      default: "dark",
    },
  ],
  mount(ctx) {
    const { url, width, height, transparent, pageColorScheme } = ctx.config;

    // Only http(s): a `javascript:`/`file:` URL here would be a footgun,
    // not a feature. An unset/invalid URL hides the card entirely so a
    // fresh install doesn't grow an empty box on the desktop.
    let src = "";
    try {
      const parsed = new URL(String(url ?? "").trim());
      if (parsed.protocol === "http:" || parsed.protocol === "https:") src = parsed.href;
    } catch {
      // fall through to hidden state
    }
    if (!src) {
      ctx.root.hidden = true;
      return;
    }

    ctx.root.classList.toggle("transparent", !!transparent);
    ctx.root.style.width = `${Math.max(120, Number(width) || 480)}px`;

    ctx.root.innerHTML = `
      <div class="webpage-handle" title="Drag to move"></div>
      <div class="webpage-viewport">
        <iframe class="webpage-frame"
          sandbox="allow-scripts allow-same-origin allow-forms"
          referrerpolicy="no-referrer"></iframe>
      </div>
    `;

    const frame = ctx.el(".webpage-frame");
    ctx.el(".webpage-viewport").style.height = `${Math.max(80, Number(height) || 320)}px`;
    frame.style.colorScheme = pageColorScheme === "light" ? "normal" : "dark";
    frame.src = src;

    ctx.onDestroy(() => {
      // Stops the page's scripts/network immediately instead of waiting for GC.
      frame.src = "about:blank";
    });
  },
};
