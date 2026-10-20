import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const ytm = $state({
  status: "checking", // "checking" | "loggedOut" | "loggedIn"
  loggingIn: false,
  library: null,
  libraryLoading: false,
  error: null,
});

const asTracks = (tracks) => tracks.map((t) => ({ ...t, source: "youtube" }));

export async function initYtm() {
  await listen("ytm-auth", (event) => {
    ytm.loggingIn = false;
    if (event.payload.loggedIn) {
      ytm.status = "loggedIn";
      loadLibrary();
    } else {
      ytm.error = event.payload.error ?? "Anmeldung fehlgeschlagen";
    }
  });

  const loggedIn = await invoke("ytm_status").catch(() => false);
  ytm.status = loggedIn ? "loggedIn" : "loggedOut";
  if (loggedIn) loadLibrary();
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
