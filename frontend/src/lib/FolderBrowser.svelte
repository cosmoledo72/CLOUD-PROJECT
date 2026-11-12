<script lang="ts">
  import EmptyState from '$lib/EmptyState.svelte';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import FolderPlus from '@lucide/svelte/icons/folder-plus';
  import ArrowUpToLine from '@lucide/svelte/icons/arrow-up-to-line';
  import Upload from '@lucide/svelte/icons/upload';
  import Plus from '@lucide/svelte/icons/plus';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Download from '@lucide/svelte/icons/download';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import Link2 from '@lucide/svelte/icons/link-2';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import Settings from '@lucide/svelte/icons/settings';
  import X from '@lucide/svelte/icons/x';
  import ArrowDownUp from '@lucide/svelte/icons/arrow-down-up';
  import History from '@lucide/svelte/icons/history';
  import FolderInput from '@lucide/svelte/icons/folder-input';
  import Copy from '@lucide/svelte/icons/copy';
  import Users from '@lucide/svelte/icons/users';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import Search from '@lucide/svelte/icons/search';
  import { api, can, docKind, folderName, formatDateTime, type FileItem, type FolderItem, type FolderView, type Version } from '$lib/api';
  import FileIcon from '$lib/FileIcon.svelte';
  import Modal from '$lib/Modal.svelte';
  import ShareDialog from '$lib/ShareDialog.svelte';
  import Author from '$lib/files/Author.svelte';
  import FolderPicker from '$lib/files/FolderPicker.svelte';
  import { downloadZip, itemsLabel, plural, portalUrl, relTime, sizeLabel } from '$lib/files/fmt';

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
  let usage = $state<{ used: number; quota: number } | null>(null);

  async function load() {
    error = '';
    try {
      view = await api.get<FolderView>(`/folders/${id}`);
      onview?.(view);
      // Selection is per folder; keep only items still here.
      const here = new Set([...view.folders.map((f) => `d:${f.id}`), ...view.files.map((f) => `f:${f.id}`)]);
      selected = new Set([...selected].filter((k) => here.has(k)));
    } catch (e) {
      error = (e as Error).message;
      view = null;
    }
    if (!embedded) api.get<typeof usage>('/storage/usage').then((u) => (usage = u)).catch(() => {});
  }

  $effect(() => {
    id;
    selected = new Set();
    filter = '';
    load();
  });

  const canEdit = $derived(can(view?.access, 'Edit'));
  const canFull = $derived(can(view?.access, 'Full'));

  // --- Notices ---------------------------------------------------------------
  let notice = $state('');
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  function tell(text: string) {
    notice = text;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = ''), 2600);
  }

  // --- Columns and sorting ---------------------------------------------------------
  type Col = 'size' | 'date' | 'author';
  const COLS: { k: Col; label: string }[] = [
    { k: 'size', label: 'Размер' },
    { k: 'date', label: 'Изменён' },
    { k: 'author', label: 'Автор' }
  ];
  function stored<T>(k: string, d: T): T {
    try {
      const v = localStorage.getItem(k);
      return v ? { ...d, ...(JSON.parse(v) as T) } : d;
    } catch {
      return d;
    }
  }
  let cols = $state(stored<Record<Col, boolean>>('wh_file_cols', { size: true, date: true, author: true }));
  let colsOpen = $state(false);
  let colsPos = $state({ x: 0, y: 0 });
  function openCols(e: MouseEvent) {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    colsPos = { x: Math.max(8, r.right - 200), y: r.bottom + 6 };
    colsOpen = !colsOpen;
  }

  // Quick filter by name inside the open folder.
  let filter = $state('');
  const needle = $derived(filter.trim().toLowerCase());
  const match = (name: string) => !needle || name.toLowerCase().includes(needle);
  function toggleCol(k: Col) {
    cols[k] = !cols[k];
    try { localStorage.setItem('wh_file_cols', JSON.stringify(cols)); } catch { /* storage unavailable */ }
  }
  type SortKey = 'name' | 'size' | 'date';
  let sort = $state<{ key: SortKey; desc: boolean }>(stored('wh_file_sort', { key: 'name' as SortKey, desc: false }));
  function sortBy(key: SortKey) {
    sort = { key, desc: sort.key === key ? !sort.desc : key !== 'name' };
    try { localStorage.setItem('wh_file_sort', JSON.stringify(sort)); } catch { /* storage unavailable */ }
  }
  const cmpName = (a: string, b: string) => a.localeCompare(b, 'ru', { numeric: true, sensitivity: 'base' });
  const folders = $derived.by(() => {
    const list = (view?.folders ?? []).filter((f) => match(f.name));
    const k = sort.key;
    list.sort((a, b) => (k === 'size' ? a.items - b.items : k === 'date' ? a.created_at.localeCompare(b.created_at) : cmpName(a.name, b.name)) * (sort.desc ? -1 : 1));
    return list;
  });
  const files = $derived.by(() => {
    const list = (view?.files ?? []).filter((f) => match(f.name));
    const k = sort.key;
    list.sort((a, b) => (k === 'size' ? a.size_bytes - b.size_bytes : k === 'date' ? a.updated_at.localeCompare(b.updated_at) : cmpName(a.name, b.name)) * (sort.desc ? -1 : 1));
    return list;
  });

  // --- Create -------------------------------------------------------------
  let newFolderOpen = $state(false);
  let newFolderName = $state('');
  async function createFolder() {
    await run(() => api.post('/folders', { parent_id: id, name: newFolderName }));
    newFolderOpen = false;
    newFolderName = '';
  }

  let createMenu = $state(false);
  let newDocOpen = $state(false);
  let newDocKind = $state<'docx' | 'xlsx' | 'pptx'>('docx');
  let newDocName = $state('');
  function openNewDoc(kind: 'docx' | 'xlsx' | 'pptx') {
    createMenu = false;
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
        error = (e as { status?: number }).status === 413 ? `«${f.name}»: место в хранилище портала закончилось` : (e as Error).message;
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
  // Files and folders can be dragged onto a folder row, the «up» row or a
  // breadcrumb; files from the computer dropped on a folder row are uploaded
  // straight into it.
  type DragItem = { type: 'folder' | 'file'; id: string; name: string };
  const DND = 'application/x-workhub-item';
  let dragging = $state<DragItem | null>(null);
  let dropTarget = $state<string | null>(null);

  function startDrag(e: DragEvent, item: DragItem) {
    if (!canFull || !e.dataTransfer || renaming) return;
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
    // Dragging a selected item moves the whole selection.
    const key = `${item.type === 'file' ? 'f' : 'd'}:${item.id}`;
    const items = selected.has(key) ? selectedItems() : [item];
    const errs = await moveItems(items.filter((i) => i.id !== folderId), folderId, folderLabel);
    if (errs.length) error = errs.join(' ');
    await load();
  }

  async function moveItems(items: DragItem[], target: string, label: string): Promise<string[]> {
    const errs: string[] = [];
    for (const item of items) {
      try {
        if (item.type === 'file') await api.patch(`/files/${item.id}`, { folder_id: target });
        else await api.patch(`/folders/${item.id}`, { parent_id: target });
      } catch (err) {
        const status = (err as { status?: number }).status;
        errs.push(status === 409
          ? `В папке «${label}» уже есть ${item.type === 'file' ? 'файл' : 'папка'} «${item.name}».`
          : status === 403 ? `Нет прав на перемещение «${item.name}».`
          : status === 400 && item.type === 'folder' ? `Папку «${item.name}» нельзя переместить в неё саму.`
          : `«${item.name}»: ${(err as Error).message}`);
      }
    }
    return errs;
  }

  // --- Selection -------------------------------------------------------------
  let selected = $state<Set<string>>(new Set());
  const total = $derived((view?.folders.length ?? 0) + (view?.files.length ?? 0));
  const allOn = $derived(total > 0 && selected.size === total);
  function toggle(key: string) {
    const s = new Set(selected);
    if (s.has(key)) s.delete(key);
    else s.add(key);
    selected = s;
  }
  function toggleAll() {
    selected = allOn ? new Set() : new Set([...(view?.folders ?? []).map((f) => `d:${f.id}`), ...(view?.files ?? []).map((f) => `f:${f.id}`)]);
  }
  function selectedItems(): DragItem[] {
    return [
      ...(view?.folders ?? []).filter((f) => selected.has(`d:${f.id}`)).map((f) => ({ type: 'folder' as const, id: f.id, name: f.name })),
      ...(view?.files ?? []).filter((f) => selected.has(`f:${f.id}`)).map((f) => ({ type: 'file' as const, id: f.id, name: f.name }))
    ];
  }
  const selSize = $derived((view?.files ?? []).filter((f) => selected.has(`f:${f.id}`)).reduce((s, f) => s + f.size_bytes, 0));
  const selFolders = $derived((view?.folders ?? []).filter((f) => selected.has(`d:${f.id}`)).length);

  let zipping = $state(false);
  async function zip(items: DragItem[]) {
    zipping = true;
    error = '';
    try {
      await downloadZip(items.filter((i) => i.type === 'file').map((i) => i.id), items.filter((i) => i.type === 'folder').map((i) => i.id));
    } catch (e) {
      error = (e as Error).message;
    } finally {
      zipping = false;
    }
  }
  async function removeItems(items: DragItem[]) {
    const what = items.length === 1 ? `«${items[0].name}»` : count(items.length);
    if (!confirm(`Удалить ${what}? Удалённое можно восстановить из корзины.`)) return;
    error = '';
    const errs: string[] = [];
    for (const i of items) {
      try {
        await api.del(`/${i.type}s/${i.id}`);
      } catch (e) {
        errs.push(`«${i.name}»: ${(e as Error).message}`);
      }
    }
    if (errs.length) error = errs.join(' ');
    else tell(items.length === 1 ? 'Удалено в корзину' : `Удалено в корзину: ${items.length}`);
    await load();
  }

  // Move / copy through the folder picker.
  let pickerOpen = $state(false);
  let pickerMode = $state<'move' | 'copy'>('move');
  let pickerItems = $state<DragItem[]>([]);
  function openPicker(mode: 'move' | 'copy', items: DragItem[]) {
    menu = null;
    pickerMode = mode;
    pickerItems = items;
    pickerOpen = true;
  }
  async function picked(target: string) {
    if (pickerMode === 'move') {
      const errs = await moveItems(pickerItems, target, 'назначения');
      if (errs.length) throw new Error(errs.join(' '));
      tell(pickerItems.length === 1 ? 'Перемещено' : `Перемещено: ${pickerItems.length}`);
    } else {
      await api.post('/files/copy', {
        files: pickerItems.filter((i) => i.type === 'file').map((i) => i.id),
        folders: pickerItems.filter((i) => i.type === 'folder').map((i) => i.id),
        target
      });
      tell(pickerItems.length === 1 ? 'Скопировано' : `Скопировано: ${pickerItems.length}`);
    }
    selected = new Set();
    await load();
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

  // Inline rename: the name turns into a field; Enter or leaving it saves, Esc cancels.
  let renaming = $state<string | null>(null);
  let renameValue = $state('');
  function startRename(item: DragItem) {
    menu = null;
    renaming = `${item.type}:${item.id}`;
    renameValue = item.name;
  }
  function focusName(el: HTMLInputElement) {
    el.focus();
    // Files: select the name without the extension.
    const dot = el.value.lastIndexOf('.');
    el.setSelectionRange(0, renaming?.startsWith('file:') && dot > 0 ? dot : el.value.length);
  }
  async function saveRename(item: DragItem) {
    if (renaming !== `${item.type}:${item.id}`) return;
    renaming = null;
    const name = renameValue.trim();
    if (!name || name === item.name) return;
    error = '';
    try {
      await api.patch(`/${item.type}s/${item.id}`, { name });
      await load();
    } catch (e) {
      error = (e as { status?: number }).status === 409 ? `Имя «${name}» здесь уже занято.` : (e as Error).message;
    }
  }
  function renameKey(e: KeyboardEvent, item: DragItem) {
    if (e.key === 'Enter') {
      e.preventDefault();
      saveRename(item);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      renaming = null;
    }
  }

  async function copyLink(item: DragItem, file?: FileItem) {
    menu = null;
    const path = item.type === 'folder'
      ? (embedded ? `/files/${item.id}` : `/files/${item.id}`)
      : file && openable(file) ? `/editor/${item.id}` : `/api/files/${item.id}/download`;
    try {
      await navigator.clipboard.writeText(portalUrl(path));
      tell('Ссылка скопирована — откроется у сотрудников с доступом');
    } catch {
      prompt('Ссылка для сотрудников портала:', portalUrl(path));
    }
  }

  let shareOpen = $state(false);
  let shareTarget = $state<{ type: 'folder' | 'file'; id: string; name: string }>({ type: 'folder', id: '', name: '' });
  function share(type: 'folder' | 'file', itemId: string, name: string) {
    menu = null;
    shareTarget = { type, id: itemId, name };
    shareOpen = true;
  }

  let versionsOpen = $state(false);
  let versionsFile = $state<FileItem | null>(null);
  let versions = $state<Version[]>([]);
  async function showVersions(f: FileItem) {
    menu = null;
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

  // --- Row menu «⋯» ------------------------------------------------------------
  type Menu = { key: string; x: number; y: number; file?: FileItem; folder?: FolderItem };
  let menu = $state<Menu | null>(null);
  function openMenu(e: MouseEvent, m: Omit<Menu, 'x' | 'y'>) {
    e.stopPropagation();
    if (menu?.key === m.key) {
      menu = null;
      return;
    }
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const h = 300;
    menu = { ...m, x: Math.max(8, Math.min(r.right - 230, window.innerWidth - 238)), y: r.bottom + h > window.innerHeight ? Math.max(8, r.top - h) : r.bottom + 4 };
  }
  $effect(() => {
    if (!menu && !colsOpen && !createMenu) return;
    const close = (e: Event) => {
      if (e instanceof KeyboardEvent && e.key !== 'Escape') return;
      if (e instanceof PointerEvent && (e.target as HTMLElement).closest('.pop')) return;
      menu = null;
      colsOpen = false;
      createMenu = false;
    };
    const t = setTimeout(() => {
      document.addEventListener('pointerdown', close);
      document.addEventListener('keydown', close);
      window.addEventListener('scroll', close, true);
    });
    return () => {
      clearTimeout(t);
      document.removeEventListener('pointerdown', close);
      document.removeEventListener('keydown', close);
      window.removeEventListener('scroll', close, true);
    };
  });

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
  const count = (n: number) => `${n} ${plural(n, 'объект', 'объекта', 'объектов')}`;
  const pickerWhat = $derived(pickerItems.length === 1 ? pickerItems[0].name : count(pickerItems.length));
  const fileItem = (f: FileItem): DragItem => ({ type: 'file', id: f.id, name: f.name });
  const folderItem = (f: FolderItem): DragItem => ({ type: 'folder', id: f.id, name: f.name });
  const usedPct = $derived(usage && usage.quota ? Math.min(100, (usage.used / usage.quota) * 100) : 0);
  const visibleCols = $derived(COLS.filter((c) => cols[c.k]));
  const span = $derived(3 + visibleCols.length);
</script>

{#snippet sortHead(key: SortKey, label: string)}
  <button class="sorth" class:on={sort.key === key} onclick={() => sortBy(key)}>
    {label}{#if sort.key === key}<span class="arr">{sort.desc ? '↓' : '↑'}</span>{/if}
  </button>
{/snippet}

{#snippet nameCell(item: DragItem, icon: { ext?: string; folder?: boolean }, href: string | null, extra?: import('svelte').Snippet)}
  {#if renaming === `${item.type}:${item.id}`}
    <span class="name">
      {#if icon.folder}<FileIcon folder />{:else}<FileIcon ext={icon.ext ?? ''} />{/if}
      <input class="input rename" bind:value={renameValue} use:focusName onkeydown={(e) => renameKey(e, item)} onblur={() => saveRename(item)}
             aria-label="Новое имя" maxlength="255" />
    </span>
  {:else if href === null}
    <button class="name linkbtn" onclick={() => go(item.id)}><FileIcon folder /><span class="nm">{item.name}</span></button>{@render extra?.()}
  {:else}
    <a class="name" {href} target={icon.folder ? undefined : '_blank'}>
      {#if icon.folder}<FileIcon folder />{:else}<FileIcon ext={icon.ext ?? ''} />{/if}<span class="nm">{item.name}</span>
    </a>{@render extra?.()}
  {/if}
{/snippet}

<div
  class="browser"
  class:embedded
  role="region"
  aria-label="Папка"
  ondragover={(e) => { e.preventDefault(); if (canEdit && !dragging) dragOver = true; }}
  ondragleave={() => (dragOver = false)}
  ondrop={onDrop}
>
  {#if view}
    <div class="page-head">
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
      {#if usage && !embedded}
        <div class="usage" title="Занято в хранилище портала (все версии файлов)">
          <span>Занято <b>{sizeLabel(usage.used)}</b> из {sizeLabel(usage.quota)}</span>
          <span class="bar" class:warn={usedPct > 85}><i style="width: {Math.max(usedPct, 1.5)}%"></i></span>
        </div>
      {/if}
      <div class="row tools">
        <label class="fsearch" title="Найти в этой папке по названию">
          <Search size={16} />
          <input type="search" bind:value={filter} placeholder="Поиск в папке" aria-label="Поиск в папке"
                 onkeydown={(e) => { if (e.key === 'Escape') filter = ''; }} />
        </label>
        {#if canFull && view.folder.parent_id}
          <button class="btn" onclick={() => share('folder', view!.folder.id, view!.folder.name)}><Users size={16} />Доступ</button>
        {/if}
        {#if canEdit}
          <button class="btn" onclick={() => (newFolderOpen = true)}><FolderPlus size={16} />Создать папку</button>
          <span class="create-wrap">
            <button class="btn" onclick={() => (createMenu = !createMenu)} aria-haspopup="menu" aria-expanded={createMenu}><Plus size={16} />Создать<ChevronDown size={14} /></button>
            {#if createMenu}
              <div class="pop create" role="menu">
                <button role="menuitem" onclick={() => openNewDoc('docx')}><FileIcon ext="docx" />Документ Word</button>
                <button role="menuitem" onclick={() => openNewDoc('xlsx')}><FileIcon ext="xlsx" />Таблицу Excel</button>
                <button role="menuitem" onclick={() => openNewDoc('pptx')}><FileIcon ext="pptx" />Презентацию</button>
              </div>
            {/if}
          </span>
          <button class="btn primary" onclick={() => fileInput?.click()}><Upload size={16} />Загрузить</button>
          <input type="file" multiple hidden bind:this={fileInput} onchange={(e) => { const t = e.currentTarget; if (t.files) uploadFiles(t.files); t.value = ''; }} />
        {/if}
      </div>
    </div>

    {#if error}<div class="error-box">{error}</div>{/if}
    {#if uploading > 0}<div class="badge up-badge">Загрузка: {uploading}…</div>{/if}

    {#if selected.size}
      <div class="selbar" role="toolbar" aria-label="Выбранные">
        <b>Выбрано: {selected.size}</b>
        <span class="selsub">{selSize ? sizeLabel(selSize) : ''}{selFolders ? `${selSize ? ' · ' : ''}папок: ${selFolders}` : ''}</span>
        <div class="spacer"></div>
        <button class="btn" disabled={zipping} onclick={() => zip(selectedItems())}><Download size={15} />{zipping ? 'Собираю архив…' : 'Скачать ZIP'}</button>
        {#if canFull}<button class="btn" onclick={() => openPicker('move', selectedItems())}><FolderInput size={15} />Переместить…</button>{/if}
        <button class="btn" onclick={() => openPicker('copy', selectedItems())}><Copy size={15} />Копировать…</button>
        {#if canFull}<button class="btn" onclick={() => removeItems(selectedItems())}><Trash2 size={15} />Удалить</button>{/if}
        <button class="btn icon" onclick={() => (selected = new Set())} aria-label="Снять выделение" title="Снять выделение"><X size={16} /></button>
      </div>
    {/if}

    <div class="card list-card" class:drag={dragOver}>
      <table class="files">
        <thead>
          <tr>
            <th class="cb"><input type="checkbox" checked={allOn} indeterminate={selected.size > 0 && !allOn} disabled={!total} onchange={toggleAll} aria-label="Выбрать все" /></th>
            <th>{@render sortHead('name', 'Название')}</th>
            {#if cols.size}<th class="c-size hide-sm">{@render sortHead('size', 'Размер')}</th>{/if}
            {#if cols.date}<th class="c-date hide-sm">{@render sortHead('date', 'Изменён')}</th>{/if}
            {#if cols.author}<th class="c-author hide-sm">Автор</th>{/if}
            <th class="c-act">
              <button class="ibtn" onclick={openCols} aria-label="Столбцы" title="Какие столбцы показывать" aria-expanded={colsOpen}><Settings size={16} /></button>
            </th>
          </tr>
        </thead>
        <tbody>
          {#if parent}
            <tr class="up" class:target={dropTarget === parent.id}
                ondragover={(e) => overTarget(e, parent!.id)} ondragleave={() => leaveTarget(parent!.id)}
                ondrop={(e) => dropOn(e, parent!.id, folderName(parent!.name))}>
              <td></td>
              <td colspan={span - 1}>
                {#if embedded}
                  <button class="uplink" onclick={() => go(parent!.id)} title="Выше: {folderName(parent.name)}"><ArrowUpToLine size={16} />На уровень вверх</button>
                {:else}
                  <a class="uplink" href="/files/{parent.id}" title="Выше: {folderName(parent.name)}"><ArrowUpToLine size={16} />На уровень вверх</a>
                {/if}
              </td>
            </tr>
          {/if}
          {#each folders as f (f.id)}
            {@const it = folderItem(f)}
            <tr draggable={canFull && renaming !== `folder:${f.id}`} class:target={dropTarget === f.id} class:moving={dragging?.id === f.id} class:sel={selected.has(`d:${f.id}`)}
                class:menu-on={menu?.key === `d:${f.id}`}
                ondragstart={(e) => startDrag(e, it)} ondragend={endDrag}
                ondragover={(e) => overTarget(e, f.id)} ondragleave={() => leaveTarget(f.id)} ondrop={(e) => dropOn(e, f.id, f.name)}>
              <td class="cb"><input type="checkbox" checked={selected.has(`d:${f.id}`)} onchange={() => toggle(`d:${f.id}`)} aria-label="Выбрать {f.name}" /></td>
              <td class="c-name">
                {@render nameCell(it, { folder: true }, embedded ? null : `/files/${f.id}`)}
                <span class="sub show-sm">{itemsLabel(f.items)} · {relTime(f.created_at)}</span>
              </td>
              {#if cols.size}<td class="c-size hide-sm muted">{itemsLabel(f.items)}</td>{/if}
              {#if cols.date}<td class="c-date hide-sm muted" title={formatDateTime(f.created_at)}>{relTime(f.created_at)}</td>{/if}
              {#if cols.author}<td class="c-author hide-sm"><Author id={f.owner_id} name={f.owner_name} /></td>{/if}
              <td class="c-act">
                <div class="acts">
                  <button class="ibtn wide" onclick={() => zip([it])} title="Скачать архивом" aria-label="Скачать {f.name} архивом"><Download size={17} /></button>
                  {#if canFull}
                    <button class="ibtn wide" onclick={() => startRename(it)} title="Переименовать" aria-label="Переименовать {f.name}"><Pencil size={17} /></button>
                    <button class="ibtn wide" onclick={() => removeItems([it])} title="Удалить" aria-label="Удалить {f.name}"><Trash2 size={17} /></button>
                  {/if}
                  <button class="ibtn wide" onclick={() => copyLink(it)} title="Скопировать ссылку" aria-label="Ссылка на {f.name}"><Link2 size={17} /></button>
                  <button class="ibtn more" onclick={(e) => openMenu(e, { key: `d:${f.id}`, folder: f })} title="Ещё" aria-label="Ещё: {f.name}" aria-haspopup="menu"><Ellipsis size={17} /></button>
                </div>
              </td>
            </tr>
          {/each}
          {#each files as f (f.id)}
            {@const it = fileItem(f)}
            <tr draggable={canFull && renaming !== `file:${f.id}`} class:moving={dragging?.id === f.id} class:sel={selected.has(`f:${f.id}`)} class:menu-on={menu?.key === `f:${f.id}`}
                ondragstart={(e) => startDrag(e, it)} ondragend={endDrag}>
              <td class="cb"><input type="checkbox" checked={selected.has(`f:${f.id}`)} onchange={() => toggle(`f:${f.id}`)} aria-label="Выбрать {f.name}" /></td>
              <td class="c-name">
                {#snippet badges()}
                  {#if f.current_version > 1}<button class="ver" onclick={() => showVersions(f)} title="Версии файла">v{f.current_version}</button>{/if}
                  {#if f.is_editing}<span class="badge editing">редактируется</span>{/if}
                {/snippet}
                {@render nameCell(it, { ext: f.extension }, openable(f) ? `/editor/${f.id}` : `/api/files/${f.id}/download`, badges)}
                <span class="sub show-sm">{sizeLabel(f.size_bytes)} · {relTime(f.updated_at)}</span>
              </td>
              {#if cols.size}<td class="c-size hide-sm muted">{sizeLabel(f.size_bytes)}</td>{/if}
              {#if cols.date}<td class="c-date hide-sm muted" title={formatDateTime(f.updated_at)}>{relTime(f.updated_at)}</td>{/if}
              {#if cols.author}<td class="c-author hide-sm"><Author id={f.owner_id} name={f.owner_name} /></td>{/if}
              <td class="c-act">
                <div class="acts">
                  <a class="ibtn wide" href="/api/files/{f.id}/download" title="Скачать" aria-label="Скачать {f.name}"><Download size={17} /></a>
                  {#if canFull}
                    <button class="ibtn wide" onclick={() => startRename(it)} title="Переименовать" aria-label="Переименовать {f.name}"><Pencil size={17} /></button>
                    <button class="ibtn wide" onclick={() => removeItems([it])} title="Удалить" aria-label="Удалить {f.name}"><Trash2 size={17} /></button>
                  {/if}
                  <button class="ibtn wide" onclick={() => copyLink(it, f)} title="Скопировать ссылку" aria-label="Ссылка на {f.name}"><Link2 size={17} /></button>
                  <button class="ibtn more" onclick={(e) => openMenu(e, { key: `f:${f.id}`, file: f })} title="Ещё" aria-label="Ещё: {f.name}" aria-haspopup="menu"><Ellipsis size={17} /></button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      {#if total > 0 && needle && !folders.length && !files.length}
        <p class="nofind">В этой папке нет «{filter.trim()}». <a href="/search?q={encodeURIComponent(filter.trim())}&type=files">Искать во всех файлах портала</a> — и по тексту внутри документов.</p>
      {/if}
      {#if total === 0}
        <EmptyState icon={FolderOpen} title="Папка пуста" text={canEdit ? 'Перетащите файлы сюда или нажмите «Загрузить».' : ''} />
      {/if}
    </div>
    {#if canFull && total > 1}
      <p class="faint small dnd-hint">Перетащите файл или папку на другую папку, на строку «На уровень вверх» или на путь вверху, чтобы переместить. Отмеченные галочками перемещаются вместе.</p>
    {/if}
  {:else if error}
    <div class="error-box">{error}</div>
  {/if}
</div>

{#if menu}
  {@const m = menu}
  {@const it = m.file ? fileItem(m.file) : folderItem(m.folder!)}
  <div class="pop rowmenu" role="menu" style="left: {m.x}px; top: {m.y}px">
    <!-- On phones the row icons are hidden, so their actions are here too. -->
    {#if m.file}
      {#if openable(m.file)}<a class="mi" role="menuitem" href="/editor/{m.file.id}" target="_blank" onclick={() => (menu = null)}><ExternalLink size={16} />Открыть в редакторе</a>{/if}
      <a class="mi touch" role="menuitem" href="/api/files/{m.file.id}/download" onclick={() => (menu = null)}><Download size={16} />Скачать</a>
      <button class="mi" role="menuitem" onclick={() => showVersions(m.file!)}><History size={16} />Версии <span class="faint">v{m.file.current_version}</span></button>
    {:else}
      <button class="mi touch" role="menuitem" onclick={() => { menu = null; zip([it]); }}><Download size={16} />Скачать архивом</button>
    {/if}
    {#if canFull}<button class="mi" role="menuitem" onclick={() => openPicker('move', [it])}><FolderInput size={16} />Переместить…</button>{/if}
    <button class="mi" role="menuitem" onclick={() => openPicker('copy', [it])}><Copy size={16} />Копировать…</button>
    <button class="mi touch" role="menuitem" onclick={() => copyLink(it, m.file)}><Link2 size={16} />Скопировать ссылку</button>
    {#if canFull}
      <button class="mi" role="menuitem" onclick={() => share(it.type, it.id, it.name)}><Users size={16} />Доступ…</button>
      <button class="mi touch" role="menuitem" onclick={() => startRename(it)}><Pencil size={16} />Переименовать</button>
      <button class="mi danger touch" role="menuitem" onclick={() => { menu = null; removeItems([it]); }}><Trash2 size={16} />Удалить</button>
    {/if}
  </div>
{/if}

{#if colsOpen}
  <div class="pop cols" role="menu" style="left: {colsPos.x}px; top: {colsPos.y}px">
    <div class="ph">Столбцы</div>
    {#each COLS as c (c.k)}
      <label class="check"><input type="checkbox" checked={cols[c.k]} onchange={() => toggleCol(c.k)} />{c.label}</label>
    {/each}
  </div>
{/if}

{#if notice}<div class="notice" role="status">{notice}</div>{/if}

<Modal bind:open={newFolderOpen} title="Новая папка">
  <!-- svelte-ignore a11y_autofocus -->
  <input class="input" bind:value={newFolderName} placeholder="Название" autofocus onkeydown={(e) => e.key === 'Enter' && newFolderName.trim() && createFolder()} />
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

<Modal bind:open={versionsOpen} title={`Версии: ${versionsFile?.name ?? ''}`} width={600}>
  <table class="list">
    <thead><tr><th>Версия</th><th>Автор</th><th>Дата</th><th>Размер</th><th></th></tr></thead>
    <tbody>
      {#each versions as v, i}
        <tr>
          <td>v{v.version}{#if i === 0} <span class="badge">текущая</span>{/if}</td>
          <td>{v.author_name ?? '—'}</td>
          <td class="muted">{formatDateTime(v.created_at)}</td>
          <td class="muted">{sizeLabel(v.size_bytes)}</td>
          <td class="actions">
            <a class="btn ghost sm" href="/api/files/{versionsFile?.id}/download?version={v.version}">Скачать</a>
            {#if i > 0 && canEdit}<button class="btn ghost sm" onclick={() => restore(v.version)}>Восстановить</button>{/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</Modal>

<FolderPicker bind:open={pickerOpen} title={`${pickerMode === 'move' ? 'Переместить' : 'Копировать'}: ${pickerWhat}`}
              action={pickerMode === 'move' ? 'Переместить сюда' : 'Копировать сюда'} start={id}
              exclude={pickerMode === 'move' ? pickerItems.filter((i) => i.type === 'folder').map((i) => i.id) : []} onpick={picked} />

{#if shareTarget.id}
  <ShareDialog bind:open={shareOpen} resourceType={shareTarget.type} resourceId={shareTarget.id} title={shareTarget.name} />
{/if}

<style>
  .crumbs { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; min-width: 0; }
  .tools { flex-wrap: wrap; gap: 8px; }
  .tools .btn { display: inline-flex; align-items: center; gap: 7px; }
  .usage { display: flex; align-items: center; gap: 10px; font-size: 13px; color: var(--text-3); white-space: nowrap; margin-right: 6px; }
  .usage b { color: var(--text); font-weight: 600; }
  .bar { width: 120px; height: 6px; border-radius: 3px; background: var(--surface-3); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--accent); border-radius: 3px; }
  .bar.warn i { background: var(--warn, #b54708); }
  .create-wrap { position: relative; display: inline-flex; }
  .fsearch { display: inline-flex; align-items: center; gap: 8px; height: 36px; padding: 0 10px; border: 1px solid var(--border); border-radius: var(--radius);
             background: var(--surface); color: var(--text-3); width: 220px; }
  .fsearch:focus-within { border-color: var(--accent); }
  .fsearch input { flex: 1; min-width: 0; border: 0; outline: 0; background: none; font: inherit; font-size: 14px; color: var(--text); }

  .list-card { overflow-x: auto; transition: outline-color .1s; outline: 2px dashed transparent; outline-offset: -6px; padding: 0; }
  .list-card.drag { outline-color: var(--accent); background: var(--accent-soft); }
  table.files { width: 100%; border-collapse: collapse; table-layout: auto; }
  .files th { text-align: left; font-weight: 500; font-size: 13px; color: var(--text-3); padding: 10px 12px; border-bottom: 1px solid var(--border); background: var(--surface-2); white-space: nowrap; }
  .files td { padding: 8px 12px; border-bottom: 1px solid var(--border); vertical-align: middle; }
  .files tbody tr:last-child td { border-bottom: 0; }
  .files tbody tr:hover td, .files tr.menu-on td { background: var(--surface-2); }
  .files tr.sel td { background: var(--accent-soft); }
  .cb { width: 44px; padding-right: 0 !important; }
  .cb input { width: 16px; height: 16px; margin: 0; vertical-align: middle; accent-color: var(--accent); }
  .c-size { width: 120px; white-space: nowrap; }
  .c-date { width: 150px; white-space: nowrap; }
  .c-author { width: 220px; max-width: 240px; }
  .c-act, .files th.c-act { width: 1%; text-align: right; white-space: nowrap; }
  .c-name { min-width: 220px; }
  .sorth { border: 0; background: none; padding: 0; font: inherit; color: inherit; cursor: pointer; display: inline-flex; gap: 4px; }
  .sorth:hover, .sorth.on { color: var(--text); }
  .arr { font-size: 12px; }
  .name { display: inline-flex; align-items: center; gap: 10px; color: var(--text); font-weight: 500; min-width: 0; max-width: 100%; vertical-align: middle; }
  .name:hover { color: var(--accent); text-decoration: none; }
  .nm { overflow-wrap: anywhere; }
  .linkbtn { border: 0; background: none; padding: 0; font: inherit; cursor: pointer; text-align: left; font-weight: 500; }
  .rename { height: 32px; min-width: 260px; font-weight: 500; }
  .ver { margin-left: 8px; font: 11.5px/1.6 inherit; color: var(--text-3); border: 1px solid var(--border); border-radius: 8px; padding: 0 6px; background: none; cursor: pointer; vertical-align: middle; }
  .ver:hover { color: var(--accent); border-color: var(--accent); }
  .editing { margin-left: 8px; background: var(--warn-soft); color: var(--warn); }
  .sub { display: none; font-size: 12px; color: var(--text-3); margin: 2px 0 0 36px; }

  /* Row icons are always shown; on touch screens only «⋯» (see below). */
  .acts { display: inline-flex; gap: 2px; justify-content: flex-end; }
  /* The action column stays pinned at the right edge when the table scrolls sideways. */
  .files td.c-act, .files th.c-act { position: sticky; right: 0; z-index: 1; background: var(--surface); box-shadow: -8px 0 8px -8px rgb(16 24 40 / 12%); }
  .files th.c-act { background: var(--surface-2); }
  .ibtn { display: inline-grid; place-items: center; width: 32px; height: 32px; border: 0; border-radius: 8px; background: none; color: var(--text-3); cursor: pointer; }
  .ibtn:hover { background: var(--surface-3); color: var(--text); text-decoration: none; }
  .ibtn:focus-visible { opacity: 1; outline: 2px solid var(--accent); outline-offset: -2px; }

  .up td { padding-top: 6px; padding-bottom: 6px; }
  .uplink { display: inline-flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text-3); border: 0; background: none; padding: 2px 0; font-family: inherit; cursor: pointer; }
  .uplink:hover { color: var(--accent); text-decoration: none; }
  .crumb { border: 0; background: none; padding: 0; font: inherit; color: var(--accent); cursor: pointer; }
  .drop-crumb { border-radius: var(--radius); padding: 2px 4px; margin: -2px -4px; }
  .drop-crumb.target { background: var(--accent-soft); outline: 2px dashed var(--accent); }
  tr[draggable='true'] { cursor: grab; }
  tr.moving { opacity: .45; }
  tr.target td { background: var(--accent-soft) !important; }
  tr.target td:first-child { box-shadow: inset 3px 0 0 var(--accent); }

  .selbar { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 8px 10px 8px 14px; margin-bottom: 10px; border-radius: 10px; background: var(--accent); color: var(--on-accent, #fff); }
  .selsub { opacity: .8; font-size: 13px; }
  .selbar .btn { height: 32px; display: inline-flex; align-items: center; gap: 6px; background: transparent; color: inherit; border-color: color-mix(in srgb, currentColor 35%, transparent); }
  .selbar .btn:hover:not(:disabled) { background: color-mix(in srgb, currentColor 12%, transparent); }
  .selbar .icon { width: 32px; padding: 0; justify-content: center; }

  .pop { position: absolute; z-index: 40; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; box-shadow: var(--shadow-lg); padding: 6px; text-align: left; color: var(--text); font-size: 13.5px; font-weight: 400; }
  .pop.create { top: calc(100% + 6px); right: 0; width: 220px; }
  .pop.cols { position: fixed; z-index: 60; width: 200px; display: grid; gap: 2px; }
  .pop.cols .check { padding: 6px 8px; border-radius: 7px; }
  .pop.cols .check:hover { background: var(--surface-3); }
  .ph { font-size: 11.5px; font-weight: 600; letter-spacing: .04em; text-transform: uppercase; color: var(--text-3); padding: 4px 8px; }
  .pop.create button, .mi { display: flex; align-items: center; gap: 10px; width: 100%; padding: 7px 10px; border: 0; border-radius: 7px; background: none; font: inherit; color: var(--text); cursor: pointer; text-align: left; }
  .pop.create button:hover, .mi:hover { background: var(--accent-soft); text-decoration: none; }
  .rowmenu { position: fixed; width: 230px; z-index: 60; }
  .mi { color: var(--text); }
  .mi.danger { color: var(--danger); }
  .mi.touch { display: none; }
  .mi .faint { margin-left: auto; font-size: 12px; }

  .notice { position: fixed; left: 50%; bottom: 24px; transform: translateX(-50%); z-index: 70; padding: 10px 16px; border-radius: 10px; background: var(--text); color: var(--surface); font-size: 13.5px; box-shadow: var(--shadow-lg); }
  .dnd-hint { margin: 8px 2px 0; }
  .nofind { margin: 0; padding: 18px; text-align: center; color: var(--text-3); font-size: 13.5px; }
  .embedded .page-head { margin-bottom: 12px; }
  .actions { text-align: right; white-space: nowrap; }
  .up-badge { margin-bottom: 10px; }
  .show-sm { display: none; }

  /* Touch screens: no hover, so only «⋯» with every action inside. */
  @media (hover: none), (max-width: 720px) {
    .acts .wide { display: none; }
    .mi.touch { display: flex; }
    .dnd-hint { display: none; }
  }
  @media (max-width: 720px) {
    .hide-sm { display: none; }
    .sub.show-sm { display: block; }
    .usage { order: 10; width: 100%; }
    .c-name { min-width: 0; }
    .rename { min-width: 0; width: 100%; }
    .tools .btn { flex: 1 1 auto; justify-content: center; }
    .fsearch { width: 100%; }
  }
</style>
