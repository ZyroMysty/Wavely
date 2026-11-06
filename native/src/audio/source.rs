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
    /// Nächstes Stück, das schon im Hintergrund lädt: (Start, Ergebnis)
    next: Option<(u64, std::sync::Mutex<std::sync::mpsc::Receiver<Result<(Vec<u8>, u64), String>>>)>,
}

impl HttpSource {
    /// Holt gleich das erste Stück (dabei kommt auch die Gesamtgröße mit).
    pub fn open(fetch: FetchChunk) -> Result<Self, String> {
        let (chunk, len) = fetch(0, CHUNK)?;
        Ok(Self { fetch, len, pos: 0, chunk_start: 0, chunk, next: None })
    }

    /// Nächstes Stück vorab laden, damit die Wiedergabe beim Wechsel nicht auf das Netz warten muss
    fn prefetch(&mut self) {
        let end = self.chunk_start + self.chunk.len() as u64;
        if self.next.is_some() || end >= self.len {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let fetch = self.fetch.clone();
        std::thread::spawn(move || {
            let _ = tx.send(fetch(end, CHUNK));
        });
        self.next = Some((end, std::sync::Mutex::new(rx)));
    }
}

impl Read for HttpSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.len || buf.is_empty() {
            return Ok(0);
        }
        let in_chunk = self.pos >= self.chunk_start && self.pos < self.chunk_start + self.chunk.len() as u64;
        if !in_chunk {
            // Vorab geladenes Stück passt → nehmen, sonst jetzt laden
            let ready = match self.next.take() {
                Some((start, rx)) if start == self.pos => rx.into_inner().ok().and_then(|rx| rx.recv().ok()).and_then(|r| r.ok()),
                _ => None,
            };
            let (data, len) = match ready {
                Some(result) => result,
                None => (self.fetch)(self.pos, CHUNK).map_err(io::Error::other)?,
            };
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
        // Ab der Hälfte des Stücks das nächste holen
        if self.pos >= self.chunk_start + self.chunk.len() as u64 / 2 {
            self.prefetch();
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Beim Lesen über viele Stücke (mit Vorladen und Springen) kommen genau die richtigen Bytes
    #[test]
    fn reads_across_chunks() {
        let data: Arc<Vec<u8>> = Arc::new((0..3_000_000u32).map(|i| (i % 251) as u8).collect());
        let d = data.clone();
        let fetch: FetchChunk = Arc::new(move |start, len| {
            std::thread::sleep(std::time::Duration::from_millis(5));
            let end = (start + len).min(d.len() as u64) as usize;
            Ok((d[start as usize..end].to_vec(), d.len() as u64))
        });
        let mut source = HttpSource::open(fetch).unwrap();
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = source.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        assert_eq!(out.len(), data.len());
        assert!(out == *data);
        // Springen mitten in ein anderes Stück
        source.seek(SeekFrom::Start(1_234_567)).unwrap();
        let n = source.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], &data[1_234_567..1_234_567 + n]);
    }
}
