/** Zustand der Oberfläche, der nicht zu einer bestimmten Ansicht gehört. */
import { settings } from "./settings.svelte.js";

function stored(key, fallback) {
  try {
    const value = localStorage.getItem(key);
    return value === null ? fallback : JSON.parse(value);
  } catch {
    return fallback;
  }
}

export const ui = $state({
  /** Hauptansicht: "local" | "youtube" | "offline" | "settings" */
  view: settings.startView,
  /** Rechte Seitenleiste: null | "queue" | "lyrics" | "history" */
  panel: null,
  /** Kontextmenü für einen Song: { x, y, track } oder null */
  menu: null,
  /** Linke Seitenleiste eingeklappt (nur Symbole) */
  sidebarCollapsed: stored("sidebarCollapsed", false),
  /** Wird erhöht, wenn YouTube Music zur Startseite zurück soll */
  ytmHome: 0,
  /** Seite, die YouTube Music öffnen soll: { type: "artist", id } oder { type: "collection", kind, id, title } */
  ytmNav: null,
  /** Dasselbe für Spotify */
  spNav: null,
  /** Wird erhöht, wenn Spotify zur Startseite zurück soll */
  spHome: 0,
  /** Dialog „Neue Playlist“: { tracks } oder null */
  newPlaylist: null,
  /** Kurzer Hinweis unten: { text, error } oder null */
  toast: null,
});

export function togglePanel(name) {
  ui.panel = ui.panel === name ? null : name;
}

export function openTrackMenu(event, track) {
  event.preventDefault();
  ui.menu = { x: event.clientX, y: event.clientY, track };
}

export function toggleSidebar() {
  ui.sidebarCollapsed = !ui.sidebarCollapsed;
  try {
    localStorage.setItem("sidebarCollapsed", JSON.stringify(ui.sidebarCollapsed));
  } catch {}
}

let toastTimer;
export function showToast(text, error = false) {
  ui.toast = { text, error };
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (ui.toast = null), error ? 5000 : 2500);
}

/** Album oder Künstler öffnen (von jeder Ansicht aus) – bei Spotify-Songs in Spotify, sonst YouTube Music */
export function openIn(source, target) {
  if (source === "spotify") {
    ui.view = "spotify";
    ui.spNav = target;
  } else {
    ui.view = "youtube";
    ui.ytmNav = target;
  }
}

export const openArtist = (artist, source = "youtube") =>
  artist?.id && openIn(source, { type: "artist", id: artist.id, title: artist.name });
export const openAlbum = (track) =>
  track?.albumId && openIn(track.source, { type: "collection", kind: "album", id: track.albumId, title: track.album });
