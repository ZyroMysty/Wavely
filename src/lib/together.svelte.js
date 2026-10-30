import { collection, deleteDoc, doc, getDoc, getDocs, onSnapshot, serverTimestamp, setDoc, writeBatch } from "firebase/firestore";
import { db, auth } from "./firebase.js";
import { player, currentTrack, playList, togglePlay, seek, setAutoplayBlocked } from "./player.svelte.js";
import { showToast } from "./ui.svelte.js";

/**
 * Zusammen hören: Der Gastgeber öffnet einen Raum (rooms/{CODE}) und schreibt Song, Stelle und
 * Play/Pause hinein; Gäste folgen live. Gäste melden sich unter rooms/{CODE}/members/{uid}.
 * Einladung per Code oder Link „wavely://join/CODE“.
 */

export const together = $state({
  /** null | "host" | "guest" */
  role: null,
  code: null,
  hostName: null,
  /** Für den Gastgeber: [{ uid, name }] der Gäste, die gerade dabei sind */
  members: [],
  /** Für Gäste: Song, der gerade nicht abspielbar ist */
  notice: null,
  busy: false,
});

// Ohne leicht verwechselbare Zeichen (0/O, 1/I)
const ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const newCode = () => Array.from(crypto.getRandomValues(new Uint8Array(6)), (b) => ALPHABET[b % ALPHABET.length]).join("");
export const inviteLink = (code) => `wavely://join/${code}`;

let unsubscribers = [];
let timers = [];
let stopHostEffect = null;

const me = () => auth.currentUser;
const myName = () => me()?.displayName || me()?.email?.split("@")[0] || "Jemand";
const roomRef = (code) => doc(db, "rooms", code);

/** Song für andere: lokale Pfade bleiben privat, Gäste suchen ihn dann bei YouTube Music */
function shareable(track) {
  if (!track) return null;
  const { qid, auto, ytId, path, ...rest } = track;
  return JSON.parse(JSON.stringify(rest));
}
const trackKey = (t) => (t ? `${t.source}:${t.id}` : null);

// --- Gastgeber ------------------------------------------------------------

function publish(extra = {}) {
  if (together.role !== "host") return;
  setDoc(
    roomRef(together.code),
    {
      track: shareable(currentTrack()),
      playing: player.playing,
      position: player.position,
      at: Date.now(),
      updatedAt: serverTimestamp(),
      ...extra,
    },
    { merge: true },
  ).catch(() => {});
}

export async function startRoom() {
  if (!me()) throw new Error("Bitte zuerst im Wavely-Konto anmelden.");
  await leaveRoom();
  together.busy = true;
  try {
    let code = newCode();
    // Sehr unwahrscheinlich, aber sicher ist sicher: Code nicht doppelt vergeben
    for (let i = 0; i < 3 && (await getDoc(roomRef(code))).exists(); i++) code = newCode();
    await setDoc(roomRef(code), {
      host: me().uid,
      hostName: myName(),
      createdAt: serverTimestamp(),
      track: shareable(currentTrack()),
      playing: player.playing,
      position: player.position,
      at: Date.now(),
    });
    Object.assign(together, { role: "host", code, hostName: myName(), members: [], notice: null });

    // Änderungen sofort weitergeben, die Stelle zusätzlich alle 10 Sekunden
    stopHostEffect = $effect.root(() => {
      let lastKey = null;
      let lastPlaying = null;
      let lastPosition = 0;
      $effect(() => {
        const key = currentTrack()?.qid;
        const playing = player.playing;
        const position = player.position;
        const jumped = Math.abs(position - lastPosition) > 3;
        lastPosition = position;
        if (key !== lastKey || playing !== lastPlaying || jumped) {
          lastKey = key;
          lastPlaying = playing;
          publish();
        }
      });
    });
    timers.push(setInterval(() => player.playing && publish(), 10_000));

    unsubscribers.push(
      onSnapshot(collection(db, "rooms", code, "members"), (snap) => {
        const now = Date.now();
        together.members = snap.docs
          .map((d) => ({ uid: d.id, ...d.data() }))
          .filter((m) => now - (m.lastSeen ?? 0) < 90_000);
      }),
    );
  } finally {
    together.busy = false;
  }
}

// --- Gast -----------------------------------------------------------------

let currentKey = null;

async function follow(room) {
  if (!room || room.ended) {
    showToast(`${together.hostName ?? "Der Gastgeber"} hat „Zusammen hören“ beendet`);
    await leaveRoom();
    return;
  }
  const track = room.track;
  if (!track) return;

  // Anderer Song → abspielen (lokale Songs des Gastgebers über YouTube Music)
  if (trackKey(track) !== currentKey) {
    currentKey = trackKey(track);
    together.notice = null;
    const elapsed = room.playing ? (Date.now() - room.at) / 1000 : 0;
    playList([{ ...track, remote: track.source === "local" ? true : undefined }], 0);
    // Stelle nachziehen, sobald der Song läuft
    setTimeout(() => seek(room.position + elapsed + 2), 2000);
    return;
  }

  // Play/Pause übernehmen
  if (room.playing !== player.playing && !player.loading) togglePlay();

  // Stelle abgleichen, wenn sie mehr als 2,5 Sekunden abweicht
  const expected = room.position + (room.playing ? (Date.now() - room.at) / 1000 : 0);
  if (Math.abs(player.position - expected) > 2.5 && !player.loading) seek(expected);
}

export async function joinRoom(input) {
  if (!me()) throw new Error("Bitte zuerst im Wavely-Konto anmelden.");
  const code = String(input).toUpperCase().replace(/[^A-Z0-9]/g, "").slice(-6);
  if (code.length !== 6) throw new Error("Der Code hat 6 Zeichen.");
  await leaveRoom();
  together.busy = true;
  try {
    const snap = await getDoc(roomRef(code));
    if (!snap.exists()) throw new Error("Diesen Raum gibt es nicht (mehr).");
    const room = snap.data();
    if (room.host === me().uid) throw new Error("Das ist dein eigener Raum.");
    Object.assign(together, { role: "guest", code, hostName: room.hostName, members: [], notice: null });
    currentKey = null;
    setAutoplayBlocked(true);

    const memberRef = doc(db, "rooms", code, "members", me().uid);
    const heartbeat = () => setDoc(memberRef, { name: myName(), lastSeen: Date.now() }).catch(() => {});
    await heartbeat();
    timers.push(setInterval(heartbeat, 30_000));

    unsubscribers.push(onSnapshot(roomRef(code), (s) => follow(s.exists() ? s.data() : null)));
    showToast(`Du hörst jetzt mit ${room.hostName} zusammen`);
  } catch (e) {
    together.role = null;
    together.code = null;
    throw e;
  } finally {
    together.busy = false;
  }
}

// --- Beenden --------------------------------------------------------------

export async function leaveRoom() {
  const { role, code } = together;
  unsubscribers.forEach((off) => off());
  unsubscribers = [];
  timers.forEach(clearInterval);
  timers = [];
  stopHostEffect?.();
  stopHostEffect = null;
  setAutoplayBlocked(false);
  Object.assign(together, { role: null, code: null, hostName: null, members: [], notice: null });
  if (!code || !me()) return;
  try {
    if (role === "host") {
      // Gäste informieren, dann aufräumen
      await setDoc(roomRef(code), { ended: true }, { merge: true });
      const members = await getDocs(collection(db, "rooms", code, "members"));
      const batch = writeBatch(db);
      members.docs.forEach((d) => batch.delete(d.ref));
      await batch.commit();
      await deleteDoc(roomRef(code));
    } else if (role === "guest") {
      await deleteDoc(doc(db, "rooms", code, "members", me().uid));
    }
  } catch {}
}

/** Einladungslink „wavely://join/CODE“ aus dem Start-Argument oder einem zweiten Aufruf */
export function codeFromLink(url) {
  const match = String(url).match(/^wavely:\/\/join\/([A-Za-z0-9]{6})/);
  return match ? match[1].toUpperCase() : null;
}

window.addEventListener("pagehide", () => {
  if (together.role) leaveRoom();
});
