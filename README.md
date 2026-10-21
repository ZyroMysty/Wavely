# Wavely

Leichtgewichtiger Desktop-Musikplayer mit eigenem Design â€“ fÃ¼r lokale Musik und YouTube Music, mit Discord Rich Presence.

Gebaut mit [Tauri 2](https://tauri.app) (Rust) und [Svelte 5](https://svelte.dev).

## Funktionen

- Lokale Musik: Ordner hinzufÃ¼gen, MP3/FLAC/M4A/OGG/OPUS/WAV werden mit Titel, KÃ¼nstler, Album und Cover eingelesen
- Eigener Player: Play/Pause, Vor/ZurÃ¼ck, Zufall, Wiederholen, Suchleiste, LautstÃ¤rke, Leertaste, Windows-Medientasten
- Discord Rich Presence: â€žHÃ¶rt â€¦â€œ mit Titel, KÃ¼nstler und Fortschritt
- YouTube Music: Anmeldung mit Google, deine Playlists, Alben und Lieblingssongs, Suche â€“ abgespielt im eigenen Player
- Künstlerseiten (beliebte Songs, Alben, Singles, ähnliche Künstler); Alben und Künstler sind überall anklickbar
- YouTube Music bearbeiten: Songs liken, neue Playlists erstellen, Songs zu eigenen Playlists hinzufÃ¼gen
- Profilbild mit KontomenÃ¼, einklappbare Seitenleiste, Einstellungsseite (hell/dunkel, Akzentfarbe, Discord, Offline-Speicher)
- Mini-Player: frei skalierbar (kompakte Leiste bis groÃŸer Player mit Cover), optional immer im Vordergrund
- Verlauf: in Wavely gespielte Songs und YouTube-Music-Verlauf; gehÃ¶rte Songs werden (abschaltbar) an den YouTube-Verlauf gemeldet
- Buttons in der Taskleisten-Vorschau (ZurÃ¼ck, Play/Pause, Weiter, GefÃ¤llt mir)
- Warteschlange, Autoplay mit passenden Songs, synchronisierte Liedtexte (LRCLIB)
- Offline-Modus: YouTube-Songs und Playlists herunterladen und ohne Internet abspielen
- Geplant: Spotify

## Aufbau

| Pfad | Zweck |
| --- | --- |
| `src/app.css` | Design-Tokens (Farben, AbstÃ¤nde) â€“ hier das Aussehen anpassen |
| `src/components/` | OberflÃ¤che: Seitenleiste, Bibliothek, Player-Leiste |
| `src/lib/player.svelte.js` | Wiedergabe, Warteschlange, Discord-Updates, Mediensteuerung |
| `src/lib/library.svelte.js` | Lokale Bibliothek laden, Ordner verwalten |
| `src-tauri/src/library.rs` | Ordner durchsuchen, Tags und Cover lesen |
| `src-tauri/src/presence.rs` | Verbindung zu Discord |
| `src-tauri/src/ytm.rs` | YouTube Music: Anmeldung, Bibliothek, Suche (RustyPipe), Stream-Dienst fÃ¼r den Player |
| `src/lib/ytm.svelte.js` | YouTube-Music-Zustand in der OberflÃ¤che |
| `src-tauri/src/ytdlp.rs` | yt-dlp: Audio-Stream-Adressen fÃ¼r YouTube, tÃ¤gliches Selbst-Update |
| `src-tauri/src/taskbar.rs` | Windows-Taskleisten-Vorschau-Buttons (ITaskbarList3) |
| `src/components/MiniPlayer.svelte`, `src/lib/bridge.svelte.js` | Mini-Player und Verbindung zu Taskleiste/Mini-Player |
| `src-tauri/src/artwork.rs` | Cover von YouTube laden und zwischenspeichern |
| `src-tauri/vendor/rustypipe` | Gepatchte Kopie von RustyPipe (siehe `vendor/README.md`) |

## Einrichtung

Voraussetzungen: Rust, Node.js und Visual Studio Build Tools mit â€žDesktopentwicklung mit C++â€œ.

FÃ¼r Discord: Auf https://discord.com/developers/applications eine Application anlegen und die **Application ID** in `src-tauri/src/presence.rs` bei `DISCORD_CLIENT_ID` eintragen.

```bash
npm install   # lÃ¤dt auch yt-dlp (mit PrÃ¼fsumme) nach src-tauri/binaries
npm run dev
```

Release-Build (Installer landet in `src-tauri/target/release/bundle/nsis/`):

```bash
npm run build
```

## Lizenz-Hinweis

Wavely nutzt [RustyPipe](https://codeberg.org/ThetaDev/rustypipe) (GPL-3.0). Wird Wavely weitergegeben, muss es daher ebenfalls unter GPL-3.0 stehen und der Quellcode verfÃ¼gbar sein. yt-dlp steht unter der Unlicense.
