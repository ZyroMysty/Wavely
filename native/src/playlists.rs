//! Wavely-Playlists: Songs aus allen Quellen gemischt (gleiches Format wie playlists.svelte.js,
//! gespeichert als playlists.json). Jede Playlist und jeder Eintrag hat eine eindeutige ID.

use serde::{Deserialize, Serialize};

use crate::{player::Track, store::now_ms};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Item {
    pub id: String,
    pub added_at: u64,
    pub track: Track,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct File {
    version: u32,
    playlists: Vec<Playlist>,
}

pub fn new_id() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let bytes: [u8; 16] = rng.random();
    // UUID v4 wie crypto.randomUUID()
    let hex: String = bytes
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let b = match i {
                6 => (b & 0x0f) | 0x40,
                8 => (b & 0x3f) | 0x80,
                _ => *b,
            };
            format!("{b:02x}")
        })
        .collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

#[derive(Default)]
pub struct Playlists {
    pub list: Vec<Playlist>,
    /// Gelöschte IDs seit dem letzten Abgleich (für die Cloud)
    pub deleted: Vec<String>,
}

impl Playlists {
    pub fn load(store: &crate::store::Store) -> Self {
        let file: File = serde_json::from_value(store.playlists_raw()).unwrap_or_default();
        Self { list: file.playlists, deleted: Vec::new() }
    }

    pub fn save(&self, store: &crate::store::Store) {
        let file = File { version: 1, playlists: self.list.clone() };
        if let Ok(value) = serde_json::to_value(&file) {
            store.save_playlists_raw(&value);
        }
    }

    pub fn get(&self, id: &str) -> Option<&Playlist> {
        self.list.iter().find(|p| p.id == id)
    }

    pub fn create(&mut self, name: &str, tracks: Vec<Track>) -> String {
        let now = now_ms();
        let name = if name.trim().is_empty() { "Neue Playlist".to_string() } else { name.trim().to_string() };
        let playlist = Playlist {
            id: new_id(),
            name,
            description: String::new(),
            created_at: now,
            updated_at: now,
            items: tracks.into_iter().map(|track| Item { id: new_id(), added_at: now, track: slim(track) }).collect(),
        };
        let id = playlist.id.clone();
        self.list.insert(0, playlist);
        id
    }

    pub fn rename(&mut self, id: &str, name: &str) {
        if let Some(p) = self.list.iter_mut().find(|p| p.id == id) {
            if !name.trim().is_empty() {
                p.name = name.trim().to_string();
                p.updated_at = now_ms();
            }
        }
    }

    pub fn delete(&mut self, id: &str) {
        self.list.retain(|p| p.id != id);
        self.deleted.push(id.to_string());
    }

    /// Songs anhängen; gibt die Anzahl der neuen zurück (Doppelte werden übersprungen)
    pub fn add(&mut self, id: &str, tracks: Vec<Track>) -> usize {
        let Some(p) = self.list.iter_mut().find(|p| p.id == id) else { return 0 };
        let mut existing: std::collections::HashSet<String> =
            p.items.iter().map(|i| format!("{}:{}", i.track.source, i.track.id)).collect();
        let now = now_ms();
        let mut added = 0;
        for track in tracks {
            if existing.insert(format!("{}:{}", track.source, track.id)) {
                p.items.push(Item { id: new_id(), added_at: now, track: slim(track) });
                added += 1;
            }
        }
        if added > 0 {
            p.updated_at = now;
        }
        added
    }

    pub fn remove_item(&mut self, id: &str, index: usize) {
        if let Some(p) = self.list.iter_mut().find(|p| p.id == id) {
            if index < p.items.len() {
                p.items.remove(index);
                p.updated_at = now_ms();
            }
        }
    }

    /// Stand aus der Cloud übernehmen: neuere Fassungen ersetzen, gelöschte entfernen.
    /// Gibt zurück, ob sich etwas geändert hat.
    pub fn apply_remote(&mut self, remote: Vec<(Playlist, bool)>) -> bool {
        let mut changed = false;
        for (r, deleted) in remote {
            let index = self.list.iter().position(|p| p.id == r.id);
            if deleted {
                if let Some(i) = index {
                    if self.list[i].updated_at <= r.updated_at {
                        self.list.remove(i);
                        changed = true;
                    }
                }
                continue;
            }
            if let Some(i) = index {
                if self.list[i].updated_at >= r.updated_at {
                    continue;
                }
                // Lokale Dateien: Pfad vom eigenen PC behalten
                let mut r = r;
                for item in r.items.iter_mut() {
                    if let Some(own) = self.list[i].items.iter().find(|o| o.id == item.id) {
                        if own.track.path.is_some() {
                            item.track.path = own.track.path.clone();
                        }
                    }
                }
                self.list[i] = r;
            } else {
                self.list.push(r);
            }
            changed = true;
        }
        if changed {
            self.list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        }
        changed
    }
}

/// Nur die Felder speichern, die zum Abspielen und Anzeigen nötig sind
fn slim(track: Track) -> Track {
    track
}
