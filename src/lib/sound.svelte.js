import { settings, updateSetting } from "./settings.svelte.js";

/**
 * Klang für Wavelys eigenen Player (lokal, YouTube, SoundCloud, Spotify Free).
 * Das Audio-Element läuft durch einen Web-Audio-Signalweg:
 *
 *   Audio → Equalizer (10 Bänder) → Angleichung → Lautstärke/Ausblenden → Begrenzer → Ausgabegerät
 *              └→ Lautheitsmessung (vor Equalizer und Lautstärke)
 *
 * Echte Spotify-Wiedergabe (Premium) läuft im Spotify-Player und ist davon nicht betroffen.
 */

export const EQ_BANDS = [32, 64, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];

export const EQ_PRESETS = [
  { id: "flat", name: "Aus (neutral)", gains: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
  { id: "bass", name: "Bass Boost", gains: [6, 5, 4, 2, 0, 0, 0, 0, 0, 0] },
  { id: "treble", name: "Höhen", gains: [0, 0, 0, 0, 0, 1, 2, 4, 5, 6] },
  { id: "vocal", name: "Gesang", gains: [-2, -2, -1, 1, 3, 4, 3, 1, 0, -1] },
  { id: "pop", name: "Pop", gains: [-1, 1, 3, 4, 2, 0, -1, -1, 1, 2] },
  { id: "rock", name: "Rock", gains: [4, 3, 1, -1, -2, -1, 1, 3, 4, 4] },
  { id: "electronic", name: "Elektronisch", gains: [5, 4, 1, 0, -2, 1, 0, 2, 4, 5] },
  { id: "acoustic", name: "Akustik", gains: [3, 2, 1, 1, 2, 2, 3, 2, 2, 1] },
  { id: "speech", name: "Sprache / Podcast", gains: [-6, -4, -2, 0, 2, 4, 4, 3, 1, -2] },
  { id: "night", name: "Nachtmodus (leise Stellen lauter)", gains: [-3, -2, 0, 1, 2, 2, 2, 1, 0, -1] },
];

/** Ziel-Lautheit (ähnlich wie Spotify/YouTube) und wie weit angeglichen wird */
const TARGET_LUFS = -14;
const MAX_BOOST_DB = 6;
const MAX_CUT_DB = -14;
/** Annahme, solange ein Song noch nicht gemessen ist (typische moderne Musik) */
const DEFAULT_LUFS = -10;

export const sound = $state({
  /** Ausgabegeräte von Windows ({ id, name }) – leer, bis die Liste geladen wurde */
  outputs: [],
  /** Gerätenamen sind erst nach einmaliger Freigabe sichtbar */
  outputsLoaded: false,
  /** Aktuelle Angleichung in dB (zur Anzeige) */
  normalizeDb: 0,
});

let ctx = null;
let audio = null;
let nodes = null;
/** Lautstärke, bevor der Signalweg existiert */
let outputGain = 1;

const dbToGain = (db) => Math.pow(10, db / 20);

/** Audio-Element an den Signalweg hängen (einmal, beim Start des Players). */
export function attachAudio(element) {
  audio = element;
  audio.crossOrigin = "anonymous";
  applyTempo();
}

/** Signalweg beim ersten Abspielen aufbauen (AudioContext erst bei Bedarf). */
export function ensureGraph() {
  if (nodes || !audio) return;
  try {
    ctx = new AudioContext({ latencyHint: "playback" });
    const source = ctx.createMediaElementSource(audio);

    const eq = EQ_BANDS.map((frequency, i) => {
      const filter = ctx.createBiquadFilter();
      filter.type = i === 0 ? "lowshelf" : i === EQ_BANDS.length - 1 ? "highshelf" : "peaking";
      filter.frequency.value = frequency;
      filter.Q.value = 1.4;
      return filter;
    });
    const preamp = ctx.createGain();
    const normalize = ctx.createGain();
    const volume = ctx.createGain();
    const fade = ctx.createGain();
    const limiter = ctx.createDynamicsCompressor();
    limiter.threshold.value = -1;
    limiter.knee.value = 0;
    limiter.ratio.value = 20;
    limiter.attack.value = 0.003;
    limiter.release.value = 0.15;

    source.connect(eq[0]);
    eq.reduce((a, b) => (a.connect(b), b));
    eq.at(-1).connect(preamp).connect(normalize).connect(volume).connect(fade).connect(limiter).connect(ctx.destination);

    // Lautheitsmessung: K-Filter (vereinfacht nach EBU R128) vor allem anderen
    const highpass = ctx.createBiquadFilter();
    highpass.type = "highpass";
    highpass.frequency.value = 38;
    highpass.Q.value = 0.5;
    const shelf = ctx.createBiquadFilter();
    shelf.type = "highshelf";
    shelf.frequency.value = 1500;
    shelf.gain.value = 4;
    const analyser = ctx.createAnalyser();
    analyser.fftSize = 4096;
    source.connect(highpass).connect(shelf).connect(analyser);

    nodes = { eq, preamp, normalize, volume, fade, analyser };
    // Ab jetzt regelt der Signalweg die Lautstärke; das Element liefert immer volle Lautstärke (für die Messung)
    volume.gain.value = outputGain;
    audio.volume = 1;
    applyEqualizer();
    applyNormalize(true);
    applyOutput();
  } catch (e) {
    console.error("Klang-Signalweg konnte nicht aufgebaut werden:", e);
  }
}

/** Nach einer Benutzeraktion oder beim Abspielen sicherstellen, dass der Ton läuft. */
export function resumeAudio() {
  ensureGraph();
  if (ctx?.state === "suspended") ctx.resume().catch(() => {});
}

// --- Lautstärke & Ausblenden -------------------------------------------------

/** Lautstärke im Signalweg (die Messung davor bleibt davon unberührt) */
export function setOutputGain(value) {
  outputGain = value;
  if (nodes) nodes.volume.gain.setTargetAtTime(value, ctx.currentTime, 0.015);
  else if (audio) audio.volume = Math.min(1, value);
}

/** Für den Schlaf-Timer: 1 = normal, 0 = still; `seconds` = Dauer der Blende */
export function setFade(value, seconds = 0.3) {
  if (!nodes) return;
  const now = ctx.currentTime;
  nodes.fade.gain.cancelScheduledValues(now);
  nodes.fade.gain.setValueAtTime(nodes.fade.gain.value, now);
  nodes.fade.gain.linearRampToValueAtTime(value, now + Math.max(seconds, 0.01));
}

// --- Equalizer --------------------------------------------------------------

export function currentEqGains() {
  return settings.eqEnabled ? settings.eqGains : EQ_PRESETS[0].gains;
}

export function applyEqualizer() {
  if (!nodes) return;
  const gains = currentEqGains();
  gains.forEach((db, i) => nodes.eq[i].gain.setTargetAtTime(db, ctx.currentTime, 0.02));
  // Anhebungen vorab ausgleichen, damit der Begrenzer kaum eingreifen muss
  const maxBoost = Math.max(0, ...gains);
  nodes.preamp.gain.setTargetAtTime(dbToGain(-maxBoost * 0.6), ctx.currentTime, 0.02);
}

export function selectEqPreset(id) {
  const preset = EQ_PRESETS.find((p) => p.id === id);
  if (!preset) return;
  updateSetting("eqPreset", id);
  updateSetting("eqGains", [...preset.gains]);
  updateSetting("eqEnabled", id !== "flat");
  applyEqualizer();
}

export function setEqBand(index, db) {
  const gains = [...settings.eqGains];
  gains[index] = db;
  updateSetting("eqGains", gains);
  updateSetting("eqPreset", "custom");
  updateSetting("eqEnabled", true);
  applyEqualizer();
}

export function setEqEnabled(on) {
  updateSetting("eqEnabled", on);
  applyEqualizer();
}

// --- Tempo ------------------------------------------------------------------

export function applyTempo() {
  if (!audio) return;
  const rate = settings.playbackRate || 1;
  audio.defaultPlaybackRate = rate;
  audio.playbackRate = rate;
  audio.preservesPitch = settings.preservePitch;
}

export function setTempo(rate) {
  updateSetting("playbackRate", Math.round(rate * 100) / 100);
  applyTempo();
}

export function setPreservePitch(on) {
  updateSetting("preservePitch", on);
  applyTempo();
}

// --- Lautstärke angleichen --------------------------------------------------
//
// Gemessen wird die Lautheit des laufenden Songs (gegatete Blöcke à 400 ms, wie bei EBU R128).
// Der Messwert wird pro Song gespeichert – beim nächsten Mal stimmt die Lautstärke sofort.

const CACHE_KEY = "loudness";
const CACHE_MAX = 3000;
let cache = loadCache();
let trackKey = null;
let blocks = [];
let pending = [];
let measureTimer = null;

function loadCache() {
  try {
    return JSON.parse(localStorage.getItem(CACHE_KEY) ?? "{}");
  } catch {
    return {};
  }
}

function saveCache() {
  const keys = Object.keys(cache);
  if (keys.length > CACHE_MAX) for (const k of keys.slice(0, keys.length - CACHE_MAX)) delete cache[k];
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify(cache));
  } catch {}
}

/**
 * Gegatete Lautheit aus 400-ms-Blöcken (absolute Schwelle −70, relative −10 LU).
 * Der Analyser mischt Stereo zu Mono (Mittelwert); die Norm addiert die Kanäle → +3 dB.
 */
function integrated(list) {
  const loud = (ms) => -0.691 + 3.01 + 10 * Math.log10(ms);
  const abs = list.filter((ms) => loud(ms) > -70);
  if (!abs.length) return null;
  const mean = abs.reduce((a, b) => a + b, 0) / abs.length;
  const rel = abs.filter((ms) => loud(ms) > loud(mean) - 10);
  return loud(rel.reduce((a, b) => a + b, 0) / rel.length);
}

function gainFor(lufs) {
  return Math.max(MAX_CUT_DB, Math.min(MAX_BOOST_DB, TARGET_LUFS - lufs));
}

function setNormalizeDb(db, smooth) {
  sound.normalizeDb = Math.round(db * 10) / 10;
  if (!nodes) return;
  const target = settings.normalize ? dbToGain(db) : 1;
  nodes.normalize.gain.setTargetAtTime(target, ctx.currentTime, smooth ? 1.2 : 0.05);
}

export function applyNormalize(immediate = false) {
  const lufs = (trackKey && cache[trackKey]) ?? integrated(blocks) ?? DEFAULT_LUFS;
  setNormalizeDb(gainFor(lufs), !immediate);
}

export function setNormalize(on) {
  updateSetting("normalize", on);
  applyNormalize(true);
}

/** Neuer Song: Messung zurücksetzen, gespeicherten Wert sofort anwenden */
export function startTrack(key) {
  finishMeasurement();
  trackKey = key;
  blocks = [];
  pending = [];
  applyNormalize(true);
  clearInterval(measureTimer);
  measureTimer = setInterval(measure, 100);
}

function measure() {
  if (!nodes || audio.paused || !trackKey) return;
  const data = new Float32Array(nodes.analyser.fftSize);
  nodes.analyser.getFloatTimeDomainData(data);
  let sum = 0;
  for (const x of data) sum += x * x;
  pending.push(sum / data.length);
  if (pending.length < 4) return;
  blocks.push(pending.reduce((a, b) => a + b, 0) / pending.length);
  pending = [];
  // Unbekannter Song: nach 3 s erste Schätzung, danach langsam nachführen
  if (!cache[trackKey] && blocks.length >= 8 && blocks.length % 5 === 0) {
    const lufs = integrated(blocks);
    if (lufs != null) setNormalizeDb(gainFor(lufs), true);
  }
  // Nach 30 s gilt die Messung als brauchbar
  if (blocks.length === 75) finishMeasurement(false);
}

function finishMeasurement(reset = true) {
  if (trackKey && blocks.length >= 75) {
    const lufs = integrated(blocks);
    if (lufs != null) {
      cache[trackKey] = Math.round(lufs * 10) / 10;
      saveCache();
    }
  }
  if (reset) clearInterval(measureTimer);
}

// --- Ausgabegerät -----------------------------------------------------------

/** Ausgabegeräte von Windows laden (Namen erst nach einmaliger Freigabe, die Wavely selbst erteilt). */
export async function loadOutputs() {
  try {
    let devices = (await navigator.mediaDevices.enumerateDevices()).filter((d) => d.kind === "audiooutput");
    if (!devices.some((d) => d.label)) {
      // Ohne Freigabe verrät Chromium die Gerätenamen nicht; das Mikrofon wird sofort wieder geschlossen
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true }).catch(() => null);
      stream?.getTracks().forEach((t) => t.stop());
      devices = (await navigator.mediaDevices.enumerateDevices()).filter((d) => d.kind === "audiooutput");
    }
    sound.outputs = devices
      .filter((d) => d.deviceId && d.deviceId !== "default" && d.deviceId !== "communications" && d.label)
      // USB-Kennungen wie „(1532:0574)“ weglassen
      .map((d) => ({ id: d.deviceId, name: d.label.replace(/\s*\([0-9a-f]{4}:[0-9a-f]{4}\)\s*$/i, "") }));
    sound.outputsLoaded = sound.outputs.length > 0;
  } catch (e) {
    console.warn("Ausgabegeräte:", e);
  }
}

export async function selectOutput(id) {
  updateSetting("outputDevice", id || "");
  await applyOutput();
}

async function applyOutput() {
  if (!ctx?.setSinkId) return;
  let id = settings.outputDevice || "";
  // Gerät nicht mehr angeschlossen → Windows-Standard
  if (id && sound.outputsLoaded && !sound.outputs.some((o) => o.id === id)) id = "";
  try {
    await ctx.setSinkId(id);
  } catch (e) {
    console.warn("Ausgabegerät nicht verfügbar:", e);
    if (id) await ctx.setSinkId("").catch(() => {});
  }
}

if (navigator.mediaDevices) {
  navigator.mediaDevices.addEventListener("devicechange", async () => {
    if (!sound.outputsLoaded && !settings.outputDevice) return;
    await loadOutputs();
    applyOutput();
  });
}

/** Beim Start: gespeichertes Gerät wieder auswählen */
export async function initSound() {
  if (settings.outputDevice) await loadOutputs();
}
