import { getVersion } from "@tauri-apps/api/app";
import changelogText from "../../CHANGELOG.md?raw";

/**
 * Versionsverlauf aus CHANGELOG.md (wird beim Bauen mit eingepackt, funktioniert also auch offline).
 * Nach einem Update zeigt Wavely einmal „Neu in Version …“.
 */

/** [{ version, date, items: [string] }], neueste zuerst, ohne „Unveröffentlicht“ */
export const releases = changelogText
  .replace(/\r/g, "")
  .split(/^## /m)
  .slice(1)
  .map((part) => {
    const [head, ...lines] = part.split("\n");
    const [version, date] = head.split(/\s+[–-]\s+/);
    return {
      version: version.trim(),
      date: date?.trim() ?? null,
      items: lines.filter((l) => l.startsWith("- ")).map((l) => l.slice(2).trim()),
    };
  })
  .filter((r) => /^\d/.test(r.version) && r.items.length);

export const changelog = $state({
  open: false,
  /** Nur diese Version hervorheben (nach einem Update) */
  highlight: null,
});

export function openChangelog(highlight = null) {
  changelog.highlight = highlight;
  changelog.open = true;
}

const SEEN_KEY = "lastSeenVersion";

/** Nach einem Update einmal die Neuerungen zeigen (nicht bei der ersten Installation). */
export async function showWhatsNew() {
  const version = await getVersion().catch(() => null);
  if (!version) return;
  let seen = null;
  try {
    seen = localStorage.getItem(SEEN_KEY);
    localStorage.setItem(SEEN_KEY, version);
  } catch {
    return;
  }
  if (seen && seen !== version && releases.some((r) => r.version === version)) openChangelog(version);
}

export function formatDate(date) {
  if (!date) return "";
  const d = new Date(date);
  return Number.isNaN(d.getTime()) ? date : d.toLocaleDateString("de-DE", { day: "numeric", month: "long", year: "numeric" });
}
