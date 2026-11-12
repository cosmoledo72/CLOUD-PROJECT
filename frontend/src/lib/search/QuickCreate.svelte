<script lang="ts">
  // «+ Создать»: task (in a chosen project), project, deal, Word/Excel
  // document (in my documents, common documents or a project folder), KB page.
  import { goto } from '$app/navigation';
  import SquareCheckBig from '@lucide/svelte/icons/square-check-big';
  import FolderKanban from '@lucide/svelte/icons/folder-kanban';
  import HandCoins from '@lucide/svelte/icons/hand-coins';
  import BookOpen from '@lucide/svelte/icons/book-open';
  import FileIcon from '$lib/FileIcon.svelte';
  import Modal from '$lib/Modal.svelte';
  import { api, type Project } from '$lib/api';
  import { session } from '$lib/session.svelte';
  import { quick } from './quick.svelte';

  type Kind = 'docx' | 'xlsx';
  let taskOpen = $state(false);
  let docOpen = $state(false);
  let docKind = $state<Kind>('docx');
  let projects = $state<Project[]>([]);
  let roots = $state<{ my: string | null; common: string | null }>({ my: null, common: null });
  let title = $state('');
  let due = $state('');
  let projectId = $state('');
  let folderId = $state('');
  let busy = $state(false);
  let error = $state('');
  let menuEl = $state<HTMLDivElement>();

  function stored(k: string) {
    try { return localStorage.getItem(k) ?? ''; } catch { return ''; }
  }
  function store(k: string, v: string) {
    try { localStorage.setItem(k, v); } catch { /* storage unavailable */ }
  }

  const canProjects = $derived(session.has('projects'));
  const items = $derived([
    canProjects && { k: 'task', label: 'Задачу', hint: 'в проекте', icon: SquareCheckBig },
    canProjects && { k: 'project', label: 'Проект', icon: FolderKanban },
    session.crm && { k: 'deal', label: 'Сделку', icon: HandCoins },
    (canProjects || session.has('storage')) && { k: 'docx', label: 'Документ Word', file: 'docx' },
    (canProjects || session.has('storage')) && { k: 'xlsx', label: 'Таблицу Excel', file: 'xlsx' },
    session.has('kb') && { k: 'kb', label: 'Страницу базы знаний', icon: BookOpen }
  ].filter(Boolean) as { k: string; label: string; hint?: string; icon?: typeof BookOpen; file?: string }[]);

  $effect(() => {
    if (!quick.menu) return;
    setTimeout(() => (menuEl?.querySelector('button') as HTMLButtonElement | null)?.focus());
    const away = (e: PointerEvent) => { if (menuEl && !menuEl.contains(e.target as Node)) quick.menu = false; };
    const t = setTimeout(() => document.addEventListener('pointerdown', away));
    return () => { clearTimeout(t); document.removeEventListener('pointerdown', away); };
  });

  async function loadProjects() {
    if (!canProjects) return;
    projects = (await api.get<{ projects: Project[] }>('/projects')).projects.filter((p) => p.status !== 'closed');
  }

  async function pick(k: string) {
    quick.menu = false;
    error = '';
    if (k === 'project') return goto('/projects?new=1');
    if (k === 'deal') return goto('/crm/deals?new=1');
    if (k === 'kb') return goto('/kb?new=1');
    if (k === 'task') {
      title = '';
      due = '';
      taskOpen = true;
      await loadProjects();
      const last = stored('wh_qc_project');
      projectId = projects.some((p) => p.id === last) ? last : (projects[0]?.id ?? '');
      return;
    }
    docKind = k as Kind;
    title = '';
    docOpen = true;
    await Promise.all([
      loadProjects(),
      session.has('storage') ? api.get<typeof roots>('/folders/roots').then((r) => (roots = r)).catch(() => {}) : Promise.resolve()
    ]);
    const last = stored('wh_qc_folder');
    folderId = places.some((p) => p.id === last) ? last : (places[0]?.id ?? '');
  }

  const places = $derived([
    ...(roots.my ? [{ id: roots.my, label: 'Мои документы' }] : []),
    ...(roots.common ? [{ id: roots.common, label: 'Общие документы' }] : []),
    ...projects.filter((p) => p.folder_id).map((p) => ({ id: p.folder_id as string, label: `Проект: ${p.name}` }))
  ]);

  async function createTask() {
    if (!title.trim() || !projectId) return;
    busy = true;
    error = '';
    try {
      const r = await api.post<{ task: { id: string } }>(`/projects/${projectId}/tasks`, { title: title.trim(), due_date: due || null });
      store('wh_qc_project', projectId);
      taskOpen = false;
      goto(`/projects/${projectId}?task=${r.task.id}`);
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }
  async function createDoc() {
    if (!folderId) return;
    busy = true;
    error = '';
    try {
      const name = title.trim() || (docKind === 'docx' ? 'Новый документ' : 'Новая таблица');
      const r = await api.post<{ id: string }>(`/folders/${folderId}/new`, { kind: docKind, name });
      store('wh_qc_folder', folderId);
      docOpen = false;
      goto(`/editor/${r.id}`);
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }
  function menuKey(e: KeyboardEvent) {
    const btns = [...(menuEl?.querySelectorAll('button') ?? [])] as HTMLButtonElement[];
    const i = btns.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === 'Escape') quick.menu = false;
    else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      btns[(i + (e.key === 'ArrowDown' ? 1 : btns.length - 1)) % btns.length]?.focus();
    }
  }
</script>

{#if quick.menu}
  <div class="qc" bind:this={menuEl} role="menu" aria-label="Создать" tabindex="-1" style="left: {quick.x}px; top: {quick.y}px" onkeydown={menuKey}>
    <div class="h">Создать</div>
    {#each items as it (it.k)}
      {@const Icon = it.icon}
      <button role="menuitem" onclick={() => pick(it.k)}>
        <span class="ic">{#if it.file}<FileIcon ext={it.file} />{:else if Icon}<Icon size={17} strokeWidth={1.8} />{/if}</span>
        {it.label}{#if it.hint}<span class="hint">{it.hint}</span>{/if}
      </button>
    {/each}
    <div class="f">Клавиша <kbd>C</kbd> — открыть это меню</div>
  </div>
{/if}

<Modal bind:open={taskOpen} title="Новая задача" width={480}>
  <form id="qc-task" class="grid" onsubmit={(e) => { e.preventDefault(); createTask(); }}>
    <!-- svelte-ignore a11y_autofocus -->
    <label class="field">Название<input class="input" bind:value={title} maxlength="300" required autofocus /></label>
    <label class="field">Проект
      <select class="input" bind:value={projectId} required>
        {#each projects as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
      </select>
    </label>
    <label class="field">Срок<input class="input" type="date" bind:value={due} /></label>
    {#if !projects.length}<p class="muted">Нет проектов, в которых можно создать задачу.</p>{/if}
    {#if error}<div class="error-box">{error}</div>{/if}
  </form>
  {#snippet footer()}
    <button class="btn" onclick={() => (taskOpen = false)}>Отмена</button>
    <button class="btn primary" form="qc-task" disabled={busy || !title.trim() || !projectId}>{busy ? 'Создаю…' : 'Создать и открыть'}</button>
  {/snippet}
</Modal>

<Modal bind:open={docOpen} title={docKind === 'docx' ? 'Новый документ Word' : 'Новая таблица Excel'} width={480}>
  <form id="qc-doc" class="grid" onsubmit={(e) => { e.preventDefault(); createDoc(); }}>
    <!-- svelte-ignore a11y_autofocus -->
    <label class="field">Название<input class="input" bind:value={title} maxlength="200" autofocus placeholder={docKind === 'docx' ? 'Новый документ' : 'Новая таблица'} /></label>
    <label class="field">Где сохранить
      <select class="input" bind:value={folderId}>
        {#each places as p (p.id)}<option value={p.id}>{p.label}</option>{/each}
      </select>
    </label>
    <p class="muted">Документ откроется в редакторе. В проекте он попадёт в корневую папку проекта.</p>
    {#if error}<div class="error-box">{error}</div>{/if}
  </form>
  {#snippet footer()}
    <button class="btn" onclick={() => (docOpen = false)}>Отмена</button>
    <button class="btn primary" form="qc-doc" disabled={busy || !folderId}>{busy ? 'Создаю…' : 'Создать и открыть'}</button>
  {/snippet}
</Modal>

<style>
  .qc { position: fixed; z-index: 60; width: 260px; background: var(--surface); border: 1px solid var(--border); border-radius: 12px; box-shadow: var(--shadow-lg); padding: 6px; }
  .h { font-size: 11.5px; font-weight: 600; letter-spacing: .04em; text-transform: uppercase; color: var(--text-3); padding: 6px 10px 4px; }
  .qc button { display: flex; align-items: center; gap: 10px; width: 100%; padding: 8px 10px; border: 0; border-radius: 8px; background: none; font: inherit; font-size: 14px;
               color: var(--text); cursor: pointer; text-align: left; }
  .qc button:hover, .qc button:focus-visible { background: var(--accent-soft); outline: none; }
  .ic { display: grid; place-items: center; width: 22px; color: var(--text-2); }
  .hint { margin-left: auto; font-size: 12px; color: var(--text-3); }
  .f { border-top: 1px solid var(--border); margin-top: 4px; padding: 8px 10px 4px; font-size: 12px; color: var(--text-3); }
  kbd { font: 11px/1 ui-monospace, monospace; padding: 2px 5px; border: 1px solid var(--border); border-bottom-width: 2px; border-radius: 5px; }
  .grid { display: grid; gap: 12px; }
  .field { display: grid; gap: 6px; font-size: 13px; font-weight: 500; }
  .muted { margin: 0; font-size: 12.5px; }
</style>
