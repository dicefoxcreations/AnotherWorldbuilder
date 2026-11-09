import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

export default defineConfig({
  plugins: [react()],
  // Tauri expects a fixed port and must not move to another on errors.
  server: { port: 5173, strictPort: true },
  // Generated at build time: the third-party notices (tools/licenses/notices.mjs).
  publicDir: fileURLToPath(new URL("../../build/public", import.meta.url)),
  build: { outDir: fileURLToPath(new URL("../../build/frontend", import.meta.url)), emptyOutDir: true, target: "es2021" },
  clearScreen: false,
});
