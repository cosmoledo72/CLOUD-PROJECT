<script lang="ts">
  // Full-screen ONLYOFFICE editor. Loads the Document Server API script from
  // the URL returned by the backend and mounts DocsAPI.DocEditor with the
  // server-signed config.
  import { page } from '$app/state';
  import { api } from '#lib/api.ts';

  interface EditorResponse {
    docs_url: string;
    can_edit: boolean;
    config: Record<string, unknown> & { document: { title: string } };
  }

  let error = $state('');
  let title = $state('');

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
    const id = page.params.id;
    const mode = page.url.searchParams.get('mode') ?? 'edit';
    let editor: { destroyEditor(): void } | null = null;

    (async () => {
      try {
        const r = await api.get<EditorResponse>(`/files/${id}/editor?mode=${mode}`);
        title = r.config.document.title;
        document.title = `${title} — CLOUD-PROJECT`;
        await loadScript(`${r.docs_url}/web-apps/apps/api/documents/api.js`);
        const config = {
          ...r.config,
          width: '100%',
          height: '100%',
          events: {
            onRequestClose: () => window.close()
          }
        };
        editor = new window.DocsAPI!.DocEditor('oo-editor', config);
      } catch (e) {
        error = (e as Error).message;
      }
    })();

    return () => editor?.destroyEditor();
  });
</script>

<div class="editor">
  {#if error}
    <div class="msg card">
      <h2>Не удалось открыть документ</h2>
      <p class="error-box">{error}</p>
      <a class="btn" href="/files">К документам</a>
    </div>
  {/if}
  <div id="oo-editor"></div>
</div>

<style>
  .editor { position: fixed; inset: 0; background: var(--surface); }
  #oo-editor { width: 100%; height: 100%; }
  .msg { position: absolute; top: 20%; left: 50%; transform: translateX(-50%); padding: 24px; display: grid; gap: 12px; width: min(420px, 90vw); }
  .msg p { margin: 0; }
</style>
