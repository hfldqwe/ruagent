import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Build stamp: shown in the panel footer so "which build am I on" is
// answerable at a glance (stale-cache confusion, 2026-09-13).
const BUILD_ID = new Date().toISOString().slice(5, 16).replace("T", " ");

export default defineConfig({
  plugins: [react()],
  define: { __BUILD_ID__: JSON.stringify(BUILD_ID) },
  build: { outDir: "dist" },
  server: {
    // Dev against a running daemon; the daemon serves /api.
    proxy: { "/api": "http://127.0.0.1:8787" },
  },
});
