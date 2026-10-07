import { defineConfig } from "vitest/config";

// Tauri expects a fixed port and no screen clearing so Rust errors stay visible.
export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "chrome110", outDir: "dist", emptyOutDir: true },
  test: { include: ["src/**/*.test.ts", "scripts/**/*.test.mjs"], environment: "node" },
});
