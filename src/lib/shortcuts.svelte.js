import { invoke } from "@tauri-apps/api/core";
import { register, unregisterAll } from "@tauri-apps/plugin-global-shortcut";
import {
  player,
  currentTrack,
  togglePlay,
  next,
  previous,
  seek,
  setVolume,
  toggleMute,
  toggleShuffle,
  cycleRepeat,
} from "./player.svelte.js";
import { settings, updateSetting } from "./settings.svelte.js";
import { ui, togglePanel, showToast } from "./ui.svelte.js";
import { ytm, isLiked, toggleLike } from "./ytm.svelte.js";

/**
 * Tastenkürzel. In Wavely gelten sie, solange nicht in ein Textfeld getippt wird.
 * Systemweite Kürzel (Strg+Alt+…) sind abschaltbar und funktionieren auch im Hintergrund.
 */

export const shortcuts = $state({ help: false });

const volumeBy = (delta) => setVolume(Math.min(1, Math.max(0, player.volume + delta)));
const seekBy = (delta) => currentTrack() && seek(Math.min(Math.max(0, player.position + delta), player.duration || Infinity));

async function like() {
  const track = currentTrack();
  if (track?.source !== "youtube" || ytm.status !== "loggedIn") return;
  try {
    await toggleLike(track);
    showToast(isLiked(track.id) ? "Zu „Gefällt mir“ hinzugefügt" : "Aus „Gefällt mir“ entfernt");
  } catch (e) {
    showToast(`Like fehlgeschlagen: ${e}`, true);
  }
}

function focusSearch() {
  if (!["youtube", "spotify", "soundcloud"].includes(ui.view)) ui.view = "youtube";
  setTimeout(() => {
    const input = document.querySelector("main .search input, main input[type=search]");
    input?.focus();
    input?.select();
  }, 50);
}

/** Für die Hilfe-Übersicht: [Tasten, Beschreibung] */
export const SHORTCUT_LIST = [
  ["Leertaste", "Abspielen / Pause"],
  ["Strg + →", "Nächster Song"],
  ["Strg + ←", "Vorheriger Song"],
  ["→ / ←", "5 Sekunden vor / zurück"],
  ["Umschalt + → / ←", "30 Sekunden vor / zurück"],
  ["↑ / ↓", "Lauter / leiser"],
  ["M", "Stumm"],
  ["S", "Zufallswiedergabe"],
  ["R", "Wiederholen"],
  ["L", "Gefällt mir (YouTube Music)"],
  ["Strg + F", "Suchen"],
  ["Strg + L", "Liedtext"],
  ["Strg + Q", "Warteschlange"],
  ["Strg + H", "Verlauf"],
  ["Strg + M", "Mini-Player"],
  ["Strg + ,", "Einstellungen"],
  ["?", "Diese Übersicht"],
];

export const GLOBAL_LIST = [
  ["Strg + Alt + Leertaste", "Abspielen / Pause"],
  ["Strg + Alt + →", "Nächster Song"],
  ["Strg + Alt + ←", "Vorheriger Song"],
  ["Strg + Alt + ↑ / ↓", "Lauter / leiser"],
];

function typing(target) {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    target?.isContentEditable
  );
}

/** keydown im Hauptfenster */
export function handleKey(e) {
  if (typing(e.target)) {
    if (e.key === "Escape") e.target.blur();
    return;
  }
  const ctrl = e.ctrlKey || e.metaKey;
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  let action = null;

  if (e.code === "Space" && !ctrl) action = togglePlay;
  else if (ctrl && key === "ArrowRight") action = next;
  else if (ctrl && key === "ArrowLeft") action = previous;
  else if (key === "ArrowRight" && !e.altKey) action = () => seekBy(e.shiftKey ? 30 : 5);
  else if (key === "ArrowLeft" && !e.altKey) action = () => seekBy(e.shiftKey ? -30 : -5);
  else if (key === "ArrowUp" && !ctrl) action = () => volumeBy(0.05);
  else if (key === "ArrowDown" && !ctrl) action = () => volumeBy(-0.05);
  else if (ctrl && key === "f") action = focusSearch;
  else if (ctrl && key === "l") action = () => togglePanel("lyrics");
  else if (ctrl && key === "q") action = () => togglePanel("queue");
  else if (ctrl && key === "h") action = () => togglePanel("history");
  else if (ctrl && key === "m")
    action = () => invoke("mini_player_open", { pinned: settings.miniPinned, width: settings.miniWidth, height: settings.miniHeight });
  else if (ctrl && key === ",") action = () => (ui.view = "settings");
  else if (!ctrl && !e.altKey) {
    if (key === "m") action = toggleMute;
    else if (key === "s") action = toggleShuffle;
    else if (key === "r") action = cycleRepeat;
    else if (key === "l") action = like;
    else if (key === "?") action = () => (shortcuts.help = !shortcuts.help);
  }

  if (!action) return;
  // Pfeiltasten in Listen/Schiebereglern nicht doppelt auslösen
  if (e.target instanceof HTMLButtonElement && e.code === "Space") e.target.blur();
  e.preventDefault();
  action();
}

// --- Systemweit -------------------------------------------------------------

const GLOBAL = {
  "CommandOrControl+Alt+Space": togglePlay,
  "CommandOrControl+Alt+Right": next,
  "CommandOrControl+Alt+Left": previous,
  "CommandOrControl+Alt+Up": () => volumeBy(0.05),
  "CommandOrControl+Alt+Down": () => volumeBy(-0.05),
};

export async function applyGlobalShortcuts() {
  await unregisterAll().catch(() => {});
  if (!settings.globalShortcuts) return;
  const failed = [];
  for (const [combo, action] of Object.entries(GLOBAL)) {
    try {
      await register(combo, (event) => event.state === "Pressed" && action());
    } catch {
      failed.push(combo.replace("CommandOrControl", "Strg"));
    }
  }
  if (failed.length) showToast(`Schon von einem anderen Programm belegt: ${failed.join(", ")}`, true);
}

export async function setGlobalShortcuts(on) {
  updateSetting("globalShortcuts", on);
  await applyGlobalShortcuts();
}
