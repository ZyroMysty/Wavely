import { convertFileSrc } from "@tauri-apps/api/core";

/** Bild-URL für die Anzeige: YouTube-Bilder laufen über den Bild-Cache im Rust-Teil. */
export function artworkSrc(url) {
  if (!url || !url.startsWith("https://")) return url;
  return convertFileSrc(url, "artwork");
}

/**
 * Größere Version eines YouTube-Music-Covers (z. B. für den großen Mini-Player).
 * Bilder von googleusercontent tragen die Größe in der URL („=w120-h120-…“).
 */
export function hiResArtwork(url) {
  if (!url) return url;
  return url.replace(/=w\d+-h\d+/, "=w544-h544").replace(/=s\d+(?=$|-)/, "=s544");
}

/**
 * Alte YouTube-Vorschaubilder (hqdefault/sddefault/mqdefault) sind 4:3 mit eingebrannten
 * schwarzen Balken oben und unten. Im quadratischen Ausschnitt müssen sie stärker
 * vergrößert werden, damit die Balken verschwinden.
 */
export function isLetterboxed(url) {
  // Funktioniert für Original-URLs und für die kodierten artwork://-Adressen
  return /ytimg\.com.*(hq|mq|sd)default/.test(url ?? "");
}
