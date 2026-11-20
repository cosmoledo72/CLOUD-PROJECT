import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
export default {
  preprocess: vitePreprocess(),
  kit: {
    // Single-page app: the Rust API and the static bundle are served behind one reverse proxy.
    adapter: adapter({ fallback: 'index.html' }),
    // Open tabs notice a rebuilt portal within a minute (UpdateBanner); index.html
    // and version.json are served with «Cache-Control: no-cache» (nginx.conf).
    version: { pollInterval: 60_000 }
  }
};
