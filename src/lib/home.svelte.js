import { invoke } from "@tauri-apps/api/core";
import { history } from "./history.svelte.js";

/** Daten für die Startseite: aus dem eigenen Wavely-Verlauf und den YouTube-Music-Charts. */

export const home = $state({
  charts: null,
  loading: false,
  error: null,
});

let loadedAt = 0;

/** Charts & Neuerscheinungen (höchstens alle 30 Minuten neu laden) */
export async function loadCharts(force = false) {
  if (home.loading || (!force && home.charts && Date.now() - loadedAt < 30 * 60_000)) return;
  home.loading = true;
  home.error = null;
  try {
    home.charts = await invoke("ytm_home");
    loadedAt = Date.now();
  } catch (e) {
    home.error = String(e);
  } finally {
    home.loading = false;
  }
}

const keyOf = (track) => `${track.source}:${track.id}`;
const firstArtist = (track) => track.artists?.[0]?.name ?? (track.artist ?? "").split(",")[0].trim();

/** Zuletzt gehört, ohne Wiederholungen */
export function recentTracks(limit = 12) {
  const seen = new Set();
  const out = [];
  for (const { track } of history.entries) {
    const key = keyOf(track);
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(track);
    if (out.length >= limit) break;
  }
  return out;
}

/** Meistgehörte Songs der letzten `days` Tage */
export function topTracks(days = 30, limit = 10) {
  const since = Date.now() - days * 24 * 60 * 60 * 1000;
  const counts = new Map();
  for (const { track, playedAt } of history.entries) {
    if (playedAt < since) continue;
    const key = keyOf(track);
    const entry = counts.get(key) ?? { track, plays: 0 };
    entry.plays++;
    counts.set(key, entry);
  }
  return [...counts.values()].sort((a, b) => b.plays - a.plays).slice(0, limit);
}

/** Meistgehörte Künstler – mit einem Song als Startpunkt für einen Mix */
export function topArtists(days = 30, limit = 6) {
  const since = Date.now() - days * 24 * 60 * 60 * 1000;
  const counts = new Map();
  for (const { track, playedAt } of history.entries) {
    if (playedAt < since) continue;
    const name = firstArtist(track);
    if (!name) continue;
    const key = name.toLowerCase();
    const entry = counts.get(key) ?? { name, plays: 0, seed: track, tracks: new Map() };
    entry.plays++;
    entry.tracks.set(keyOf(track), (entry.tracks.get(keyOf(track)) ?? 0) + 1);
    // Bevorzugt einen YouTube-Song als Startpunkt (Mix kommt von YouTube Music)
    if (entry.seed.source !== "youtube" && track.source === "youtube") entry.seed = track;
    counts.set(key, entry);
  }
  return [...counts.values()].sort((a, b) => b.plays - a.plays).slice(0, limit);
}

/** Tageszeit-Gruß */
export function greeting() {
  const h = new Date().getHours();
  if (h < 5) return "Gute Nacht";
  if (h < 11) return "Guten Morgen";
  if (h < 18) return "Guten Tag";
  return "Guten Abend";
}
