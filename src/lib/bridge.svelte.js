import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { player, currentTrack, togglePlay, next, previous, seek } from "./player.svelte.js";
import { ytm, isLiked, toggleLike } from "./ytm.svelte.js";
import { coverFor } from "./covers.js";
import { showToast } from "./ui.svelte.js";

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
  else if (action === "next") next();
  else if (action === "previous") previous();
  else if (action === "like") likeCurrent();
  else if (action === "seek") seek(command.position);
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
  };
}

async function sendCover() {
  const track = currentTrack();
  const key = track ? `${track.source}:${track.id}:${track.qid}` : null;
  const src = track ? await coverFor(track) : null;
  emitTo("mini", "player-cover", { key, src }).catch(() => {});
}

export async function initBridge() {
  await listen("taskbar-action", (e) => runCommand(e.payload));
  await listen("player-command", (e) => runCommand(e.payload));
  await listen("mini-ready", () => {
    emitTo("mini", "player-state", miniState()).catch(() => {});
    sendCover();
  });

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

    // Mini-Player: Zustand inkl. Position
    $effect(() => {
      emitTo("mini", "player-state", miniState()).catch(() => {});
    });

    // Cover nur beim Songwechsel schicken (kann ein großes Bild sein)
    $effect(() => {
      currentTrack()?.qid;
      sendCover();
    });
  });
}
