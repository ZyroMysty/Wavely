import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ui } from "./ui.svelte.js";

export const soundcloud = $state({
  status: "checking", // "checking" | "loggedOut" | "loggedIn"
  loggingIn: false,
  account: null,
  library: null,
  libraryLoading: false,
  error: null,
});

const asTracks = (tracks) => tracks.map((t) => ({ ...t, source: "soundcloud" }));

export async function initSoundCloud() {
  await listen("soundcloud-auth", (event) => {
    soundcloud.loggingIn = false;
    if (event.payload.loggedIn) {
      soundcloud.status = "loggedIn";
      loadAccountData();
    } else {
      soundcloud.error = event.payload.error ?? "Anmeldung fehlgeschlagen";
    }
  });
  const loggedIn = await invoke("soundcloud_status").catch(() => false);
  soundcloud.status = loggedIn ? "loggedIn" : "loggedOut";
  if (loggedIn) loadAccountData();
}

function loadAccountData() {
  loadLibrary();
  invoke("soundcloud_account")
    .then((account) => (soundcloud.account = account))
    .catch(() => {});
}

export async function login() {
  soundcloud.error = null;
  soundcloud.loggingIn = true;
  try {
    await invoke("soundcloud_login");
  } catch (e) {
    soundcloud.loggingIn = false;
    soundcloud.error = String(e);
  }
}

export async function logout() {
  await invoke("soundcloud_logout");
  soundcloud.status = "loggedOut";
  soundcloud.account = null;
  soundcloud.library = null;
  ui.scHome++;
}

export async function loadLibrary() {
  soundcloud.libraryLoading = true;
  soundcloud.error = null;
  try {
    soundcloud.library = await invoke("soundcloud_library");
  } catch (e) {
    soundcloud.error = String(e);
  } finally {
    soundcloud.libraryLoading = false;
  }
}

export async function fetchCollection(kind, id) {
  const detail = await invoke("soundcloud_collection", { id });
  return { ...detail, tracks: asTracks(detail.tracks) };
}

export async function search(query) {
  const result = await invoke("soundcloud_search", { query });
  return { ...result, tracks: asTracks(result.tracks) };
}

export async function fetchArtist(id) {
  const artist = await invoke("soundcloud_artist", { id });
  return { ...artist, tracks: asTracks(artist.tracks) };
}

export async function recentlyPlayed() {
  return asTracks(await invoke("soundcloud_recent"));
}

/** Ähnliche Songs für Autoplay */
export async function relatedTracks(id) {
  return asTracks(await invoke("soundcloud_related", { id }));
}

/** Direkte MP3-Adresse oder null (nur 30-Sekunden-Ausschnitt → Wiedergabe über YouTube Music) */
export function streamUrl(track) {
  return track.snipped ? Promise.resolve(null) : invoke("soundcloud_stream", { id: track.id });
}
