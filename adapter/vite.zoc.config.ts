import { defineConfig } from "vite"
import desktopPlugin, { channel } from "./vite.js"
import { icons } from "./vite.icons"

// ZOC-owned build configuration for the official OpenCode Web UI.
//
// It reuses the upstream app's Vite plugins (Solid, Tailwind, theme preload,
// aliases, worker and Office asset handling), and deliberately omits the
// PWA service worker. The installed desktop app is updated as a whole, so a
// stale browser cache must never shadow embedded assets.
//
// The build script copies this file next to the upstream app's vite config
// before running `vite build`; the relative imports above are intentional.
export default defineConfig({
  plugins: [desktopPlugin, icons(channel)] as any,
  build: {
    assetsDir: "_assets",
    target: "esnext",
    sourcemap: false,
    outDir: "dist",
    emptyOutDir: true,
  },
  server: {
    host: "127.0.0.1",
    port: 3000,
    strictPort: false,
  },
})
