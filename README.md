# ytmusic-desktop

Leichtgewichtige Desktop-Anwendung zum Abspielen von YouTube Music – mit Discord Rich Presence.

Gebaut mit [Tauri 2](https://tauri.app) (Rust + WebView2), dadurch klein und sparsam im Vergleich zu Electron.

## Funktionen

- YouTube Music in einem eigenen, schlanken Fenster (Login bleibt gespeichert)
- Discord Rich Presence: „Hört …“ mit Titel, Künstler, Album-Cover, Fortschrittsbalken und Button zum Song

## Aufbau

| Datei | Zweck |
| --- | --- |
| `src-tauri/src/lib.rs` | Erstellt das Fenster mit music.youtube.com und registriert die Commands |
| `src-tauri/src/inject.js` | Wird in die Seite injiziert, liest den aktuellen Song aus und meldet Änderungen |
| `src-tauri/src/presence.rs` | Verbindung zu Discord und Aufbau der Rich Presence |
| `src-tauri/capabilities/default.json` | Erlaubt nur music.youtube.com, die Presence-Commands aufzurufen |

## Einrichtung

### 1. Voraussetzungen (Windows)

- [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) mit „Desktopentwicklung mit C++“
- [Rust](https://rustup.rs) (`rustup-init.exe`)
- Node.js (für die Tauri-CLI)
- WebView2 ist bei Windows 11 schon dabei

### 2. Discord-App anlegen

1. Auf https://discord.com/developers/applications eine neue Application erstellen (der Name erscheint im Status, z. B. „YouTube Music“).
2. Die **Application ID** kopieren und in `src-tauri/src/presence.rs` bei `DISCORD_CLIENT_ID` eintragen.

### 3. Starten

```bash
npm install
npm run dev
```

Release-Build (Installer landet in `src-tauri/target/release/bundle/nsis/`):

```bash
npm run build
```
