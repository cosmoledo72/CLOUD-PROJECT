<script lang="ts">
  import { api, type CrmTask, type UserItem } from '#lib/api.ts';
  import { crmDue } from '#lib/crm/due.svelte.ts';
  import TaskDialog from '#lib/crm/TaskDialog.svelte';
  import TaskItem from '#lib/crm/TaskItem.svelte';
  import { session } from '#lib/session.svelte.ts';

  let tasks = $state<CrmTask[]>([]);
  let done = $state<CrmTask[]>([]);
  let today = $state('');
  let users = $state<UserItem[]>([]);
  let scope = $state<'mine' | 'all'>('mine');
  let assignee = $state('');
  let view = $state<'open' | 'done'>('open');
  let error = $state('');
  let loaded = $state(false);

  async function load() {
    const q = new URLSearchParams();
    if (assignee) q.set('assignee_id', assignee);
    else if (scope === 'all') q.set('scope', 'all');
    if (view === 'done') q.set('status', 'done');
    const r = await api.get<{ tasks: CrmTask[]; today: string }>(`/crm/tasks?${q}`);
    if (view === 'done') done = r.tasks;
    else tasks = r.tasks;
    today = r.today;
    loaded = true;
  }
  $effect(() => {
    [scope, assignee, view];
    load().catch((e) => (error = e.message));
  });
  $effect(() => {
    api.get<{ users: UserItem[] }>('/users').then((r) => (users = r.users)).catch(() => {});
  });

  const people = $derived(
    users.filter((u) => u.is_active && u.role !== 'guest' && u.crm_role !== 'none').map((u) => ({ id: u.id, name: u.display_name }))
  );

  function addDays(iso: string, n: number) {
    const d = new Date(iso + 'T00:00:00');
    d.setDate(d.getDate() + n);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  }
  const groups = $derived.by(() => {
    if (!today) return [];
    const tomorrow = addDays(today, 1);
    const week = addDays(today, 7);
    const g = [
      { key: 'overdue', title: 'Просрочено', items: tasks.filter((t) => t.due_date < today) },
      { key: 'today', title: 'Сегодня', items: tasks.filter((t) => t.due_date === today) },
      { key: 'tomorrow', title: 'Завтра', items: tasks.filter((t) => t.due_date === tomorrow) },
      { key: 'week', title: 'На неделе', items: tasks.filter((t) => t.due_date > tomorrow && t.due_date <= week) },
      { key: 'later', title: 'Позже', items: tasks.filter((t) => t.due_date > week) }
    ];
    return g.filter((x) => x.items.length);
  });

  // Dialog: edit, new, or the next step of a completed task.
  let dialogOpen = $state(false);
  let editing = $state<CrmTask | null>(null);
  let fixedCompany = $state<string | null>(null);
  let fixedDeal = $state<string | null>(null);
  function edit(t: CrmTask | null) {
    editing = t;
    fixedCompany = null;
    fixedDeal = null;
    dialogOpen = true;
  }
  function next(t: CrmTask) {
    editing = null;
    fixedDeal = t.deal_id;
    fixedCompany = t.deal_id ? null : t.company_id;
    dialogOpen = true;
  }
  const reload = () => {
    crmDue.refresh();
    return load().catch((e) => (error = e.message));
  };
</script>

<div class="page-head">
  <h1>Дела</h1>
  <div class="spacer"></div>
  <div class="seg" role="radiogroup" aria-label="Чьи дела">
    <button role="radio" aria-checked={scope === 'mine' && !assignee} class:on={scope === 'mine' && !assignee} onclick={() => { scope = 'mine'; assignee = ''; }}>Мои</button>
    <button role="radio" aria-checked={scope === 'all' && !assignee} class:on={scope === 'all' && !assignee} onclick={() => { scope = 'all'; assignee = ''; }}>Все</button>
  </div>
  {#if session.crmManager}
    <select class="input who" bind:value={assignee} aria-label="Сотрудник">
      <option value="">Сотрудник…</option>
      {#each people as p}<option value={p.id}>{p.name}</option>{/each}
    </select>
  {/if}
  <select class="input who" bind:value={view} aria-label="Статус">
    <option value="open">Запланированные</option>
    <option value="done">Выполненные</option>
  </select>
  <button class="btn primary" onclick={() => edit(null)}>Новое дело</button>
</div>
{#if error}<div class="error-box">{error}</div>{/if}

{#if view === 'done'}
  <section class="card list">
    {#each done as t (t.id)}
      <TaskItem task={t} {today} onchange={reload} onedit={edit} />
    {:else}
      <p class="faint empty">Выполненных дел нет.</p>
    {/each}
  </section>
{:else if loaded && !groups.length}
  <div class="card empty-state">
    <p class="muted">Запланированных дел нет.</p>
    <p class="faint small">Дела — это следующие шаги по клиентам: позвонить, встретиться, отправить КП. О каждом придёт напоминание.</p>
  </div>
{:else}
  {#each groups as g (g.key)}
    <section class="card list">
      <h2 class="g-{g.key}">{g.title} <span class="faint">{g.items.length}</span></h2>
      {#each g.items as t (t.id)}
        <TaskItem task={t} {today} onchange={reload} onedit={edit} onnext={next} />
      {/each}
    </section>
  {/each}
{/if}

<TaskDialog bind:open={dialogOpen} task={editing} companyId={fixedCompany} dealId={fixedDeal} {people} {today} onsaved={reload} />

<style>
  .who { width: auto; }
  .error-box { margin-bottom: 10px; }
  .list { padding: 6px 14px 8px; margin-bottom: 12px; }
  h2 { font-size: 14px; margin: 6px 0 2px; }
  .g-overdue { color: var(--danger); }
  .g-today { color: var(--warn); }
  .empty { margin: 10px 0; }
  .empty-state { padding: 36px; text-align: center; }
  .empty-state p { margin: 0 0 6px; }
  @media (max-width: 760px) { .who { flex: 1; } }
</style>
