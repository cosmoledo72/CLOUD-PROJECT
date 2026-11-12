<script lang="ts">
  // Search results: every section the user can see, or one section with
  // paging; tasks, files, comments and chats can be narrowed to a project
  // and a period. The query and filters live in the address (shareable, Back works).
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import Search from '@lucide/svelte/icons/search';
  import SearchX from '@lucide/svelte/icons/search-x';
  import EmptyState from '$lib/EmptyState.svelte';
  import Hl from '$lib/search/Hl.svelte';
  import HitIcon from '$lib/search/HitIcon.svelte';
  import { queryWords, remember, runSearch, type Hit, type HitGroup, type HitType } from '$lib/search/search.svelte';
  import { api, localToday, type Project } from '$lib/api';
  import { session } from '$lib/session.svelte';

  const TABS: { k: HitType | 'all'; label: string; scoped?: boolean }[] = [
    { k: 'all', label: 'Все', scoped: true },
    { k: 'projects', label: 'Проекты' },
    { k: 'tasks', label: 'Задачи', scoped: true },
    { k: 'files', label: 'Файлы', scoped: true },
    { k: 'kb', label: 'База знаний' },
    { k: 'companies', label: 'Контрагенты' },
    { k: 'contacts', label: 'Контакты' },
    { k: 'deals', label: 'Сделки' },
    { k: 'comments', label: 'Комментарии', scoped: true },
    { k: 'messages', label: 'Чаты', scoped: true },
    { k: 'people', label: 'Сотрудники' }
  ];
  const PERIODS = [['', 'За всё время'], ['7', 'За неделю'], ['30', 'За месяц'], ['365', 'За год']];
  const PAGE = 30;

  const url = $derived(page.url.searchParams);
  const q = $derived((url.get('q') ?? '').trim());
  const type = $derived((url.get('type') ?? 'all') as HitType | 'all');
  const project = $derived(url.get('project') ?? '');
  const period = $derived(url.get('period') ?? '');
  const scopedTab = $derived(TABS.find((t) => t.k === type)?.scoped ?? false);

  let text = $state('');
  let groups = $state<HitGroup[]>([]);
  let more = $state(false);
  let busy = $state(false);
  let error = $state('');
  let projects = $state<Project[]>([]);
  // Sections seen in the «all» answer: tabs without hits are dimmed.
  let found = $state<Set<string>>(new Set());

  $effect(() => { text = q; });
  $effect(() => {
    if (session.has('projects')) api.get<{ projects: Project[] }>('/projects').then((r) => (projects = r.projects)).catch(() => {});
  });

  function since(days: string): string | undefined {
    if (!days) return undefined;
    const d = new Date(localToday());
    d.setDate(d.getDate() - Number(days));
    return d.toISOString().slice(0, 10);
  }
  const filters = $derived(scopedTab ? { project: project || undefined, since: since(period) } : {});

  let seq = 0;
  $effect(() => {
    const p = { q, type, ...filters };
    if (q.length < 2) {
      groups = [];
      return;
    }
    const my = ++seq;
    busy = true;
    error = '';
    runSearch({ ...p, limit: type === 'all' ? 6 : PAGE })
      .then((g) => {
        if (my !== seq) return;
        groups = g;
        more = type !== 'all' && !!g[0]?.more;
        if (type === 'all' && !p.project && !p.since) found = new Set(g.map((x) => x.type));
      })
      .catch((e) => { if (my === seq) error = (e as Error).message; })
      .finally(() => { if (my === seq) busy = false; });
  });

  async function loadMore() {
    const g = groups[0];
    if (!g) return;
    busy = true;
    try {
      const next = await runSearch({ q, type, ...filters, offset: g.items.length, limit: PAGE });
      g.items = [...g.items, ...(next[0]?.items ?? [])];
      more = !!next[0]?.more;
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  function nav(change: Record<string, string>) {
    const u = new URLSearchParams(page.url.searchParams);
    for (const [k, v] of Object.entries(change)) {
      if (v) u.set(k, v);
      else u.delete(k);
    }
    goto(`/search?${u}`, { keepFocus: true, noScroll: true, replaceState: 'q' in change && !!q });
  }
  function submit(e: SubmitEvent) {
    e.preventDefault();
    remember(text);
    nav({ q: text.trim() });
  }
  const words = $derived(queryWords(q));
  const hasFilters = $derived(!!project || !!period);
  const visibleTabs = $derived(TABS.filter((t) => t.k === 'all' || found.size === 0 || found.has(t.k) || t.k === type));
  const fmtSize = (b: number) => (b < 1024 * 1024 ? `${Math.max(1, Math.round(b / 1024))} КБ` : `${(b / 1024 / 1024).toFixed(1).replace('.', ',')} МБ`);
  const sub = (h: Hit & { size?: number }) => (h.type === 'files' && h.size ? `${h.subtitle} · ${fmtSize(h.size)}` : h.subtitle);
</script>

<svelte:head><title>{q ? `${q} — поиск` : 'Поиск'}</title></svelte:head>

<div class="page">
  <div class="page-head"><h1>Поиск</h1></div>
  <form class="qbar" onsubmit={submit} role="search">
    <Search size={18} />
    <!-- svelte-ignore a11y_autofocus -->
    <input class="qin" bind:value={text} placeholder="Проекты, задачи, документы, контрагенты…" aria-label="Что найти" autofocus={!q} />
    <button class="btn primary">Найти</button>
  </form>

  {#if q.length >= 2}
    <div class="tabs-line" role="tablist">
      {#each visibleTabs as t (t.k)}
        <button role="tab" aria-selected={type === t.k} onclick={() => nav({ type: t.k === 'all' ? '' : t.k })}>{t.label}</button>
      {/each}
    </div>

    {#if scopedTab}
      <div class="filters">
        {#if projects.length}
          <select class="input" value={project} onchange={(e) => nav({ project: e.currentTarget.value })} aria-label="Проект">
            <option value="">Все проекты</option>
            {#each projects as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
          </select>
        {/if}
        <div class="seg" role="radiogroup" aria-label="Период">
          {#each PERIODS as [k, l] (k)}
            <button role="radio" aria-checked={period === k} onclick={() => nav({ period: k })}>{l}</button>
          {/each}
        </div>
        {#if hasFilters}<button class="btn ghost sm" onclick={() => nav({ project: '', period: '' })}>Сбросить</button>{/if}
        {#if hasFilters && type === 'all'}<span class="note">С фильтром — задачи, файлы, комментарии и чаты</span>{/if}
      </div>
    {/if}

    {#if error}<div class="error-box">{error}</div>{/if}
    {#if !busy && !error && !groups.some((g) => g.items.length)}
      <EmptyState icon={SearchX} title="Ничего не найдено" text="Попробуйте другое слово или начало слова: поиск учитывает окончания и опечатки. В тексте ищутся документы Word, Excel и PDF; старые DOC, XLS и RTF — только по названию." />
    {/if}

    {#each groups as g (g.type)}
      {#if g.items.length}
        <section class="grp" class:solo={type !== 'all'}>
          {#if type === 'all'}
            <h2>{g.label}{#if g.more}<button class="all" onclick={() => nav({ type: g.type })}>Показать все</button>{/if}</h2>
          {/if}
          <ul class="card">
            {#each g.items as h (h.type + h.id)}
              <li>
                <a href={h.url}>
                  <HitIcon hit={h} />
                  <span class="tx">
                    <span class="t">{#if h.number}<span class="num">#{h.number}</span>{/if}<Hl text={h.title} {words} /></span>
                    {#if h.snippet}<span class="sn"><Hl text={h.snippet} /></span>{/if}
                    {#if h.subtitle}<span class="sub">{sub(h)}</span>{/if}
                  </span>
                </a>
              </li>
            {/each}
          </ul>
          {#if type !== 'all' && more}
            <button class="btn ghost loadmore" disabled={busy} onclick={loadMore}>{busy ? 'Загружаю…' : 'Показать ещё'}</button>
          {/if}
        </section>
      {/if}
    {/each}
  {:else}
    <p class="muted hint">Введите не меньше двух символов. Поиск идёт по названиям и тексту документов, задачам и комментариям, CRM, чатам и сотрудникам — только там, куда у вас есть доступ.</p>
  {/if}
</div>

<style>
  .qbar { display: flex; align-items: center; gap: 10px; max-width: 760px; padding: 6px 6px 6px 14px; border: 1px solid var(--border); border-radius: 12px;
          background: var(--surface); color: var(--text-3); margin-bottom: 18px; box-shadow: var(--shadow); }
  .qbar:focus-within { border-color: var(--accent); }
  .qin { flex: 1; min-width: 0; border: 0; outline: 0; background: none; font: inherit; font-size: 16px; color: var(--text); }
  .filters { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; margin: -6px 0 16px; }
  .filters select { width: auto; max-width: 320px; }
  .note { font-size: 12.5px; color: var(--text-3); }
  .grp { max-width: 980px; margin-bottom: 22px; }
  .grp h2 { display: flex; align-items: baseline; gap: 12px; font-size: 13px; text-transform: uppercase; letter-spacing: .04em; color: var(--text-3); margin: 0 0 8px; }
  .all { border: 0; background: none; font: inherit; font-size: 13px; text-transform: none; letter-spacing: 0; color: var(--accent); cursor: pointer; padding: 0; }
  ul { list-style: none; margin: 0; padding: 4px; }
  li + li { border-top: 1px solid var(--border); }
  li a { display: flex; gap: 12px; align-items: flex-start; padding: 10px 10px; border-radius: 8px; color: var(--text); }
  li a:hover { background: var(--surface-3); text-decoration: none; }
  .tx { display: grid; gap: 2px; min-width: 0; }
  .t { font-weight: 500; font-size: 14.5px; overflow-wrap: anywhere; }
  .num { color: var(--text-3); margin-right: 6px; font-variant-numeric: tabular-nums; }
  .sn { font-size: 13px; color: var(--text-2); line-height: 1.5; }
  .sub { font-size: 12.5px; color: var(--text-3); overflow-wrap: anywhere; }
  .loadmore { margin-top: 10px; }
  .hint { max-width: 640px; }
  @media (max-width: 640px) {
    .filters select { max-width: 100%; width: 100%; }
  }
</style>
