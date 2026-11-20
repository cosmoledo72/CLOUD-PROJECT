<script lang="ts">
  // Tasks as rows: grouping, filters, sorting, one-click completion. Used in a
  // project and, with `showProject`, in "All tasks" across projects.
  import { localToday, priorityLabel, statusLabel, type Task, type TaskStatus } from './api';
  import { isOverdue, rangeLabel } from './tasks/dates';
  import { labels } from './tasks/labels.svelte';
  import LabelChip from './tasks/LabelChip.svelte';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Clock from '@lucide/svelte/icons/clock';
  import ListChecks from '@lucide/svelte/icons/list-checks';
  import MessageSquare from '@lucide/svelte/icons/message-square';
  import Paperclip from '@lucide/svelte/icons/paperclip';
  import ProjectBadge from '#lib/projects/ProjectBadge.svelte';
  import Diamond from '@lucide/svelte/icons/diamond';
  import Search from '@lucide/svelte/icons/search';

  let {
    tasks,
    canEdit,
    canEditTask,
    showProject = false,
    storageKey = 'wh_task_rows',
    search = true,
    meId,
    onopen,
    onstatus
  }: {
    tasks: Task[];
    canEdit: boolean;
    /** Per-task edit right (overrides `canEdit`). */
    canEditTask?: (t: Task) => boolean;
    /** Show the project column and allow grouping by project. */
    showProject?: boolean;
    /** localStorage key for the view settings. */
    storageKey?: string;
    /** Show the local search box (off when the page has its own search). */
    search?: boolean;
    meId: string | undefined;
    onopen: (t: Task) => void;
    onstatus: (t: Task, s: TaskStatus) => void;
  } = $props();

  type GroupBy = 'status' | 'assignee' | 'project' | 'none';
  type SortBy = 'order' | 'due' | 'title' | 'priority' | 'number';
  const editable = (t: Task) => (canEditTask ? canEditTask(t) : canEdit);
  let groupBy = $state<GroupBy>('status');
  let sortBy = $state<SortBy>('order');
  let query = $state('');
  let mine = $state(false);
  let hideDone = $state(false);
  let folded = $state<Record<string, boolean>>({});

  $effect(() => {
    labels.load().catch(() => {});
  });

  // Remember the view settings per browser.
  $effect(() => {
    try {
      const v = JSON.parse(localStorage.getItem(storageKey) ?? '{}');
      if (v.groupBy && (v.groupBy !== 'project' || showProject)) groupBy = v.groupBy;
      if (v.sortBy) sortBy = v.sortBy;
      hideDone = !!v.hideDone;
    } catch {
      /* storage unavailable */
    }
  });
  $effect(() => {
    const v = JSON.stringify({ groupBy, sortBy, hideDone });
    try {
      localStorage.setItem(storageKey, v);
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
        (!q || t.title.toLowerCase().includes(q) || (t.assignee_name ?? '').toLowerCase().includes(q) ||
          `#${t.number}` === q || String(t.number) === q) &&
        (!mine || t.assignee_id === meId) &&
        (!hideDone || t.status !== 'done')
    );
    const cmpDate = (a: string | null, b: string | null) => (a ?? '9999') .localeCompare(b ?? '9999');
    if (sortBy === 'due') list = [...list].sort((a, b) => cmpDate(a.due_date, b.due_date));
    else if (sortBy === 'title') list = [...list].sort((a, b) => a.title.localeCompare(b.title, 'ru'));
    else if (sortBy === 'priority') list = [...list].sort((a, b) => b.priority - a.priority || cmpDate(a.due_date, b.due_date));
    else if (sortBy === 'number') list = [...list].sort((a, b) => b.number - a.number);
    return list;
  });

  const groups = $derived.by(() => {
    if (groupBy === 'none') return [{ key: 'all', title: 'Все задачи', items: filtered }];
    if (groupBy === 'status') {
      return statusOrder
        .map((s) => ({ key: s, title: statusLabel[s], items: filtered.filter((t) => t.status === s) }))
        .filter((g) => g.items.length);
    }
    if (groupBy === 'project') {
      const projects = new Map<string, string>();
      for (const t of filtered) projects.set(t.project_id, t.project_name);
      return [...projects.entries()]
        .sort((a, b) => a[1].localeCompare(b[1], 'ru'))
        .map(([id, name]) => ({ key: id, title: name, items: filtered.filter((t) => t.project_id === id) }));
    }
    const names = new Map<string, string>();
    for (const t of filtered) names.set(t.assignee_id ?? '', t.assignee_name ?? 'Без исполнителя');
    return [...names.entries()]
      .sort((a, b) => (a[0] === '' ? 1 : b[0] === '' ? -1 : a[1].localeCompare(b[1], 'ru')))
      .map(([id, name]) => ({ key: id || 'none', title: name, items: filtered.filter((t) => (t.assignee_id ?? '') === id) }));
  });

  function dueClass(t: Task) {
    if (isOverdue(t, today)) return 'late';
    if (t.due_date === today && t.status !== 'done') return 'today';
    return '';
  }
  const initials = (n: string | null) => (n ?? '?').split(/\s+/).map((w) => w[0]).slice(0, 2).join('').toUpperCase();
  // Checklist progress when there is a checklist, otherwise the task's own percentage.
  const pct = (t: Task) =>
    t.status === 'done' ? 100 : t.checklist_total ? Math.round((t.checklist_done / t.checklist_total) * 100) : t.progress;
  function toggleDone(t: Task) {
    onstatus(t, t.status === 'done' ? 'open' : 'done');
  }
</script>

<div class="toolbar">
  {#if search}
    <label class="search">
      <Search size={16} strokeWidth={1.75} />
      <input class="input" placeholder="Поиск по задачам" bind:value={query} aria-label="Поиск по задачам" />
    </label>
  {/if}
  <label class="opt">Группировать
    <select class="input sm" bind:value={groupBy}>
      <option value="status">по статусу</option><option value="assignee">по исполнителю</option>
      {#if showProject}<option value="project">по проекту</option>{/if}<option value="none">нет</option>
    </select>
  </label>
  <label class="opt">Сортировать
    <select class="input sm" bind:value={sortBy}>
      <option value="order">по порядку</option><option value="due">по сроку</option><option value="priority">по приоритету</option>
      <option value="number">по номеру (новые сверху)</option><option value="title">по названию</option>
    </select>
  </label>
  <label class="check"><input type="checkbox" bind:checked={mine} /> Мои</label>
  <label class="check"><input type="checkbox" bind:checked={hideDone} /> Скрыть выполненные</label>
</div>

<div class="card rows" class:wp={showProject}>
  <div class="head-row" aria-hidden="true">
    <span></span><span>Задача</span>{#if showProject}<span>Проект</span>{/if}<span>Статус</span><span>Исполнитель</span><span>Сроки</span><span>Готово</span>
  </div>
  {#each groups as g (g.key)}
    {#if groupBy !== 'none'}
      <button class="group" onclick={() => (folded[g.key] = !folded[g.key])} aria-expanded={!folded[g.key]}>
        <span class="chev" class:shut={folded[g.key]}><ChevronDown size={16} strokeWidth={2} /></span>
        {#if groupBy === 'status'}<span class="dot st-{g.key}"></span>{/if}
        {#if groupBy === 'project' && g.items[0]}
          <ProjectBadge id={g.key} name={g.title} color={g.items[0].project_badge_color} icon={g.items[0].project_badge_icon} size={20} />
        {/if}
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
              <button class="tick" class:on={t.status === 'done'} disabled={!editable(t) && t.assignee_id !== meId}
                      onclick={() => toggleDone(t)} aria-label={t.status === 'done' ? `Вернуть в работу: ${t.title}` : `Выполнено: ${t.title}`}>
                <svg viewBox="0 0 16 16" width="11" height="11" aria-hidden="true"><path d="M3 8.5l3 3 7-7" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg>
              </button>
            {/if}
          </span>
          <span class="c-title">
            <span class="line1">
              <button class="ttl" onclick={() => onopen(t)} title={t.title}><span class="num">#{t.number}</span>{t.title}</button>
              {#if t.priority === 4}<span class="prio p4">{priorityLabel[4]}</span>{:else if t.priority === 3}<span class="prio p3" title="Высокий приоритет">!</span>{/if}
            </span>
            {#if t.label_ids.length || t.checklist_total || t.comment_count || t.attachment_count}
            <span class="extra">
            {#each t.label_ids as lid (lid)}
              {@const l = labels.byId(lid)}
              {#if l}<LabelChip label={l} small />{/if}
            {/each}
            {#if t.checklist_total}
              <span class="meta" class:all={t.checklist_done === t.checklist_total} title="Чек-лист: {t.checklist_done} из {t.checklist_total}">
                <ListChecks size={13} strokeWidth={2} />{t.checklist_done}/{t.checklist_total}
              </span>
            {/if}
            {#if t.comment_count}<span class="meta" title="Комментарии"><MessageSquare size={13} strokeWidth={2} />{t.comment_count}</span>{/if}
            {#if t.attachment_count}<span class="meta" title="Файлы"><Paperclip size={13} strokeWidth={2} />{t.attachment_count}</span>{/if}
            </span>
            {/if}
          </span>
          {#if showProject}
            <a class="c-proj" href="/projects/{t.project_id}?tab=list" title={t.project_name}>
              <ProjectBadge id={t.project_id} name={t.project_name} color={t.project_badge_color} icon={t.project_badge_icon} size={18} /><span>{t.project_name}</span>
            </a>
          {/if}
          <span class="c-status">
            {#if editable(t)}
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
            {#if t.start_date || t.due_date}<span class="dpill {dueClass(t)}">{rangeLabel(t.start_date, t.due_date)}</span>
            {:else}<span class="faint">—</span>{/if}
          </span>
          <span class="c-prog">
            <span class="bar" title="{pct(t)}%{t.checklist_total ? ' по чек-листу' : ''}"><span style="width: {pct(t)}%"></span></span>
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
    display: grid; grid-template-columns: 36px minmax(260px, 1fr) 132px 190px 130px 130px; align-items: center; gap: 8px;
    padding: 0 12px; min-width: 900px;
  }
  .wp .head-row, .wp .row { grid-template-columns: 36px minmax(260px, 1fr) 160px 132px 170px 130px 110px; min-width: 1060px; }
  .wp .group { min-width: 1060px; }
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
  .c-title { display: grid; gap: 2px; min-width: 0; padding: 5px 0; }
  .line1 { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .ttl { border: 0; background: none; padding: 0; text-align: left; font: inherit; color: var(--text); cursor: pointer; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: min(100%, 170px); flex: 0 1 auto; }
  /* Badges give way to the title when the column is narrow. */
  /* Second line: labels and counters. Badges that do not fit wrap onto a hidden line instead of being cut. */
  .extra { display: flex; flex-wrap: wrap; align-content: flex-start; align-items: center; gap: 0 6px; height: 18px; overflow: hidden; }
  .extra > :global(*) { height: 18px; }
  .ttl:hover { color: var(--accent); }
  .num { color: var(--text-3); font-size: 12px; margin-right: 6px; font-variant-numeric: tabular-nums; }
  .done .ttl { color: var(--text-3); text-decoration: line-through; }
  .prio { flex: none; font-size: 11px; font-weight: 600; border-radius: 999px; padding: 1px 7px; }
  .prio.p4 { background: var(--danger); color: #fff; }
  .prio.p3 { background: var(--warn-soft); color: var(--warn); padding: 1px 6px; }
  .meta { flex: none; display: inline-flex; align-items: center; gap: 3px; font-size: 12px; color: var(--text-3); }
  .meta.all { color: var(--ok); }
  .c-proj { font-size: 13px; color: var(--text-2); text-decoration: none; display: flex; align-items: center; gap: 6px; min-width: 0; }
  .c-proj span:last-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .c-proj:hover { color: var(--accent); }
  .dpill { display: inline-block; padding: 2px 9px; border-radius: 999px; background: var(--surface-2); color: var(--text-2); font-size: 12.5px; }
  .dpill.late { background: var(--danger-soft); color: var(--danger); font-weight: 600; }
  .dpill.today { background: var(--warn-soft); color: var(--warn); font-weight: 600; }
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
  .c-prog { display: flex; align-items: center; gap: 8px; }
  .bar { flex: 1; height: 6px; border-radius: 3px; background: var(--surface-2); overflow: hidden; max-width: 70px; }
  .bar span { display: block; height: 100%; background: var(--ok); border-radius: 3px; }
  .spent { display: flex; gap: 3px; align-items: center; font-size: 12px; color: var(--text-3); white-space: nowrap; }
  .empty { padding: 24px; text-align: center; margin: 0; }
  @media (max-width: 760px) { .search .input { width: 100%; } .search { flex: 1; } }
</style>
