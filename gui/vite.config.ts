import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Vite config tuned for Tauri: a fixed dev port and no clearing of the
// terminal so Tauri's own output stays visible. Build logic lives in the
// engine; this only serves/bundles the static frontend.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "es2021",
    outDir: "dist",
  },
});
