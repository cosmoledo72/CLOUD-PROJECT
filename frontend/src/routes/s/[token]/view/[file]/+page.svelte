<script lang="ts">
  // Read-only ONLYOFFICE viewer for a file opened through a public link.
  import { page } from '$app/state';
  import { api } from '$lib/api';

  interface ViewerResponse {
    docs_url: string;
    config: Record<string, unknown> & { document: { title: string } };
  }

  let error = $state('');

  function loadScript(src: string): Promise<void> {
    if (window.DocsAPI) return Promise.resolve();
    return new Promise((resolve, reject) => {
      const s = document.createElement('script');
      s.src = src;
      s.onload = () => resolve();
      s.onerror = () => reject(new Error('Сервер документов недоступен'));
      document.head.appendChild(s);
    });
  }

  $effect(() => {
    const token = page.params.token ?? '';
    const file = page.params.file ?? '';
    const k = page.url.searchParams.get('k');
    let viewer: { destroyEditor(): void } | null = null;
    (async () => {
      try {
        const r = await api.get<ViewerResponse>(`/public/s/${encodeURIComponent(token)}/view/${file}${k ? `?k=${encodeURIComponent(k)}` : ''}`);
        document.title = r.config.document.title;
        await loadScript(`${r.docs_url}/web-apps/apps/api/documents/api.js`);
        viewer = new window.DocsAPI!.DocEditor('oo-viewer', { ...r.config, width: '100%', height: '100%' });
      } catch (e) {
        const st = (e as { status?: number }).status;
        error = st === 404 || st === 401 ? 'Ссылка не работает или документ больше недоступен.' : (e as Error).message;
      }
    })();
    return () => viewer?.destroyEditor();
  });
</script>

<svelte:head><meta name="robots" content="noindex, nofollow" /></svelte:head>

<div class="viewer">
  {#if error}
    <div class="msg card">
      <h2>Не удалось открыть документ</h2>
      <p class="error-box">{error}</p>
      <a class="btn" href="/s/{page.params.token}">К документам по ссылке</a>
    </div>
  {/if}
  <div id="oo-viewer"></div>
</div>

<style>
  .viewer { position: fixed; inset: 0; background: var(--surface); }
  #oo-viewer { width: 100%; height: 100%; }
  .msg { position: absolute; top: 20%; left: 50%; transform: translateX(-50%); padding: 24px; display: grid; gap: 12px; width: min(420px, 90vw); }
  .msg p { margin: 0; }
</style>
