import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { relaunch } from "@tauri-apps/plugin-process";
import { settings } from "./settings.svelte.js";
import { account } from "./account.svelte.js";

/**
 * Automatische Updates: Wavely fragt bei GitHub (Releases → latest.json) nach einer neuen Version,
 * lädt den signierten Installer herunter und startet ihn. Ohne gültige Signatur wird nichts installiert.
 * Admins können zusätzlich Testversionen bekommen (Test-Kanal, siehe channels.rs).
 */

const CHECK_INTERVAL = 6 * 60 * 60 * 1000;

export const updater = $state({
  /** idle | checking | none | available | downloading | installing | error */
  status: "idle",
  version: null,
  notes: null,
  /** Testversion (nur Admins) */
  test: false,
  /** Download-Fortschritt 0–1, solange die Größe bekannt ist */
  progress: null,
  error: null,
  /** Hinweis für diese Sitzung weggeklickt */
  dismissed: false,
});

/** Test-Kanal nur für Admins, die ihn eingeschaltet haben */
export const testChannelActive = () => Boolean(account.admin && settings.testChannel);

/** Nach einer neuen Version suchen. `silent`: Fehler nicht anzeigen (automatische Prüfung). */
export async function checkForUpdates(silent = false) {
  if (["checking", "downloading", "installing"].includes(updater.status)) return;
  updater.status = "checking";
  updater.error = null;
  try {
    const found = await invoke("update_check", { test: testChannelActive() });
    if (found) {
      updater.status = "available";
      updater.version = found.version;
      updater.notes = found.notes ?? null;
      updater.test = found.test;
      updater.dismissed = false;
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
  updater.status = "downloading";
  updater.progress = null;
  const off = await listen("update-progress", ({ payload }) => {
    if (payload.finished) updater.status = "installing";
    else if (payload.total) updater.progress = Math.min(payload.received / payload.total, 1);
  });
  try {
    await invoke("update_install");
    await relaunch();
  } catch (e) {
    updater.status = "error";
    updater.error = String(e);
  } finally {
    off();
  }
}

/** Beim Start und danach alle paar Stunden prüfen (abschaltbar in den Einstellungen). */
export function initUpdater() {
  const auto = () => settings.autoUpdateCheck && checkForUpdates(true);
  // Etwas warten, damit das Wavely-Konto (Admin oder nicht) schon bekannt ist
  setTimeout(auto, 8000);
  setInterval(auto, CHECK_INTERVAL);
}
