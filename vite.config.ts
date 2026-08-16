import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Tauri drives dev on a fixed port and expects a deterministic dist.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Rust rebuilds are Cargo's job; watching target/ melts the file watcher.
      ignored: ['**/target/**', '**/src-tauri/**'],
    },
  },
  build: {
    // Windows 10 1809 / macOS 12 floor (SPEC-V1 §8): both ship evergreen
    // WebView2 and WKWebView, so a modern baseline is safe.
    target: ['chrome110', 'safari15'],
    sourcemap: false,
    emptyOutDir: true,
  },
});
