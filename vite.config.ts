import { defineConfig } from "vite";

export default defineConfig({
  root: "ui",
  base: "./",
  clearScreen: false,
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "es2022",
    chunkSizeWarningLimit: 8000,
  },
  server: {
    port: 1420,
    strictPort: true,
    // Dev-server mode proxies the engine API to `beany serve`.
    proxy: { "/api": "http://127.0.0.1:7777" },
  },
});
