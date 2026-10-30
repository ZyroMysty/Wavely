import { invoke } from "@tauri-apps/api/core";
import { history } from "./history.svelte.js";

/**
 * Hörstatistik für den Rückblick: Jedes Hören wird mit der wirklich gehörten Zeit in
 * stats/plays.jsonl gespeichert (ohne Obergrenze, anders als der Verlauf).
 * Kurze Einträge: t = Start (ms), s = gehörte Sekunden, d = Länge, src/id/title/artist/album/art = Song.
 */

const MIN_SECONDS = 10;
/** Sicherung im Browser-Speicher (synchron): überlebt auch das Schließen mitten im Song */
const BACKUP_KEY = "statsBackup";

let current = null;
let pending = [];
let flushTimer;
let lastBackup = 0;

function entryOf({ track, startedAt, listened }) {
  return {
    t: startedAt,
    s: Math.round(listened),
    d: Math.round(track.duration || 0),
    src: track.source,
    id: String(track.id),
    title: track.title,
    artist: track.artist ?? null,
    album: track.album ?? null,
    art: track.artworkUrl ?? null,
  };
}

function backup() {
  const list = [...pending];
  if (current && current.listened >= MIN_SECONDS) list.push(entryOf(current));
  try {
    if (list.length) localStorage.setItem(BACKUP_KEY, JSON.stringify(list));
    else localStorage.removeItem(BACKUP_KEY);
  } catch {}
}

export function statsStarted(track) {
  statsFlush();
  current = { track, startedAt: Date.now(), listened: 0, lastPos: 0 };
}

/** Position (Sekunden) – nur zusammenhängend gehörte Zeit zählt, Springen nicht */
export function statsProgress(position) {
  if (!current) return;
  const delta = position - current.lastPos;
  if (delta > 0 && delta < 5) current.listened += delta;
  current.lastPos = position;
  if (Date.now() - lastBackup > 5000) {
    lastBackup = Date.now();
    backup();
  }
}

/** Laufenden Song abschließen (Songwechsel, Ende, Beenden) */
export function statsFlush() {
  if (current && current.listened >= MIN_SECONDS) {
    pending.push(entryOf(current));
    clearTimeout(flushTimer);
    flushTimer = setTimeout(write, 500);
  }
  current = null;
  backup();
}

/** Für den Abgleich mit dem Wavely-Konto: neue Einträge melden */
const listeners = new Set();
export function onPlaysWritten(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Einträge von anderen Geräten in die Datei übernehmen */
export async function appendPlays(entries) {
  await invoke("stats_append", { entries });
}

function write() {
  if (!pending.length) return;
  const entries = pending;
  pending = [];
  invoke("stats_append", { entries })
    .then(() => listeners.forEach((fn) => fn(entries)))
    .then(backup)
    .catch(() => (pending = [...entries, ...pending]));
}

// Beim Schließen: Sicherung aktualisieren (das Schreiben in die Datei schafft es evtl. nicht mehr)
window.addEventListener("pagehide", backup);

/** Einträge ab `since` (ms) laden */
export async function loadPlays(since = 0) {
  return invoke("stats_load", { since });
}

/** Beim ersten Start: bisherigen Verlauf übernehmen (gehörte Zeit geschätzt = Songlänge) */
export async function initStats() {
  try {
    const existing = await loadPlays(0);
    // Vom letzten Mal übrig (z. B. Wavely mitten im Song geschlossen)
    let leftover = [];
    try {
      leftover = JSON.parse(localStorage.getItem(BACKUP_KEY) ?? "[]");
      localStorage.removeItem(BACKUP_KEY);
    } catch {}
    if (existing.length || !history.entries.length) {
      if (leftover.length) await invoke("stats_append", { entries: leftover });
      return;
    }
    const entries = [...history.entries].reverse().map(({ track, playedAt }) => ({
      t: playedAt,
      s: Math.round(track.duration || 180),
      d: Math.round(track.duration || 0),
      src: track.source,
      id: String(track.id),
      title: track.title,
      artist: track.artist ?? null,
      album: track.album ?? null,
      art: track.artworkUrl ?? null,
      est: true,
    }));
    await invoke("stats_append", { entries });
  } catch (e) {
    console.warn("Statistik:", e);
  }
}

// --- Auswertung für den Rückblick --------------------------------------------

const firstArtist = (p) => (p.artist ?? "").split(",")[0].trim();

/** Alles, was der Rückblick anzeigt, aus einer Liste von Einträgen */
export function summarize(plays, since) {
  const songs = new Map();
  const artists = new Map();
  const sources = {};
  const hours = new Array(24).fill(0);
  const weekdays = new Array(7).fill(0);
  const days = new Set();
  let seconds = 0;

  for (const p of plays) {
    seconds += p.s;
    const key = `${p.src}:${p.id}`;
    const song = songs.get(key) ?? { ...p, plays: 0, seconds: 0 };
    song.plays++;
    song.seconds += p.s;
    songs.set(key, song);

    const name = firstArtist(p);
    if (name) {
      const k = name.toLowerCase();
      const artist = artists.get(k) ?? { name, plays: 0, seconds: 0, art: p.art, songs: new Set() };
      artist.plays++;
      artist.seconds += p.s;
      artist.songs.add(key);
      artists.set(k, artist);
    }

    sources[p.src] = (sources[p.src] ?? 0) + p.s;
    const date = new Date(p.t);
    hours[date.getHours()] += p.s;
    weekdays[(date.getDay() + 6) % 7] += p.s; // Montag zuerst
    days.add(date.toDateString());
  }

  // Längste Serie: aufeinanderfolgende Tage mit Musik
  const dayNumbers = [...days].map((d) => Math.floor(new Date(d).getTime() / 86_400_000)).sort((a, b) => a - b);
  let streak = 0;
  let run = 0;
  dayNumbers.forEach((d, i) => {
    run = i && d === dayNumbers[i - 1] + 1 ? run + 1 : 1;
    streak = Math.max(streak, run);
  });

  return {
    minutes: Math.round(seconds / 60),
    plays: plays.length,
    songCount: songs.size,
    artistCount: artists.size,
    days: days.size,
    streak,
    topSongs: [...songs.values()].sort((a, b) => b.plays - a.plays || b.seconds - a.seconds).slice(0, 10),
    topArtists: [...artists.values()]
      .sort((a, b) => b.seconds - a.seconds)
      .slice(0, 10)
      .map((a) => ({ ...a, songs: a.songs.size })),
    sources: Object.entries(sources)
      .map(([src, s]) => ({ src, share: seconds ? s / seconds : 0 }))
      .sort((a, b) => b.share - a.share),
    hours,
    weekdays,
    since,
  };
}
