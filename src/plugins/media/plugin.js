// Now-playing card. Talks to src-tauri/src/media.rs (Windows Media
// Transport Controls) via the `media://now-playing` event for state and a
// few plain commands for transport control.

export default {
  id: "media-card",
  name: "Now Playing",
  position: { top: 660, right: 32 },
  styles: ["./style.css"],
  permissions: {
    invoke: ["media_toggle_play_pause", "media_previous", "media_next"],
  },
  configSchema: [{ key: "showThumbnail", label: "Show album art", type: "boolean", default: true }],
  mount(ctx) {
    ctx.root.innerHTML = `
      <h2>Now Playing</h2>
      <div class="media-body">
        <img id="media-thumb" class="media-thumb" alt="" hidden />
        <div class="media-info">
          <p class="media-title" id="media-title">Nothing playing</p>
          <p class="media-artist" id="media-artist"></p>
        </div>
      </div>
      <div class="media-controls">
        <button id="media-prev" class="media-btn" type="button" aria-label="Previous">&#x23EE;</button>
        <button id="media-toggle" class="media-btn media-btn-primary" type="button" aria-label="Play/Pause">&#x23EF;</button>
        <button id="media-next" class="media-btn" type="button" aria-label="Next">&#x23ED;</button>
      </div>
    `;

    const thumbEl = ctx.el("#media-thumb");
    const titleEl = ctx.el("#media-title");
    const artistEl = ctx.el("#media-artist");
    const toggleBtn = ctx.el("#media-toggle");
    const prevBtn = ctx.el("#media-prev");
    const nextBtn = ctx.el("#media-next");

    toggleBtn?.addEventListener("click", () => {
      ctx.invoke("media_toggle_play_pause").catch((err) => console.error("media_toggle_play_pause failed", err));
    });
    prevBtn?.addEventListener("click", () => {
      ctx.invoke("media_previous").catch((err) => console.error("media_previous failed", err));
    });
    nextBtn?.addEventListener("click", () => {
      ctx.invoke("media_next").catch((err) => console.error("media_next failed", err));
    });

    ctx.on("media://now-playing", (nowPlaying) => {
      const hasTrack = Boolean(nowPlaying && nowPlaying.title);

      if (!hasTrack) {
        if (titleEl) titleEl.textContent = "Nothing playing";
        if (artistEl) artistEl.textContent = "";
        if (thumbEl) thumbEl.hidden = true;
        if (toggleBtn) toggleBtn.textContent = "⏯";
        return;
      }

      if (titleEl) titleEl.textContent = nowPlaying.title;
      if (artistEl) artistEl.textContent = nowPlaying.artist;
      if (toggleBtn) toggleBtn.textContent = nowPlaying.status === "playing" ? "⏸" : "▶";

      if (thumbEl) {
        if (nowPlaying.thumbnail_data_url) {
          thumbEl.src = nowPlaying.thumbnail_data_url;
          thumbEl.hidden = !ctx.config.showThumbnail;
        } else {
          thumbEl.removeAttribute("src");
          thumbEl.hidden = true;
        }
      }
    });
  },
};
