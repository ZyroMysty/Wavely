import { addDoc, collection, serverTimestamp } from "firebase/firestore";
import { getVersion } from "@tauri-apps/api/app";
import { db, auth } from "./firebase.js";

/**
 * Feedback aus Wavely: landet in Firestore unter „feedback“ (nur schreiben, lesen kann es nur
 * die Projekt-Inhaberin in der Firebase-Konsole). Ein Wavely-Konto ist nicht nötig.
 */

export const feedback = $state({ open: false });

export const KINDS = [
  { id: "bug", label: "Fehler melden" },
  { id: "idea", label: "Idee / Wunsch" },
  { id: "other", label: "Sonstiges" },
];

/** Schickt das Feedback; ohne Internet wird es nachgereicht (Rückgabe: "sent" | "queued") */
export async function sendFeedback({ kind, text, contact, withInfo }) {
  const entry = {
    kind,
    text: text.trim().slice(0, 5000),
    contact: contact.trim().slice(0, 200) || null,
    createdAt: serverTimestamp(),
    uid: auth.currentUser?.uid ?? null,
  };
  if (withInfo) {
    entry.version = await getVersion().catch(() => null);
    entry.system = navigator.userAgent.match(/\(([^)]+)\)/)?.[1] ?? null;
  }
  const write = addDoc(collection(db, "feedback"), entry);
  // Firestore bestätigt erst, wenn der Server es hat – ohne Internet nicht warten
  const timeout = new Promise((resolve) => setTimeout(() => resolve("queued"), 6000));
  return Promise.race([write.then(() => "sent"), timeout]);
}
