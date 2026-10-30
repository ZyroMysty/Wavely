import { invoke } from "@tauri-apps/api/core";
import { showToast } from "./ui.svelte.js";

/**
 * Wavely-Playlists: Songs aus allen Quellen (lokal, YouTube, Spotify, SoundCloud) gemischt.
 * Gespeichert als playlists.json im App-Ordner. Jede Playlist und jeder Eintrag hat eine
 * eindeutige ID – damit lassen sie sich später zwischen Geräten abgleichen.
 */

export const playlists = $state({ list: [], loaded: false });

const newId = () => crypto.randomUUID();

/** Nur die Felder, die zum Abspielen und Anzeigen nötig sind */
function slim(track) {
  const { qid, auto, ytId, ...rest } = track;
  return rest;
}

let markReady;
/** Erfüllt, sobald die Playlists von der Platte geladen sind (der Abgleich wartet darauf) */
export const playlistsReady = new Promise((resolve) => (markReady = resolve));

export async function initPlaylists() {
  try {
    const text = await invoke("playlists_load");
    const data = text ? JSON.parse(text) : { playlists: [] };
    playlists.list = Array.isArray(data.playlists) ? data.playlists : [];
  } catch (e) {
    console.error("Playlists:", e);
  }
  playlists.loaded = true;
  markReady();
}

/** Für den Abgleich mit dem Wavely-Konto */
const listeners = new Set();
let deletedIds = [];
export function onPlaylistsChanged(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

let saveTimer;
let notifyPending = false;
function save(notify = true) {
  clearTimeout(saveTimer);
  notifyPending ||= notify;
  saveTimer = setTimeout(() => {
    if (notifyPending) {
      notifyPending = false;
      const deleted = deletedIds;
      deletedIds = [];
      listeners.forEach((fn) => fn(playlists.list, deleted));
    }
    const json = JSON.stringify({ version: 1, playlists: $state.snapshot(playlists.list) });
    invoke("playlists_save", { json }).catch((e) => showToast(`Playlists konnten nicht gespeichert werden: ${e}`, true));
  }, 300);
}

export const getPlaylist = (id) => playlists.list.find((p) => p.id === id) ?? null;

export function createPlaylist(name, tracks = []) {
  const now = Date.now();
  const playlist = {
    id: newId(),
    name: name.trim() || "Neue Playlist",
    description: "",
    createdAt: now,
    updatedAt: now,
    items: tracks.map((t) => ({ id: newId(), addedAt: now, track: slim(t) })),
  };
  playlists.list.unshift(playlist);
  save();
  return playlist;
}

export function renamePlaylist(id, name) {
  const p = getPlaylist(id);
  if (!p || !name.trim()) return;
  p.name = name.trim();
  p.updatedAt = Date.now();
  save();
}

export function deletePlaylist(id) {
  playlists.list = playlists.list.filter((p) => p.id !== id);
  deletedIds.push(id);
  save();
}

/** Stand aus der Cloud übernehmen: neuere Fassungen ersetzen, gelöschte entfernen */
export function applyRemotePlaylists(remote) {
  let changed = false;
  for (const r of remote) {
    const index = playlists.list.findIndex((p) => p.id === r.id);
    const mine = playlists.list[index];
    if (r.deleted) {
      if (mine && mine.updatedAt <= r.updatedAt) {
        playlists.list.splice(index, 1);
        changed = true;
      }
      continue;
    }
    if (mine && mine.updatedAt >= r.updatedAt) continue;
    const { deleted, ...data } = r;
    // Lokale Dateien: Pfad vom eigenen PC behalten, falls der Song hier existiert
    data.items = (data.items ?? []).map((item) => {
      const ownItem = mine?.items.find((i) => i.id === item.id);
      return ownItem?.track.path ? { ...item, track: { ...item.track, path: ownItem.track.path } } : item;
    });
    if (mine) playlists.list[index] = data;
    else playlists.list.push(data);
    changed = true;
  }
  if (changed) {
    playlists.list.sort((a, b) => b.createdAt - a.createdAt);
    save(false);
  }
}

/** Songs anhängen; gibt die Anzahl der neu hinzugefügten zurück (Doppelte werden übersprungen) */
export function addTracks(id, tracks) {
  const p = getPlaylist(id);
  if (!p) return 0;
  const existing = new Set(p.items.map((i) => `${i.track.source}:${i.track.id}`));
  const now = Date.now();
  let added = 0;
  for (const t of tracks) {
    const key = `${t.source}:${t.id}`;
    if (existing.has(key)) continue;
    existing.add(key);
    p.items.push({ id: newId(), addedAt: now, track: slim(t) });
    added++;
  }
  if (added) {
    p.updatedAt = now;
    save();
  }
  return added;
}

export function removeItem(id, index) {
  const p = getPlaylist(id);
  if (!p) return;
  p.items.splice(index, 1);
  p.updatedAt = Date.now();
  save();
}

export function moveItem(id, from, to) {
  const p = getPlaylist(id);
  if (!p || from === to || to < 0 || to >= p.items.length) return;
  const [item] = p.items.splice(from, 1);
  p.items.splice(to, 0, item);
  p.updatedAt = Date.now();
  save();
}

/** Bis zu vier Cover für das Mosaik-Bild */
export function coversOf(playlist) {
  const seen = new Set();
  const out = [];
  for (const { track } of playlist.items) {
    if (!track.artworkUrl || seen.has(track.artworkUrl)) continue;
    seen.add(track.artworkUrl);
    out.push(track);
    if (out.length === 4) break;
  }
  return out;
}
