<script lang="ts">
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import { api, can, docKind, folderName, formatDateTime, formatSize, type FileItem, type FolderView, type Version } from '$lib/api';
  import FileIcon from '$lib/FileIcon.svelte';
  import Modal from '$lib/Modal.svelte';
  import ShareDialog from '$lib/ShareDialog.svelte';

  /**
   * Folder browser. As a page it links folders to /files/{id}; embedded (e.g. a
   * project's "Files" tab) it navigates in place and never shows breadcrumbs above
   * the `root` folder.
   */
  let {
    folderId,
    embedded = false,
    root = null,
    onview
  }: { folderId: string; embedded?: boolean; root?: string | null; onview?: (v: FolderView) => void } = $props();

  // Embedded mode keeps its own current folder; page mode follows the URL.
  let local = $state<string | null>(null);
  $effect(() => {
    folderId;
    local = null;
  });
  const id = $derived(embedded && local ? local : folderId);
  let view = $state<FolderView | null>(null);
  let error = $state('');

  async function load() {
    error = '';
    try {
      view = await api.get<FolderView>(`/folders/${id}`);
      onview?.(view);
    } catch (e) {
      error = (e as Error).message;
      view = null;
    }
  }

  $effect(() => {
    id;
    load();
  });

  const canEdit = $derived(can(view?.access, 'Edit'));

  // --- Create -------------------------------------------------------------
  let newFolderOpen = $state(false);
  let newFolderName = $state('');
  async function createFolder() {
    await run(() => api.post('/folders', { parent_id: id, name: newFolderName }));
    newFolderOpen = false;
    newFolderName = '';
  }

  let newDocOpen = $state(false);
  let newDocKind = $state<'docx' | 'xlsx' | 'pptx'>('docx');
  let newDocName = $state('');
  function openNewDoc(kind: 'docx' | 'xlsx' | 'pptx') {
    newDocKind = kind;
    newDocName = { docx: 'Новый документ', xlsx: 'Новая таблица', pptx: 'Новая презентация' }[kind];
    newDocOpen = true;
  }
  async function createDoc() {
    const r = await api.post<{ id: string }>(`/folders/${id}/new`, { kind: newDocKind, name: newDocName });
    newDocOpen = false;
    window.open(`/editor/${r.id}`, '_blank');
    await load();
  }

  // --- Upload -------------------------------------------------------------
  let uploading = $state(0);
  let dragOver = $state(false);
  let fileInput = $state<HTMLInputElement>();

  async function uploadFiles(list: FileList | File[], target: string = id) {
    for (const f of Array.from(list)) {
      uploading++;
      try {
        const fd = new FormData();
        fd.append('file', f, f.name);
        try {
          await api.post(`/folders/${target}/upload`, fd);
        } catch (e) {
          if ((e as { status?: number }).status === 409 && confirm(`Файл «${f.name}» уже есть. Сохранить как новую версию?`)) {
            await api.post(`/folders/${target}/upload?overwrite=true`, fd);
          } else if ((e as { status?: number }).status !== 409) {
            throw e;
          }
        }
      } catch (e) {
        error = (e as Error).message;
      } finally {
        uploading--;
      }
    }
    await load();
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    dragOver = false;
    // Items dragged inside the list are handled by the folder rows.
    if (dragging) return;
    if (canEdit && e.dataTransfer?.files.length) uploadFiles(e.dataTransfer.files);
  }

  // --- Drag and drop between folders ---------------------------------------------
  // Files and folders can be dragged onto a folder row or onto a breadcrumb above
  // the current folder; files from the computer dropped on a folder row are
  // uploaded straight into it.
  type DragItem = { type: 'folder' | 'file'; id: string; name: string };
  const DND = 'application/x-workhub-item';
  let dragging = $state<DragItem | null>(null);
  let dropTarget = $state<string | null>(null);
  const canMove = $derived(can(view?.access, 'Full'));

  function startDrag(e: DragEvent, item: DragItem) {
    if (!canMove || !e.dataTransfer) return;
    dragging = item;
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData(DND, JSON.stringify(item));
    e.dataTransfer.setData('text/plain', item.name);
  }
  function endDrag() {
    dragging = null;
    dropTarget = null;
  }
  const isOsFiles = (e: DragEvent) => !dragging && !!e.dataTransfer?.types.includes('Files');
  function overTarget(e: DragEvent, folderId: string) {
    if (dragging ? dragging.id === folderId : !(canEdit && isOsFiles(e))) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer) e.dataTransfer.dropEffect = dragging ? 'move' : 'copy';
    dropTarget = folderId;
    dragOver = false;
  }
  function leaveTarget(folderId: string) {
    if (dropTarget === folderId) dropTarget = null;
  }
  async function dropOn(e: DragEvent, folderId: string, folderLabel: string) {
    e.preventDefault();
    e.stopPropagation();
    dropTarget = null;
    dragOver = false;
    const item = dragging;
    dragging = null;
    if (!item) {
      if (canEdit && e.dataTransfer?.files.length) await uploadFiles(e.dataTransfer.files, folderId);
      return;
    }
    if (item.id === folderId) return;
    error = '';
    try {
      if (item.type === 'file') await api.patch(`/files/${item.id}`, { folder_id: folderId });
      else await api.patch(`/folders/${item.id}`, { parent_id: folderId });
      await load();
    } catch (err) {
      const status = (err as { status?: number }).status;
      error = status === 409
        ? `В папке «${folderLabel}» уже есть ${item.type === 'file' ? 'файл' : 'папка'} с именем «${item.name}».`
        : status === 403
          ? `Нет прав на перемещение в папку «${folderLabel}».`
          : (err as Error).message;
    }
  }

  // --- Item actions ------------------------------------------------------
  async function run(fn: () => Promise<unknown>) {
    error = '';
    try {
      await fn();
      await load();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  async function rename(kind: 'folder' | 'file', itemId: string, current: string) {
    const name = prompt('Новое имя', current);
    if (!name || name === current) return;
    await run(() => api.patch(`/${kind}s/${itemId}`, { name }));
  }

  async function remove(kind: 'folder' | 'file', itemId: string, name: string) {
    if (!confirm(`Удалить «${name}»?`)) return;
    await run(() => api.del(`/${kind}s/${itemId}`));
  }

  let shareOpen = $state(false);
  let shareTarget = $state<{ type: 'folder' | 'file'; id: string; name: string }>({ type: 'folder', id: '', name: '' });
  function share(type: 'folder' | 'file', itemId: string, name: string) {
    shareTarget = { type, id: itemId, name };
    shareOpen = true;
  }

  let versionsOpen = $state(false);
  let versionsFile = $state<FileItem | null>(null);
  let versions = $state<Version[]>([]);
  async function showVersions(f: FileItem) {
    versionsFile = f;
    versions = (await api.get<{ versions: Version[] }>(`/files/${f.id}/versions`)).versions;
    versionsOpen = true;
  }
  async function restore(v: number) {
    if (!versionsFile || !confirm(`Восстановить версию ${v}? Она станет новой текущей версией.`)) return;
    await run(() => api.post(`/files/${versionsFile!.id}/versions/${v}/restore`));
    versionsOpen = false;
  }

  const openable = (f: FileItem) => docKind(f.extension) !== 'other';

  // Breadcrumbs, trimmed to the embedding root.
  const crumbs = $derived.by(() => {
    const all = view?.breadcrumbs ?? [];
    if (!embedded || !root) return all;
    const i = all.findIndex((c) => c.id === root);
    return i >= 0 ? all.slice(i) : all;
  });
  // Parent folder within the visible path (none at the root or at the embedding root).
  const parent = $derived(crumbs.length > 1 ? crumbs[crumbs.length - 2] : null);
  function go(folder: string) {
    if (embedded) local = folder;
  }
</script>

<div
  class="browser"
  class:embedded
  role="region"
  aria-label="Папка"
  ondragover={(e) => { e.preventDefault(); if (canEdit) dragOver = true; }}
  ondragleave={() => (dragOver = false)}
  ondrop={onDrop}
>
  {#if view}
    <div class="page-head">
      {#if parent}
        <!-- Up one level; also a drop target for moving items to the parent folder. -->
        <span class="up-wrap" class:target={dropTarget === parent.id} role="listitem"
              ondragover={(e) => overTarget(e, parent!.id)} ondragleave={() => leaveTarget(parent!.id)}
              ondrop={(e) => dropOn(e, parent!.id, folderName(parent!.name))}>
          {#if embedded}
            <button class="btn up" title="Выше: {folderName(parent.name)}" aria-label="На уровень выше" onclick={() => go(parent!.id)}><ArrowUp size={16} /> Выше</button>
          {:else}
            <a class="btn up" href="/files/{parent.id}" title="Выше: {folderName(parent.name)}" aria-label="На уровень выше"><ArrowUp size={16} /> Выше</a>
          {/if}
        </span>
      {/if}
      <nav class="crumbs" aria-label="Путь">
        {#each crumbs as c, i}
          {#if i > 0}<span class="faint">/</span>{/if}
          {#if i < crumbs.length - 1}
            <span class="drop-crumb" class:target={dropTarget === c.id} role="listitem"
                  ondragover={(e) => overTarget(e, c.id)} ondragleave={() => leaveTarget(c.id)} ondrop={(e) => dropOn(e, c.id, folderName(c.name))}>
              {#if embedded}<button class="crumb" onclick={() => go(c.id)}>{folderName(c.name)}</button>{:else}<a href="/files/{c.id}">{folderName(c.name)}</a>{/if}
            </span>
          {:else if embedded}<strong>{folderName(c.name)}</strong>{:else}<h1>{folderName(c.name)}</h1>{/if}
        {/each}
      </nav>
      <div class="spacer"></div>
      {#if canEdit}
        <div class="row tools">
          <button class="btn" onclick={() => openNewDoc('docx')}><span class="dot word"></span>Документ</button>
          <button class="btn" onclick={() => openNewDoc('xlsx')}><span class="dot cell"></span>Таблица</button>
          <button class="btn" onclick={() => openNewDoc('pptx')}><span class="dot slide"></span>Презентация</button>
          <button class="btn" onclick={() => (newFolderOpen = true)}>Папка</button>
          <button class="btn primary" onclick={() => fileInput?.click()}>Загрузить</button>
          <input type="file" multiple hidden bind:this={fileInput} onchange={(e) => { const t = e.currentTarget; if (t.files) uploadFiles(t.files); t.value = ''; }} />
        </div>
      {/if}
      {#if can(view.access, 'Full') && view.folder.parent_id}
        <button class="btn" onclick={() => share('folder', view!.folder.id, view!.folder.name)}>Доступ</button>
      {/if}
    </div>

    {#if error}<div class="error-box">{error}</div>{/if}
    {#if uploading > 0}<div class="badge">Загрузка: {uploading}…</div>{/if}

    <div class="card" class:drag={dragOver}>
      {#if view.folders.length === 0 && view.files.length === 0}
        <div class="empty">
          <p>Папка пуста.</p>
          {#if canEdit}<p class="faint">Перетащите файлы сюда или создайте документ.</p>{/if}
        </div>
      {:else}
        <table class="list">
          <thead>
            <tr><th>Имя</th><th class="hide-sm">Владелец</th><th class="hide-sm">Изменён</th><th class="hide-sm">Размер</th><th></th></tr>
          </thead>
          <tbody>
            {#each view.folders as f (f.id)}
              <tr draggable={canMove} class:target={dropTarget === f.id} class:moving={dragging?.id === f.id}
                  ondragstart={(e) => startDrag(e, { type: 'folder', id: f.id, name: f.name })} ondragend={endDrag}
                  ondragover={(e) => overTarget(e, f.id)} ondragleave={() => leaveTarget(f.id)} ondrop={(e) => dropOn(e, f.id, f.name)}>
                <td>
                  {#if embedded}<button class="name linkbtn" onclick={() => go(f.id)}><FileIcon folder />{f.name}</button>
                  {:else}<a class="name" href="/files/{f.id}"><FileIcon folder />{f.name}</a>{/if}
                </td>
                <td class="hide-sm muted"></td>
                <td class="hide-sm muted">{formatDateTime(f.created_at)}</td>
                <td class="hide-sm muted">—</td>
                <td class="actions">
                  {#if can(view.access, 'Full')}
                    <button class="btn ghost sm" onclick={() => share('folder', f.id, f.name)}>Доступ</button>
                    <button class="btn ghost sm" onclick={() => rename('folder', f.id, f.name)}>Переименовать</button>
                    <button class="btn ghost sm danger" onclick={() => remove('folder', f.id, f.name)}>Удалить</button>
                  {/if}
                </td>
              </tr>
            {/each}
            {#each view.files as f (f.id)}
              <tr draggable={canMove} class:moving={dragging?.id === f.id}
                  ondragstart={(e) => startDrag(e, { type: 'file', id: f.id, name: f.name })} ondragend={endDrag}>
                <td>
                  {#if openable(f)}
                    <a class="name" href="/editor/{f.id}" target="_blank"><FileIcon ext={f.extension} />{f.name}</a>
                  {:else}
                    <a class="name" href="/api/files/{f.id}/download"><FileIcon ext={f.extension} />{f.name}</a>
                  {/if}
                  {#if f.is_editing}<span class="badge editing">редактируется</span>{/if}
                </td>
                <td class="hide-sm muted">{f.owner_name}</td>
                <td class="hide-sm muted">{formatDateTime(f.updated_at)}</td>
                <td class="hide-sm muted">{formatSize(f.size_bytes)}</td>
                <td class="actions">
                  <a class="btn ghost sm" href="/api/files/{f.id}/download">Скачать</a>
                  <button class="btn ghost sm" onclick={() => showVersions(f)}>v{f.current_version}</button>
                  {#if can(view.access, 'Full')}
                    <button class="btn ghost sm" onclick={() => share('file', f.id, f.name)}>Доступ</button>
                    <button class="btn ghost sm" onclick={() => rename('file', f.id, f.name)}>Переименовать</button>
                    <button class="btn ghost sm danger" onclick={() => remove('file', f.id, f.name)}>Удалить</button>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
    {#if canMove && view.folders.length + view.files.length > 1}
      <p class="faint small dnd-hint">Перетащите файл или папку на другую папку, чтобы переместить; на путь вверху — чтобы поднять уровнем выше.</p>
    {/if}
  {:else if error}
    <div class="error-box">{error}</div>
  {/if}
</div>

<Modal bind:open={newFolderOpen} title="Новая папка">
  <input class="input" bind:value={newFolderName} placeholder="Название" onkeydown={(e) => e.key === 'Enter' && createFolder()} />
  {#snippet footer()}
    <button class="btn" onclick={() => (newFolderOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!newFolderName.trim()} onclick={createFolder}>Создать</button>
  {/snippet}
</Modal>

<Modal bind:open={newDocOpen} title="Новый файл">
  <label class="field">Название <input class="input" bind:value={newDocName} onkeydown={(e) => e.key === 'Enter' && createDoc()} /></label>
  {#snippet footer()}
    <button class="btn" onclick={() => (newDocOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!newDocName.trim()} onclick={createDoc}>Создать и открыть</button>
  {/snippet}
</Modal>

<Modal bind:open={versionsOpen} title={`Версии: ${versionsFile?.name ?? ''}`} width={560}>
  <table class="list">
    <thead><tr><th>Версия</th><th>Автор</th><th>Дата</th><th>Размер</th><th></th></tr></thead>
    <tbody>
      {#each versions as v, i}
        <tr>
          <td>v{v.version}{#if i === 0} <span class="badge">текущая</span>{/if}</td>
          <td>{v.author_name ?? '—'}</td>
          <td class="muted">{formatDateTime(v.created_at)}</td>
          <td class="muted">{formatSize(v.size_bytes)}</td>
          <td class="actions">
            <a class="btn ghost sm" href="/api/files/{versionsFile?.id}/download?version={v.version}">Скачать</a>
            {#if i > 0 && canEdit}<button class="btn ghost sm" onclick={() => restore(v.version)}>Восстановить</button>{/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</Modal>

{#if shareTarget.id}
  <ShareDialog bind:open={shareOpen} resourceType={shareTarget.type} resourceId={shareTarget.id} title={shareTarget.name} />
{/if}

<style>
  .crumbs { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; min-width: 0; }
  .tools { flex-wrap: wrap; }
  .dot { width: 9px; height: 9px; border-radius: 2px; display: inline-block; }
  .dot.word { background: var(--word); }
  .dot.cell { background: var(--cell); }
  .dot.slide { background: var(--slide); }
  .card { overflow-x: auto; transition: outline-color .1s; outline: 2px dashed transparent; outline-offset: -6px; }
  .card.drag { outline-color: var(--accent); background: var(--accent-soft); }
  .name { display: inline-flex; align-items: center; gap: 10px; color: var(--text); }
  .linkbtn { border: 0; background: none; padding: 0; font: inherit; cursor: pointer; text-align: left; }
  .linkbtn:hover { color: var(--accent); }
  .crumb { border: 0; background: none; padding: 0; font: inherit; color: var(--accent); cursor: pointer; }
  .up { display: inline-flex; align-items: center; gap: 4px; padding-left: 8px; padding-right: 10px; text-decoration: none; }
  .up:hover { text-decoration: none; }
  .up-wrap { display: inline-flex; border-radius: var(--radius); }
  .up-wrap.target { outline: 2px dashed var(--accent); outline-offset: 2px; }
  .drop-crumb { border-radius: var(--radius); padding: 2px 4px; margin: -2px -4px; }
  .drop-crumb.target { background: var(--accent-soft); outline: 2px dashed var(--accent); }
  tr[draggable='true'] { cursor: grab; }
  tr.moving { opacity: .45; }
  tr.target td { background: var(--accent-soft); }
  tr.target td:first-child { box-shadow: inset 3px 0 0 var(--accent); }
  .dnd-hint { margin: 8px 2px 0; }
  .embedded .page-head { margin-bottom: 12px; }
  .actions { text-align: right; white-space: nowrap; }
  .editing { margin-left: 8px; background: var(--warn-soft); color: var(--warn); }
  .empty { padding: 48px 16px; text-align: center; }
  .empty p { margin: 4px 0; }
  .badge { margin-bottom: 10px; }
  @media (max-width: 720px) { .hide-sm { display: none; } }
</style>
