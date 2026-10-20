//! Debug-Hilfe: rohe YouTube-Music-Antwort für eine Playlist ausgeben.
//! Aufruf: cargo run --example ytm_raw -- <playlist-id> <speicherordner>
use rustypipe::client::{ClientType, RustyPipe};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let id = args.next().expect("Playlist-ID fehlt");
    let storage = args.next().expect("Speicherordner fehlt");
    let rp = RustyPipe::builder().storage_dir(storage).build().unwrap();
    let body = serde_json::json!({ "browseId": format!("VL{id}") });
    let raw = rp
        .query()
        .authenticated()
        .raw(ClientType::DesktopMusic, "browse", &body)
        .await
        .unwrap();
    println!("{raw}");
}
