<script lang="ts">
  import { onIntent } from '#lib/intent.ts';
  import { goto, beforeNavigate } from '$app/navigation';
  import { page } from '$app/state';
  import { api, formatDateTime, type KbHit, type KbNode, type KbPage, type KbViewer } from '#lib/api.ts';
  import { session } from '#lib/session.svelte.ts';
  import KbTree from '#lib/kb/KbTree.svelte';
  import KbEditor from '#lib/kb/KbEditor.svelte';
  import KbView from '#lib/kb/KbView.svelte';
  import KbHistory from '#lib/kb/KbHistory.svelte';
  import Avatar from '#lib/Avatar.svelte';
  import BookOpen from '@lucide/svelte/icons/book-open';
  import Search from '@lucide/svelte/icons/search';
  import Plus from '@lucide/svelte/icons/plus';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Archive from '@lucide/svelte/icons/archive';
  import ArchiveRestore from '@lucide/svelte/icons/archive-restore';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import FolderInput from '@lucide/svelte/icons/folder-input';
  import X from '@lucide/svelte/icons/x';
  import FilePlus2 from '@lucide/svelte/icons/file-plus-2';
  import Share2 from '@lucide/svelte/icons/share-2';
  import History from '@lucide/svelte/icons/history';
  import Pin from '@lucide/svelte/icons/pin';
  import PinOff from '@lucide/svelte/icons/pin-off';
  import Modal from '#lib/Modal.svelte';
  import PublicLink from '#lib/files/PublicLink.svelte';

  let nodes = $state<KbNode[]>([]);
  let expanded = $state<Record<string, boolean>>({});
  let current = $state<KbPage | null>(null);
  let crumbs = $state<{ id: string; title: string }[]>([]);
  let canDelete = $state(false);
  let viewers = $state<KbViewer[]>([]);
  let error = $state('');
  let loaded = $state(false);

  const pid = $derived(page.url.searchParams.get('p'));
  const showArchive = $derived(page.url.searchParams.has('archive'));

  async function loadTree() {
    nodes = (await api.get<{ pages: KbNode[] }>('/kb/pages')).pages;
    loaded = true;
  }
  $effect(() => { loadTree().catch((e) => (error = e.message)); });

  async function loadPage(id: string) {
    const r = await api.get<{ page: KbPage; breadcrumbs: { id: string; title: string }[]; can_delete: boolean; viewers: KbViewer[] }>(`/kb/pages/${id}`);
    current = r.page;
    crumbs = r.breadcrumbs;
    canDelete = r.can_delete;
    viewers = r.viewers;
    for (const c of r.breadcrumbs) expanded[c.id] = true;
  }
  // Leaving for another page closes the editor (not the first run: «?new=1»
  // may have opened it already).
  let lastPid: string | null | undefined = undefined;
  $effect(() => {
    const id = pid;
    if (lastPid !== undefined && lastPid !== id) editing = false;
    lastPid = id;
    moving = false;
    error = '';
    if (!id) { current = null; return; }
    loadPage(id).catch((e) => { current = null; error = e.status === 404 ? 'Страница не найдена или удалена.' : e.message; });
  });

  const children = $derived(current ? nodes.filter((n) => n.parent_id === current!.id) : []);
  const roots = $derived(nodes.filter((n) => !n.parent_id));
  const pinned = $derived(nodes.filter((n) => n.pinned));

  // ------------------------------------------------------------------ who is here
  // Heartbeat every 20 s while a page is open; shows who else looks at it and
  // who is editing it right now.
  $effect(() => {
    const id = current?.id;
    const ed = editing;
    if (!id) return;
    let stop = false;
    const beat = () => api.post<{ viewers: KbViewer[] }>(`/kb/pages/${id}/presence`, { editing: ed })
      .then((r) => { if (!stop) viewers = r.viewers; }).catch(() => {});
    beat();
    const t = setInterval(beat, 20_000);
    return () => {
      stop = true;
      clearInterval(t);
      api.post(`/kb/pages/${id}/presence`, { leave: true }).catch(() => {});
    };
  });
  const editors = $derived(viewers.filter((v) => v.editing));

  // ------------------------------------------------------------------ search
  let q = $state('');
  let hits = $state<KbHit[] | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const text = q.trim();
    clearTimeout(timer);
    if (!text) { hits = null; return; }
    timer = setTimeout(() => {
      api.get<{ results: KbHit[] }>(`/kb/search?q=${encodeURIComponent(text)}`).then((r) => (hits = r.results)).catch(() => {});
    }, 250);
  });
  // Snippets mark matches with «»; escape everything else and highlight them.
  const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]!);
  const snippet = (s: string) => esc(s).replace(/«([^»]*)»/g, '<mark>$1</mark>');

  // ------------------------------------------------------------------ editing
  let editing = $state(false);
  /** Page being edited (null: a new page not saved yet). */
  let editId = $state<string | null>(null);
  let baseVersion = $state(0);
  let newParent = $state<string | null>(null);
  let eTitle = $state('');
  let eIcon = $state('');
  let startHtml = $state('');
  let eHtml = $state('');
  let startTitle = '';
  let busy = $state(false);
  let saved = $state('');
  let editorKey = $state(0);
  const ICONS = ['', '📘', '📄', '📌', '🛠️', '🧭', '📐', '🏗️', '⚙️', '🧾', '💡', '✅', '⚠️', '👥', '📞', '🎓'];
  // The editor reports an empty document as «<p></p>»: same as no text at all.
  const sameHtml = (a: string, b: string) => a.replace(/^<p><\/p>$/, '') === b.replace(/^<p><\/p>$/, '');
  const dirty = $derived(editing && (!sameHtml(eHtml, startHtml) || eTitle !== startTitle));

  onIntent('new', () => startNew(null));
  function startNew(parent: string | null) {
    if (dirty && !confirm('Есть несохранённые изменения. Закрыть без сохранения?')) return;
    editId = null;
    baseVersion = 0;
    newParent = parent;
    eTitle = '';
    startTitle = '';
    eIcon = '';
    startHtml = '';
    eHtml = '';
    editing = true;
    editorKey++;
    error = '';
    saved = '';
    if (parent) expanded[parent] = true;
  }
  function startEdit() {
    if (!current) return;
    editId = current.id;
    baseVersion = current.version;
    eTitle = current.title;
    startTitle = current.title;
    eIcon = current.icon;
    startHtml = current.content ?? '';
    eHtml = startHtml;
    editing = true;
    editorKey++;
    error = '';
    saved = '';
  }
  /** Files need a page: a new page is created (and kept in edit mode) first. */
  async function ensurePage(): Promise<string> {
    if (editId) return editId;
    const r = await api.post<{ id: string }>('/kb/pages', { parent_id: newParent, title: eTitle.trim() || 'Новая страница', icon: eIcon, content: eHtml });
    if (!eTitle.trim()) eTitle = 'Новая страница';
    editId = r.id;
    baseVersion = 1;
    startHtml = eHtml;
    startTitle = eTitle;
    await loadTree();
    return r.id;
  }
  async function save(force = false) {
    if (!eTitle.trim()) { error = 'Введите название страницы'; return; }
    busy = true;
    error = '';
    try {
      if (!editId) {
        const r = await api.post<{ id: string }>('/kb/pages', { parent_id: newParent, title: eTitle, icon: eIcon, content: eHtml });
        editing = false;
        await loadTree();
        goto(`/kb?p=${r.id}`);
        return;
      }
      const r = await api.put<{ page: KbPage }>(`/kb/pages/${editId}`, { title: eTitle, icon: eIcon, content: eHtml, base_version: baseVersion, force });
      const id = editId;
      editing = false;
      await loadTree();
      if (pid !== id) goto(`/kb?p=${id}`);
      else { current = r.page; await loadPage(id); }
    } catch (e) {
      if ((e as { status?: number }).status === 409) {
        if (confirm(`${(e as Error).message}.\n\nСохранить вашу версию поверх? Чужие изменения останутся в истории страницы.`)) {
          busy = false;
          return save(true);
        }
        error = 'Ваши правки не сохранены. Скопируйте нужный текст, отмените редактирование и откройте страницу заново.';
      } else error = (e as Error).message;
    } finally {
      busy = false;
    }
  }
  function cancel() {
    if (dirty && !confirm('Есть несохранённые изменения. Закрыть без сохранения?')) return;
    const created = editId && !current;
    editing = false;
    error = '';
    if (created) goto(`/kb?p=${editId}`);
  }
  beforeNavigate((nav) => {
    if (nav.shallow && nav.type === 'goto') return;
    if (dirty && nav.type !== 'leave' && !confirm('Есть несохранённые изменения. Уйти со страницы без сохранения?')) nav.cancel();
  });
  function onUnload(e: BeforeUnloadEvent) {
    if (dirty) e.preventDefault();
  }

  // ------------------------------------------------------------------ pin, move, archive
  async function togglePin() {
    if (!current) return;
    const r = await api.patch<{ pinned: boolean }>(`/kb/pages/${current.id}/pin`, { pinned: !current.pinned });
    current.pinned = r.pinned;
    await loadTree();
  }
  let moving = $state(false);
  let moveTo = $state('');
  // Pages that cannot be a new parent: the page itself and its descendants.
  const blocked = $derived.by(() => {
    const out = new Set<string>();
    if (!current) return out;
    const stack = [current.id];
    while (stack.length) {
      const id = stack.pop()!;
      out.add(id);
      for (const n of nodes) if (n.parent_id === id) stack.push(n.id);
    }
    return out;
  });
  function pathOf(n: KbNode): string {
    const parts = [n.title];
    let p = n.parent_id;
    while (p) {
      const x = nodes.find((y) => y.id === p);
      if (!x) break;
      parts.unshift(x.title);
      p = x.parent_id;
    }
    return parts.join(' / ');
  }
  async function move() {
    if (!current) return;
    try {
      await api.patch(`/kb/pages/${current.id}/move`, { parent_id: moveTo || null });
      moving = false;
      await loadTree();
      await loadPage(current.id);
    } catch (e) {
      error = (e as Error).message;
    }
  }
  async function toArchive() {
    if (!current) return;
    const n = blocked.size;
    if (!confirm(n > 1 ? `Убрать «${current.title}» и вложенные страницы (${n - 1}) в архив? Их можно будет вернуть.` : `Убрать страницу «${current.title}» в архив? Её можно будет вернуть.`)) return;
    try {
      await api.del(`/kb/pages/${current.id}`);
      const parent = current.parent_id;
      await loadTree();
      goto(parent ? `/kb?p=${parent}` : '/kb');
    } catch (e) {
      error = (e as Error).message;
    }
  }
  interface Archived { id: string; title: string; icon: string; archived_at: string; archived_by_name: string | null; subpages: number; can_delete: boolean }
  let archived = $state<Archived[]>([]);
  $effect(() => {
    if (showArchive) api.get<{ pages: Archived[] }>('/kb/archive').then((r) => (archived = r.pages)).catch((e) => (error = e.message));
  });
  async function restorePage(a: Archived) {
    await api.post(`/kb/pages/${a.id}/restore`);
    await loadTree();
    goto(`/kb?p=${a.id}`);
  }
  async function deleteForever(a: Archived) {
    if (!confirm(`Удалить «${a.title}»${a.subpages ? ' и вложенные страницы' : ''} навсегда? Восстановить будет нельзя.`)) return;
    await api.del(`/kb/pages/${a.id}?permanent=true`);
    archived = archived.filter((x) => x.id !== a.id);
  }

  let shareOpen = $state(false);
  let historyOpen = $state(false);
  const editorName = (v: KbViewer) => v.name.split(' ')[0];
</script>

<svelte:head><title>{current ? `${current.title} — ` : ''}База знаний</title></svelte:head>
<svelte:window onbeforeunload={onUnload} />

<div class="page">

<div class="kb">
  <aside class="side card">
    <label class="search">
      <Search size={15} />
      <input class="input" placeholder="Поиск по базе" bind:value={q} />
      {#if q}<button class="clear" aria-label="Очистить" onclick={() => (q = '')}><X size={14} /></button>{/if}
    </label>
    <div class="tree">
      <a class="home" class:on={!pid && !editing && !showArchive} href="/kb"><BookOpen size={15} /> Все страницы</a>
      {#if pinned.length}
        <div class="pinned">
          <div class="ph">Закреплённые</div>
          {#each pinned as n (n.id)}
            <a class="pn" class:on={n.id === pid} href="/kb?p={n.id}"><span>{n.icon || '📌'}</span><span class="t">{n.title}</span></a>
          {/each}
        </div>
      {/if}
      <KbTree {nodes} current={pid} bind:expanded onadd={startNew} />
    </div>
    <button class="btn new" onclick={() => startNew(null)}><Plus size={15} /> Новая страница</button>
    <a class="arch" class:on={showArchive} href="/kb?archive"><Archive size={14} /> Архив</a>
  </aside>

  <main class="content">
    {#if error}<div class="error-box">{error}</div>{/if}

    {#if hits}
      <section class="card pane">
        <h1 class="h">Результаты поиска</h1>
        {#each hits as h (h.id)}
          <a class="hit" href="/kb?p={h.id}" onclick={() => (q = '')}>
            <div class="ht">{h.icon || '📄'} {h.title}</div>
            {#if h.snippet}<div class="hs muted">{@html snippet(h.snippet)}</div>{/if}
          </a>
        {:else}
          <p class="muted">Ничего не найдено.</p>
        {/each}
      </section>
    {:else if editing}
      <section class="card pane editor">
        <div class="ehead">
          <select class="input icon" bind:value={eIcon} aria-label="Значок">
            {#each ICONS as i}<option value={i}>{i || '—'}</option>{/each}
          </select>
          <!-- svelte-ignore a11y_autofocus -->
          <input class="input title" placeholder="Название страницы" bind:value={eTitle} maxlength="200" autofocus={!editId} />
          <button class="btn" onclick={cancel}>Отмена</button>
          <button class="btn primary" disabled={busy || !eTitle.trim()} onclick={() => save()}>{busy ? 'Сохраняю…' : editId ? 'Сохранить' : 'Создать'}</button>
        </div>
        {#if !editId && newParent}
          <p class="faint small">Вложенная страница в «{nodes.find((n) => n.id === newParent)?.title}»</p>
        {/if}
        {#if editors.length}
          <div class="warn-box">Эту страницу сейчас тоже правит {editors.map(editorName).join(', ')}. Если сохраните после него, портал спросит, чью версию оставить; обе останутся в истории.</div>
        {/if}
        {#key editorKey}
          <KbEditor content={startHtml} bind:html={eHtml} pages={nodes.filter((n) => n.id !== editId)} {ensurePage} onsave={() => save()} />
        {/key}
        <p class="faint small foot">Ctrl+S — сохранить. Вставка из Word и из буфера сохраняет заголовки, списки и таблицы; картинки можно вставить или перетащить. «/» в начале строки — меню вставки.</p>
      </section>
    {:else if showArchive}
      <section class="card pane">
        <h1 class="h"><Archive size={22} /> Архив</h1>
        <p class="muted">Страницы, убранные из базы знаний. Их можно вернуть вместе с вложенными страницами или удалить навсегда (автор страницы или администратор).</p>
        {#each archived as a (a.id)}
          <div class="arow">
            <span class="ai">{a.icon || '📄'}</span>
            <div class="at"><b>{a.title}</b><span class="faint small">{formatDateTime(a.archived_at)}{a.archived_by_name ? ` · ${a.archived_by_name}` : ''}{a.subpages ? ` · вложенных: ${a.subpages}` : ''}</span></div>
            <button class="btn sm" onclick={() => restorePage(a)}><ArchiveRestore size={14} /> Вернуть</button>
            {#if a.can_delete}<button class="btn ghost sm danger" onclick={() => deleteForever(a)}><Trash2 size={14} /> Удалить навсегда</button>{/if}
          </div>
        {:else}
          <p class="faint">Архив пуст.</p>
        {/each}
      </section>
    {:else if current}
      <article class="card pane">
        <nav class="crumbs small">
          <a href="/kb">База знаний</a>
          {#each crumbs as c (c.id)}<span>/</span><a href="/kb?p={c.id}">{c.title}</a>{/each}
        </nav>
        {#if current.archived_at}<div class="warn-box">Страница в архиве. <a href="/kb?archive">Открыть архив</a></div>{/if}
        <div class="phead">
          <h1 class="h">{#if current.pinned}<span class="pinmark" title="Закреплена"><Pin size={18} /></span>{/if}{#if current.icon}<span class="pi">{current.icon}</span>{/if}{current.title}</h1>
          <div class="pact">
            {#if viewers.length}
              <span class="who" title="Сейчас на странице: {viewers.map((v) => v.name + (v.editing ? ' (правит)' : '')).join(', ')}">
                {#each viewers.slice(0, 4) as v (v.id)}<span class="wa" class:ed={v.editing}><Avatar id={v.id} name={v.name} size={26} /></span>{/each}
              </span>
            {/if}
            {#if !current.archived_at}
              <button class="btn sm" onclick={startEdit}><Pencil size={14} /> Редактировать</button>
              <button class="btn sm" onclick={() => (shareOpen = true)}><Share2 size={14} /> Поделиться</button>
              <button class="btn ghost sm" title={current.pinned ? 'Открепить' : 'Закрепить вверху списка для всех'} aria-label={current.pinned ? 'Открепить' : 'Закрепить'} onclick={togglePin}>{#if current.pinned}<PinOff size={15} />{:else}<Pin size={15} />{/if}</button>
              <button class="btn ghost sm" title="История изменений" aria-label="История изменений" onclick={() => (historyOpen = true)}><History size={15} /></button>
              <button class="btn ghost sm" title="Вложенная страница" aria-label="Вложенная страница" onclick={() => startNew(current!.id)}><FilePlus2 size={15} /></button>
              <button class="btn ghost sm" title="Переместить" aria-label="Переместить" onclick={() => { moving = !moving; moveTo = current?.parent_id ?? ''; }}><FolderInput size={15} /></button>
              {#if canDelete}<button class="btn ghost sm" title="В архив" aria-label="В архив" onclick={toArchive}><Archive size={15} /></button>{/if}
            {/if}
          </div>
        </div>
        <div class="meta faint small">
          Создана {formatDateTime(current.created_at)}{#if current.created_by_name} · {current.created_by_name}{/if}
          {#if current.version > 1 || current.updated_at !== current.created_at} · Обновлена {formatDateTime(current.updated_at)}{#if current.updated_by_name} · {current.updated_by_name}{/if}{/if}
          {#if editors.length}<span class="editing-now"> · сейчас правит {editors.map(editorName).join(', ')}</span>{/if}
        </div>
        {#if moving}
          <div class="move">
            <select class="input" bind:value={moveTo} aria-label="Новый раздел">
              <option value="">Верхний уровень</option>
              {#each nodes.filter((n) => !blocked.has(n.id)) as n (n.id)}<option value={n.id}>{pathOf(n)}</option>{/each}
            </select>
            <button class="btn primary sm" onclick={move}>Переместить</button>
            <button class="btn sm" onclick={() => (moving = false)}>Отмена</button>
          </div>
        {/if}
        {#if (current.content ?? '').replace(/<[^>]*>/g, '').trim() || (current.content ?? '').includes('<img') || (current.content ?? '').includes('file-card')}
          <KbView html={current.content ?? ''} />
        {:else}
          <p class="muted empty-body">Страница пока пустая. <button class="linkish" onclick={startEdit}>Добавить текст</button></p>
        {/if}
        {#if children.length}
          <div class="kids">
            <h2>Вложенные страницы</h2>
            <div class="cards">
              {#each children as c (c.id)}
                <a class="kid" href="/kb?p={c.id}"><span>{c.icon || '📄'}</span>{c.title}</a>
              {/each}
            </div>
          </div>
        {/if}
        <Modal bind:open={shareOpen} title={`Поделиться: ${current.title}`} width={560}>
          <p class="faint small share-note">Все сотрудники портала уже видят базу знаний. Чтобы показать страницу заказчику или подрядчику, создайте ссылку для внешних — они увидят только эту страницу (с картинками и файлами).</p>
          <PublicLink resourceType="kb_page" resourceId={current.id} active={shareOpen} />
        </Modal>
        <KbHistory bind:open={historyOpen} pageId={current.id} current={current.version} onrestored={() => loadPage(current!.id)} />
      </article>
    {:else if loaded && !pid}
      <section class="card pane">
        <h1 class="h"><BookOpen size={22} /> База знаний</h1>
        <p class="muted">Регламенты, инструкции, шаблоны ответов и всё, что команде важно не потерять. Страницы можно вкладывать друг в друга.</p>
        {#if roots.length}
          <div class="cards">
            {#each roots as c (c.id)}
              {@const n = nodes.filter((x) => x.parent_id === c.id).length}
              <a class="kid big" href="/kb?p={c.id}">
                <span class="e">{c.icon || '📄'}</span>
                <span><b>{c.title}</b>{#if n}<span class="faint small"> · {n} {n === 1 ? 'страница' : n < 5 ? 'страницы' : 'страниц'}</span>{/if}</span>
              </a>
            {/each}
          </div>
        {:else}
          <button class="btn primary" onclick={() => startNew(null)}><Plus size={15} /> Создать первую страницу</button>
        {/if}
      </section>
    {/if}
  </main>
</div>
</div>

<style>
  .share-note { margin: 0 0 12px; }
  .kb { display: grid; grid-template-columns: 270px minmax(0, 1fr); gap: 16px; align-items: start; }
  .side { padding: 10px; position: sticky; top: 12px; max-height: calc(100vh - 40px); display: flex; flex-direction: column; gap: 8px; }
  .tree { overflow-y: auto; flex: 1; min-height: 60px; }
  .home { display: flex; gap: 8px; align-items: center; padding: 6px 8px; border-radius: var(--radius); color: var(--text); font-size: 13.5px; text-decoration: none; }
  .home:hover { background: var(--surface-2); }
  .home.on { background: var(--accent-soft); color: var(--accent); font-weight: 500; }
  .search { position: relative; }
  .search > :global(svg) { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); color: var(--text-3); }
  .search input { padding-left: 32px; width: 100%; }
  .clear { position: absolute; right: 6px; top: 50%; transform: translateY(-50%); border: 0; background: none; color: var(--text-3); cursor: pointer; display: grid; }
  .new, .pact .btn, .h, .arow .btn { display: inline-flex; gap: 6px; align-items: center; }
  .pane { padding: 22px 28px; }
  .h { font-size: 24px; margin: 0 0 6px; }
  .pi { margin-right: 8px; }
  .crumbs { display: flex; gap: 6px; flex-wrap: wrap; color: var(--text-3); margin-bottom: 8px; }
  .crumbs a { color: var(--text-2); }
  .phead { display: flex; gap: 12px; align-items: flex-start; justify-content: space-between; flex-wrap: wrap; }
  .pact { display: flex; gap: 4px; }
  .meta { margin-bottom: 18px; }
  .move { display: flex; gap: 8px; margin-bottom: 16px; padding: 10px; background: var(--surface-2); border-radius: var(--radius); }
  .move select { flex: 1; }
  .empty-body { margin: 24px 0; }
  .linkish { border: 0; background: none; color: var(--accent); cursor: pointer; font: inherit; padding: 0; }
  .kids { margin-top: 26px; border-top: 1px solid var(--border); padding-top: 14px; }
  .kids h2 { font-size: 14px; color: var(--text-2); margin: 0 0 10px; }
  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 10px; margin-top: 12px; }
  .kid { display: flex; gap: 10px; align-items: center; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--radius-lg); color: var(--text); text-decoration: none; }
  .kid:hover { border-color: var(--accent); background: var(--accent-soft); }
  .kid.big { padding: 14px; }
  .kid .e { font-size: 22px; }
  .hit { display: block; padding: 10px 0; border-bottom: 1px solid var(--border); color: var(--text); text-decoration: none; }
  .hit:hover .ht { color: var(--accent); }
  .ht { font-weight: 600; }
  .hs { font-size: 13px; margin-top: 3px; }
  .hs :global(mark) { background: var(--warn-soft); color: inherit; padding: 0 2px; border-radius: 3px; }
  .ehead { display: flex; gap: 8px; margin-bottom: 10px; flex-wrap: wrap; }
  .icon { width: 70px; font-size: 16px; }
  .title { flex: 1; min-width: 220px; font-size: 18px; font-weight: 600; height: 40px; }
  .foot { margin: 10px 2px 0; }
  .warn-box { margin: 0 0 12px; padding: 8px 12px; border-radius: 8px; background: var(--warn-soft); color: var(--warn); font-size: 13px; }
  .pinned { margin: 6px 0 8px; padding-bottom: 6px; border-bottom: 1px solid var(--border); }
  .ph { font-size: 11px; font-weight: 600; letter-spacing: .04em; text-transform: uppercase; color: var(--text-3); padding: 4px 8px; }
  .pn { display: flex; gap: 8px; align-items: center; padding: 5px 8px; border-radius: var(--radius); color: var(--text); font-size: 13.5px; text-decoration: none; }
  .pn:hover { background: var(--surface-2); }
  .pn.on { background: var(--accent-soft); color: var(--accent); font-weight: 500; }
  .pn .t { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .arch { display: flex; gap: 6px; align-items: center; justify-content: center; font-size: 13px; color: var(--text-3); padding: 4px; border-radius: var(--radius); text-decoration: none; }
  .arch:hover, .arch.on { color: var(--accent); background: var(--surface-2); }
  .pinmark { color: var(--warn, #b54708); margin-right: 6px; display: inline-flex; vertical-align: middle; }
  .who { display: inline-flex; align-items: center; margin-right: 8px; padding-left: 6px; }
  .wa { margin-left: -6px; border: 2px solid var(--surface); border-radius: 50%; display: inline-flex; }
  .wa.ed { border-color: var(--success, #2f9e5b); }
  .editing-now { color: var(--success, #2f9e5b); font-weight: 500; }
  .arow { display: flex; align-items: center; gap: 10px; padding: 10px 0; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .ai { font-size: 20px; }
  .at { flex: 1; min-width: 200px; display: grid; }
  .pact { align-items: center; flex-wrap: wrap; }
  @media (max-width: 860px) {
    .kb { grid-template-columns: minmax(0, 1fr); }
    .side { position: static; max-height: 320px; }
    .pane { padding: 16px; }
  }
</style>
