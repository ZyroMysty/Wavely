import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { player, currentTrack, togglePlay, next, previous, seek, setVolume, toggleMute } from "./player.svelte.js";
import { ytm, isLiked, toggleLike } from "./ytm.svelte.js";
import { hiResArtwork } from "./artwork.js";
import { ui, showToast } from "./ui.svelte.js";
import { settings, updateSetting } from "./settings.svelte.js";

/**
 * Verbindet den Player im Hauptfenster mit den Taskleisten-Buttons und dem Mini-Player.
 * Beide schicken Befehle („toggle“, „next“, …) und bekommen den aktuellen Zustand zurück.
 */

const canLike = (track) => track?.source === "youtube" && ytm.status === "loggedIn";

async function likeCurrent() {
  const track = currentTrack();
  if (!canLike(track)) return;
  try {
    await toggleLike(track);
  } catch (e) {
    showToast(`Like fehlgeschlagen: ${e}`, true);
  }
}

function runCommand(command) {
  const action = typeof command === "string" ? command : command.action;
  if (action === "toggle") togglePlay();
  else if (action === "play") !player.playing && togglePlay();
  else if (action === "pause") player.playing && togglePlay();
  else if (action === "next") next();
  else if (action === "previous") previous();
  else if (action === "like") likeCurrent();
  else if (action === "seek") seek(command.position);
  else if (action === "volume") setVolume(command.value);
  else if (action === "mute") toggleMute();
  // Vom Mini-Player: Einstellungen merken
  else if (action === "miniPinned") updateSetting("miniPinned", command.value);
  else if (action === "miniSize") {
    updateSetting("miniWidth", command.width);
    updateSetting("miniHeight", command.height);
  }
}

function miniState() {
  const track = currentTrack();
  return {
    key: track ? `${track.source}:${track.id}:${track.qid}` : null,
    title: track?.title ?? null,
    artist: track?.artist ?? null,
    playing: player.playing,
    loading: player.loading,
    position: player.position,
    duration: player.duration,
    canLike: canLike(track),
    liked: track ? isLiked(track.id) : false,
    volume: player.volume,
    muted: player.muted,
  };
}

/** Cover für den Mini-Player: Rust lädt es selbst (Bild-Cache bzw. lokale Datei) */
function sendCover() {
  const track = currentTrack();
  const key = track ? `${track.source}:${track.id}:${track.qid}` : null;
  invoke("mini_cover", {
    key,
    // Für den großen Mini-Player die hochaufgelöste Version
    url: track?.artworkUrl ? hiResArtwork(track.artworkUrl) : null,
    path: track && !track.artworkUrl ? (track.path ?? null) : null,
  }).catch(() => {});
}

/** CSS-Farbe → [r, g, b, a] (0–1); der Canvas wandelt jede Schreibweise in #rrggbb oder rgba() um */
const canvas = document.createElement("canvas").getContext("2d");
function rgba(value) {
  canvas.fillStyle = "#000";
  canvas.fillStyle = value.trim();
  const c = canvas.fillStyle;
  if (c.startsWith("#")) return [1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16) / 255).concat(1);
  const [r, g, b, a = 1] = c.match(/[\d.]+/g).map(Number);
  return [r / 255, g / 255, b / 255, a];
}

/** Farben des aktuellen Designs an den Mini-Player (er zeichnet selbst, ohne CSS) */
function sendTheme() {
  // Erst nach applyAppearance lesen (läuft im selben Durchgang)
  setTimeout(() => {
    const css = getComputedStyle(document.documentElement);
    const v = (name) => rgba(css.getPropertyValue(name));
    const theme = {
      bg: v("--bg"),
      surface: v("--surface"),
      hover: v("--hover"),
      border: v("--border"),
      text: v("--text"),
      textDim: v("--text-dim"),
      accent: v("--accent"),
    };
    invoke("mini_theme", { theme }).catch(() => {});
  });
}

export async function initBridge() {
  await listen("taskbar-action", (e) => runCommand(e.payload));
  await listen("player-command", (e) => runCommand(e.payload));
  await listen("mini-closed", () => (ui.miniOpen = false));
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => ui.miniOpen && sendTheme());

  $effect.root(() => {
    // Taskleisten-Buttons: nur bei Änderungen von Song, Play/Pause oder Like
    $effect(() => {
      const track = currentTrack();
      const state = {
        hasTrack: Boolean(track),
        playing: player.playing,
        liked: canLike(track) ? isLiked(track.id) : null,
      };
      invoke("taskbar_update", { state }).catch(() => {});
    });

    // Windows-Mediensteuerung (Medientasten, Overlay)
    $effect(() => {
      const track = currentTrack();
      const info = {
        hasTrack: Boolean(track),
        playing: player.playing,
        title: track?.title ?? null,
        artist: track?.artist ?? null,
        album: track?.album ?? null,
        artworkUrl: track ? hiResArtwork(track.artworkUrl) ?? null : null,
      };
      invoke("media_update", { info }).catch(() => {});
    });

    // Mini-Player: Zustand inkl. Position, nur solange er offen ist
    $effect(() => {
      if (!ui.miniOpen) return;
      invoke("mini_update", { state: miniState() }).catch(() => {});
    });

    // Cover nur beim Songwechsel schicken
    $effect(() => {
      if (!ui.miniOpen) return;
      currentTrack()?.qid;
      sendCover();
    });

    $effect(() => {
      if (!ui.miniOpen) return;
      settings.theme;
      settings.accent;
      sendTheme();
    });
  });
}
