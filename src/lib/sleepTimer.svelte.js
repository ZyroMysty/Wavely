import { player, pauseForSleep } from "./player.svelte.js";
import { showToast } from "./ui.svelte.js";

/**
 * Schlaf-Timer: hält die Musik nach einer Zeit oder am Ende des Songs an.
 * Die letzten Sekunden werden sanft ausgeblendet (siehe pauseForSleep).
 */

export const SLEEP_OPTIONS = [5, 10, 15, 30, 45, 60, 90, 120];

export const sleep = $state({
  /** Zeitpunkt (ms) oder null */
  endsAt: null,
  /** Am Ende des aktuellen Songs anhalten */
  endOfTrack: false,
  /** Verbleibende Sekunden (zur Anzeige) */
  remaining: 0,
});

let timer = null;

export function startSleepTimer(minutes) {
  stopSleepTimer(false);
  sleep.endsAt = Date.now() + minutes * 60_000;
  tick();
  timer = setInterval(tick, 1000);
  showToast(`Schlaf-Timer: Musik stoppt in ${minutes} Minuten`);
}

export function sleepAtEndOfTrack() {
  stopSleepTimer(false);
  sleep.endOfTrack = true;
  showToast("Schlaf-Timer: Musik stoppt am Ende des Songs");
}

export function stopSleepTimer(notify = true) {
  clearInterval(timer);
  timer = null;
  const wasActive = sleep.endsAt || sleep.endOfTrack;
  sleep.endsAt = null;
  sleep.endOfTrack = false;
  sleep.remaining = 0;
  if (notify && wasActive) showToast("Schlaf-Timer aus");
}

function tick() {
  if (!sleep.endsAt) return;
  sleep.remaining = Math.max(0, Math.round((sleep.endsAt - Date.now()) / 1000));
  if (sleep.remaining <= 0) {
    stopSleepTimer(false);
    if (player.playing) pauseForSleep();
  }
}

/** Vom Player aufgerufen, wenn ein Song zu Ende ist: true = nicht weiterspielen */
export function consumeEndOfTrack() {
  if (!sleep.endOfTrack) return false;
  stopSleepTimer(false);
  return true;
}

export function formatRemaining(seconds) {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return m >= 60 ? `${Math.floor(m / 60)}:${String(m % 60).padStart(2, "0")} h` : `${m}:${String(s).padStart(2, "0")}`;
}
