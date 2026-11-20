<script lang="ts">
  // Destination for «Переместить» / «Копировать»: my documents, common
  // documents and project folders, browsed down to any subfolder.
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import FolderPlus from '@lucide/svelte/icons/folder-plus';
  import { api, can, folderName, type FolderView, type Project } from '#lib/api.ts';
  import FileIcon from '#lib/FileIcon.svelte';
  import Modal from '#lib/Modal.svelte';
  import { session } from '#lib/session.svelte.ts';

  let {
    open = $bindable(false),
    title,
    action,
    start = null,
    exclude = [],
    onpick
  }: {
    open?: boolean;
    title: string;
    /** Button text, e.g. «Переместить сюда». */
    action: string;
    /** Folder to open first. */
    start?: string | null;
    /** Folders that cannot be chosen (being moved) — hidden from the list. */
    exclude?: string[];
    onpick: (folderId: string) => Promise<void> | void;
  } = $props();

  type Root = { id: string; label: string };
  let roots = $state<Root[]>([]);
  let view = $state<FolderView | null>(null);
  let error = $state('');
  let busy = $state(false);

  $effect(() => {
    if (!open) return;
    error = '';
    view = null;
    loadRoots();
    if (start) go(start);
  });

  async function loadRoots() {
    const list: Root[] = [];
    try {
      const r = await api.get<{ my: string | null; common: string | null }>('/folders/roots');
      if (r.my) list.push({ id: r.my, label: 'Мои документы' });
      if (r.common) list.push({ id: r.common, label: 'Общие документы' });
      if (session.has('projects')) {
        const p = await api.get<{ projects: Project[] }>('/projects');
        for (const x of p.projects) if (x.folder_id && x.status !== 'closed') list.push({ id: x.folder_id, label: x.name });
      }
    } catch (e) {
      error = (e as Error).message;
    }
    roots = list;
  }
  async function go(id: string | null) {
    error = '';
    if (!id) {
      view = null;
      return;
    }
    try {
      view = await api.get<FolderView>(`/folders/${id}`);
    } catch (e) {
      error = (e as Error).message;
    }
  }
  const canDrop = $derived(!!view && can(view.access, 'Edit'));
  async function pick() {
    if (!view) return;
    busy = true;
    error = '';
    try {
      await onpick(view.folder.id);
      open = false;
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }
  let newName = $state('');
  let adding = $state(false);
  async function addFolder() {
    if (!view || !newName.trim()) return;
    try {
      const r = await api.post<{ id: string }>('/folders', { parent_id: view.folder.id, name: newName.trim() });
      newName = '';
      adding = false;
      await go(r.id);
    } catch (e) {
      error = (e as Error).message;
    }
  }
  const label = (c: { id: string; name: string }) => roots.find((r) => r.id === c.id)?.label ?? folderName(c.name);
</script>

<Modal bind:open {title} width={560}>
  <nav class="crumbs" aria-label="Путь">
    <button class="linkbtn" onclick={() => go(null)}>Все места</button>
    {#each view?.breadcrumbs ?? [] as c, i (c.id)}
      <ChevronRight size={14} />
      {#if i === (view?.breadcrumbs.length ?? 0) - 1}<b>{label(c)}</b>{:else}<button class="linkbtn" onclick={() => go(c.id)}>{label(c)}</button>{/if}
    {/each}
  </nav>
  <ul class="pick">
    {#if !view}
      {#each roots as r (r.id)}
        <li><button class="row" onclick={() => go(r.id)}><FileIcon folder /><span class="nm">{r.label}</span><ChevronRight size={15} /></button></li>
      {/each}
    {:else}
      {#each view.folders.filter((f) => !exclude.includes(f.id)) as f (f.id)}
        <li><button class="row" onclick={() => go(f.id)}><FileIcon folder /><span class="nm">{f.name}</span><ChevronRight size={15} /></button></li>
      {:else}
        <li class="faint small empty">Вложенных папок нет — можно выбрать эту.</li>
      {/each}
    {/if}
  </ul>
  {#if view && canDrop}
    {#if adding}
      <form class="add" onsubmit={(e) => { e.preventDefault(); addFolder(); }}>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="input" bind:value={newName} placeholder="Название новой папки" autofocus />
        <button class="btn" disabled={!newName.trim()}>Создать</button>
      </form>
    {:else}
      <button class="btn ghost sm newf" onclick={() => (adding = true)}><FolderPlus size={15} />Новая папка здесь</button>
    {/if}
  {/if}
  {#if view && !canDrop}<p class="faint small">В эту папку у вас нет права добавлять.</p>{/if}
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (open = false)}>Отмена</button>
    <button class="btn primary" disabled={!canDrop || busy} onclick={pick}>{busy ? 'Выполняю…' : action}</button>
  {/snippet}
</Modal>

<style>
  .crumbs { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; font-size: 13px; margin-bottom: 8px; color: var(--text-2); }
  .linkbtn { border: 0; background: none; padding: 0; color: var(--accent); cursor: pointer; font: inherit; }
  .linkbtn:hover { text-decoration: underline; }
  .pick { list-style: none; margin: 0 0 8px; padding: 0; max-height: min(50vh, 380px); overflow-y: auto; border: 1px solid var(--border); border-radius: var(--radius); }
  .pick li + li { border-top: 1px solid var(--border); }
  .row { display: flex; align-items: center; gap: 9px; width: 100%; padding: 8px 10px; border: 0; background: none; text-align: left; cursor: pointer; font: inherit; color: var(--text); }
  .row:hover { background: var(--surface-2); }
  .nm { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { padding: 14px; text-align: center; }
  .add { display: flex; gap: 8px; }
  .newf { display: inline-flex; gap: 6px; }
</style>
