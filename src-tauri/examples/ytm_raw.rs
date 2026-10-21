//! Debug-Hilfe: rohe, angemeldete YouTube-Music-Anfrage ausgeben.
//! Aufruf: cargo run --example ytm_raw -- <endpoint> '<json-body>' <speicherordner>
//! Beispiel: cargo run --example ytm_raw -- browse '{"browseId":"VLPL…"}' "%APPDATA%/com.wavely.app/ytm"
use rustypipe::client::{ClientType, RustyPipe};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let endpoint = args.next().expect("Endpoint fehlt");
    let body: serde_json::Value = serde_json::from_str(&args.next().expect("JSON-Body fehlt")).unwrap();
    let storage = args.next().expect("Speicherordner fehlt");
    let rp = RustyPipe::builder().storage_dir(storage).build().unwrap();
    let raw = rp
        .query()
        .authenticated()
        .raw(ClientType::DesktopMusic, &endpoint, &body)
        .await
        .unwrap();
    println!("{raw}");
}
