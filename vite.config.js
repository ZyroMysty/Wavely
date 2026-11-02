import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri erwartet einen festen Port und soll die Rust-Ausgabe nicht überdecken.
export default defineConfig({
  plugins: [svelte()],
  build: {
    rollupOptions: {
      input: { main: "index.html" },
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
