import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { coverFor } from "./covers.js";
import { library } from "./library.svelte.js";

// Ein einziges Audio-Element für alle Quellen (lokal und YouTube Music).
const audio = new Audio();
audio.preload = "auto";

function stored(key, fallback) {
  try {
    const value = localStorage.getItem(key);
    return value === null ? fallback : JSON.parse(value);
  } catch {
    return fallback;
  }
}

function store(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {}
}

export const player = $state({
  /** Einträge: Track + `qid` (eindeutig in der Warteschlange) + `auto` (von Autoplay ergänzt) */
  queue: [],
  index: -1,
  playing: false,
  position: 0,
  duration: 0,
  /** Stellung des Lautstärkereglers (0–1), nicht die tatsächliche Lautstärke */
  volume: stored("volumeSlider", 0.6),
  muted: false,
  shuffle: false,
  repeat: "off", // "off" | "all" | "one"
  /** Nach dem Ende der Warteschlange passende Songs anhängen */
  autoplay: stored("autoplay", true),
  loading: false,
  error: null,
});

/**
 * Das Ohr hört Lautstärke logarithmisch. Mit einer Kubik-Kurve wird der Regler
 * im leisen Bereich viel feiner; ganz rechts bleibt es gleich laut.
 */
const gain = (slider) => Math.pow(slider, 3);
audio.volume = gain(player.volume);

let nextQid = 1;
const entry = (track, auto = false) => ({ ...track, qid: nextQid++, auto });

// Ursprüngliche Reihenfolge, um Zufallswiedergabe wieder aufheben zu können
let originalQueue = [];

export function currentTrack() {
  return player.queue[player.index] ?? null;
}

function sourceUrl(track) {
  // YouTube läuft über den eigenen Stream-Dienst im Rust-Teil (ytstream://<videoId>)
  return track.source === "local"
    ? convertFileSrc(track.path)
    : convertFileSrc(track.id, "ytstream");
}

function shuffled(list) {
  const a = [...list];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

function prefetchUpcoming() {
  const upcoming = player.queue[player.index + 1];
  if (upcoming?.source === "youtube") {
    invoke("ytm_prefetch", { id: upcoming.id }).catch(() => {});
  }
}

function load(index) {
  const track = player.queue[index];
  if (!track) return;
  player.index = index;
  player.position = 0;
  player.duration = track.duration || 0;
  player.loading = true;
  player.error = null;
  updateMediaSession(track);

  audio.src = sourceUrl(track);
  audio.play().catch((e) => console.error("Wiedergabe fehlgeschlagen:", e));

  prefetchUpcoming();
  extendWithAutoplay();
}

// --- Warteschlange -------------------------------------------------------

/** Spielt eine Liste ab, beginnend bei `index`. */
export function playList(tracks, index = 0) {
  const entries = tracks.map((t) => entry(t));
  originalQueue = entries;
  if (player.shuffle) {
    const first = entries[index];
    player.queue = [first, ...shuffled(entries.filter((_, i) => i !== index))];
    load(0);
  } else {
    player.queue = [...entries];
    load(index);
  }
}

/** Fügt einen Song direkt hinter dem aktuellen ein. */
export function playNext(track) {
  if (!currentTrack()) return playList([track]);
  const item = entry(track);
  player.queue.splice(player.index + 1, 0, item);
  originalQueue.push(item);
  prefetchUpcoming();
}

/** Hängt einen Song vor den Autoplay-Vorschlägen an. */
export function addToQueue(track) {
  if (!currentTrack()) return playList([track]);
  const item = entry(track);
  const firstAuto = player.queue.findIndex((t, i) => i > player.index && t.auto);
  player.queue.splice(firstAuto === -1 ? player.queue.length : firstAuto, 0, item);
  originalQueue.push(item);
}

export function removeFromQueue(index) {
  if (index === player.index) return;
  const [removed] = player.queue.splice(index, 1);
  originalQueue = originalQueue.filter((t) => t.qid !== removed.qid);
  if (index < player.index) player.index--;
}

export function jumpTo(index) {
  load(index);
}

// --- Autoplay: passende Songs nach dem Ende ------------------------------

let autoplayPending = false;

async function youtubeRadio(seed) {
  const tracks = await invoke("ytm_radio", { id: seed.id });
  return tracks.map((t) => ({ ...t, source: "youtube" }));
}

/** Lokal: bevorzugt gleicher Künstler, gleiches Album und gleicher Ordner, aufgefüllt mit Zufall. */
function localRadio(seed) {
  const folder = (path) => path.replace(/[\\/][^\\/]*$/, "");
  const seedFolder = folder(seed.path);
  const related = [];
  const others = [];
  for (const t of library.tracks) {
    if (t.id === seed.id) continue;
    const close =
      (seed.artist && t.artist === seed.artist) ||
      (seed.album && t.album === seed.album) ||
      folder(t.path) === seedFolder;
    (close ? related : others).push(t);
  }
  return shuffled([...shuffled(related).slice(0, 8), ...shuffled(others).slice(0, 12)]);
}

async function extendWithAutoplay() {
  if (!player.autoplay || player.repeat !== "off" || autoplayPending) return;
  if (player.index < player.queue.length - 2) return;
  const seed = currentTrack();
  if (!seed) return;

  autoplayPending = true;
  try {
    const suggestions = seed.source === "youtube" ? await youtubeRadio(seed) : localRadio(seed);
    const known = new Set(player.queue.map((t) => t.id));
    const fresh = suggestions.filter((t) => !known.has(t.id)).slice(0, 20);
    if (fresh.length) {
      player.queue.push(...fresh.map((t) => entry(t, true)));
      prefetchUpcoming();
    }
  } catch (e) {
    console.error("Autoplay fehlgeschlagen:", e);
  } finally {
    autoplayPending = false;
  }
}

export function toggleAutoplay() {
  player.autoplay = !player.autoplay;
  store("autoplay", player.autoplay);
  if (player.autoplay) extendWithAutoplay();
  else player.queue = player.queue.filter((t, i) => i <= player.index || !t.auto);
}

// --- Steuerung -----------------------------------------------------------

export function togglePlay() {
  if (!currentTrack()) return;
  if (audio.paused) audio.play();
  else audio.pause();
}

export async function next() {
  if (player.index + 1 >= player.queue.length && player.repeat === "off") {
    await extendWithAutoplay();
  }
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
  audio.volume = gain(value);
  audio.muted = false;
  store("volumeSlider", value);
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
    // Autoplay-Vorschläge bleiben hinten erhalten
    const autos = player.queue.filter((t) => t.auto && !originalQueue.some((o) => o.qid === t.qid));
    player.queue = [...originalQueue, ...autos];
    player.index = Math.max(0, player.queue.findIndex((t) => t.qid === current.qid));
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
audio.addEventListener("playing", () => (player.loading = false));
audio.addEventListener("waiting", () => (player.loading = true));
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
  const track = currentTrack();
  if (!track || !audio.src) return;
  player.loading = false;
  player.error = `„${track.title}“ konnte nicht abgespielt werden.`;
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
    if (currentTrack()?.qid !== track.qid) return;
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
