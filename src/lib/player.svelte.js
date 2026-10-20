import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { coverFor } from "./covers.js";

// Ein einziges Audio-Element für alle Quellen (lokal, später YouTube Music).
const audio = new Audio();
audio.preload = "auto";

function loadVolume() {
  try {
    const v = parseFloat(localStorage.getItem("volume"));
    return Number.isFinite(v) ? v : 0.8;
  } catch {
    return 0.8;
  }
}

export const player = $state({
  queue: [],
  index: -1,
  playing: false,
  position: 0,
  duration: 0,
  volume: loadVolume(),
  muted: false,
  shuffle: false,
  repeat: "off", // "off" | "all" | "one"
});

audio.volume = player.volume;

// Ursprüngliche Reihenfolge, um Zufallswiedergabe wieder aufheben zu können
let originalQueue = [];

export function currentTrack() {
  return player.queue[player.index] ?? null;
}

function sourceUrl(track) {
  return track.source === "local" ? convertFileSrc(track.path) : track.streamUrl;
}

function shuffled(list) {
  const a = [...list];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

function load(index) {
  const track = player.queue[index];
  if (!track) return;
  player.index = index;
  player.position = 0;
  player.duration = track.duration || 0;
  audio.src = sourceUrl(track);
  audio.play().catch((e) => console.error("Wiedergabe fehlgeschlagen:", e));
  updateMediaSession(track);
}

/** Spielt eine Liste ab, beginnend bei `index`. */
export function playList(tracks, index = 0) {
  originalQueue = [...tracks];
  if (player.shuffle) {
    const first = tracks[index];
    player.queue = [first, ...shuffled(tracks.filter((_, i) => i !== index))];
    load(0);
  } else {
    player.queue = [...tracks];
    load(index);
  }
}

export function togglePlay() {
  if (!currentTrack()) return;
  if (audio.paused) audio.play();
  else audio.pause();
}

export function next() {
  if (player.index + 1 < player.queue.length) load(player.index + 1);
  else if (player.repeat === "all" && player.queue.length) load(0);
  else {
    audio.pause();
    audio.currentTime = 0;
  }
}

export function previous() {
  if (audio.currentTime > 3 || player.index === 0) audio.currentTime = 0;
  else load(player.index - 1);
}

export function seek(seconds) {
  audio.currentTime = seconds;
}

export function setVolume(value) {
  player.volume = value;
  player.muted = false;
  audio.volume = value;
  audio.muted = false;
  try {
    localStorage.setItem("volume", String(value));
  } catch {}
}

export function toggleMute() {
  player.muted = !player.muted;
  audio.muted = player.muted;
}

export function toggleShuffle() {
  const current = currentTrack();
  player.shuffle = !player.shuffle;
  if (!current) return;
  if (player.shuffle) {
    const rest = player.queue.filter((_, i) => i !== player.index);
    player.queue = [current, ...shuffled(rest)];
    player.index = 0;
  } else {
    player.queue = [...originalQueue];
    player.index = Math.max(0, originalQueue.findIndex((t) => t.id === current.id));
  }
}

export function cycleRepeat() {
  player.repeat = { off: "all", all: "one", one: "off" }[player.repeat];
}

// --- Audio-Events ---------------------------------------------------------

audio.addEventListener("timeupdate", () => (player.position = audio.currentTime));
audio.addEventListener("durationchange", () => {
  if (Number.isFinite(audio.duration)) player.duration = audio.duration;
});
audio.addEventListener("play", () => {
  player.playing = true;
  sendPresence();
});
audio.addEventListener("pause", () => {
  player.playing = false;
  sendPresence();
});
audio.addEventListener("seeked", sendPresence);
audio.addEventListener("ended", () => {
  if (player.repeat === "one") {
    audio.currentTime = 0;
    audio.play();
  } else {
    next();
  }
});
audio.addEventListener("error", () => {
  console.error("Datei konnte nicht abgespielt werden:", currentTrack()?.path);
  if (player.index + 1 < player.queue.length) next();
});

// --- Discord Rich Presence ------------------------------------------------

function sendPresence() {
  const t = currentTrack();
  if (!t) {
    invoke("clear_presence").catch(() => {});
    return;
  }
  invoke("update_presence", {
    track: {
      title: t.title,
      artist: t.artist || "Unbekannter Künstler",
      album: t.album ?? null,
      artwork: t.artworkUrl ?? null,
      url: t.url ?? null,
      position: audio.currentTime,
      duration: player.duration,
      paused: audio.paused,
    },
  }).catch(() => {});
}

// --- Windows-Mediensteuerung (Medientasten, Overlay) ----------------------

function updateMediaSession(track) {
  if (!("mediaSession" in navigator)) return;
  const set = (cover) => {
    if (currentTrack() !== track) return;
    navigator.mediaSession.metadata = new MediaMetadata({
      title: track.title,
      artist: track.artist ?? "",
      album: track.album ?? "",
      artwork: cover ? [{ src: cover }] : [],
    });
  };
  set(null);
  coverFor(track).then(set);
}

if ("mediaSession" in navigator) {
  navigator.mediaSession.setActionHandler("play", () => audio.play());
  navigator.mediaSession.setActionHandler("pause", () => audio.pause());
  navigator.mediaSession.setActionHandler("nexttrack", next);
  navigator.mediaSession.setActionHandler("previoustrack", previous);
  navigator.mediaSession.setActionHandler("seekto", (e) => seek(e.seekTime));
}
