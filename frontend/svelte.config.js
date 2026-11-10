import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
export default {
  preprocess: vitePreprocess(),
  kit: {
    // Single-page app: the Rust API and the static bundle are served behind one reverse proxy.
    adapter: adapter({ fallback: 'index.html' })
  }
};
