import {
  onAuthStateChanged,
  createUserWithEmailAndPassword,
  signInWithEmailAndPassword,
  signOut,
  sendPasswordResetEmail,
  updateProfile,
  deleteUser,
  reauthenticateWithCredential,
  EmailAuthProvider,
} from "firebase/auth";
import { auth } from "./firebase.js";
import { startSync, stopSync, deleteCloudData } from "./sync.svelte.js";
import { leaveRoom } from "./together.svelte.js";
import { showToast } from "./ui.svelte.js";

/** Wavely-Konto (Firebase Authentication mit E-Mail und Passwort) */
export const account = $state({
  /** "checking" | "loggedOut" | "loggedIn" */
  status: "checking",
  /** { uid, email, name } */
  user: null,
});

export function initAccount() {
  onAuthStateChanged(auth, (user) => {
    if (user) {
      account.user = { uid: user.uid, email: user.email, name: user.displayName || user.email?.split("@")[0] };
      account.status = "loggedIn";
      startSync(user.uid);
    } else {
      account.user = null;
      account.status = "loggedOut";
      stopSync();
    }
  });
}

/** Firebase-Fehlercodes in verständliche Sätze */
export function errorText(e) {
  const code = e?.code ?? "";
  return (
    {
      "auth/invalid-email": "Diese E-Mail-Adresse ist ungültig.",
      "auth/missing-password": "Bitte ein Passwort eingeben.",
      "auth/weak-password": "Das Passwort muss mindestens 6 Zeichen haben.",
      "auth/email-already-in-use": "Mit dieser E-Mail gibt es schon ein Konto – melde dich an.",
      "auth/invalid-credential": "E-Mail oder Passwort stimmt nicht.",
      "auth/wrong-password": "Das Passwort stimmt nicht.",
      "auth/user-not-found": "Zu dieser E-Mail gibt es kein Konto.",
      "auth/too-many-requests": "Zu viele Versuche – bitte kurz warten.",
      "auth/network-request-failed": "Keine Verbindung – bist du online?",
      "auth/requires-recent-login": "Bitte melde dich zur Sicherheit noch einmal an.",
    }[code] ?? (e?.message || String(e))
  );
}

export async function register(name, email, password) {
  const { user } = await createUserWithEmailAndPassword(auth, email.trim(), password);
  if (name.trim()) {
    await updateProfile(user, { displayName: name.trim() });
    account.user = { ...account.user, name: name.trim() };
  }
}

export async function login(email, password) {
  await signInWithEmailAndPassword(auth, email.trim(), password);
}

export async function logout() {
  await leaveRoom();
  await signOut(auth);
}

export async function resetPassword(email) {
  await sendPasswordResetEmail(auth, email.trim());
}

export async function rename(name) {
  if (!auth.currentUser || !name.trim()) return;
  await updateProfile(auth.currentUser, { displayName: name.trim() });
  account.user = { ...account.user, name: name.trim() };
}

/** Konto und alle Daten in der Cloud löschen (lokale Daten auf diesem PC bleiben) */
export async function deleteAccount(password) {
  const user = auth.currentUser;
  if (!user) return;
  await reauthenticateWithCredential(user, EmailAuthProvider.credential(user.email, password));
  await leaveRoom();
  await deleteCloudData(user.uid);
  await deleteUser(user);
  showToast("Konto und Cloud-Daten gelöscht");
}
