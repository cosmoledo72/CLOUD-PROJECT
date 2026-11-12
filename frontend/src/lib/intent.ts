// One-shot URL intents from quick create and search: «?new=1» opens a page's
// create form, «?q=…» prefills its filter. The parameter is removed from the
// address afterwards so a reload or Back does not repeat it.
import { onMount } from 'svelte';
import { afterNavigate, replaceState } from '$app/navigation';
import { page } from '$app/state';

export function onIntent(name: string, run: (value: string) => void) {
  // Both hooks: afterNavigate for in-app navigation, onMount for a page
  // opened directly (it may mount after the first navigation completed).
  let pending = false;
  const handle = () => {
    const v = page.url.searchParams.get(name);
    if (v === null || pending) return;
    pending = true;
    const u = new URL(page.url);
    u.searchParams.delete(name);
    setTimeout(() => {
      try {
        replaceState(u.pathname + u.search + u.hash, page.state);
      } catch { /* router not ready */ }
      pending = false;
      run(v);
    });
  };
  afterNavigate(handle);
  onMount(handle);
}
