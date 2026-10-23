/** Einstellungen, die der Nutzer auf der Einstellungsseite ändern kann (lokal gespeichert). */

const KEY = "settings";

const defaults = {
  /** "system" | "dark" | "light" */
  theme: "dark",
  accent: "#ff4f7b",
  /** Ansicht beim Start: "local" | "youtube" | "spotify" | "soundcloud" | "offline" */
  startView: "youtube",
  discordEnabled: true,
  /** Was Discord in der Mitgliederliste zeigt: "details" (Songtitel) | "state" (Künstler) | "name" (Wavely) */
  discordDisplay: "details",
  /** Mini-Player immer im Vordergrund */
  miniPinned: true,
  /** In Wavely gehörte YouTube-Songs auch im YouTube-Music-Verlauf speichern */
  syncYouTubeHistory: true,
  /** Beim Start und alle paar Stunden nach neuen Wavely-Versionen suchen */
  autoUpdateCheck: true,
  /** Spotify (Premium): "wavely" = direkt in Wavely, sonst ID eines anderen Spotify-Geräts */
  spotifyDevice: "wavely",
  /** Zuletzt benutzte Größe des Mini-Players */
  miniWidth: 360,
  miniHeight: 104,
};

function load() {
  try {
    return { ...defaults, ...JSON.parse(localStorage.getItem(KEY) ?? "{}") };
  } catch {
    return { ...defaults };
  }
}

export const settings = $state(load());

export const ACCENTS = [
  { name: "Pink", value: "#ff4f7b" },
  { name: "Violett", value: "#8b5cf6" },
  { name: "Blau", value: "#3b82f6" },
  { name: "Türkis", value: "#14b8a6" },
  { name: "Grün", value: "#22c55e" },
  { name: "Gelb", value: "#eab308" },
  { name: "Orange", value: "#f97316" },
  { name: "Rot", value: "#ef4444" },
];

export function saveSettings() {
  try {
    localStorage.setItem(KEY, JSON.stringify(settings));
  } catch {}
}

export function updateSetting(key, value) {
  settings[key] = value;
  saveSettings();
}

export function resetAppearance() {
  settings.theme = defaults.theme;
  settings.accent = defaults.accent;
  saveSettings();
}

function hexToRgb(hex) {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex);
  if (!m) return [255, 79, 123];
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

const systemDark = window.matchMedia("(prefers-color-scheme: dark)");

/** Wendet Design und Akzentfarbe auf die Seite an; reagiert auch auf den Windows-Modus. */
export function applyAppearance() {
  const dark = settings.theme === "dark" || (settings.theme === "system" && systemDark.matches);
  const root = document.documentElement;
  root.dataset.theme = dark ? "dark" : "light";
  const [r, g, b] = hexToRgb(settings.accent);
  root.style.setProperty("--accent", settings.accent);
  root.style.setProperty("--accent-soft", `rgba(${r}, ${g}, ${b}, ${dark ? 0.16 : 0.13})`);
}

systemDark.addEventListener("change", () => {
  if (settings.theme === "system") applyAppearance();
});

// Änderungen aus dem anderen Fenster (Hauptfenster ↔ Mini-Player) übernehmen
window.addEventListener("storage", (e) => {
  if (e.key !== KEY) return;
  try {
    Object.assign(settings, JSON.parse(e.newValue ?? "{}"));
    applyAppearance();
  } catch {}
});
