import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ui } from "./ui.svelte.js";

export const spotify = $state({
  status: "checking", // "checking" | "loggedOut" | "loggedIn"
  loggingIn: false,
  account: null,
  library: null,
  libraryLoading: false,
  error: null,
});

const asTracks = (tracks) => tracks.map((t) => ({ ...t, source: "spotify" }));

export async function initSpotify() {
  await listen("spotify-auth", (event) => {
    spotify.loggingIn = false;
    if (event.payload.loggedIn) {
      spotify.status = "loggedIn";
      loadAccountData();
    } else {
      spotify.error = event.payload.error ?? "Anmeldung fehlgeschlagen";
    }
  });
  const loggedIn = await invoke("spotify_status").catch(() => false);
  spotify.status = loggedIn ? "loggedIn" : "loggedOut";
  if (loggedIn) loadAccountData();
}

function loadAccountData() {
  loadLibrary();
  invoke("spotify_account")
    .then((account) => (spotify.account = account))
    .catch(() => {});
}

export async function login() {
  spotify.error = null;
  spotify.loggingIn = true;
  try {
    await invoke("spotify_login");
  } catch (e) {
    spotify.loggingIn = false;
    spotify.error = String(e);
  }
}

export async function logout() {
  await invoke("spotify_logout");
  spotify.status = "loggedOut";
  spotify.account = null;
  spotify.library = null;
  ui.spHome++;
}

export async function loadLibrary() {
  spotify.libraryLoading = true;
  spotify.error = null;
  try {
    spotify.library = await invoke("spotify_library");
  } catch (e) {
    spotify.error = String(e);
  } finally {
    spotify.libraryLoading = false;
  }
}

export async function fetchCollection(kind, id) {
  const detail = await invoke("spotify_collection", { kind, id });
  return { ...detail, tracks: asTracks(detail.tracks) };
}

export async function search(query) {
  const result = await invoke("spotify_search", { query });
  return { ...result, tracks: asTracks(result.tracks) };
}

export async function fetchArtist(id) {
  const artist = await invoke("spotify_artist", { id });
  return { ...artist, tracks: asTracks(artist.tracks) };
}

export async function recentlyPlayed() {
  return asTracks(await invoke("spotify_recent"));
}

/** YouTube-Video-ID zu einem Spotify-Song (wird am Song-Objekt gemerkt) */
export async function resolveVideo(track) {
  if (track.ytId) return track.ytId;
  const id = await invoke("spotify_resolve", {
    id: track.id,
    title: track.title,
    artist: track.artist ?? null,
    duration: track.duration || 0,
  });
  track.ytId = id;
  return id;
}
