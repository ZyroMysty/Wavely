/**
 * Echte Spotify-Wiedergabe für Premium-Konten.
 *
 * - In Wavely selbst über den Spotify Web Playback SDK: Wavely wird zu einem Spotify-Gerät („Wavely“).
 * - Auf anderen Geräten (Handy, Lautsprecher, Spotify-App) über Spotify Connect (Web API).
 *
 * Free-Konten meldet der SDK mit „account_error“ – dann bleibt es bei der Wiedergabe über YouTube Music.
 */
import { invoke } from "@tauri-apps/api/core";
import { spotify } from "./spotify.svelte.js";
import { settings, updateSetting } from "./settings.svelte.js";

export const spotifyOut = $state({
  /** null = noch unbekannt, true = Premium (SDK bereit), false = Free → YouTube Music */
  premium: null,
  /** Geräte-ID des SDK-Players „Wavely“ */
  deviceId: null,
  /** Liste der Spotify-Geräte (für die Auswahl) */
  devices: [],
  /** Hinweis, z. B. dass neue Rechte fehlen */
  notice: null,
});

/** Ziel-Gerät: "wavely" (dieser PC) oder die ID eines anderen Spotify-Geräts */
export const targetDevice = () => settings.spotifyDevice ?? "wavely";

let sdkPlayer = null;
let sdkLoading = null;
let listener = null;
let pollTimer = null;

/** Aufrufer (player.svelte.js) bekommt Zustandsänderungen: { playing, position, duration, trackId } */
export function onSpotifyState(fn) {
  listener = fn;
}

function premiumKey() {
  return spotify.account?.id ? `spotifyPremium:${spotify.account.id}` : null;
}

function rememberPremium(value) {
  spotifyOut.premium = value;
  const key = premiumKey();
  if (key) {
    try {
      localStorage.setItem(key, JSON.stringify(value));
    } catch {}
  }
}

/** SDK laden und den Player „Wavely“ anmelden (einmal pro Sitzung) */
export function initSpotifyPlayer() {
  if (sdkLoading || spotify.status !== "loggedIn") return sdkLoading;
  // Schon bekannt, dass das Konto Free ist → SDK gar nicht erst laden
  const key = premiumKey();
  if (key) {
    try {
      if (JSON.parse(localStorage.getItem(key)) === false) {
        spotifyOut.premium = false;
        return null;
      }
    } catch {}
  }

  sdkLoading = new Promise((resolve) => {
    window.onSpotifyWebPlaybackSDKReady = () => {
      sdkPlayer = new window.Spotify.Player({
        name: "Wavely",
        getOAuthToken: (cb) => invoke("spotify_access_token").then(cb).catch(() => cb("")),
        volume: 0.6,
      });
      sdkPlayer.addListener("ready", ({ device_id }) => {
        spotifyOut.deviceId = device_id;
        resolve(true);
      });
      sdkPlayer.addListener("not_ready", () => (spotifyOut.deviceId = null));
      sdkPlayer.addListener("account_error", () => {
        // Kein Premium → Wiedergabe über YouTube Music
        rememberPremium(false);
        resolve(false);
      });
      sdkPlayer.addListener("authentication_error", ({ message }) => {
        spotifyOut.notice = "Für die Spotify-Wiedergabe bitte einmal bei Spotify ab- und wieder anmelden.";
        console.warn("Spotify SDK:", message);
        resolve(false);
      });
      sdkPlayer.addListener("initialization_error", ({ message }) => {
        console.warn("Spotify SDK:", message);
        resolve(false);
      });
      sdkPlayer.addListener("player_state_changed", (state) => {
        if (!state || targetDevice() !== "wavely") return;
        const track = state.track_window?.current_track;
        listener?.({
          playing: !state.paused,
          position: state.position / 1000,
          duration: state.duration / 1000,
          trackIds: [track?.id, track?.linked_from?.id].filter(Boolean),
        });
      });
      sdkPlayer.connect();
    };
    const script = document.createElement("script");
    script.src = "https://sdk.scdn.co/spotify-player.js";
    script.onerror = () => resolve(false);
    document.head.appendChild(script);
  });
  return sdkLoading;
}

/** Kann dieser Song echt über Spotify laufen? (Spotify-Song, angemeldet, nicht als Free bekannt) */
export function canPlayOnSpotify(track) {
  return track?.source === "spotify" && spotify.status === "loggedIn" && spotifyOut.premium !== false;
}

async function deviceForPlayback() {
  const target = targetDevice();
  if (target !== "wavely") return target;
  await initSpotifyPlayer();
  if (!spotifyOut.deviceId) throw new Error("FALLBACK");
  return spotifyOut.deviceId;
}

/** Song über Spotify starten. Wirft „FALLBACK“, wenn es nur über YouTube Music geht. */
export async function playOnSpotify(track, positionSec = 0) {
  const deviceId = await deviceForPlayback();
  try {
    await invoke("spotify_play", { deviceId, trackId: track.id, positionMs: Math.round(positionSec * 1000) });
    if (spotifyOut.premium !== true) rememberPremium(true);
    spotifyOut.notice = null;
    startPolling();
  } catch (e) {
    const code = String(e);
    if (code === "PREMIUM_REQUIRED") rememberPremium(false);
    if (code === "SCOPE") spotifyOut.notice = "Für die Spotify-Wiedergabe bitte einmal bei Spotify ab- und wieder anmelden.";
    if (code === "NO_ACTIVE_DEVICE" && targetDevice() !== "wavely") {
      spotifyOut.notice = "Das gewählte Spotify-Gerät ist nicht erreichbar – zurück zu „Wavely“.";
      updateSetting("spotifyDevice", "wavely");
    }
    throw new Error("FALLBACK");
  }
}

// --- Steuerung (SDK in Wavely oder Connect-Befehle für andere Geräte) ----

const remote = () => targetDevice() !== "wavely";

export function spotifyPause() {
  if (!remote() && sdkPlayer) return sdkPlayer.pause();
  return invoke("spotify_pause", { deviceId: null }).catch(() => {});
}

export function spotifyResume() {
  if (!remote() && sdkPlayer) return sdkPlayer.resume();
  return invoke("spotify_resume", { deviceId: null }).catch(() => {});
}

export function spotifySeek(seconds) {
  const ms = Math.max(0, Math.round(seconds * 1000));
  if (!remote() && sdkPlayer) return sdkPlayer.seek(ms);
  return invoke("spotify_seek", { positionMs: ms }).catch(() => {});
}

let volumeTimer;
/** @param {number} value Stellung des Lautstärkereglers (0–1) */
export function spotifyVolume(value) {
  // In Wavely dieselbe Kurve wie beim normalen Player (feiner im leisen Bereich)
  if (!remote() && sdkPlayer) return sdkPlayer.setVolume(Math.pow(value, 3));
  // Fremde Geräte nicht mit jeder Reglerbewegung fluten
  clearTimeout(volumeTimer);
  volumeTimer = setTimeout(() => invoke("spotify_volume", { percent: Math.round(value * 100) }).catch(() => {}), 250);
}

/**
 * Zustand jede Sekunde abfragen: der SDK meldet von sich aus nur Änderungen (Play/Pause/Sprung),
 * nicht den Fortschritt – den braucht Wavely aber, um das Song-Ende zu erkennen.
 * In Wavely: lokal beim SDK (kein Netzwerk); auf anderen Geräten: über die Web API.
 */
function startPolling() {
  stopPolling();
  pollTimer = setInterval(async () => {
    if (!remote() && sdkPlayer) {
      const state = await sdkPlayer.getCurrentState().catch(() => null);
      if (!state || remote()) return;
      const track = state.track_window?.current_track;
      listener?.({
        playing: !state.paused,
        position: state.position / 1000,
        duration: state.duration / 1000,
        trackIds: [track?.id, track?.linked_from?.id].filter(Boolean),
      });
      return;
    }
    const state = await invoke("spotify_playback").catch(() => null);
    if (!state || !remote()) return;
    listener?.({
      playing: state.isPlaying,
      position: state.progressMs / 1000,
      duration: state.durationMs / 1000,
      trackIds: [state.trackId].filter(Boolean),
    });
  }, 1000);
}

export function stopPolling() {
  clearInterval(pollTimer);
  pollTimer = null;
}

// --- Geräteauswahl -------------------------------------------------------

export async function loadDevices() {
  try {
    const devices = await invoke("spotify_devices");
    // „Wavely“ selbst steht schon als erster Eintrag in der Auswahl
    spotifyOut.devices = devices.filter((d) => d.id !== spotifyOut.deviceId);
  } catch (e) {
    if (String(e) === "SCOPE") spotifyOut.notice = "Für die Geräteauswahl bitte einmal bei Spotify ab- und wieder anmelden.";
    spotifyOut.devices = [];
  }
}

/**
 * Ausgabegerät wechseln. Läuft gerade ein Spotify-Song, wird die Wiedergabe mitgenommen.
 * @param {string} target "wavely" oder Geräte-ID
 * @param {boolean} playing ob gerade über Spotify abgespielt wird
 */
export async function selectDevice(target, playing) {
  updateSetting("spotifyDevice", target);
  const id = target === "wavely" ? (await initSpotifyPlayer(), spotifyOut.deviceId) : target;
  if (playing && id) {
    await invoke("spotify_transfer", { deviceId: id, play: true }).catch((e) => (spotifyOut.notice = String(e)));
  }
  startPolling();
}
