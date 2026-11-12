<script lang="ts">
  // Page history: every save is a version (who and when); look at an old
  // version and bring it back — it becomes the newest one.
  import History from '@lucide/svelte/icons/history';
  import { api, formatDateTime } from '$lib/api';
  import Modal from '$lib/Modal.svelte';
  import KbView from './KbView.svelte';

  let { open = $bindable(false), pageId, current, onrestored }: { open?: boolean; pageId: string; current: number; onrestored: () => void } = $props();

  interface V { version: number; title: string; author_name: string | null; created_at: string; length: number }
  let list = $state<V[]>([]);
  let sel = $state<number | null>(null);
  let shown = $state<{ title: string; content: string } | null>(null);
  let error = $state('');
  let busy = $state(false);

  $effect(() => {
    if (!open) return;
    error = '';
    shown = null;
    api.get<{ versions: V[] }>(`/kb/pages/${pageId}/versions`).then((r) => {
      list = r.versions;
      if (list.length) pick(list[0].version);
    }).catch((e) => (error = e.message));
  });
  async function pick(v: number) {
    sel = v;
    try {
      shown = await api.get<{ title: string; content: string }>(`/kb/pages/${pageId}/versions/${v}`);
    } catch (e) {
      error = (e as Error).message;
    }
  }
  async function restore() {
    if (sel === null || !confirm(`Вернуть версию ${sel}? Текущий текст останется в истории.`)) return;
    busy = true;
    try {
      await api.post(`/kb/pages/${pageId}/versions/${sel}/restore`);
      open = false;
      onrestored();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }
</script>

<Modal bind:open title="История изменений" width={960}>
  {#if error}<div class="error-box">{error}</div>{/if}
  <div class="hist">
    <ul class="vers">
      {#each list as v (v.version)}
        <li>
          <button class:on={sel === v.version} onclick={() => pick(v.version)}>
            <b>Версия {v.version}{#if v.version === current} <span class="cur">текущая</span>{/if}</b>
            <span>{formatDateTime(v.created_at)}</span>
            <span class="who">{v.author_name ?? '—'}</span>
          </button>
        </li>
      {:else}
        <li class="faint small empty"><History size={16} /> Истории пока нет</li>
      {/each}
    </ul>
    <div class="pv">
      {#if shown}
        <h2>{shown.title}</h2>
        <KbView html={shown.content || '<p><em>Пусто</em></p>'} />
      {/if}
    </div>
  </div>
  {#snippet footer()}
    <span class="faint small note">Хранятся последние 100 версий.</span>
    <button class="btn" onclick={() => (open = false)}>Закрыть</button>
    <button class="btn primary" disabled={busy || sel === null || sel === current} onclick={restore}>Вернуть эту версию</button>
  {/snippet}
</Modal>

<style>
  .hist { display: grid; grid-template-columns: 230px 1fr; gap: 14px; min-height: 420px; }
  .vers { list-style: none; margin: 0; padding: 0; border-right: 1px solid var(--border); max-height: 60vh; overflow-y: auto; }
  .vers button { display: grid; gap: 1px; width: 100%; padding: 8px 10px; border: 0; border-radius: 8px; background: none; font: inherit; text-align: left; cursor: pointer; color: var(--text); }
  .vers button:hover { background: var(--surface-2); }
  .vers button.on { background: var(--accent-soft); }
  .vers span { font-size: 12.5px; color: var(--text-3); }
  .vers .who { color: var(--text-2); }
  .cur { font-size: 11px; font-weight: 500; color: var(--accent); margin-left: 4px; }
  .pv { max-height: 60vh; overflow-y: auto; padding-right: 6px; }
  .pv h2 { margin: 0 0 10px; font-size: 20px; }
  .empty { display: flex; gap: 8px; padding: 12px; }
  .note { margin-right: auto; }
  @media (max-width: 720px) { .hist { grid-template-columns: 1fr; } .vers { border-right: 0; max-height: 180px; } }
</style>
