fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "update_presence",
            "clear_presence",
            "get_library",
            "add_folder",
            "remove_folder",
            "get_cover",
        ]),
    ))
    .expect("failed to run tauri-build");
}
