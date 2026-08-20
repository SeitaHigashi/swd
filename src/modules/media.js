// Now-playing card. Talks to src-tauri/src/media.rs (Windows Media
// Transport Controls) via the `media://now-playing` event for state and a
// few plain commands for transport control.

const { invoke } = window.__TAURI__.core;

export function initMedia() {
  const thumbEl = document.querySelector("#media-thumb");
  const titleEl = document.querySelector("#media-title");
  const artistEl = document.querySelector("#media-artist");
  const toggleBtn = document.querySelector("#media-toggle");
  const prevBtn = document.querySelector("#media-prev");
  const nextBtn = document.querySelector("#media-next");

  toggleBtn?.addEventListener("click", () => {
    invoke("media_toggle_play_pause").catch((err) => console.error("media_toggle_play_pause failed", err));
  });
  prevBtn?.addEventListener("click", () => {
    invoke("media_previous").catch((err) => console.error("media_previous failed", err));
  });
  nextBtn?.addEventListener("click", () => {
    invoke("media_next").catch((err) => console.error("media_next failed", err));
  });

  return function onNowPlaying(nowPlaying) {
    const hasTrack = Boolean(nowPlaying && nowPlaying.title);

    if (!hasTrack) {
      if (titleEl) titleEl.textContent = "再生中のメディアはありません";
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
        thumbEl.hidden = false;
      } else {
        thumbEl.removeAttribute("src");
        thumbEl.hidden = true;
      }
    }
  };
}
