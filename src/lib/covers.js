import { invoke } from "@tauri-apps/api/core";

const cache = new Map();

/** Liefert eine Bild-URL fürs Cover (oder null). Lokale Cover werden einmal geladen und gecacht. */
export function coverFor(track) {
  if (!track) return Promise.resolve(null);
  if (track.artworkUrl) return Promise.resolve(track.artworkUrl);
  if (!cache.has(track.path)) {
    cache.set(track.path, invoke("get_cover", { path: track.path }).catch(() => null));
  }
  return cache.get(track.path);
}
