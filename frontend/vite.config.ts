import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [
    sveltekit({
      preprocess: vitePreprocess(),
      // Single-page app: the Rust API and the static bundle are served behind one reverse proxy.
      adapter: adapter({ fallback: 'index.html' }),

      // Open tabs notice a rebuilt portal within a minute (UpdateBanner); index.html
      // and version.json are served with «Cache-Control: no-cache» (nginx.conf).
      version: { pollInterval: 60_000 }
    })
  ],
  server: {
    port: 5173,
    // In development the API runs separately; proxy keeps cookies same-origin.
    proxy: { '/api': { target: process.env.API_URL ?? 'http://localhost:8080', changeOrigin: false } }
  }
});
