# Lokale Kopien von Abhängigkeiten

## rustypipe 0.11.4 (GPL-3.0)

Kopie von https://crates.io/crates/rustypipe/0.11.4, eingebunden über `[patch.crates-io]` in `Cargo.toml`.

Änderungen gegenüber dem Original:

- `src/client/response/music_playlist.rs`: `Header` versteht zusätzlich
  `musicEditablePlaylistDetailHeaderRenderer`. Ohne das werden eigene Playlists
  als Album erkannt und schlagen mit „album playlist containing items from different albums“ fehl.
