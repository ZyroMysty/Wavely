# Wavely

Leichtgewichtiger Desktop-Musikplayer mit eigenem Design – für lokale Musik und (bald) YouTube Music, mit Discord Rich Presence.

Gebaut mit [Tauri 2](https://tauri.app) (Rust) und [Svelte 5](https://svelte.dev).

## Funktionen

- Lokale Musik: Ordner hinzufügen, MP3/FLAC/M4A/OGG/OPUS/WAV werden mit Titel, Künstler, Album und Cover eingelesen
- Eigener Player: Play/Pause, Vor/Zurück, Zufall, Wiederholen, Suchleiste, Lautstärke, Leertaste, Windows-Medientasten
- Discord Rich Presence: „Hört …“ mit Titel, Künstler und Fortschritt
- Geplant: YouTube Music mit Konto (Playlists, Likes, Suche), später Spotify

## Aufbau

| Pfad | Zweck |
| --- | --- |
| `src/app.css` | Design-Tokens (Farben, Abstände) – hier das Aussehen anpassen |
| `src/components/` | Oberfläche: Seitenleiste, Bibliothek, Player-Leiste |
| `src/lib/player.svelte.js` | Wiedergabe, Warteschlange, Discord-Updates, Mediensteuerung |
| `src/lib/library.svelte.js` | Lokale Bibliothek laden, Ordner verwalten |
| `src-tauri/src/library.rs` | Ordner durchsuchen, Tags und Cover lesen |
| `src-tauri/src/presence.rs` | Verbindung zu Discord |

## Einrichtung

Voraussetzungen: Rust, Node.js und Visual Studio Build Tools mit „Desktopentwicklung mit C++“.

Für Discord: Auf https://discord.com/developers/applications eine Application anlegen und die **Application ID** in `src-tauri/src/presence.rs` bei `DISCORD_CLIENT_ID` eintragen.

```bash
npm install
npm run dev
```

Release-Build (Installer landet in `src-tauri/target/release/bundle/nsis/`):

```bash
npm run build
```
