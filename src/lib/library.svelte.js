import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export const library = $state({
  folders: [],
  tracks: [],
  loading: false,
  error: null,
});

export async function refreshLibrary() {
  library.loading = true;
  library.error = null;
  try {
    const result = await invoke("get_library");
    library.folders = result.folders;
    library.tracks = result.tracks.map((t) => ({ ...t, source: "local" }));
  } catch (e) {
    library.error = String(e);
  } finally {
    library.loading = false;
  }
}

export async function addFolder() {
  const path = await open({ directory: true, multiple: false, title: "Musikordner auswählen" });
  if (!path) return;
  await invoke("add_folder", { path });
  await refreshLibrary();
}

export async function removeFolder(path) {
  await invoke("remove_folder", { path });
  await refreshLibrary();
}
