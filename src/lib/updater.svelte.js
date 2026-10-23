import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { settings } from "./settings.svelte.js";

/**
 * Automatische Updates: Wavely fragt bei GitHub (Releases → latest.json) nach einer neuen Version,
 * lädt den signierten Installer herunter und startet ihn. Ohne gültige Signatur wird nichts installiert.
 */

const CHECK_INTERVAL = 6 * 60 * 60 * 1000;

export const updater = $state({
  /** idle | checking | none | available | downloading | installing | error */
  status: "idle",
  version: null,
  notes: null,
  /** Download-Fortschritt 0–1, solange die Größe bekannt ist */
  progress: null,
  error: null,
  /** Hinweis für diese Sitzung weggeklickt */
  dismissed: false,
});

let update = null;

/** Nach einer neuen Version suchen. `silent`: Fehler nicht anzeigen (automatische Prüfung). */
export async function checkForUpdates(silent = false) {
  if (["checking", "downloading", "installing"].includes(updater.status)) return;
  updater.status = "checking";
  updater.error = null;
  try {
    update = await check();
    if (update) {
      updater.status = "available";
      updater.version = update.version;
      updater.notes = update.body ?? null;
    } else {
      updater.status = "none";
    }
  } catch (e) {
    updater.status = silent ? "idle" : "error";
    updater.error = String(e);
  }
}

/** Update herunterladen und installieren; Wavely beendet sich dabei und startet neu. */
export async function installUpdate() {
  if (!update) return;
  updater.status = "downloading";
  updater.progress = null;
  let total = 0;
  let received = 0;
  try {
    await update.downloadAndInstall((event) => {
      if (event.event === "Started") {
        total = event.data.contentLength ?? 0;
      } else if (event.event === "Progress") {
        received += event.data.chunkLength;
        if (total) updater.progress = Math.min(received / total, 1);
      } else if (event.event === "Finished") {
        updater.status = "installing";
      }
    });
    await relaunch();
  } catch (e) {
    updater.status = "error";
    updater.error = String(e);
  }
}

/** Beim Start und danach alle paar Stunden prüfen (abschaltbar in den Einstellungen). */
export function initUpdater() {
  const auto = () => settings.autoUpdateCheck && checkForUpdates(true);
  setTimeout(auto, 5000);
  setInterval(auto, CHECK_INTERVAL);
}
