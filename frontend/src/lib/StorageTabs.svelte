<script lang="ts">
  // Tabs of the «Хранилище» section: personal and common documents, shared
  // with me, trash.
  import { page } from '$app/state';
  import { api } from './api';
  import Building2 from '@lucide/svelte/icons/building-2';
  import FolderClosed from '@lucide/svelte/icons/folder-closed';
  import Share2 from '@lucide/svelte/icons/share-2';
  import Trash2 from '@lucide/svelte/icons/trash-2';

  /** Root folder of the folder being shown (on /files/...). */
  let { activeRoot = null }: { activeRoot?: string | null } = $props();

  let roots = $state<{ my: string | null; common: string | null }>({ my: null, common: null });
  $effect(() => {
    api.get<typeof roots>('/folders/roots').then((r) => (roots = r)).catch(() => {});
  });
  const path = $derived(page.url.pathname);
</script>

<nav class="storage" aria-label="Хранилище">
  {#if roots.my}
    <a href="/files/{roots.my}" class:on={activeRoot === roots.my} aria-current={activeRoot === roots.my ? 'page' : undefined}>
      <FolderClosed size={17} strokeWidth={1.75} />Мои документы
    </a>
  {/if}
  {#if roots.common}
    <a href="/files/{roots.common}" class:on={activeRoot === roots.common} aria-current={activeRoot === roots.common ? 'page' : undefined}>
      <Building2 size={17} strokeWidth={1.75} />Общие документы
    </a>
  {/if}
  <a href="/shared" class:on={path.startsWith('/shared')} aria-current={path.startsWith('/shared') ? 'page' : undefined}>
    <Share2 size={17} strokeWidth={1.75} />Доступные мне
  </a>
  <a href="/trash" class:on={path.startsWith('/trash')} aria-current={path.startsWith('/trash') ? 'page' : undefined}>
    <Trash2 size={17} strokeWidth={1.75} />Корзина
  </a>
</nav>

<style>
  .storage { display: flex; gap: 4px; flex-wrap: wrap; border-bottom: 1px solid var(--border); margin-bottom: 14px; }
  a {
    display: flex; align-items: center; gap: 7px; padding: 8px 12px; color: var(--text-2);
    border-bottom: 2px solid transparent; margin-bottom: -1px; font-size: 14px;
  }
  a:hover { color: var(--text); text-decoration: none; }
  a.on { color: var(--accent); border-bottom-color: var(--accent); font-weight: 500; }
</style>
