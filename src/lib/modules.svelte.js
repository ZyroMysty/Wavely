import { settings, updateSetting, moduleOn, applyQuality } from "./settings.svelte.js";
import { ui } from "./ui.svelte.js";
import { refreshLibrary, watchLibrary } from "./library.svelte.js";
import { initYtm } from "./ytm.svelte.js";
import { initSpotify } from "./spotify.svelte.js";
import { initSoundCloud } from "./soundcloud.svelte.js";
import { initLastFm } from "./lastfm.svelte.js";
import { leaveRoom } from "./together.svelte.js";

/**
 * Dienste ein- und ausschalten (Einstellungen → Dienste). Abgeschaltete Dienste werden beim Start
 * nicht geladen und verschwinden aus Seitenleiste, Einstellungen und Player-Leiste. Songs aus diesen
 * Quellen in Verlauf oder Wavely-Playlists bleiben abspielbar.
 */

export const MODULES = [
  { id: "youtube", name: "YouTube Music", hint: "Bibliothek, Suche und Likes (abgespielt wird über YouTube trotzdem)" },
  { id: "spotify", name: "Spotify", hint: "Bibliothek, Suche, Premium-Wiedergabe und Spotify-Geräte" },
  { id: "soundcloud", name: "SoundCloud", hint: "Likes, Playlists, Suche und Künstler" },
  { id: "local", name: "Lokale Musik", hint: "Musikordner auf diesem PC" },
  { id: "lastfm", name: "Last.fm", hint: "Scrobbeln und „Hört gerade“" },
  { id: "together", name: "Zusammen hören", hint: "Knopf in der Player-Leiste" },
];

/** Ansichten in der Seitenleiste, die zu einem Dienst gehören */
export const MODULE_VIEWS = ["youtube", "spotify", "soundcloud", "local"];

const loaders = {
  youtube: initYtm,
  spotify: initSpotify,
  soundcloud: initSoundCloud,
  lastfm: initLastFm,
  local: () => {
    refreshLibrary();
    watchLibrary();
  },
};

const started = new Set();

/** Beim Start: nur eingeschaltete Dienste laden */
export function initModules() {
  applyQuality();
  for (const [id, load] of Object.entries(loaders)) {
    if (moduleOn(id)) {
      started.add(id);
      load();
    }
  }
  if (MODULE_VIEWS.includes(ui.view) && !moduleOn(ui.view)) ui.view = "home";
}

export function setModule(id, on) {
  updateSetting("modules", { ...settings.modules, [id]: on });
  if (on && !started.has(id) && loaders[id]) {
    started.add(id);
    loaders[id]();
  }
  if (!on) {
    if (ui.view === id) ui.view = "home";
    if (id === "together") leaveRoom();
  }
}
