fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "update_presence",
            "clear_presence",
            "get_library",
            "add_folder",
            "remove_folder",
            "get_cover",
            "ytm_status",
            "ytm_login",
            "ytm_logout",
            "ytm_library",
            "ytm_collection",
            "ytm_search",
            "ytm_prefetch",
        ]),
    ))
    .expect("failed to run tauri-build");
}
