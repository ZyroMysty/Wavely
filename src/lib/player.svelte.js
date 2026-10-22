import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { coverFor } from "./covers.js";
import { library } from "./library.svelte.js";
import { settings } from "./settings.svelte.js";
import { recordPlay } from "./history.svelte.js";
import { ytm } from "./ytm.svelte.js";
import { resolveVideo } from "./spotify.svelte.js";
import { streamUrl as soundcloudStream, relatedTracks as soundcloudRelated } from "./soundcloud.svelte.js";
import { offlinePath, allTracks as offlineTracks } from "./offline.svelte.js";
import {
  canPlayOnSpotify,
  playOnSpotify,
  spotifyPause,
  spotifyResume,
  spotifySeek,
  spotifyVolume,
  stopPolling as stopSpotifyPolling,
  onSpotifyState,
} from "./spotifyPlayer.svelte.js";

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
/** Song, der beim ersten „playing“ in den Verlauf kommt */
let pendingHistory = null;
/** Fehler hintereinander – verhindert, dass bei einem Problem die ganze Liste übersprungen wird */
let errorStreak = 0;
const MAX_ERROR_STREAK = 3;
const entry = (track, auto = false) => ({ ...track, qid: nextQid++, auto });

// Ursprüngliche Reihenfolge, um Zufallswiedergabe wieder aufheben zu können
let originalQueue = [];

export function currentTrack() {
  return player.queue[player.index] ?? null;
}

/** YouTube-Video zum Song: YouTube-Songs direkt, Spotify-/SoundCloud-Songs über die Zuordnung */
async function videoId(track) {
  return track.source === "spotify" || track.source === "soundcloud" ? await resolveVideo(track) : track.id;
}

async function sourceUrl(track) {
  if (track.source === "local") return convertFileSrc(track.path);
  // Offline gespeichert: Datei direkt abspielen (auch ohne Internet)
  const saved = offlinePath(track.id);
  if (saved) return convertFileSrc(saved);
  // SoundCloud: direkt als MP3; nur 30-Sekunden-Ausschnitte laufen über YouTube Music
  if (track.source === "soundcloud") {
    const direct = await soundcloudStream(track);
    if (direct) return direct;
  }
  // Sonst über den eigenen Stream-Dienst im Rust-Teil (ytstream://<videoId>)
  return convertFileSrc(await videoId(track), "ytstream");
}

/** Verhindert, dass ein langsam aufgelöster Song einen später gewählten überschreibt */
let loadToken = 0;

/**
 * Wer gerade spielt: "audio" (Wavelys Player: lokal, YouTube, SoundCloud, Spotify über YouTube)
 * oder "spotify" (echte Spotify-Wiedergabe mit Premium, in Wavely oder auf einem anderen Gerät).
 */
let engine = "audio";
/** Spotify: erwarteter Song läuft bereits / Ende erreicht / Ende schon behandelt */
let spStarted = false;
let spNearEnd = false;
let spFinished = false;

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
  if (!upcoming || upcoming.source === "local" || offlinePath(upcoming.id)) return;
  if (upcoming.source === "soundcloud" && !upcoming.snipped) return; // direkte MP3, nichts vorzubereiten
  videoId(upcoming)
    .then((id) => invoke("ytm_prefetch", { id }))
    .catch(() => {});
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
  // Erst in den Verlauf, wenn der Song wirklich läuft (siehe „playing“)
  pendingHistory = track;

  const token = ++loadToken;

  // Premium: echt über Spotify (wirft „FALLBACK“, dann über YouTube Music)
  if (canPlayOnSpotify(track)) {
    audio.pause();
    engine = "spotify";
    spStarted = spNearEnd = spFinished = false;
    playOnSpotify(track)
      .then(() => {
        if (token !== loadToken) return;
        extendWithAutoplay();
      })
      .catch(() => {
        if (token !== loadToken) return;
        engine = "audio";
        stopSpotifyPolling();
        loadAudio(track, token);
      });
    return;
  }

  if (engine === "spotify") {
    spotifyPause();
    stopSpotifyPolling();
  }
  engine = "audio";
  loadAudio(track, token);
}

function loadAudio(track, token) {
  sourceUrl(track)
    .then((src) => {
      if (token !== loadToken) return;
      audio.src = src;
      audio.play().catch((e) => console.error("Wiedergabe fehlgeschlagen:", e));
      prefetchUpcoming();
      extendWithAutoplay();
    })
    .catch((e) => {
      if (token !== loadToken) return;
      player.loading = false;
      player.error = `„${track.title}“ konnte nicht gefunden werden: ${e}`;
    });
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
  try {
    const tracks = await invoke("ytm_radio", { id: await videoId(seed) });
    return tracks.map((t) => ({ ...t, source: "youtube" }));
  } catch {
    // Kein Internet: zufällige Offline-Songs
    return shuffled(offlineTracks().filter((t) => t.id !== seed.id)).slice(0, 20);
  }
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
    const suggestions =
      seed.source === "local"
        ? localRadio(seed)
        : seed.source === "soundcloud"
          ? await soundcloudRelated(seed.id).catch(() => youtubeRadio(seed))
          : await youtubeRadio(seed);
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
  if (engine === "spotify") {
    if (player.playing) spotifyPause();
    else spotifyResume();
    player.playing = !player.playing;
    sendPresence();
    return;
  }
  if (audio.paused) audio.play();
  else audio.pause();
}

export async function next() {
  if (player.index + 1 >= player.queue.length && player.repeat === "off") {
    await extendWithAutoplay();
  }
  if (player.index + 1 < player.queue.length) load(player.index + 1);
  else if (player.repeat === "all" && player.queue.length) load(0);
  else if (engine === "spotify") {
    spotifyPause();
    player.playing = false;
  } else {
    audio.pause();
    audio.currentTime = 0;
  }
}

export function previous() {
  if (player.position > 3 || player.index === 0) seek(0);
  else load(player.index - 1);
}

export function seek(seconds) {
  if (engine === "spotify") {
    spotifySeek(seconds);
    player.position = seconds;
    spNearEnd = false;
    sendPresence();
    return;
  }
  audio.currentTime = seconds;
}

export function setVolume(value) {
  player.volume = value;
  player.muted = false;
  audio.volume = gain(value);
  audio.muted = false;
  if (engine === "spotify") spotifyVolume(value);
  store("volumeSlider", value);
}

export function toggleMute() {
  player.muted = !player.muted;
  audio.muted = player.muted;
  if (engine === "spotify") spotifyVolume(player.muted ? 0 : player.volume);
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
function startedPlaying() {
  player.loading = false;
  errorStreak = 0;
  if (pendingHistory) {
    recordPlay(pendingHistory);
    // Nur echte YouTube-Music-Songs an den YouTube-Verlauf melden – Spotify-/SoundCloud-Songs,
    // die nur über YouTube abgespielt werden, gehören nicht in den YouTube-Verlauf
    const video = pendingHistory.source === "youtube" ? pendingHistory.id : null;
    if (video && settings.syncYouTubeHistory && ytm.status === "loggedIn") {
      invoke("ytm_report_play", { id: video }).catch((e) => console.warn("YouTube-Verlauf:", e));
    }
    pendingHistory = null;
  }
}
audio.addEventListener("playing", startedPlaying);
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
  pendingHistory = null;
  errorStreak++;
  if (errorStreak >= MAX_ERROR_STREAK) {
    player.error = "Mehrere Songs hintereinander konnten nicht abgespielt werden – Wiedergabe angehalten.";
    player.playing = false;
    errorStreak = 0;
    return;
  }
  player.error = `„${track.title}“ konnte nicht abgespielt werden.`;
  if (player.index + 1 < player.queue.length) next();
});

// --- Echte Spotify-Wiedergabe: Zustand übernehmen ------------------------

onSpotifyState((state) => {
  if (engine !== "spotify") return;
  const track = currentTrack();
  if (!track) return;
  const ours = state.trackIds.includes(track.id);

  if (ours) {
    const changed = state.playing !== player.playing;
    player.playing = state.playing;
    player.position = state.position;
    if (state.duration) player.duration = state.duration;
    if (state.playing && !spStarted) {
      spStarted = true;
      startedPlaying();
    }
    if (state.duration && state.position >= state.duration - 2.5) spNearEnd = true;
    // Ende: nach Erreichen des Endes pausiert bzw. wieder am Anfang
    if (spStarted && spNearEnd && (!state.playing || state.position < 2)) finishSpotifyTrack();
    else if (changed) sendPresence();
  } else if (spStarted) {
    // Spotify spielt etwas anderes (z. B. eigenes Autoplay) → unser Song ist vorbei
    finishSpotifyTrack();
  }
});

function finishSpotifyTrack() {
  if (spFinished) return;
  spFinished = true;
  if (player.repeat === "one") {
    load(player.index);
  } else {
    next();
  }
}

// --- Discord Rich Presence ------------------------------------------------

export function sendPresence() {
  const t = currentTrack();
  if (!t || !settings.discordEnabled) {
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
      position: player.position,
      duration: player.duration,
      paused: !player.playing,
      display: settings.discordDisplay,
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
  navigator.mediaSession.setActionHandler("play", () => !player.playing && togglePlay());
  navigator.mediaSession.setActionHandler("pause", () => player.playing && togglePlay());
  navigator.mediaSession.setActionHandler("nexttrack", next);
  navigator.mediaSession.setActionHandler("previoustrack", previous);
  navigator.mediaSession.setActionHandler("seekto", (e) => seek(e.seekTime));
}
