import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const offline = $state({
  /** Gespeicherte Songs: id → { …Track, path } */
  tracks: {},
  /** Gespeicherte Playlists/Alben: { id, kind, title, subtitle, artworkUrl, trackIds } */
  collections: [],
  /** Laufende Downloads: id → "queued" | "downloading" */
  pending: {},
  error: null,
});

const asTrack = (t) => ({ ...t, source: "youtube" });

async function refresh() {
  const view = await invoke("offline_list");
  apply(view);
}

function apply(view) {
  offline.tracks = Object.fromEntries(view.tracks.map((t) => [t.id, asTrack(t)]));
  offline.collections = view.collections;
}

let refreshTimer;
function refreshSoon() {
  clearTimeout(refreshTimer);
  refreshTimer = setTimeout(refresh, 300);
}

export async function initOffline() {
  await listen("offline-progress", ({ payload }) => {
    if (payload.state === "queued" || payload.state === "downloading") {
      offline.pending[payload.id] = payload.state;
      return;
    }
    delete offline.pending[payload.id];
    if (payload.state === "error") offline.error = payload.error;
    refreshSoon();
  });
  await refresh();
}

export const isOffline = (id) => Boolean(offline.tracks[id]?.path);
export const offlinePath = (id) => offline.tracks[id]?.path || null;
export const isSavedCollection = (id) => offline.collections.some((c) => c.id === id);

const trackInput = (t) => ({
  id: t.id,
  title: t.title,
  artist: t.artist ?? null,
  album: t.album ?? null,
  duration: t.duration || 0,
  artworkUrl: t.artworkUrl ?? null,
  url: t.url ?? null,
});

export async function saveCollection(info, tracks) {
  offline.error = null;
  await invoke("offline_save", {
    tracks: tracks.map(trackInput),
    collection: {
      id: info.id,
      kind: info.kind,
      title: info.title,
      subtitle: info.subtitle ?? null,
      artworkUrl: info.artworkUrl ?? null,
    },
  });
  await refresh();
}

export async function saveTrack(track) {
  offline.error = null;
  await invoke("offline_save", { tracks: [trackInput(track)], collection: null });
  await refresh();
}

export async function removeCollection(id) {
  apply(await invoke("offline_remove", { collectionId: id, trackIds: null }));
}

export async function removeTrack(id) {
  apply(await invoke("offline_remove", { collectionId: null, trackIds: [id] }));
}

/** Fortschritt einer gespeicherten Playlist: wie viele Songs schon heruntergeladen sind */
export function progressOf(trackIds) {
  const done = trackIds.filter((id) => isOffline(id)).length;
  const pending = trackIds.filter((id) => offline.pending[id]).length;
  return { done, pending, total: trackIds.length };
}

/** Songs einer gespeicherten Playlist (nur fertig heruntergeladene) */
export function tracksOf(collection) {
  return collection.trackIds.map((id) => offline.tracks[id]).filter((t) => t?.path);
}

export function allTracks() {
  return Object.values(offline.tracks)
    .filter((t) => t.path)
    .sort((a, b) => (a.artist ?? "").localeCompare(b.artist ?? "") || a.title.localeCompare(b.title));
}
