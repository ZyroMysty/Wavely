import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ui } from "./ui.svelte.js";

export const ytm = $state({
  status: "checking", // "checking" | "loggedOut" | "loggedIn"
  loggingIn: false,
  library: null,
  libraryLoading: false,
  error: null,
  /** { name, handle, photoUrl } des angemeldeten Kontos */
  account: null,
  /** Gelikte Songs: videoId → true */
  liked: {},
});

const asTracks = (tracks) => tracks.map((t) => ({ ...t, source: "youtube" }));

export async function initYtm() {
  await listen("ytm-auth", (event) => {
    ytm.loggingIn = false;
    if (event.payload.loggedIn) {
      ytm.status = "loggedIn";
      loadAccountData();
    } else {
      ytm.error = event.payload.error ?? "Anmeldung fehlgeschlagen";
    }
  });

  const loggedIn = await invoke("ytm_status").catch(() => false);
  ytm.status = loggedIn ? "loggedIn" : "loggedOut";
  if (loggedIn) loadAccountData();
}

/** Bibliothek, Profil und Likes nach der Anmeldung laden */
function loadAccountData() {
  loadLibrary();
  invoke("ytm_account")
    .then((account) => (ytm.account = account))
    .catch(() => {});
  invoke("ytm_liked_ids")
    .then((ids) => (ytm.liked = Object.fromEntries(ids.map((id) => [id, true]))))
    .catch(() => {});
}

export async function login() {
  ytm.error = null;
  ytm.loggingIn = true;
  try {
    await invoke("ytm_login");
  } catch (e) {
    ytm.loggingIn = false;
    ytm.error = String(e);
  }
}

export async function logout() {
  await invoke("ytm_logout");
  ytm.status = "loggedOut";
  ytm.library = null;
  ytm.account = null;
  ytm.liked = {};
  ui.ytmHome++;
}

export async function loadLibrary() {
  ytm.libraryLoading = true;
  ytm.error = null;
  try {
    ytm.library = await invoke("ytm_library");
  } catch (e) {
    ytm.error = String(e);
  } finally {
    ytm.libraryLoading = false;
  }
}

export async function fetchCollection(kind, id) {
  const detail = await invoke("ytm_collection", { kind, id });
  return { ...detail, tracks: asTracks(detail.tracks) };
}

export async function search(query) {
  const result = await invoke("ytm_search", { query });
  return { ...result, tracks: asTracks(result.tracks) };
}

// --- Likes & Playlists bearbeiten ----------------------------------------

export const isLiked = (id) => Boolean(ytm.liked[id]);

export async function toggleLike(track) {
  const liked = !isLiked(track.id);
  // Sofort anzeigen, bei Fehler zurücknehmen
  if (liked) ytm.liked[track.id] = true;
  else delete ytm.liked[track.id];
  try {
    await invoke("ytm_set_like", { id: track.id, liked });
  } catch (e) {
    if (liked) delete ytm.liked[track.id];
    else ytm.liked[track.id] = true;
    throw e;
  }
}

/** Eigene Playlists, zu denen man Songs hinzufügen kann */
export function editablePlaylists() {
  return ytm.library?.playlists.filter((p) => p.editable) ?? [];
}

export async function addToPlaylist(playlistId, tracks) {
  await invoke("ytm_add_to_playlist", { playlistId, videoIds: tracks.map((t) => t.id) });
}

export async function createPlaylist(title, privacy, tracks = []) {
  const id = await invoke("ytm_create_playlist", { title, privacy, videoIds: tracks.map((t) => t.id) });
  loadLibrary();
  return id;
}
