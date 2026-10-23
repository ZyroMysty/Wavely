import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { settings } from "./settings.svelte.js";
import { showToast } from "./ui.svelte.js";

/**
 * Last.fm: „Hört gerade“ und Scrobbeln nach den Last.fm-Regeln
 * (Song länger als 30 s, gehört bis zur Hälfte oder 4 Minuten – je nachdem, was früher kommt).
 */

export const lastfm = $state({
  /** Last.fm ist in diesem Build eingerichtet */
  available: false,
  user: null,
  pending: 0,
  loggingIn: false,
  error: null,
});

export async function initLastFm() {
  await refresh();
  await listen("lastfm-auth", async (e) => {
    lastfm.loggingIn = false;
    if (e.payload.loggedIn) {
      await refresh();
      showToast(`Mit Last.fm verbunden: ${lastfm.user}`);
    } else if (e.payload.error) {
      lastfm.error = e.payload.error;
    }
  });
}

async function refresh() {
  try {
    const status = await invoke("lastfm_status");
    lastfm.available = status.available;
    lastfm.user = status.user;
    lastfm.pending = status.pending;
  } catch {}
}

export async function login() {
  lastfm.error = null;
  lastfm.loggingIn = true;
  try {
    await invoke("lastfm_login");
  } catch (e) {
    lastfm.loggingIn = false;
    lastfm.error = String(e);
  }
}

export async function logout() {
  await invoke("lastfm_logout").catch(() => {});
  await refresh();
}

const active = () => lastfm.user && settings.lastfmScrobble;

/** Erster Künstler (Wavely speichert mehrere Künstler mit Komma getrennt) */
function scrobbleData(track, startedAt) {
  const artist = track.artists?.[0]?.name ?? (track.artist ?? "").split(",")[0].trim();
  return {
    artist,
    title: track.title,
    album: track.album ?? null,
    duration: track.duration ? Math.round(track.duration) : null,
    timestamp: Math.floor(startedAt / 1000),
  };
}

// Laufender Song
let current = null;

/** Vom Player: neuer Song läuft */
export function trackStarted(track) {
  current = { track, startedAt: Date.now(), listened: 0, lastPos: 0, done: false };
  if (!active() || !scrobbleData(track, 0).artist) return;
  invoke("lastfm_now_playing", { track: scrobbleData(track, current.startedAt) }).catch(() => {});
}

/** Vom Player: Position (Sekunden) – zählt nur wirklich gehörte Zeit, Springen zählt nicht */
export function trackProgress(position, duration) {
  if (!current || current.done) return;
  const delta = position - current.lastPos;
  if (delta > 0 && delta < 5) current.listened += delta;
  current.lastPos = position;
  const length = duration || current.track.duration || 0;
  if (length < 30) return;
  if (current.listened >= Math.min(length / 2, 240)) {
    current.done = true;
    scrobble(current.track, current.startedAt);
  }
}

async function scrobble(track, startedAt) {
  if (!active()) return;
  const data = scrobbleData(track, startedAt);
  if (!data.artist) return;
  try {
    await invoke("lastfm_scrobble", { track: data });
  } catch (e) {
    lastfm.error = String(e);
  }
  refresh();
}
