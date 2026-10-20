import { invoke } from "@tauri-apps/api/core";

const cache = new Map();

/** Wandelt LRC-Text („[01:23.45] Zeile“) in [{ time, text }] um, sortiert nach Zeit. */
export function parseLrc(lrc) {
  const lines = [];
  for (const raw of lrc.split(/\r?\n/)) {
    const stamps = [...raw.matchAll(/\[(\d+):(\d+(?:\.\d+)?)\]/g)];
    if (!stamps.length) continue;
    const text = raw.replace(/\[[^\]]*\]/g, "").trim();
    for (const [, min, sec] of stamps) {
      lines.push({ time: Number(min) * 60 + Number(sec), text });
    }
  }
  return lines.sort((a, b) => a.time - b.time);
}

/** Holt den Liedtext für einen Song (einmal pro Song, danach aus dem Cache). */
export function lyricsFor(track) {
  const key = `${track.source}:${track.id}`;
  if (!cache.has(key)) {
    const query = {
      title: track.title,
      artist: track.artist ?? null,
      album: track.album ?? null,
      duration: track.duration || 0,
      videoId: track.source === "youtube" ? track.id : null,
    };
    const request = invoke("get_lyrics", { query }).then((result) =>
      result ? { ...result, lines: result.synced ? parseLrc(result.synced) : null } : null,
    );
    // Fehler nicht dauerhaft cachen, damit ein erneuter Versuch möglich ist
    request.catch(() => cache.delete(key));
    cache.set(key, request);
  }
  return cache.get(key);
}
