# Wavely

Leichtgewichtiger Desktop-Musikplayer mit eigenem Design – für lokale Musik und YouTube Music, mit Discord Rich Presence.

Gebaut mit [Tauri 2](https://tauri.app) (Rust) und [Svelte 5](https://svelte.dev).

## Funktionen

- Lokale Musik: Ordner hinzufügen, MP3/FLAC/M4A/OGG/OPUS/WAV werden mit Titel, Künstler, Album und Cover eingelesen
- Eigener Player: Play/Pause, Vor/Zurück, Zufall, Wiederholen, Suchleiste, Lautstärke, Leertaste, Windows-Medientasten
- Discord Rich Presence: „Hört …“ mit Titel, Künstler und Fortschritt
- YouTube Music: Anmeldung mit Google, deine Playlists, Alben und Lieblingssongs, Suche – abgespielt im eigenen Player
- YouTube Music bearbeiten: Songs liken, neue Playlists erstellen, Songs zu eigenen Playlists hinzufügen
- Profilbild mit Kontomenü, einklappbare Seitenleiste, Einstellungsseite (hell/dunkel, Akzentfarbe, Discord, Offline-Speicher)
- Mini-Player: frei skalierbar (kompakte Leiste bis großer Player mit Cover), optional immer im Vordergrund
- Verlauf: in Wavely gespielte Songs und YouTube-Music-Verlauf
- Buttons in der Taskleisten-Vorschau (Zurück, Play/Pause, Weiter, Gefällt mir)
- Warteschlange, Autoplay mit passenden Songs, synchronisierte Liedtexte (LRCLIB)
- Offline-Modus: YouTube-Songs und Playlists herunterladen und ohne Internet abspielen
- Geplant: Spotify

## Aufbau

| Pfad | Zweck |
| --- | --- |
| `src/app.css` | Design-Tokens (Farben, Abstände) – hier das Aussehen anpassen |
| `src/components/` | Oberfläche: Seitenleiste, Bibliothek, Player-Leiste |
| `src/lib/player.svelte.js` | Wiedergabe, Warteschlange, Discord-Updates, Mediensteuerung |
| `src/lib/library.svelte.js` | Lokale Bibliothek laden, Ordner verwalten |
| `src-tauri/src/library.rs` | Ordner durchsuchen, Tags und Cover lesen |
| `src-tauri/src/presence.rs` | Verbindung zu Discord |
| `src-tauri/src/ytm.rs` | YouTube Music: Anmeldung, Bibliothek, Suche (RustyPipe), Stream-Dienst für den Player |
| `src/lib/ytm.svelte.js` | YouTube-Music-Zustand in der Oberfläche |
| `src-tauri/src/ytdlp.rs` | yt-dlp: Audio-Stream-Adressen für YouTube, tägliches Selbst-Update |
| `src-tauri/src/taskbar.rs` | Windows-Taskleisten-Vorschau-Buttons (ITaskbarList3) |
| `src/components/MiniPlayer.svelte`, `src/lib/bridge.svelte.js` | Mini-Player und Verbindung zu Taskleiste/Mini-Player |
| `src-tauri/src/artwork.rs` | Cover von YouTube laden und zwischenspeichern |
| `src-tauri/vendor/rustypipe` | Gepatchte Kopie von RustyPipe (siehe `vendor/README.md`) |

## Einrichtung

Voraussetzungen: Rust, Node.js und Visual Studio Build Tools mit „Desktopentwicklung mit C++“.

Für Discord: Auf https://discord.com/developers/applications eine Application anlegen und die **Application ID** in `src-tauri/src/presence.rs` bei `DISCORD_CLIENT_ID` eintragen.

```bash
npm install   # lädt auch yt-dlp (mit Prüfsumme) nach src-tauri/binaries
npm run dev
```

Release-Build (Installer landet in `src-tauri/target/release/bundle/nsis/`):

```bash
npm run build
```

## Lizenz-Hinweis

Wavely nutzt [RustyPipe](https://codeberg.org/ThetaDev/rustypipe) (GPL-3.0). Wird Wavely weitergegeben, muss es daher ebenfalls unter GPL-3.0 stehen und der Quellcode verfügbar sein. yt-dlp steht unter der Unlicense.
