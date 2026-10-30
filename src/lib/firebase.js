import { initializeApp } from "firebase/app";
import { initializeAuth, indexedDBLocalPersistence } from "firebase/auth";
import { initializeFirestore, persistentLocalCache, persistentSingleTabManager } from "firebase/firestore";

/**
 * Verbindung zum Wavely-Firebase-Projekt (Konto, Abgleich zwischen Geräten, Zusammen hören).
 * Diese Werte sind öffentlich gedacht – geschützt werden die Daten durch die Firestore-Regeln
 * (firebase/firestore.rules). Google Analytics wird bewusst nicht geladen.
 */
const config = {
  apiKey: "AIzaSyD1Tu2lqe2vUPmUtWIUttDUfmXwwtlACEg",
  authDomain: "wavely-cf217.firebaseapp.com",
  projectId: "wavely-cf217",
  storageBucket: "wavely-cf217.firebasestorage.app",
  messagingSenderId: "770088113316",
  appId: "1:770088113316:web:dbc95b703645efe5e88231",
};

export const app = initializeApp(config);

// Anmeldung bleibt im Browser-Speicher von Wavely erhalten
export const auth = initializeAuth(app, { persistence: indexedDBLocalPersistence });
auth.languageCode = "de";

// Offline-Zwischenspeicher: Änderungen ohne Internet werden später automatisch hochgeladen
export const db = initializeFirestore(app, {
  localCache: persistentLocalCache({ tabManager: persistentSingleTabManager() }),
});
