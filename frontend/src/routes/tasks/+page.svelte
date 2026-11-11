<script lang="ts">
  // "All tasks": tasks of every project the user can see, as a list, a month
  // calendar or a combined Gantt chart, with server-side filters.
  import { page } from '$app/state';
  import { api, localToday, priorityLabel, type BoardProject, type BoardResponse, type Dependency, type Task, type TaskStatus, type UserItem } from '$lib/api';
  import Gantt from '$lib/Gantt.svelte';
  import Modal from '$lib/Modal.svelte';
  import TaskRows from '$lib/TaskRows.svelte';
  import { session } from '$lib/session.svelte';
  import LabelChip from '$lib/tasks/LabelChip.svelte';
  import MultiFilter from '$lib/tasks/MultiFilter.svelte';
  import TaskCalendar from '$lib/tasks/TaskCalendar.svelte';
  import TaskCard from '$lib/tasks/TaskCard.svelte';
  import { LABEL_COLORS, labels } from '$lib/tasks/labels.svelte';
  import CalendarDays from '@lucide/svelte/icons/calendar-days';
  import ChartGantt from '@lucide/svelte/icons/chart-gantt';
  import List from '@lucide/svelte/icons/list';
  import Plus from '@lucide/svelte/icons/plus';
  import Search from '@lucide/svelte/icons/search';
  import Tags from '@lucide/svelte/icons/tags';

  type View = 'list' | 'calendar' | 'gantt';
  type StatusScope = 'open' | 'all' | 'done';
  type Saved = { view: View; projects: string[]; people: string[]; labelIds: string[]; priorities: string[]; scope: StatusScope; closed: boolean };

  const STORE = 'wh_tasks_board';
  function restore(): Partial<Saved> {
    try {
      return JSON.parse(localStorage.getItem(STORE) ?? '{}');
    } catch {
      return {};
    }
  }
  const saved = restore();
  const pick = <T extends string>(v: unknown, allowed: readonly T[], d: T): T => (allowed.includes(v as T) ? (v as T) : d);
  const arr = (v: unknown) => (Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : []);

  let view = $state<View>(pick(page.url.searchParams.get('view') ?? saved.view, ['list', 'calendar', 'gantt'] as const, 'list'));
  let projects = $state<string[]>(arr(saved.projects));
  let people = $state<string[]>(arr(saved.people));
  let labelIds = $state<string[]>(arr(saved.labelIds));
  let priorities = $state<string[]>(arr(saved.priorities));
  let scope = $state<StatusScope>(pick(saved.scope, ['open', 'all', 'done'] as const, 'open'));
  let closed = $state(!!saved.closed);
  let q = $state('');

  $effect(() => {
    const v: Saved = { view, projects, people, labelIds, priorities, scope, closed };
    try {
      localStorage.setItem(STORE, JSON.stringify(v));
    } catch {
      /* storage unavailable */
    }
  });

  let tasks = $state<Task[]>([]);
  let deps = $state<Dependency[]>([]);
  let projectList = $state<BoardProject[]>([]);
  let today = $state(localToday());
  let truncated = $state(false);
  let users = $state<UserItem[]>([]);
  let loading = $state(true);
  let error = $state('');

  const query = $derived.by(() => {
    const p = new URLSearchParams();
    if (projects.length) p.set('project', projects.join(','));
    if (people.length) p.set('assignee', people.map((x) => (x === 'me' ? session.me?.id ?? '' : x)).join(','));
    if (labelIds.length) p.set('label', labelIds.join(','));
    if (priorities.length) p.set('priority', priorities.join(','));
    if (scope === 'open') p.set('status', 'open,in_progress,review');
    if (scope === 'done') p.set('status', 'done');
    if (closed) p.set('closed', 'true');
    if (q.trim()) p.set('q', q.trim());
    return p.toString();
  });

  let seq = 0;
  async function load() {
    const my = ++seq;
    error = '';
    try {
      const r = await api.get<BoardResponse>(`/tasks${query ? `?${query}` : ''}`);
      if (my !== seq) return;
      tasks = r.tasks;
      deps = r.dependencies;
      projectList = r.projects;
      today = r.today;
      truncated = r.truncated;
      // Drop filters for projects that are no longer visible.
      const ids = new Set(r.projects.map((p) => p.id));
      if (projects.some((p) => !ids.has(p))) projects = projects.filter((p) => ids.has(p));
    } catch (e) {
      if (my === seq) error = (e as Error).message;
    } finally {
      if (my === seq) loading = false;
    }
  }
  // Reload on any filter change; typing in the search box is debounced.
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    query;
    clearTimeout(timer);
    timer = setTimeout(load, 250);
    return () => clearTimeout(timer);
  });
  $effect(() => {
    labels.load().catch(() => {});
    api.get<{ users: UserItem[] }>('/users').then((r) => (users = r.users)).catch(() => {});
  });

  const roleOf = $derived(new Map(projectList.map((p) => [p.id, p.role])));
  const canEditTask = (t: Task) => roleOf.get(t.project_id) !== 'viewer' || t.assignee_id === session.me?.id;
  const editableProjects = $derived(projectList.filter((p) => p.role !== 'viewer' && p.status === 'active'));

  const projectOptions = $derived(projectList.map((p) => ({ id: p.id, label: p.name, hint: p.code ?? undefined })));
  const peopleOptions = $derived([
    { id: 'me', label: 'Я' },
    { id: 'none', label: 'Без исполнителя' },
    ...users.filter((u) => u.is_active && u.id !== session.me?.id).map((u) => ({ id: u.id, label: u.display_name }))
  ]);
  const labelOptions = $derived(labels.list.map((l) => ({ id: l.id, label: l.name, color: l.color })));
  const priorityOptions = [4, 3, 2, 1].map((p) => ({ id: String(p), label: priorityLabel[p] }));
  const filtered = $derived(projects.length + people.length + labelIds.length + priorities.length > 0 || !!q.trim());
  function resetFilters() {
    projects = [];
    people = [];
    labelIds = [];
    priorities = [];
    q = '';
  }

  // Gantt: dated tasks grouped by project.
  const ganttTasks = $derived(
    tasks.filter((t) => t.start_date || t.due_date).sort((a, b) => a.project_name.localeCompare(b.project_name, 'ru') || a.sort_order - b.sort_order)
  );

  // --- Task card -----------------------------------------------------------
  let cardOpen = $state(false);
  let cardTask = $state<string | null>(null);
  let cardProject = $state('');
  function openTask(t: Task) {
    cardTask = t.id;
    cardOpen = true;
  }
  let info = $state('');
  function onChanged(shifted?: Task[]) {
    if (shifted?.length) {
      info = `Сдвинуты связанные задачи (${shifted.length}): ${shifted.slice(0, 3).map((t) => `«${t.title}»`).join(', ')}${shifted.length > 3 ? ' и др.' : ''}`;
      setTimeout(() => (info = ''), 8000);
    }
    load();
  }
  // Links like /tasks?task=<id> open the card directly.
  let fromLink = '';
  $effect(() => {
    const want = page.url.searchParams.get('task');
    if (want && want !== fromLink) {
      fromLink = want;
      cardTask = want;
      cardOpen = true;
    }
  });

  // --- New task: choose the project first --------------------------------------
  let pickOpen = $state(false);
  let pickProject = $state('');
  function newTask() {
    if (editableProjects.length === 1) {
      startNew(editableProjects[0].id);
      return;
    }
    pickProject = projects.find((p) => editableProjects.some((e) => e.id === p)) ?? '';
    pickOpen = true;
  }
  function startNew(pid: string) {
    pickOpen = false;
    cardTask = null;
    cardProject = pid;
    cardOpen = true;
  }

  async function setStatus(t: Task, status: TaskStatus) {
    error = '';
    try {
      const r = await api.patch<{ task: Task }>(`/tasks/${t.id}`, { status });
      Object.assign(t, r.task);
      await load();
    } catch (e) {
      error = (e as Error).message;
    }
  }
  async function reschedule(taskId: string, start: string, due: string) {
    const t = tasks.find((x) => x.id === taskId);
    if (!t) return;
    const prev = { s: t.start_date, d: t.due_date };
    t.start_date = start;
    t.due_date = due;
    try {
      const r = await api.patch<{ shifted: Task[] }>(`/tasks/${taskId}`, { start_date: start, due_date: due });
      onChanged(r.shifted);
    } catch (e) {
      t.start_date = prev.s;
      t.due_date = prev.d;
      error = (e as Error).message;
    }
  }

  // --- Label catalogue (admins) ------------------------------------------------
  let labelsOpen = $state(false);
  let labelError = $state('');
  let newName = $state('');
  let newColor = $state(LABEL_COLORS[1]);
  async function labelAction(fn: () => Promise<unknown>) {
    labelError = '';
    try {
      await fn();
      load();
    } catch (e) {
      labelError = (e as Error).message;
    }
  }
</script>

<div class="page">
  <div class="page-head">
    <h1>Задачи</h1>
    <div class="seg" role="tablist" aria-label="Вид">
      <button class="btn sm" role="tab" aria-selected={view === 'list'} class:on={view === 'list'} onclick={() => (view = 'list')}><List size={15} strokeWidth={2} />Список</button>
      <button class="btn sm" role="tab" aria-selected={view === 'calendar'} class:on={view === 'calendar'} onclick={() => (view = 'calendar')}><CalendarDays size={15} strokeWidth={2} />Календарь</button>
      {#if session.has('planning')}<button class="btn sm" role="tab" aria-selected={view === 'gantt'} class:on={view === 'gantt'} onclick={() => (view = 'gantt')}><ChartGantt size={15} strokeWidth={2} />Гант</button>{/if}
    </div>
    <div class="spacer"></div>
    {#if labels.canManage}<button class="btn" onclick={() => (labelsOpen = true)}><Tags size={16} strokeWidth={1.75} />Метки</button>{/if}
    {#if editableProjects.length}<button class="btn primary" onclick={newTask}><Plus size={16} strokeWidth={2} />Задача</button>{/if}
  </div>

  <div class="filters">
    <label class="search">
      <Search size={16} strokeWidth={1.75} />
      <input class="input" placeholder="Название или #номер" bind:value={q} aria-label="Поиск задач по названию или номеру" />
    </label>
    <MultiFilter label="Проекты" options={projectOptions} bind:selected={projects} search={projectOptions.length > 8} />
    <MultiFilter label="Участники" options={peopleOptions} bind:selected={people} search={peopleOptions.length > 8} />
    <MultiFilter label="Метки" options={labelOptions} bind:selected={labelIds} />
    <MultiFilter label="Приоритет" options={priorityOptions} bind:selected={priorities} />
    <div class="seg" role="group" aria-label="Статус">
      <button class="btn sm" class:on={scope === 'open'} onclick={() => (scope = 'open')}>Открытые</button>
      <button class="btn sm" class:on={scope === 'done'} onclick={() => (scope = 'done')}>Выполненные</button>
      <button class="btn sm" class:on={scope === 'all'} onclick={() => (scope = 'all')}>Все</button>
    </div>
    <label class="check small" title="Показывать задачи приостановленных и закрытых проектов"><input type="checkbox" bind:checked={closed} />Закрытые проекты</label>
    {#if filtered}<button class="btn ghost sm" onclick={resetFilters}>Сбросить фильтры</button>{/if}
  </div>

  {#if error}<div class="error-box">{error}</div>{/if}
  {#if info}<div class="info-box" role="status">{info}</div>{/if}
  {#if truncated}<div class="warn-box">Показаны первые 3000 задач — уточните фильтры.</div>{/if}

  {#if loading}
    <p class="faint">Загрузка…</p>
  {:else if !projectList.length}
    <div class="card empty">
      <p>Вы пока не участвуете ни в одном проекте.</p>
      <a class="btn" href="/projects">Перейти к проектам</a>
    </div>
  {:else if view === 'list'}
    <TaskRows {tasks} canEdit={false} {canEditTask} showProject search={false} storageKey="wh_task_rows_all" meId={session.me?.id} onopen={openTask} onstatus={setStatus} />
  {:else if view === 'calendar' || (view === 'gantt' && !session.has('planning'))}
    <TaskCalendar {tasks} {today} onopen={openTask} onundated={() => (view = 'list')} />
  {:else}
    {#if ganttTasks.length < tasks.length}<p class="faint small hint">Задачи без сроков на диаграмме не показаны: {tasks.length - ganttTasks.length}.</p>{/if}
    <Gantt tasks={ganttTasks} dependencies={deps} {canEditTask} groupOf={(t) => t.project_name} onchange={reschedule} onselect={openTask} />
  {/if}
</div>

<TaskCard bind:open={cardOpen} taskId={cardTask} projectId={cardProject} onchanged={onChanged} />

<Modal bind:open={pickOpen} title="Новая задача" width={440}>
  <label class="field">Проект
    <select class="input" bind:value={pickProject}>
      <option value="" disabled>Выберите проект…</option>
      {#each editableProjects as p (p.id)}<option value={p.id}>{p.code ? `${p.code} · ` : ''}{p.name}</option>{/each}
    </select>
  </label>
  {#snippet footer()}
    <button class="btn" onclick={() => (pickOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!pickProject} onclick={() => startNew(pickProject)}>Далее</button>
  {/snippet}
</Modal>

<Modal bind:open={labelsOpen} title="Метки задач" width={520}>
  <p class="faint small none">Метки общие для всего портала. Добавлять их может любой сотрудник прямо в карточке задачи; переименовывать и удалять — администратор.</p>
  <div class="lbl-list">
    {#each labels.list as l (l.id)}
      <div class="lbl-row">
        <input type="color" value={l.color} aria-label="Цвет метки {l.name}" onchange={(e) => labelAction(() => labels.update(l.id, { color: e.currentTarget.value }))} />
        <input class="input sm" value={l.name} maxlength="40" aria-label="Название метки"
               onchange={(e) => labelAction(() => labels.update(l.id, { name: e.currentTarget.value }))} />
        <span class="faint small cnt">{l.task_count} зад.</span>
        <button class="btn ghost sm danger" onclick={() => confirm(`Удалить метку «${l.name}»? Она снимется со всех задач.`) && labelAction(() => labels.remove(l.id))}>Удалить</button>
      </div>
    {:else}
      <p class="faint none">Меток пока нет.</p>
    {/each}
  </div>
  <div class="lbl-row add">
    <LabelChip label={{ name: newName.trim() || 'Новая метка', color: newColor }} />
    <input class="input sm" placeholder="Название" maxlength="40" bind:value={newName} aria-label="Название новой метки" />
    <div class="swatches">
      {#each LABEL_COLORS as c}<button class="sw" class:on={newColor === c} style="background:{c}" onclick={() => (newColor = c)} aria-label="Цвет {c}"></button>{/each}
    </div>
    <button class="btn sm" disabled={!newName.trim()} onclick={() => labelAction(async () => { await labels.create(newName.trim(), newColor); newName = ''; })}>Добавить</button>
  </div>
  {#if labelError}<div class="error-box">{labelError}</div>{/if}
  {#snippet footer()}<button class="btn" onclick={() => (labelsOpen = false)}>Готово</button>{/snippet}
</Modal>

<style>
  .page-head { flex-wrap: wrap; gap: 10px; }
  .seg { display: inline-flex; }
  .seg .btn { border-radius: 0; margin-left: -1px; gap: 5px; }
  .seg .btn:first-child { border-radius: var(--radius) 0 0 var(--radius); margin-left: 0; }
  .seg .btn:last-child { border-radius: 0 var(--radius) var(--radius) 0; }
  .seg .btn.on { background: var(--accent-soft); color: var(--accent); border-color: var(--accent); position: relative; z-index: 1; }
  .filters { display: flex; flex-wrap: wrap; gap: 8px 10px; align-items: center; margin-bottom: 12px; }
  .search { position: relative; display: flex; align-items: center; color: var(--text-3); }
  .search :global(svg) { position: absolute; left: 9px; pointer-events: none; }
  .search .input { padding-left: 30px; width: 230px; height: 30px; }
  .check { display: flex; gap: 6px; align-items: center; color: var(--text-2); }
  .info-box { background: var(--accent-soft); color: var(--accent); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; margin-bottom: 10px; }
  .warn-box { background: var(--warn-soft); color: var(--warn); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; margin-bottom: 10px; }
  .empty { padding: 32px; text-align: center; display: grid; gap: 10px; justify-items: center; }
  .empty p { margin: 0; }
  .hint { margin: 0 0 6px; }
  .none { margin: 0; }
  .lbl-list { display: grid; gap: 6px; max-height: 340px; overflow-y: auto; }
  .lbl-row { display: flex; align-items: center; gap: 8px; }
  .lbl-row .input { flex: 1; min-width: 0; }
  .lbl-row input[type='color'] { width: 30px; height: 28px; padding: 0; border: 1px solid var(--border); border-radius: 6px; background: none; flex: none; }
  .lbl-row.add { flex-wrap: wrap; border-top: 1px solid var(--border); padding-top: 10px; }
  .cnt { white-space: nowrap; }
  .swatches { display: flex; gap: 4px; }
  .sw { width: 18px; height: 18px; border-radius: 50%; border: 2px solid transparent; padding: 0; cursor: pointer; }
  .sw.on { border-color: var(--text); box-shadow: inset 0 0 0 2px var(--surface); }
  @media (max-width: 760px) { .search { flex: 1 1 100%; } .search .input { width: 100%; } }
</style>
