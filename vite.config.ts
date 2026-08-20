import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Tauri drives dev on a fixed port and expects a deterministic dist.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  optimizeDeps: {
    // Scan from our entry alone.
    //
    // Vite's dependency scanner otherwise globs the project root for entry points and
    // finds the vendored logo repositories under `handoffs/brand-icons/`, each of which
    // is a whole application with its own dependency tree. thesvg ships a browser
    // extension importing `fuse.js`, which we do not have and must not acquire, so the
    // scan fails and Vite silently skips pre-bundling for the *real* app — slower cold
    // starts, caused by files that are not ours and are never built.
    entries: ['index.html'],
  },
  server: {
    port: 1420,
    strictPort: true,
    fs: {
      // A design delivery and two vendored logo repositories, none of which the dev
      // server should be willing to serve.
      deny: ['handoffs/**'],
    },
    watch: {
      // Rust rebuilds are Cargo's job; watching target/ melts the file watcher.
      // `handoffs/` is 14,000 vendored files that never change and never rebuild.
      ignored: ['**/target/**', '**/src-tauri/**', '**/handoffs/**'],
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
