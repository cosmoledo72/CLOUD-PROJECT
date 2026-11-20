<script lang="ts">
  // Files of a task: upload from the computer (button or drag and drop) into
  // the task's folder inside the project folder, attach files that already lie
  // in the project folder, open, download and detach.
  import { api, docKind, type FolderView, type TaskAttachment } from '#lib/api.ts';
  import FileIcon from '#lib/FileIcon.svelte';
  import Modal from '#lib/Modal.svelte';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Download from '@lucide/svelte/icons/download';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import Paperclip from '@lucide/svelte/icons/paperclip';
  import Upload from '@lucide/svelte/icons/upload';
  import X from '@lucide/svelte/icons/x';
  import Link2 from '@lucide/svelte/icons/link-2';
  import Author from '#lib/files/Author.svelte';
  import { portalUrl, relTime, sizeLabel } from '#lib/files/fmt.ts';

  let { taskId, projectId, attachments = $bindable([]), canEdit = false, canDetachAny = false, meId, onchange }: {
    taskId: string;
    projectId: string;
    attachments?: TaskAttachment[];
    canEdit?: boolean;
    /** Project managers detach any file; others only what they attached. */
    canDetachAny?: boolean;
    meId?: string;
    /** The list changed (count for the rows behind the card). */
    onchange?: (count: number) => void;
  } = $props();

  let error = $state('');
  let busy = $state(false);
  let over = $state(false);
  let input = $state<HTMLInputElement>();

  function apply(list: TaskAttachment[]) {
    attachments = list;
    onchange?.(list.length);
  }

  async function uploadFiles(files: FileList | File[] | null | undefined) {
    const list = Array.from(files ?? []);
    if (!list.length || !canEdit) return;
    error = '';
    busy = true;
    try {
      const fd = new FormData();
      for (const f of list) fd.append('file', f, f.name);
      const r = await api.post<{ attachments: TaskAttachment[] }>(`/tasks/${taskId}/attachments`, fd);
      apply(r.attachments);
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
      if (input) input.value = '';
    }
  }

  function drop(e: DragEvent) {
    over = false;
    if (!canEdit || !e.dataTransfer?.files.length) return;
    e.preventDefault();
    uploadFiles(e.dataTransfer.files);
  }

  async function detach(a: TaskAttachment) {
    if (!confirm(`Открепить «${a.name}» от задачи? Сам файл останется в папке проекта.`)) return;
    error = '';
    try {
      const r = await api.del<{ attachments: TaskAttachment[] }>(`/tasks/${taskId}/attachments/${a.file_id}`);
      apply(r.attachments);
    } catch (e) {
      error = (e as Error).message;
    }
  }
  let copied = $state('');
  async function copyLink(a: TaskAttachment) {
    const url = portalUrl(openable(a) ? `/editor/${a.file_id}` : `/api/files/${a.file_id}/download`);
    try {
      await navigator.clipboard.writeText(url);
      copied = a.file_id;
      setTimeout(() => (copied = ''), 2000);
    } catch {
      prompt('Ссылка для сотрудников портала:', url);
    }
  }
  const canDetach = (a: TaskAttachment) => canDetachAny || (!!meId && a.added_by === meId);
  const openable = (a: TaskAttachment) => docKind(a.extension) !== 'other';

  // --- Picker: files of the project folder -----------------------------------
  let pickOpen = $state(false);
  let view = $state<FolderView | null>(null);
  let rootId = $state<string | null>(null);
  let picked = $state<Record<string, boolean>>({});
  let pickError = $state('');
  const attached = $derived(new Set(attachments.map((a) => a.file_id)));
  const pickedIds = $derived(Object.keys(picked).filter((k) => picked[k]));

  async function openPicker() {
    pickError = '';
    picked = {};
    view = null;
    pickOpen = true;
    try {
      if (!rootId) {
        const r = await api.get<{ project: { folder_id: string | null } }>(`/projects/${projectId}`);
        rootId = r.project.folder_id;
      }
      if (!rootId) throw new Error('У проекта нет папки документов');
      await go(rootId);
    } catch (e) {
      pickError = (e as Error).message;
    }
  }
  async function go(id: string) {
    try {
      view = await api.get<FolderView>(`/folders/${id}`);
    } catch (e) {
      pickError = (e as Error).message;
    }
  }
  // Breadcrumbs from the project folder down.
  const crumbs = $derived.by(() => {
    const all = view?.breadcrumbs ?? [];
    const i = all.findIndex((c) => c.id === rootId);
    return i >= 0 ? all.slice(i) : all;
  });
  async function attachPicked() {
    pickError = '';
    try {
      const r = await api.post<{ attachments: TaskAttachment[] }>(`/tasks/${taskId}/attachments/link`, { file_ids: pickedIds });
      apply(r.attachments);
      pickOpen = false;
    } catch (e) {
      pickError = (e as Error).message;
    }
  }
</script>

<section class="sec" class:over role="group" aria-label="Файлы задачи"
         ondragover={(e) => { if (canEdit && e.dataTransfer?.types.includes('Files')) { e.preventDefault(); over = true; } }}
         ondragleave={() => (over = false)} ondrop={drop}>
  <h3><Paperclip size={16} strokeWidth={2} />Файлы {#if attachments.length}<span class="faint">{attachments.length}</span>{/if}</h3>
  {#if attachments.length}
    <ul class="files">
      {#each attachments as a (a.file_id)}
        <li>
          <FileIcon ext={a.extension} />
          <div class="main">
            {#if openable(a)}
              <a class="name" href="/editor/{a.file_id}" target="_blank" title="Открыть в редакторе">{a.name}</a>
            {:else}
              <a class="name" href="/api/files/{a.file_id}/download" title="Скачать">{a.name}</a>
            {/if}
            <span class="meta">{sizeLabel(a.size_bytes)} · <span title={new Date(a.added_at).toLocaleString('ru-RU')}>{relTime(a.added_at)}</span>
              {#if a.added_by && a.added_by_name}<Author id={a.added_by} name={a.added_by_name} />{/if}</span>
          </div>
          <div class="acts">
            <a class="ibtn" href="/api/files/{a.file_id}/download" title="Скачать" aria-label="Скачать {a.name}"><Download size={16} /></a>
            <button class="ibtn" onclick={() => copyLink(a)} title={copied === a.file_id ? 'Ссылка скопирована' : 'Скопировать ссылку'} aria-label="Ссылка на {a.name}"><Link2 size={16} /></button>
            {#if canDetach(a)}
              <button class="ibtn" onclick={() => detach(a)} title="Открепить от задачи" aria-label="Открепить {a.name}"><X size={16} /></button>
            {/if}
          </div>
          {#if copied === a.file_id}<span class="copied">Ссылка скопирована</span>{/if}
        </li>
      {/each}
    </ul>
  {:else if !canEdit}
    <p class="faint small none">Файлов нет.</p>
  {/if}
  {#if canEdit}
    <div class="actions">
      <input bind:this={input} type="file" multiple hidden onchange={(e) => uploadFiles(e.currentTarget.files)} />
      <button class="btn sm" onclick={() => input?.click()} disabled={busy}><Upload size={14} />{busy ? 'Загрузка…' : 'Загрузить'}</button>
      <button class="btn sm" onclick={openPicker}><FolderOpen size={14} />Из папки проекта</button>
      <span class="faint small hint">или перетащите файлы сюда — они сохранятся в папке проекта «Задачи»</span>
    </div>
  {/if}
  {#if error}<div class="error-box">{error}</div>{/if}
</section>

<Modal bind:open={pickOpen} title="Файлы из папки проекта" width={560}>
  {#if pickError}<div class="error-box">{pickError}</div>{/if}
  {#if view}
    <nav class="crumbs" aria-label="Папка">
      {#each crumbs as c, i (c.id)}
        {#if i}<ChevronRight size={14} />{/if}
        {#if i === crumbs.length - 1}<b>{i === 0 ? 'Папка проекта' : c.name}</b>
        {:else}<button class="linkbtn" onclick={() => go(c.id)}>{i === 0 ? 'Папка проекта' : c.name}</button>{/if}
      {/each}
    </nav>
    <ul class="pick">
      {#each view.folders as f (f.id)}
        <li><button class="row" onclick={() => go(f.id)}><FileIcon folder /><span class="nm">{f.name}</span><ChevronRight size={15} /></button></li>
      {/each}
      {#each view.files as f (f.id)}
        <li>
          <label class="row" class:dim={attached.has(f.id)}>
            <input type="checkbox" bind:checked={picked[f.id]} disabled={attached.has(f.id)} />
            <FileIcon ext={f.extension} /><span class="nm">{f.name}</span>
            <span class="faint small">{attached.has(f.id) ? 'уже прикреплён' : sizeLabel(f.size_bytes)}</span>
          </label>
        </li>
      {/each}
      {#if !view.folders.length && !view.files.length}<li class="faint small empty">Папка пуста.</li>{/if}
    </ul>
  {:else if !pickError}
    <p class="faint">Загрузка…</p>
  {/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (pickOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!pickedIds.length} onclick={attachPicked}>Прикрепить{pickedIds.length ? ` (${pickedIds.length})` : ''}</button>
  {/snippet}
</Modal>

<style>
  .sec { border-top: 1px solid var(--border); padding-top: 12px; display: grid; gap: 8px; }
  .sec.over { outline: 2px dashed var(--accent); outline-offset: 4px; background: var(--accent-soft); border-radius: var(--radius); }
  h3 { display: flex; align-items: center; gap: 7px; margin: 0; font-size: 14px; }
  h3 .faint { font-weight: 400; font-size: 12.5px; }
  .files { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .files li { display: flex; align-items: center; gap: 9px; padding: 5px 6px; border-radius: var(--radius); }
  .files li:hover { background: var(--surface-2); }
  .main { flex: 1; min-width: 0; display: grid; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; color: var(--text); }
  .name:hover { color: var(--accent); }
  .meta { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 8px; font-size: 12.5px; color: var(--text-3); margin-top: 2px; }
  .meta :global(.who) { font-size: 12px; }
  /* Row icons on hover or focus (always on touch screens). */
  .acts { display: inline-flex; gap: 2px; flex: none; opacity: 0; transition: opacity .1s; }
  .files li:hover .acts, .files li:focus-within .acts { opacity: 1; }
  .ibtn { display: inline-grid; place-items: center; width: 30px; height: 30px; border: 0; border-radius: 8px; background: none; color: var(--text-3); cursor: pointer; }
  .ibtn:hover { background: var(--surface-3); color: var(--text); }
  .copied { font-size: 12px; color: var(--success, #2f9e5b); flex: none; }
  @media (hover: none) { .acts { opacity: 1; } }
  .actions { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
  .hint { flex: 1; min-width: 180px; }
  .none { margin: 0; }
  .crumbs { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; font-size: 13px; margin-bottom: 8px; color: var(--text-2); }
  .linkbtn { border: 0; background: none; padding: 0; color: var(--accent); cursor: pointer; font: inherit; }
  .linkbtn:hover { text-decoration: underline; }
  .pick { list-style: none; margin: 0; padding: 0; max-height: min(52vh, 420px); overflow-y: auto; border: 1px solid var(--border); border-radius: var(--radius); }
  .pick li + li { border-top: 1px solid var(--border); }
  .row { display: flex; align-items: center; gap: 9px; width: 100%; padding: 8px 10px; border: 0; background: none; text-align: left; cursor: pointer; font: inherit; color: var(--text); }
  .row:hover { background: var(--surface-2); }
  .row.dim { opacity: .6; cursor: default; }
  .nm { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { padding: 14px; text-align: center; }
</style>
