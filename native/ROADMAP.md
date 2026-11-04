# Wavely nativ – Fahrplan

Ziel: Wavely ohne WebView2 als reine Rust-App (Branch `native`). Erwartet: rund 30–50 MB statt 160 MB.

## Technik

- **Oberfläche:** [Slint](https://slint.dev) mit Software-Renderer (zeichnet auf dem Prozessor, kein Grafiktreiber-Speicher).
  Die Oberfläche steht in `ui/*.slint`, die Logik in `src/`.
- **Audio:** Symphonia (Entpacken/Dekodieren: AAC, MP3, FLAC, Vorbis, Opus) → eigener Signalweg
  (Lautstärke, Equalizer, Überblenden) → cpal (WASAPI).
- **Dienste:** die bisherigen Rust-Module aus `src-tauri/src` (YouTube Music, SoundCloud, Spotify, Last.fm,
  Liedtexte, Offline, yt-dlp …), ohne Tauri – `src/host.rs` ersetzt AppHandle (Ordner, Ereignisse).
- **Wavely-Konto:** Firebase über REST (Identity Toolkit + Firestore) statt Firebase-JS.
- **Spotify-Wiedergabe:** kleines, unsichtbares WebView2-Fenster nur solange Spotify spielt.
- **Daten:** eigener Ordner `com.wavely.native`; beim ersten Start werden Anmeldungen aus `com.wavely.app` übernommen.

## Schritte

1. [ ] Grundgerüst: Fenster, Design (dunkel/hell, Akzentfarbe), Seitenleiste, Player-Leiste
2. [ ] Audio-Engine: lokale Dateien + YouTube-Streams, Position, Lautstärke, Ende → nächster Song
3. [ ] YouTube Music: Suche, Bibliothek, Playlists/Alben, Künstler, Likes, Radio/Autoplay
4. [ ] Warteschlange, Verlauf, Fortsetzen nach Neustart, Zufall/Wiederholen
5. [ ] Startseite, Lokale Musik (Ordner, Cover), Offline-Downloads
6. [ ] Wavely-Konto (Anmeldung, Sync von Likes/Playlists/Verlauf/Einstellungen), Wavely-Playlists
7. [ ] SoundCloud, Spotify (Bibliothek, Zuordnung, Premium-Wiedergabe im WebView)
8. [ ] Einstellungen komplett, Klang (Equalizer, Normalisierung, Überblenden, Tempo), Schlaf-Timer
9. [ ] Liedtexte, Rückblick/Statistik, Last.fm, Discord, Zusammen hören
10. [ ] Windows: Mediensteuerung, Taskleisten-Buttons, Mini-Player, Tastenkürzel, Autostart, Einladungslinks
11. [ ] Updater, Feedback, Diagnose, Neuigkeiten-Dialog, Installer
