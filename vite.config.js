import { defineConfig } from "vite";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  root: "ui",
  base: "./",
  plugins: [tailwindcss()],
  clearScreen: false,
  optimizeDeps: {
    noDiscovery: true,
  },
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    fs: {
      allow: [process.cwd()],
    },
  },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
  },
});
