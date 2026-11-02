import { invoke } from "@tauri-apps/api/core";
import { artworkSrc } from "./artwork.js";

/** Höchstens so viele lokale Cover im Speicher (die zuletzt benutzten bleiben) */
const MAX_CACHED = 300;
const cache = new Map();

/**
 * Liefert eine Bild-URL fürs Cover (oder null). Lokale Cover lädt Rust verkleinert:
 * kleine Größe für Listen, große für Player und Mini-Player.
 */
export function coverFor(track, size = 512) {
  if (!track) return Promise.resolve(null);
  if (track.artworkUrl) return Promise.resolve(artworkSrc(track.artworkUrl));
  const pixels = size <= 64 ? 128 : 512;
  const key = `${pixels}:${track.path}`;
  let cover = cache.get(key);
  if (cover) {
    cache.delete(key);
  } else {
    cover = invoke("get_cover", { path: track.path, size: pixels }).catch(() => null);
    if (cache.size >= MAX_CACHED) cache.delete(cache.keys().next().value);
  }
  cache.set(key, cover);
  return cover;
}
