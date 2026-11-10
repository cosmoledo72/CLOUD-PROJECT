import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    port: 5173,
    // In development the API runs separately; proxy keeps cookies same-origin.
    proxy: { '/api': { target: process.env.API_URL ?? 'http://localhost:8080', changeOrigin: false } }
  }
});
