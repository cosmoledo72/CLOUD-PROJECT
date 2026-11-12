// One-shot URL intents from quick create and search: «?new=1» opens a page's
// create form, «?q=…» prefills its filter. The parameter is removed from the
// address afterwards so a reload or Back does not repeat it.
import { afterNavigate, replaceState } from '$app/navigation';
import { page } from '$app/state';

export function onIntent(name: string, run: (value: string) => void) {
  afterNavigate(() => {
    const v = page.url.searchParams.get(name);
    if (v === null) return;
    const u = new URL(page.url);
    u.searchParams.delete(name);
    setTimeout(() => {
      try {
        replaceState(u.pathname + u.search + u.hash, page.state);
      } catch { /* router not ready */ }
      run(v);
    });
  });
}
