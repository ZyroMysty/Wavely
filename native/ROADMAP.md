# Wavely nativ – Stand

Ziel: Wavely ohne WebView2 als reine Rust-App (Branch `native`).
Gemessen (Release-Build, Startseite + laufender Song + Liedtext): **rund 40 MB** statt 160 MB.

## Technik

- **Oberfläche:** [Slint](https://slint.dev) mit Software-Renderer (`ui/*.slint`), Seiten aus Zeilen (`src/pages.rs`).
- **Audio:** Symphonia (AAC, MP3, FLAC, Vorbis, Opus) → Equalizer/Angleichung (`audio/dsp.rs`) → cpal (WASAPI).
- **Dienste:** die Rust-Module der bisherigen Wavely ohne Tauri (`src/services/`).
- **Wavely-Konto:** Firebase über REST (`src/account.rs`), Abgleich alle 60 Sekunden.
- **Browser nur bei Bedarf** (`win/web.rs`, WebView2 über wry): Anmeldung Google/SoundCloud, Spotify-Premium-Wiedergabe.
- **Daten:** eigener Ordner `com.wavely.native`; Anmeldungen, Playlists, Hörstatistik und Offline-Verzeichnis
  werden beim ersten Start aus `com.wavely.app` übernommen.
- **Installer/Updates:** `node scripts/build-native.mjs` (NSIS, signiert mit dem Wavely-Schlüssel,
  `updates/latest-native.json`).

## Erledigt

- [x] Fenster, Design (dunkel/hell/System, Akzentfarbe), Seitenleiste mit Playlists, Player-Leiste
- [x] Audio-Engine: lokale Dateien, YouTube-/SoundCloud-Streams, Offline-Dateien, Spulen, Lautstärke
- [x] YouTube Music: Suche, Bibliothek, Playlists/Alben, Künstler, Likes, Radio/Autoplay, YouTube-Playlists, Verlauf melden
- [x] Warteschlange, Verlauf, Fortsetzen nach Neustart, Zufall/Wiederholen, Kontextmenü
- [x] Startseite, Lokale Musik (Ordner, Cover, Beobachten), Offline-Downloads
- [x] Wavely-Konto (Anmeldung, Registrierung, Abgleich von Playlists und Hören), Wavely-Playlists
- [x] SoundCloud, Spotify (Anmeldung im Browser, Zuordnung zu YouTube Music, Premium-Wiedergabe)
- [x] Einstellungen, Equalizer, Lautstärke angleichen, Tempo, Ausgabegerät, Schlaf-Timer
- [x] Liedtexte, Rückblick, Last.fm, Discord, Zusammen hören, Feedback
- [x] Medientasten, Taskleisten-Buttons, Mini-Player, Tastenkürzel (auch systemweit), Autostart, nur eine Instanz
- [x] Updater, Installer, Diagnose für Admins

## Noch offen

- [ ] Playlists zu anderen Diensten übertragen („Übertragen“)
- [ ] Neuigkeiten-Dialog nach Updates
- [ ] Tonhöhe beim Tempo beibehalten (zurzeit ändert sich die Tonhöhe mit)
- [ ] Einladungslinks „wavely://“ dieser Version zuordnen (bewusst nicht, solange die normale Wavely sie nutzt)
