//! Quellen für die Audio-Engine: lokale Dateien direkt, Streams stückweise über HTTP.

use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::Arc,
};

use symphonia::core::io::MediaSource;

/// Lädt ein Stück einer entfernten Datei: (Start, gewünschte Länge) → (Daten, Gesamtgröße)
pub type FetchChunk = Arc<dyn Fn(u64, u64) -> Result<(Vec<u8>, u64), String> + Send + Sync>;

/// Größe der Stücke, die auf einmal geladen werden
const CHUNK: u64 = 512 * 1024;

/// Entfernte Datei, die beim Lesen stückweise nachgeladen wird (wie der Browser-Player mit Range-Anfragen).
/// Das zuletzt geladene Stück bleibt im Speicher; der Rest liegt nicht im RAM.
pub struct HttpSource {
    fetch: FetchChunk,
    len: u64,
    pos: u64,
    chunk_start: u64,
    chunk: Vec<u8>,
}

impl HttpSource {
    /// Holt gleich das erste Stück (dabei kommt auch die Gesamtgröße mit).
    pub fn open(fetch: FetchChunk) -> Result<Self, String> {
        let (chunk, len) = fetch(0, CHUNK)?;
        Ok(Self { fetch, len, pos: 0, chunk_start: 0, chunk })
    }
}

impl Read for HttpSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.len || buf.is_empty() {
            return Ok(0);
        }
        let in_chunk = self.pos >= self.chunk_start && self.pos < self.chunk_start + self.chunk.len() as u64;
        if !in_chunk {
            let (data, len) = (self.fetch)(self.pos, CHUNK).map_err(io::Error::other)?;
            if data.is_empty() {
                return Ok(0);
            }
            self.len = len.max(self.len);
            self.chunk_start = self.pos;
            self.chunk = data;
        }
        let offset = (self.pos - self.chunk_start) as usize;
        let n = buf.len().min(self.chunk.len() - offset);
        buf[..n].copy_from_slice(&self.chunk[offset..offset + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for HttpSource {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(p) => p as i64,
            SeekFrom::End(d) => self.len as i64 + d,
            SeekFrom::Current(d) => self.pos as i64 + d,
        };
        if target < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Position vor dem Anfang"));
        }
        self.pos = target as u64;
        Ok(self.pos)
    }
}

impl MediaSource for HttpSource {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.len)
    }
}
