//! Cover-Bilder: im Hintergrund laden, auf Anzeigegröße verkleinern, als Slint-Bild übergeben.
//! Im Speicher bleiben nur die verkleinerten Pixel (z. B. 80×80 = 25 KB pro Cover).

use std::{
    collections::{HashMap, VecDeque},
    rc::Rc,
    sync::{Arc, Mutex},
};

use slint::{Image, Model, Rgba8Pixel, SharedPixelBuffer, VecModel};

use crate::{App, TrackRow};

/// Höchstens so viele verkleinerte Cover im Speicher
const MAX_CACHED: usize = 400;
/// Größe der Cover in Listen (Pixel, reicht für 200 % Skalierung)
pub const LIST_SIZE: u32 = 80;

pub struct Covers {
    cache: Mutex<(HashMap<(String, u32), SharedPixelBuffer<Rgba8Pixel>>, VecDeque<(String, u32)>)>,
    http: reqwest::Client,
    /// Nicht zu viele Downloads gleichzeitig
    limit: Arc<tokio::sync::Semaphore>,
}

impl Default for Covers {
    fn default() -> Self {
        Self { cache: Default::default(), http: reqwest::Client::new(), limit: Arc::new(tokio::sync::Semaphore::new(6)) }
    }
}

impl Covers {
    fn cached(&self, key: &(String, u32)) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
        self.cache.lock().unwrap().0.get(key).cloned()
    }

    fn store(&self, key: (String, u32), pixels: SharedPixelBuffer<Rgba8Pixel>) {
        let mut cache = self.cache.lock().unwrap();
        if cache.0.insert(key.clone(), pixels).is_none() {
            cache.1.push_back(key);
            while cache.1.len() > MAX_CACHED {
                if let Some(old) = cache.1.pop_front() {
                    cache.0.remove(&old);
                }
            }
        }
    }

    /// Lädt ein Cover; `done` läuft im UI-Thread.
    pub fn load(&self, app: &Arc<App>, url: String, size: u32, done: impl FnOnce(Image) + Send + 'static) {
        let key = (url.clone(), size);
        if let Some(pixels) = self.cached(&key) {
            let _ = slint::invoke_from_event_loop(move || done(Image::from_rgba8(pixels)));
            return;
        }
        let app = app.clone();
        app.rt.clone().spawn(async move {
            let Ok(_permit) = app.covers.limit.clone().acquire_owned().await else { return };
            let Some(pixels) = fetch(&app.covers.http, &url, size).await else { return };
            app.covers.store(key, pixels.clone());
            let _ = slint::invoke_from_event_loop(move || done(Image::from_rgba8(pixels)));
        });
    }
}

/// Bild-Adresse eines Songs: Cover-URL oder (lokale Datei) das eingebettete Bild
pub fn cover_url(track: &crate::player::Track) -> Option<String> {
    track.artwork_url.clone().or_else(|| track.path.as_ref().map(|p| format!("{LOCAL_PREFIX}{p}")))
}

const LOCAL_PREFIX: &str = "localcover:";

impl Covers {
    /// Rohdaten eines Bildes (für den Mini-Player, der selbst zuschneidet)
    pub async fn raw(&self, url: &str) -> Option<Vec<u8>> {
        if let Some(path) = url.strip_prefix(LOCAL_PREFIX) {
            let path = path.to_string();
            return tokio::task::spawn_blocking(move || crate::services::library::cover_bytes(&path).map(|(d, _)| d)).await.ok()?;
        }
        Some(self.http.get(url).send().await.ok()?.bytes().await.ok()?.to_vec())
    }
}

async fn fetch(http: &reqwest::Client, url: &str, size: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let bytes = if let Some(path) = url.strip_prefix(LOCAL_PREFIX) {
        let path = path.to_string();
        tokio::task::spawn_blocking(move || crate::services::library::cover_bytes(&path).map(|(data, _)| data)).await.ok()??
    } else if let Some(path) = url.strip_prefix("file://") {
        tokio::fs::read(path).await.ok()?
    } else {
        http.get(url).send().await.ok()?.bytes().await.ok()?.to_vec()
    };
    tokio::task::spawn_blocking(move || {
        let image = image::load_from_memory(&bytes).ok()?;
        let square = image.resize_to_fill(size, size, image::imageops::FilterType::Triangle).to_rgba8();
        Some(SharedPixelBuffer::clone_from_slice(square.as_raw(), square.width(), square.height()))
    })
    .await
    .ok()?
}

thread_local! {
    /// Listen, in die gerade Cover eingetragen werden (nur im UI-Thread)
    static MODELS: std::cell::RefCell<HashMap<u64, Box<dyn std::any::Any>>> = Default::default();
}
static NEXT_MODEL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Trägt Cover nach und nach in eine Liste ein (im UI-Thread aufrufen). `items`: (Zeile, Bild-URL).
/// Ist die Liste inzwischen ersetzt oder die Zeile eine andere (`id_of`), passiert nichts.
pub fn fill_with<R: Clone + 'static>(
    app: &Arc<App>,
    model: &Rc<VecModel<R>>,
    items: Vec<(usize, String)>,
    size: u32,
    id_of: fn(&R) -> slint::SharedString,
    set: fn(&mut R, Image),
) {
    let key = NEXT_MODEL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    MODELS.with(|m| {
        let mut m = m.borrow_mut();
        // Aufräumen: nicht mehr angezeigte Listen
        m.retain(|_, weak| {
            weak.downcast_ref::<std::rc::Weak<VecModel<R>>>().is_none_or(|w| w.strong_count() > 0)
        });
        m.insert(key, Box::new(Rc::downgrade(model)));
    });
    for (i, url) in items {
        let Some(id) = model.row_data(i).map(|r| id_of(&r)) else { continue };
        app.covers.load(app, url, size, move |image| {
            let model = MODELS.with(|m| {
                m.borrow().get(&key).and_then(|w| w.downcast_ref::<std::rc::Weak<VecModel<R>>>()).and_then(|w| w.upgrade())
            });
            let Some(model) = model else { return };
            if let Some(mut row) = model.row_data(i) {
                if id_of(&row) == id {
                    set(&mut row, image);
                    model.set_row_data(i, row);
                }
            }
        });
    }
}

/// Cover einer Songliste
pub fn fill(app: &Arc<App>, model: &Rc<VecModel<TrackRow>>, urls: Vec<Option<String>>) {
    let items = urls.into_iter().enumerate().filter_map(|(i, u)| u.map(|u| (i, u))).collect();
    fill_with(app, model, items, LIST_SIZE, |r| r.id.clone(), |r, image| {
        r.cover = image;
        r.has_cover = true;
    });
}
