import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

/**
 * The test bench loads the same components as the app — only `invoke` comes
 * from `core.ts` and talks to the Rust server instead of Tauri.
 */
export default defineConfig({
  root: resolve(import.meta.dirname, "."),
  plugins: [react()],
  server: { port: 5321, strictPort: true },
  build: { outDir: resolve(import.meta.dirname, "../../../../build/testbench"), emptyOutDir: true },
  resolve: {
    alias: { "@tauri-apps/api/core": resolve(import.meta.dirname, "core.ts") },
  },
});
