fn main() {
    slint_build::compile("ui/app.slint").unwrap();

    // Last.fm-Zugang (nicht im Repository): ~/.tauri/lastfm.json mit { "apiKey": "…", "secret": "…" }
    println!("cargo:rerun-if-env-changed=LASTFM_API_KEY");
    if std::env::var("LASTFM_API_KEY").is_err() {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
        if let Some(file) = home.map(|h| std::path::PathBuf::from(h).join(".tauri").join("lastfm.json")) {
            println!("cargo:rerun-if-changed={}", file.display());
            if let Some(value) = std::fs::read_to_string(&file).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()) {
                if let (Some(key), Some(secret)) = (value["apiKey"].as_str(), value["secret"].as_str()) {
                    println!("cargo:rustc-env=LASTFM_API_KEY={key}");
                    println!("cargo:rustc-env=LASTFM_API_SECRET={secret}");
                }
            }
        }
    }

    // Programmsymbol in der exe (Explorer, Taskleiste)
    #[cfg(windows)]
    {
        // Ohne diese Zeile bliebe nach einem Logo-Wechsel das alte Symbol in der exe
        println!("cargo:rerun-if-changed=../src-tauri/icons/icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../src-tauri/icons/icon.ico");
        res.set("FileDescription", "Wavely Native");
        res.set("ProductName", "Wavely Native");
        let _ = res.compile();
    }
}
