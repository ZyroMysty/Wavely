import {
  collection,
  doc,
  getDocs,
  onSnapshot,
  orderBy,
  query,
  serverTimestamp,
  setDoc,
  Timestamp,
  where,
  writeBatch,
} from "firebase/firestore";
import { db } from "./firebase.js";
import { loadPlays, appendPlays, onPlaysWritten } from "./stats.svelte.js";
import { mergeHistory } from "./history.svelte.js";
import { playlists, playlistsReady, onPlaylistsChanged, applyRemotePlaylists } from "./playlists.svelte.js";

/**
 * Abgleich mit dem Wavely-Konto (Firestore):
 *  users/{uid}/plays/{key}      – jedes Hören (Verlauf + Rückblick), nur hinzufügen, nie ändern
 *  users/{uid}/playlists/{id}   – Wavely-Playlists; die neueste Änderung gewinnt, Löschen als Markierung
 * Ohne Internet sammelt Firestore die Änderungen und lädt sie später hoch.
 */

export const sync = $state({
  /** "off" | "syncing" | "ready" | "error" */
  status: "off",
  error: null,
  lastSync: null,
});

/** Kennung dieses Geräts – eigene Einträge nicht doppelt herunterladen */
const DEVICE = (() => {
  try {
    let id = localStorage.getItem("deviceId");
    if (!id) localStorage.setItem("deviceId", (id = crypto.randomUUID()));
    return id;
  } catch {
    return crypto.randomUUID();
  }
})();

let uid = null;
let unsubscribers = [];

/** Kurzer Hash (FNV-1a) – Song-IDs lokaler Dateien sind Pfade und dürfen keine „/“ enthalten */
function hash(text) {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return (h >>> 0).toString(36);
}
export const playKey = (p) => `${p.t}_${hash(`${p.src}:${p.id}`)}`;

const local = (key) => `${key}:${uid}`;
function stored(key, fallback) {
  try {
    return JSON.parse(localStorage.getItem(local(key)) ?? "null") ?? fallback;
  } catch {
    return fallback;
  }
}
function store(key, value) {
  try {
    localStorage.setItem(local(key), JSON.stringify(value));
  } catch {}
}

// --- Hören ------------------------------------------------------------------

/** Lokale Pfade nicht in die Cloud schicken – nur Titel/Künstler (andere Geräte haben die Datei nicht) */
function cloudPlay(p) {
  const entry = { ...p, dev: DEVICE, at: serverTimestamp() };
  if (p.src === "local") entry.id = hash(p.id);
  delete entry.est;
  return entry;
}

async function uploadPlays(list) {
  for (let i = 0; i < list.length; i += 400) {
    const batch = writeBatch(db);
    for (const p of list.slice(i, i + 400)) batch.set(doc(db, "users", uid, "plays", playKey(p)), cloudPlay(p));
    await batch.commit();
  }
}

/** Einträge anderer Geräte übernehmen (Rückblick-Datei + Verlauf) */
async function takeRemotePlays(docs) {
  const foreign = docs
    .filter((d) => !d.metadata.hasPendingWrites)
    .map((d) => d.data())
    .filter((p) => p.dev !== DEVICE);
  if (!foreign.length) return;
  const existing = new Set((await loadPlays(0)).map(playKey));
  const fresh = foreign
    .filter((p) => !existing.has(playKey(p)))
    .map(({ dev, at, ...p }) => p)
    .sort((a, b) => a.t - b.t);
  if (!fresh.length) return;
  await appendPlays(fresh);
  mergeHistory(
    fresh.map((p) => ({
      playedAt: p.t,
      track: {
        source: p.src,
        id: p.id,
        title: p.title,
        artist: p.artist,
        album: p.album,
        artworkUrl: p.art,
        duration: p.d,
        // Song von einem anderen PC – die Datei gibt es hier nicht
        remote: p.src === "local" ? true : undefined,
      },
    })),
  );
}

async function syncPlays() {
  // 1. Beim ersten Mal mit diesem Konto: alles hochladen, was dieser PC schon gesammelt hat
  if (!stored("syncUploaded", false)) {
    await uploadPlays(await loadPlays(0));
    store("syncUploaded", true);
  }
  // 2. Neues von anderen Geräten seit dem letzten Abgleich
  const plays = collection(db, "users", uid, "plays");
  const since = Timestamp.fromMillis(stored("syncDownloadedAt", 0));
  const startedAt = Date.now();
  const snapshot = await getDocs(query(plays, where("at", ">", since), orderBy("at")));
  await takeRemotePlays(snapshot.docs);
  store("syncDownloadedAt", startedAt - 60_000);
  // 3. Ab jetzt live mithören
  unsubscribers.push(
    onSnapshot(query(plays, where("at", ">", Timestamp.fromMillis(startedAt - 60_000))), (snap) => {
      takeRemotePlays(snap.docChanges().filter((c) => c.type === "added").map((c) => c.doc));
      store("syncDownloadedAt", Date.now() - 60_000);
      sync.lastSync = Date.now();
    }),
  );
}

// --- Playlists --------------------------------------------------------------

const playlistDoc = (id) => doc(db, "users", uid, "playlists", id);

function cloudPlaylist(p) {
  return {
    name: p.name,
    description: p.description ?? "",
    createdAt: p.createdAt,
    updatedAt: p.updatedAt,
    // Pfade lokaler Dateien bleiben auf dem PC
    items: p.items.map((i) => (i.track.source === "local" ? { ...i, track: { ...i.track, path: null } } : i)),
    deleted: false,
  };
}

/** Was zuletzt hoch- oder heruntergeladen wurde: id → updatedAt */
let known = {};

async function pushPlaylists(list, deletedIds = []) {
  if (!uid) return;
  for (const p of list) {
    if (known[p.id] === p.updatedAt) continue;
    known[p.id] = p.updatedAt;
    await setDoc(playlistDoc(p.id), cloudPlaylist($state.snapshot(p)));
  }
  for (const id of deletedIds) {
    known[id] = Date.now();
    await setDoc(playlistDoc(id), { deleted: true, updatedAt: known[id], items: [], name: "" });
  }
}

async function syncPlaylists() {
  await playlistsReady;
  const snapshot = await getDocs(collection(db, "users", uid, "playlists"));
  const remote = snapshot.docs.map((d) => ({ id: d.id, ...d.data() }));
  remote.forEach((r) => (known[r.id] = r.updatedAt));
  applyRemotePlaylists(remote);
  // Lokale Playlists, die neuer sind oder in der Cloud fehlen
  await pushPlaylists(playlists.list);

  unsubscribers.push(
    onSnapshot(collection(db, "users", uid, "playlists"), (snap) => {
      const changed = snap
        .docChanges()
        .filter((c) => !c.doc.metadata.hasPendingWrites)
        .map((c) => ({ id: c.doc.id, ...c.doc.data() }))
        .filter((r) => known[r.id] !== r.updatedAt);
      if (!changed.length) return;
      changed.forEach((r) => (known[r.id] = r.updatedAt));
      applyRemotePlaylists(changed);
    }),
  );
}

// --- Start / Stopp ----------------------------------------------------------

export async function startSync(userId) {
  stopSync();
  uid = userId;
  known = {};
  sync.status = "syncing";
  sync.error = null;
  try {
    await Promise.all([syncPlays(), syncPlaylists()]);
    unsubscribers.push(onPlaysWritten((entries) => uid && uploadPlays(entries).catch(() => {})));
    unsubscribers.push(onPlaylistsChanged((list, deleted) => pushPlaylists(list, deleted).catch(() => {})));
    sync.status = "ready";
    sync.lastSync = Date.now();
  } catch (e) {
    console.error("Abgleich:", e);
    sync.status = "error";
    sync.error =
      e?.code === "permission-denied"
        ? "Firebase verweigert den Zugriff – sind die Sicherheitsregeln eingetragen?"
        : String(e?.message ?? e);
  }
}

export function stopSync() {
  unsubscribers.forEach((off) => off());
  unsubscribers = [];
  uid = null;
  sync.status = "off";
}

/** Beim Löschen des Kontos: alle Cloud-Daten entfernen */
export async function deleteCloudData(userId) {
  for (const name of ["plays", "playlists"]) {
    const snapshot = await getDocs(collection(db, "users", userId, name));
    for (let i = 0; i < snapshot.docs.length; i += 400) {
      const batch = writeBatch(db);
      snapshot.docs.slice(i, i + 400).forEach((d) => batch.delete(d.ref));
      await batch.commit();
    }
  }
}
