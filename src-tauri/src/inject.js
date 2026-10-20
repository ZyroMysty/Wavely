// Läuft in jeder geladenen Seite, aktiv aber nur auf music.youtube.com im Hauptframe.
(function () {
  if (location.hostname !== "music.youtube.com" || window.top !== window) return;

  const invoke = (cmd, args) =>
    window.__TAURI_INTERNALS__?.invoke(cmd, args).catch(() => {});

  let lastKey = null;
  let lastStart = 0;

  function readTrack() {
    const md = navigator.mediaSession?.metadata;
    const video = document.querySelector("video");
    if (!md || !md.title || !video || !video.src) return null;

    const artwork = [...(md.artwork || [])].pop()?.src || null;
    const videoId = document.getElementById("movie_player")?.getVideoData?.()?.video_id;

    return {
      title: md.title,
      artist: md.artist || "Unbekannt",
      album: md.album || null,
      artwork,
      url: videoId ? `https://music.youtube.com/watch?v=${videoId}` : null,
      position: video.currentTime || 0,
      duration: Number.isFinite(video.duration) ? video.duration : 0,
      paused: video.paused,
    };
  }

  setInterval(() => {
    const track = readTrack();
    if (!track) {
      if (lastKey !== null) {
        lastKey = null;
        invoke("clear_presence");
      }
      return;
    }

    // Nur bei Änderungen senden: neuer Titel, Pause/Play oder Sprung im Song.
    const key = `${track.title}|${track.artist}|${track.paused}`;
    const start = Date.now() / 1000 - track.position;
    if (key === lastKey && (track.paused || Math.abs(start - lastStart) < 3)) return;

    lastKey = key;
    lastStart = start;
    invoke("update_presence", { track });
  }, 1000);
})();
