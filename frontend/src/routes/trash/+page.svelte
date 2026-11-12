<script lang="ts">
  import StorageTabs from '$lib/StorageTabs.svelte';
  import EmptyState from '$lib/EmptyState.svelte';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { api, formatDate, formatDateTime, formatSize, type TrashItem } from '$lib/api';
  import FileIcon from '$lib/FileIcon.svelte';
  import { session } from '$lib/session.svelte';

  let items = $state<TrashItem[]>([]);
  let retention = $state(30);
  let error = $state('');
  let notice = $state('');
  let loaded = $state(false);
  let restoredParent = $state('');

  async function load() {
    const r = await api.get<{ items: TrashItem[]; retention_days: number }>('/trash');
    items = r.items;
    retention = r.retention_days;
    loaded = true;
  }
  $effect(() => {
    load().catch((e) => (error = e.message));
  });

  async function run(fn: () => Promise<unknown>, ok: string) {
    error = '';
    notice = '';
    restoredParent = '';
    try {
      await fn();
      notice = ok;
      await load();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  async function restore(i: TrashItem) {
    let name = '';
    let parent = '';
    await run(async () => {
      const r = await api.post<{ name: string; parent_id: string }>(`/trash/${i.item_type}/${i.id}/restore`);
      name = r.name;
      parent = r.parent_id;
    }, '');
    if (!error) notice = `«${name}» восстановлено.`;
    if (!error && parent) restoredParent = parent;
  }

  function purge(i: TrashItem) {
    const what = i.item_type === 'folder' ? `папку «${i.name}» со всем содержимым` : `файл «${i.name}» со всеми версиями`;
    if (!confirm(`Удалить ${what} навсегда? Восстановить будет невозможно.`)) return;
    run(() => api.del(`/trash/${i.item_type}/${i.id}`), 'Удалено навсегда.');
  }

  function empty() {
    if (!confirm(`Очистить корзину (${items.length})? Все элементы будут удалены навсегда.`)) return;
    run(() => api.post('/trash/empty'), 'Корзина очищена.');
  }

  const total = $derived(items.reduce((s, i) => s + i.size_bytes, 0));
</script>

<div class="page">
  <StorageTabs />
  <div class="page-head">
    <h1>Корзина</h1>
    <div class="spacer"></div>
    {#if items.length}<button class="btn danger" onclick={empty}>Очистить корзину</button>{/if}
  </div>
  <p class="muted lead">
    Удалённые файлы и папки хранятся {retention} дней, затем удаляются автоматически.
    {#if session.isAdmin}Как администратор вы видите удалённое всеми сотрудниками портала.{:else}Здесь показано то, что удалили вы.{/if}
    Файлы в корзине учитываются в квоте хранилища.
  </p>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#if notice}<div class="ok-box">{notice}{#if restoredParent} <a href="/files/{restoredParent}">Открыть папку</a>{/if}</div>{/if}

  {#if loaded && items.length === 0}
    <div class="card"><EmptyState icon={Trash2} title="Корзина пуста" text="Удалённые файлы и папки появятся здесь и будут храниться {retention} дней." /></div>
  {:else if items.length}
    <div class="card scroll">
      <table class="list">
        <thead>
          <tr><th>Имя</th><th class="hide-sm">Откуда</th><th class="hide-sm">Удалил</th><th>Удалено</th><th class="hide-sm">Удалится</th><th class="hide-sm">Размер</th><th></th></tr>
        </thead>
        <tbody>
          {#each items as i (i.id)}
            <tr>
              <td>
                <span class="name">
                  {#if i.item_type === 'folder'}<FileIcon folder />{:else}<FileIcon ext={i.kind} />{/if}
                  {i.name}
                  {#if !i.restorable}<span class="badge grey">папка удалённого проекта</span>{/if}
                </span>
              </td>
              <td class="hide-sm muted">{i.location ?? '—'}</td>
              <td class="hide-sm muted">{i.deleted_by_name ?? '—'}</td>
              <td class="muted nowrap">{formatDateTime(i.deleted_at)}</td>
              <td class="hide-sm faint nowrap">{formatDate(i.purge_at)}</td>
              <td class="hide-sm muted">{i.item_type === 'file' ? formatSize(i.size_bytes) : '—'}</td>
              <td class="actions">
                {#if i.restorable}<button class="btn sm" onclick={() => restore(i)}>Восстановить</button>{/if}
                <button class="btn ghost sm danger" onclick={() => purge(i)}>Удалить навсегда</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    {#if total}<p class="faint small total">Файлы в корзине занимают {formatSize(total)} (без учёта содержимого папок).</p>{/if}
  {/if}
</div>

<style>
  .lead { margin: -6px 0 14px; max-width: 760px; }
  .error-box, .ok-box { margin-bottom: 12px; }
  .scroll { overflow-x: auto; }
  .name { display: inline-flex; align-items: center; gap: 10px; }
  .nowrap { white-space: nowrap; }
  .actions { text-align: right; white-space: nowrap; }
  .total { margin: 10px 0 0; }
  @media (max-width: 760px) { .hide-sm { display: none; } }
</style>
