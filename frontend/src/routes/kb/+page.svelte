<script lang="ts">
  import { onIntent } from '$lib/intent';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, formatDateTime, type KbHit, type KbNode, type KbPage } from '$lib/api';
  import KbTree from '$lib/kb/KbTree.svelte';
  import { marked } from 'marked';
  import DOMPurify from 'dompurify';
  import BookOpen from '@lucide/svelte/icons/book-open';
  import Search from '@lucide/svelte/icons/search';
  import Plus from '@lucide/svelte/icons/plus';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import FolderInput from '@lucide/svelte/icons/folder-input';
  import X from '@lucide/svelte/icons/x';
  import Eye from '@lucide/svelte/icons/eye';
  import FilePlus2 from '@lucide/svelte/icons/file-plus-2';

  let nodes = $state<KbNode[]>([]);
  let expanded = $state<Record<string, boolean>>({});
  let current = $state<KbPage | null>(null);
  let crumbs = $state<{ id: string; title: string }[]>([]);
  let canDelete = $state(false);
  let error = $state('');
  let loaded = $state(false);

  const pid = $derived(page.url.searchParams.get('p'));

  async function loadTree() {
    nodes = (await api.get<{ pages: KbNode[] }>('/kb/pages')).pages;
    loaded = true;
  }
  $effect(() => { loadTree().catch((e) => (error = e.message)); });

  $effect(() => {
    const id = pid;
    editing = false;
    if (!id) { current = null; return; }
    api.get<{ page: KbPage; breadcrumbs: { id: string; title: string }[]; can_delete: boolean }>(`/kb/pages/${id}`)
      .then((r) => {
        current = r.page;
        crumbs = r.breadcrumbs;
        canDelete = r.can_delete;
        for (const c of r.breadcrumbs) expanded[c.id] = true;
      })
      .catch((e) => { current = null; error = e.status === 404 ? 'Страница не найдена или удалена.' : e.message; });
  });

  // ------------------------------------------------------------------ rendering
  marked.setOptions({ gfm: true, breaks: true });
  const html = (md: string) => DOMPurify.sanitize(marked.parse(md, { async: false }) as string);
  const children = $derived(current ? nodes.filter((n) => n.parent_id === current!.id) : []);
  const roots = $derived(nodes.filter((n) => !n.parent_id));

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
  let isNew = $state(false);
  let newParent = $state<string | null>(null);
  let eTitle = $state('');
  let eIcon = $state('');
  let eBody = $state('');
  let preview = $state(false);
  let busy = $state(false);
  const ICONS = ['', '📘', '📄', '📌', '🛠️', '🧭', '📐', '🏗️', '⚙️', '🧾', '💡', '✅', '⚠️', '👥', '📞', '🎓'];

  onIntent('new', () => startNew(null));
  function startNew(parent: string | null) {
    isNew = true;
    newParent = parent;
    eTitle = '';
    eIcon = '';
    eBody = '';
    preview = false;
    editing = true;
    error = '';
    if (parent) expanded[parent] = true;
  }
  function startEdit() {
    if (!current) return;
    isNew = false;
    eTitle = current.title;
    eIcon = current.icon;
    eBody = current.body;
    preview = false;
    editing = true;
    error = '';
  }
  async function save() {
    busy = true;
    error = '';
    try {
      if (isNew) {
        const r = await api.post<{ id: string }>('/kb/pages', { parent_id: newParent, title: eTitle, icon: eIcon, body: eBody });
        await loadTree();
        editing = false;
        goto(`/kb?p=${r.id}`);
      } else if (current) {
        const r = await api.put<{ page: KbPage }>(`/kb/pages/${current.id}`, {
          title: eTitle, icon: eIcon, body: eBody, base_updated_at: current.updated_at
        });
        current = r.page;
        editing = false;
        await loadTree();
      }
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }
  function onKey(e: KeyboardEvent) {
    if (editing && (e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault();
      if (eTitle.trim()) save();
    }
  }

  // Small formatting helpers for the textarea.
  let ta = $state<HTMLTextAreaElement | null>(null);
  function wrap(before: string, after = before, placeholder = 'текст') {
    if (!ta) return;
    const { selectionStart: s, selectionEnd: en } = ta;
    const sel = eBody.slice(s, en) || placeholder;
    eBody = eBody.slice(0, s) + before + sel + after + eBody.slice(en);
    requestAnimationFrame(() => { ta!.focus(); ta!.setSelectionRange(s + before.length, s + before.length + sel.length); });
  }
  function linePrefix(prefix: string) {
    if (!ta) return;
    const s = ta.selectionStart;
    const ls = eBody.lastIndexOf('\n', s - 1) + 1;
    eBody = eBody.slice(0, ls) + prefix + eBody.slice(ls);
    requestAnimationFrame(() => { ta!.focus(); ta!.setSelectionRange(s + prefix.length, s + prefix.length); });
  }

  // ------------------------------------------------------------------ move / delete
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
      const r = await api.get<{ breadcrumbs: { id: string; title: string }[] }>(`/kb/pages/${current.id}`);
      crumbs = r.breadcrumbs;
      current.parent_id = moveTo || null;
    } catch (e) {
      error = (e as Error).message;
    }
  }
  async function remove() {
    if (!current) return;
    const n = blocked.size;
    if (!confirm(n > 1 ? `Удалить «${current.title}» и вложенные страницы (${n - 1})?` : `Удалить страницу «${current.title}»?`)) return;
    try {
      await api.del(`/kb/pages/${current.id}`);
      const parent = current.parent_id;
      await loadTree();
      goto(parent ? `/kb?p=${parent}` : '/kb');
    } catch (e) {
      error = (e as Error).message;
    }
  }
</script>

<svelte:head><title>{current ? `${current.title} — ` : ''}База знаний</title></svelte:head>
<svelte:window onkeydown={onKey} />

<div class="page">

<div class="kb">
  <aside class="side card">
    <label class="search">
      <Search size={15} />
      <input class="input" placeholder="Поиск по базе" bind:value={q} />
      {#if q}<button class="clear" aria-label="Очистить" onclick={() => (q = '')}><X size={14} /></button>{/if}
    </label>
    <div class="tree">
      <a class="home" class:on={!pid && !editing} href="/kb"><BookOpen size={15} /> Все страницы</a>
      <KbTree {nodes} current={pid} bind:expanded onadd={startNew} />
    </div>
    <button class="btn new" onclick={() => startNew(null)}><Plus size={15} /> Новая страница</button>
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
          <input class="input title" placeholder="Название страницы" bind:value={eTitle} maxlength="200" />
        </div>
        {#if isNew && newParent}
          <p class="faint small">Вложенная страница в «{nodes.find((n) => n.id === newParent)?.title}»</p>
        {/if}
        <div class="tools">
          <button class="btn ghost sm" title="Заголовок" onclick={() => linePrefix('## ')}>H2</button>
          <button class="btn ghost sm" title="Подзаголовок" onclick={() => linePrefix('### ')}>H3</button>
          <button class="btn ghost sm b" title="Жирный" onclick={() => wrap('**')}>B</button>
          <button class="btn ghost sm i" title="Курсив" onclick={() => wrap('_')}>I</button>
          <button class="btn ghost sm" title="Список" onclick={() => linePrefix('- ')}>• Список</button>
          <button class="btn ghost sm" title="Нумерованный список" onclick={() => linePrefix('1. ')}>1. Список</button>
          <button class="btn ghost sm" title="Чек-лист" onclick={() => linePrefix('- [ ] ')}>☐ Чек-лист</button>
          <button class="btn ghost sm" title="Ссылка" onclick={() => wrap('[', '](https://)', 'ссылка')}>Ссылка</button>
          <button class="btn ghost sm" title="Код" onclick={() => wrap('`')}>{'</>'}</button>
          <div class="spacer"></div>
          <button class="btn ghost sm" class:on={preview} onclick={() => (preview = !preview)}><Eye size={14} /> Просмотр</button>
        </div>
        <div class="split" class:both={preview}>
          <textarea class="input body" bind:this={ta} bind:value={eBody} placeholder="Текст в формате Markdown: ## заголовок, **жирный**, - список, | таблица |"></textarea>
          {#if preview}<div class="md preview">{@html html(eBody || '*Пусто*')}</div>{/if}
        </div>
        <div class="actions">
          <button class="btn primary" disabled={busy || !eTitle.trim()} onclick={save}>{isNew ? 'Создать' : 'Сохранить'}</button>
          <button class="btn" onclick={() => { editing = false; error = ''; }}>Отмена</button>
          <span class="faint small">Ctrl+S — сохранить</span>
        </div>
      </section>
    {:else if current}
      <article class="card pane">
        <nav class="crumbs small">
          <a href="/kb">База знаний</a>
          {#each crumbs as c (c.id)}<span>/</span><a href="/kb?p={c.id}">{c.title}</a>{/each}
        </nav>
        <div class="phead">
          <h1 class="h">{#if current.icon}<span class="pi">{current.icon}</span>{/if}{current.title}</h1>
          <div class="pact">
            <button class="btn sm" onclick={startEdit}><Pencil size={14} /> Редактировать</button>
            <button class="btn ghost sm" title="Вложенная страница" onclick={() => startNew(current!.id)}><FilePlus2 size={15} /></button>
            <button class="btn ghost sm" title="Переместить" onclick={() => { moving = !moving; moveTo = current?.parent_id ?? ''; }}><FolderInput size={15} /></button>
            {#if canDelete}<button class="btn ghost sm danger" title="Удалить" onclick={remove}><Trash2 size={15} /></button>{/if}
          </div>
        </div>
        <div class="meta faint small">
          Изменено {formatDateTime(current.updated_at)}{#if current.updated_by_name} · {current.updated_by_name}{/if}
          {#if current.created_by_name && current.created_by_name !== current.updated_by_name} · автор {current.created_by_name}{/if}
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
        {#if current.body.trim()}
          <div class="md">{@html html(current.body)}</div>
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
  .new, .pact .btn, .actions .btn, .h, .tools .btn { display: inline-flex; gap: 6px; align-items: center; }
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
  .ehead { display: flex; gap: 8px; margin-bottom: 6px; }
  .icon { width: 70px; font-size: 16px; }
  .title { flex: 1; font-size: 18px; font-weight: 600; height: 40px; }
  .tools { display: flex; gap: 2px; flex-wrap: wrap; padding: 6px 0; border-bottom: 1px solid var(--border); margin-bottom: 8px; }
  .tools .b { font-weight: 700; }
  .tools .i { font-style: italic; }
  .tools .on { background: var(--accent-soft); color: var(--accent); }
  .split { display: grid; gap: 12px; }
  .split.both { grid-template-columns: 1fr 1fr; }
  .body { min-height: 420px; font-family: var(--mono); font-size: 13px; line-height: 1.55; resize: vertical; height: auto; padding: 10px; }
  .preview { border: 1px dashed var(--border-strong); border-radius: var(--radius); padding: 10px 16px; overflow: auto; max-height: 640px; }
  .actions { display: flex; gap: 8px; align-items: center; margin-top: 12px; }
  /* Rendered Markdown */
  .md { line-height: 1.65; font-size: 15px; overflow-wrap: anywhere; }
  .md :global(h1) { font-size: 22px; margin: 22px 0 8px; }
  .md :global(h2) { font-size: 19px; margin: 22px 0 8px; padding-bottom: 4px; border-bottom: 1px solid var(--border); }
  .md :global(h3) { font-size: 16px; margin: 18px 0 6px; }
  .md :global(p) { margin: 0 0 12px; }
  .md :global(ul), .md :global(ol) { padding-left: 22px; margin: 0 0 12px; }
  .md :global(li) { margin: 3px 0; }
  .md :global(code) { font-family: var(--mono); font-size: .9em; background: var(--surface-2); padding: 1px 5px; border-radius: 4px; }
  .md :global(pre) { background: var(--surface-2); padding: 12px; border-radius: var(--radius); overflow-x: auto; }
  .md :global(pre code) { background: none; padding: 0; }
  .md :global(blockquote) { margin: 0 0 12px; padding: 4px 14px; border-left: 3px solid var(--accent); color: var(--text-2); background: var(--surface-2); }
  .md :global(table) { border-collapse: collapse; margin: 0 0 12px; font-size: 14px; display: block; overflow-x: auto; }
  .md :global(th), .md :global(td) { border: 1px solid var(--border); padding: 6px 10px; text-align: left; }
  .md :global(th) { background: var(--surface-2); }
  .md :global(img) { max-width: 100%; }
  .md :global(hr) { border: 0; border-top: 1px solid var(--border); margin: 18px 0; }
  .md :global(input[type='checkbox']) { margin-right: 8px; accent-color: var(--accent); }
  .md :global(li:has(> input[type='checkbox'])) { list-style: none; margin-left: -20px; }
  @media (max-width: 860px) {
    .kb { grid-template-columns: 1fr; }
    .side { position: static; max-height: 320px; }
    .pane { padding: 16px; }
    .split.both { grid-template-columns: 1fr; }
  }
</style>
