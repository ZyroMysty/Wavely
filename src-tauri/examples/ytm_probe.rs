//! Testet, ob Suche und Audio-Streams von YouTube Music funktionieren.
//! Aufruf: cargo run --example ytm_probe -- "suchbegriff"

use rustypipe::{client::RustyPipe, param::StreamFilter};

#[tokio::main]
async fn main() {
    let query = std::env::args().nth(1).unwrap_or_else(|| "daft punk get lucky".into());
    let rp = RustyPipe::builder()
        .storage_dir(std::env::temp_dir().join("wavely-probe"))
        .build()
        .unwrap();

    let result = rp.query().music_search_tracks(&query).await.unwrap();
    let track = result.items.items.first().expect("keine Treffer");
    println!(
        "Treffer: {} – {} ({}s) [{}]",
        track.name,
        track.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "),
        track.duration.unwrap_or(0),
        track.id
    );

    let player = rp.query().player(&track.id).await.unwrap();
    let audio = player
        .select_audio_stream(&StreamFilter::default())
        .expect("kein Audio-Stream");
    println!("Format: {} {} kbit/s", audio.mime, audio.bitrate / 1000);
    println!("URL: {}", audio.url);
}
