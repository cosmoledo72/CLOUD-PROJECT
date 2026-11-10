<script lang="ts">
  // Project tasks as rows: grouping, filters, sorting, one-click completion.
  import { localToday, statusLabel, type Task, type TaskStatus } from './api';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Clock from '@lucide/svelte/icons/clock';
  import Diamond from '@lucide/svelte/icons/diamond';
  import Search from '@lucide/svelte/icons/search';

  let {
    tasks,
    canEdit,
    meId,
    onopen,
    onstatus
  }: {
    tasks: Task[];
    canEdit: boolean;
    meId: string | undefined;
    onopen: (t: Task) => void;
    onstatus: (t: Task, s: TaskStatus) => void;
  } = $props();

  type GroupBy = 'status' | 'assignee' | 'none';
  type SortBy = 'order' | 'due' | 'title';
  let groupBy = $state<GroupBy>('status');
  let sortBy = $state<SortBy>('order');
  let query = $state('');
  let mine = $state(false);
  let hideDone = $state(false);
  let folded = $state<Record<string, boolean>>({});

  // Remember the view settings per browser.
  $effect(() => {
    try {
      const v = JSON.parse(localStorage.getItem('wh_task_rows') ?? '{}');
      if (v.groupBy) groupBy = v.groupBy;
      if (v.sortBy) sortBy = v.sortBy;
      hideDone = !!v.hideDone;
    } catch {
      /* storage unavailable */
    }
  });
  $effect(() => {
    const v = JSON.stringify({ groupBy, sortBy, hideDone });
    try {
      localStorage.setItem('wh_task_rows', v);
    } catch {
      /* storage unavailable */
    }
  });

  const today = localToday();
  const statusOrder: TaskStatus[] = ['in_progress', 'review', 'open', 'done'];

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    let list = tasks.filter(
      (t) =>
        (!q || t.title.toLowerCase().includes(q) || (t.assignee_name ?? '').toLowerCase().includes(q)) &&
        (!mine || t.assignee_id === meId) &&
        (!hideDone || t.status !== 'done')
    );
    const cmpDate = (a: string | null, b: string | null) => (a ?? '9999') .localeCompare(b ?? '9999');
    if (sortBy === 'due') list = [...list].sort((a, b) => cmpDate(a.due_date, b.due_date));
    else if (sortBy === 'title') list = [...list].sort((a, b) => a.title.localeCompare(b.title, 'ru'));
    return list;
  });

  const groups = $derived.by(() => {
    if (groupBy === 'none') return [{ key: 'all', title: 'Все задачи', items: filtered }];
    if (groupBy === 'status') {
      return statusOrder
        .map((s) => ({ key: s, title: statusLabel[s], items: filtered.filter((t) => t.status === s) }))
        .filter((g) => g.items.length);
    }
    const names = new Map<string, string>();
    for (const t of filtered) names.set(t.assignee_id ?? '', t.assignee_name ?? 'Без исполнителя');
    return [...names.entries()]
      .sort((a, b) => (a[0] === '' ? 1 : b[0] === '' ? -1 : a[1].localeCompare(b[1], 'ru')))
      .map(([id, name]) => ({ key: id || 'none', title: name, items: filtered.filter((t) => (t.assignee_id ?? '') === id) }));
  });

  function short(d: string | null) {
    if (!d) return '';
    return new Date(d + 'T00:00:00').toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' }).replace('.', '');
  }
  function dueClass(t: Task) {
    if (!t.due_date || t.status === 'done') return '';
    if (t.due_date < today) return 'late';
    if (t.due_date === today) return 'today';
    return '';
  }
  const initials = (n: string | null) => (n ?? '?').split(/\s+/).map((w) => w[0]).slice(0, 2).join('').toUpperCase();
  function toggleDone(t: Task) {
    onstatus(t, t.status === 'done' ? 'open' : 'done');
  }
</script>

<div class="toolbar">
  <label class="search">
    <Search size={16} strokeWidth={1.75} />
    <input class="input" placeholder="Поиск по задачам" bind:value={query} aria-label="Поиск по задачам" />
  </label>
  <label class="opt">Группировать
    <select class="input sm" bind:value={groupBy}>
      <option value="status">по статусу</option><option value="assignee">по исполнителю</option><option value="none">нет</option>
    </select>
  </label>
  <label class="opt">Сортировать
    <select class="input sm" bind:value={sortBy}>
      <option value="order">по порядку</option><option value="due">по сроку</option><option value="title">по названию</option>
    </select>
  </label>
  <label class="check"><input type="checkbox" bind:checked={mine} /> Мои</label>
  <label class="check"><input type="checkbox" bind:checked={hideDone} /> Скрыть выполненные</label>
</div>

<div class="card rows">
  <div class="head-row" aria-hidden="true">
    <span></span><span>Задача</span><span>Статус</span><span>Исполнитель</span><span>Сроки</span><span>Готово</span>
  </div>
  {#each groups as g (g.key)}
    {#if groupBy !== 'none'}
      <button class="group" onclick={() => (folded[g.key] = !folded[g.key])} aria-expanded={!folded[g.key]}>
        <span class="chev" class:shut={folded[g.key]}><ChevronDown size={16} strokeWidth={2} /></span>
        {#if groupBy === 'status'}<span class="dot st-{g.key}"></span>{/if}
        {g.title}<span class="cnt">{g.items.length}</span>
      </button>
    {/if}
    {#if !folded[g.key]}
      {#each g.items as t (t.id)}
        <div class="row" class:done={t.status === 'done'}>
          <span class="c-check">
            {#if t.is_milestone}
              <span class="ms" title="Веха"><Diamond size={15} strokeWidth={2} /></span>
            {:else}
              <button class="tick" class:on={t.status === 'done'} disabled={!canEdit && t.assignee_id !== meId}
                      onclick={() => toggleDone(t)} aria-label={t.status === 'done' ? `Вернуть в работу: ${t.title}` : `Выполнено: ${t.title}`}>
                <svg viewBox="0 0 16 16" width="11" height="11" aria-hidden="true"><path d="M3 8.5l3 3 7-7" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg>
              </button>
            {/if}
          </span>
          <button class="c-title" onclick={() => onopen(t)}>{t.title}</button>
          <span class="c-status">
            {#if canEdit}
              <select class="pill st-{t.status}" value={t.status} onchange={(e) => onstatus(t, e.currentTarget.value as TaskStatus)} aria-label="Статус: {t.title}">
                {#each Object.entries(statusLabel) as [v, l]}<option value={v}>{l}</option>{/each}
              </select>
            {:else}<span class="pill st-{t.status}">{statusLabel[t.status]}</span>{/if}
          </span>
          <span class="c-who">
            {#if t.assignee_id}<span class="ava" class:me={t.assignee_id === meId}>{initials(t.assignee_name)}</span><span class="nm">{t.assignee_name}</span>
            {:else}<span class="faint">—</span>{/if}
          </span>
          <span class="c-dates">
            {#if t.start_date && t.due_date && t.start_date !== t.due_date}<span class="faint">{short(t.start_date)} –</span>{/if}
            <span class="due {dueClass(t)}">{t.due_date ? short(t.due_date) : '—'}</span>
          </span>
          <span class="c-prog">
            <span class="bar"><span style="width: {t.status === 'done' ? 100 : t.progress}%"></span></span>
            {#if t.spent_minutes}<span class="spent" title="Затрачено"><Clock size={12} strokeWidth={2} />{(t.spent_minutes / 60).toFixed(1)} ч</span>{/if}
          </span>
        </div>
      {/each}
    {/if}
  {:else}
    <p class="faint empty">{tasks.length ? 'Ничего не найдено.' : 'Задач пока нет.'}</p>
  {/each}
</div>

<style>
  .toolbar { display: flex; gap: 10px 16px; align-items: center; flex-wrap: wrap; margin-bottom: 10px; }
  .search { position: relative; display: flex; align-items: center; color: var(--text-3); }
  .search :global(svg) { position: absolute; left: 9px; pointer-events: none; }
  .search .input { padding-left: 30px; width: 240px; }
  .opt { display: flex; gap: 6px; align-items: center; color: var(--text-2); font-size: 13px; }
  .opt .input { width: auto; }
  .check { display: flex; gap: 6px; align-items: center; font-size: 13px; color: var(--text-2); }
  .rows { padding: 0; overflow-x: auto; }
  .head-row, .row {
    display: grid; grid-template-columns: 36px minmax(220px, 1fr) 132px 190px 130px 130px; align-items: center; gap: 8px;
    padding: 0 12px; min-width: 860px;
  }
  .head-row { height: 34px; font-size: 11.5px; text-transform: uppercase; letter-spacing: .04em; color: var(--text-3); border-bottom: 1px solid var(--border); }
  .group {
    display: flex; align-items: center; gap: 8px; width: 100%; border: 0; background: var(--surface-2); cursor: pointer;
    padding: 7px 12px; font: inherit; font-weight: 600; font-size: 13px; color: var(--text); border-bottom: 1px solid var(--border); min-width: 860px;
  }
  .chev { display: grid; color: var(--text-3); transition: transform .12s; }
  .chev.shut { transform: rotate(-90deg); }
  .cnt { color: var(--text-3); font-weight: 500; }
  .dot { width: 8px; height: 8px; border-radius: 50%; }
  .row { min-height: 44px; border-bottom: 1px solid var(--border); }
  .row:hover { background: color-mix(in srgb, var(--accent) 4%, transparent); }
  .tick {
    width: 20px; height: 20px; border-radius: 50%; border: 1.5px solid var(--border-strong); background: var(--surface);
    display: grid; place-items: center; color: transparent; cursor: pointer; padding: 0;
  }
  .tick:hover:not(:disabled) { border-color: var(--ok); color: var(--ok); }
  .tick.on { background: var(--ok); border-color: var(--ok); color: #fff; }
  .tick:disabled { cursor: default; opacity: .5; }
  .ms { display: grid; place-items: center; width: 20px; color: var(--warn); }
  .c-title { border: 0; background: none; padding: 0; text-align: left; font: inherit; color: var(--text); cursor: pointer; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .c-title:hover { color: var(--accent); }
  .done .c-title { color: var(--text-3); text-decoration: line-through; }
  .pill {
    border: 0; border-radius: 999px; padding: 3px 10px; font: inherit; font-size: 12px; font-weight: 500; cursor: pointer;
    appearance: none; -webkit-appearance: none; max-width: 100%;
  }
  span.pill { display: inline-block; cursor: default; }
  .st-open { background: var(--surface-2); color: var(--text-2); }
  .st-in_progress { background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--accent); }
  .st-review { background: var(--warn-soft); color: var(--warn); }
  .st-done { background: var(--ok-soft); color: var(--ok); }
  .dot.st-open { background: var(--text-3); }
  .dot.st-in_progress { background: var(--accent); }
  .dot.st-review { background: var(--warn); }
  .dot.st-done { background: var(--ok); }
  .c-who { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .ava {
    width: 26px; height: 26px; border-radius: 50%; display: grid; place-items: center; flex: none; font-size: 10.5px; font-weight: 600;
    background: var(--surface-2); color: var(--text-2);
  }
  .ava.me { background: var(--accent-soft); color: var(--accent); }
  .nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13.5px; }
  .c-dates { font-size: 13px; display: flex; gap: 4px; white-space: nowrap; }
  .due.late { color: var(--danger); font-weight: 600; }
  .due.today { color: var(--warn); font-weight: 600; }
  .c-prog { display: flex; align-items: center; gap: 8px; }
  .bar { flex: 1; height: 6px; border-radius: 3px; background: var(--surface-2); overflow: hidden; max-width: 70px; }
  .bar span { display: block; height: 100%; background: var(--ok); border-radius: 3px; }
  .spent { display: flex; gap: 3px; align-items: center; font-size: 12px; color: var(--text-3); white-space: nowrap; }
  .empty { padding: 24px; text-align: center; margin: 0; }
  @media (max-width: 760px) { .search .input { width: 100%; } .search { flex: 1; } }
</style>
