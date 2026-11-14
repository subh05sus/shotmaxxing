import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';

// Tauri expects a fixed port in development and loads the built files in production.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [tailwindcss(), svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    // WKWebView on macOS 14 is Safari 17; WebView2 is current Chromium.
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome120' : 'safari17',
    minify: !process.env.TAURI_ENV_DEBUG ? 'oxc' : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    cssMinify: true,
  },
});
