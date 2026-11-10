<script lang="ts">
  // «Дела» block of a deal or company card: open to-dos, planning, history.
  import { api, type CrmTask } from '../api';
  import { crmDue } from './due.svelte';
  import TaskDialog from './TaskDialog.svelte';
  import TaskItem from './TaskItem.svelte';

  let {
    companyId = null,
    dealId = null,
    people = [],
    onchange
  }: {
    companyId?: string | null;
    dealId?: string | null;
    people?: { id: string; name: string }[];
    /** Completion adds a history entry: the card reloads its feed. */
    onchange?: () => void;
  } = $props();

  let tasks = $state<CrmTask[]>([]);
  let done = $state<CrmTask[]>([]);
  let today = $state('');
  let showDone = $state(false);
  let dialogOpen = $state(false);
  let editing = $state<CrmTask | null>(null);

  const filter = $derived(dealId ? `deal_id=${dealId}` : `company_id=${companyId}`);
  async function load() {
    const [o, d] = await Promise.all([
      api.get<{ tasks: CrmTask[]; today: string }>(`/crm/tasks?${filter}`),
      api.get<{ tasks: CrmTask[] }>(`/crm/tasks?${filter}&status=done`)
    ]);
    tasks = o.tasks;
    today = o.today;
    done = d.tasks;
  }
  $effect(() => {
    filter;
    load().catch(() => {});
  });

  function changed() {
    load().catch(() => {});
    crmDue.refresh();
    onchange?.();
  }
  function plan(t: CrmTask | null = null) {
    editing = t;
    dialogOpen = true;
  }
</script>

<div class="block">
  <div class="head">
    <h3>Дела {#if tasks.length}<span class="faint">— {tasks.length}</span>{/if}</h3>
    <div class="spacer"></div>
    <button class="btn sm" onclick={() => plan()}>Запланировать</button>
  </div>
  {#each tasks as t (t.id)}
    <TaskItem task={t} {today} showLinks={!!companyId && !dealId && !!t.deal_id} onchange={changed} onedit={plan} onnext={() => plan()} />
  {:else}
    <p class="faint none">Нет запланированных дел. Запланируйте следующий шаг — звонок, встречу или письмо.</p>
  {/each}
  {#if done.length}
    <button class="btn ghost sm more" onclick={() => (showDone = !showDone)}>
      {showDone ? 'Скрыть выполненные' : `Выполненные (${done.length})`}
    </button>
    {#if showDone}
      {#each done as t (t.id)}<TaskItem task={t} {today} showLinks={false} onchange={changed} onedit={plan} />{/each}
    {/if}
  {/if}
</div>

<TaskDialog bind:open={dialogOpen} task={editing} {companyId} {dealId} {people} {today} onsaved={changed} />

<style>
  .block { display: grid; gap: 2px; }
  .head { display: flex; align-items: center; margin-bottom: 4px; }
  h3 { margin: 0; }
  .none { margin: 4px 0; font-size: 13px; }
  .more { justify-self: start; margin-top: 4px; }
</style>
