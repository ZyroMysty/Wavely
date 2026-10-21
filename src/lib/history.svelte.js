/** Verlauf: zuletzt gespielte Songs (lokal gespeichert, neueste zuerst). */

const KEY = "history";
const MAX_ENTRIES = 300;

function load() {
  try {
    const entries = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(entries) ? entries : [];
  } catch {
    return [];
  }
}

export const history = $state({ entries: load() });

function save() {
  try {
    localStorage.setItem(KEY, JSON.stringify(history.entries));
  } catch {}
}

/** Nur die Felder speichern, die zum erneuten Abspielen und Anzeigen nötig sind */
function slim(track) {
  const { qid, auto, ...rest } = track;
  return rest;
}

export function recordPlay(track) {
  const last = history.entries[0];
  // Derselbe Song direkt nochmal (z. B. Wiederholen) innerhalb einer Minute: nicht doppelt eintragen
  if (last && last.track.id === track.id && Date.now() - last.playedAt < 60_000) return;
  history.entries.unshift({ track: slim(track), playedAt: Date.now() });
  if (history.entries.length > MAX_ENTRIES) history.entries.length = MAX_ENTRIES;
  save();
}

export function removeEntry(index) {
  history.entries.splice(index, 1);
  save();
}

export function clearHistory() {
  history.entries = [];
  save();
}

/** Einträge nach Tag gruppieren: „Heute“, „Gestern“, sonst Datum */
export function groupedByDay(entries) {
  const groups = [];
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const dayMs = 24 * 60 * 60 * 1000;

  for (const [index, entry] of entries.entries()) {
    const day = new Date(entry.playedAt);
    day.setHours(0, 0, 0, 0);
    const diff = Math.round((today - day) / dayMs);
    const label =
      diff === 0
        ? "Heute"
        : diff === 1
          ? "Gestern"
          : day.toLocaleDateString("de-DE", { weekday: "long", day: "numeric", month: "long" });
    if (groups.at(-1)?.label !== label) groups.push({ label, items: [] });
    groups.at(-1).items.push({ entry, index });
  }
  return groups;
}
