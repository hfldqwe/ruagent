import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  build: { outDir: "dist" },
  server: {
    // Dev against a running daemon; the daemon serves /api.
    proxy: { "/api": "http://127.0.0.1:8787" },
  },
});
