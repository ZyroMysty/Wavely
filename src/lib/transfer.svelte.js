import { invoke } from "@tauri-apps/api/core";
import { ytm, loadLibrary as loadYtmLibrary } from "./ytm.svelte.js";
import { spotify, loadLibrary as loadSpotifyLibrary } from "./spotify.svelte.js";

/**
 * Playlist von einem Dienst zum anderen übertragen (YouTube Music ↔ Spotify, SoundCloud → beide).
 * Jeder Song wird beim Ziel gesucht (gleicher Titel, Künstler, ähnliche Länge); nicht gefundene
 * Songs werden am Ende aufgelistet.
 */

export const TARGETS = [
  { id: "youtube", name: "YouTube Music" },
  { id: "spotify", name: "Spotify" },
];

export const transfer = $state({
  /** Quelle: { title, source, tracks } oder null (Dialog zu) */
  from: null,
  /** "idle" | "running" | "done" | "error" */
  status: "idle",
  done: 0,
  total: 0,
  missing: [],
  /** Ergebnis: { target, id, title, count } */
  result: null,
  error: null,
});

export function openTransfer(collection, source) {
  Object.assign(transfer, {
    from: { title: collection.title, source, tracks: [...collection.tracks] },
    status: "idle",
    done: 0,
    total: 0,
    missing: [],
    result: null,
    error: null,
  });
}

export function closeTransfer() {
  if (transfer.status === "running") return;
  transfer.from = null;
}

export function targetAvailable(id) {
  return id === "youtube" ? ytm.status === "loggedIn" : spotify.status === "loggedIn";
}

/** Video-ID bei YouTube Music (eigener Song oder Zuordnung wie bei der Wiedergabe) */
async function youtubeId(track) {
  if (track.source === "youtube") return track.id;
  return invoke("spotify_resolve", {
    id: track.source === "spotify" ? track.id : `${track.source}:${track.id}`,
    title: track.title,
    artist: track.artist ?? null,
    duration: track.duration || 0,
  });
}

async function spotifyId(track) {
  if (track.source === "spotify") return track.id;
  const found = await invoke("spotify_match", {
    title: track.title,
    artist: track.artist ?? null,
    duration: track.duration || 0,
  });
  return found?.id ?? null;
}

/** Alle Songs mit begrenzter Parallelität zuordnen (Reihenfolge bleibt erhalten) */
async function matchAll(tracks, find) {
  const ids = new Array(tracks.length).fill(null);
  let next = 0;
  async function worker() {
    while (next < tracks.length) {
      const i = next++;
      try {
        ids[i] = await find(tracks[i]);
      } catch {
        ids[i] = null;
      }
      if (!ids[i]) transfer.missing.push(tracks[i]);
      transfer.done++;
    }
  }
  await Promise.all(Array.from({ length: 4 }, worker));
  // Doppelte entfernen (zwei Songs können auf denselben Treffer zeigen)
  return [...new Set(ids.filter(Boolean))];
}

export async function startTransfer(target, name, isPublic) {
  const tracks = transfer.from.tracks;
  Object.assign(transfer, { status: "running", done: 0, total: tracks.length, missing: [], error: null });
  try {
    if (target === "youtube") {
      const ids = await matchAll(tracks, youtubeId);
      if (!ids.length) throw new Error("Keiner der Songs wurde bei YouTube Music gefunden.");
      // In Paketen anlegen und auffüllen (YouTube nimmt lange Listen nicht auf einmal)
      const id = await invoke("ytm_create_playlist", {
        title: name,
        privacy: isPublic ? "PUBLIC" : "PRIVATE",
        videoIds: ids.slice(0, 50),
      });
      for (let i = 50; i < ids.length; i += 50) {
        await invoke("ytm_add_to_playlist", { playlistId: id, videoIds: ids.slice(i, i + 50) });
      }
      transfer.result = { target, id, title: name, count: ids.length };
      loadYtmLibrary();
    } else {
      const ids = await matchAll(tracks, spotifyId);
      if (!ids.length) throw new Error("Keiner der Songs wurde bei Spotify gefunden.");
      const created = await invoke("spotify_create_playlist", {
        name,
        description: `Mit Wavely übertragen`,
        public: isPublic,
        trackIds: ids,
      });
      transfer.result = { target, id: created.id, title: name, count: ids.length };
      loadSpotifyLibrary();
    }
    transfer.status = "done";
  } catch (e) {
    const message = String(e?.message ?? e);
    transfer.error =
      message === "SCOPE" || /scope|permission|insufficient/i.test(message)
        ? "Wavely braucht neue Spotify-Rechte zum Anlegen von Playlists. Bitte bei Spotify einmal ab- und wieder anmelden."
        : message;
    transfer.status = "error";
  }
}
