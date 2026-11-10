<script lang="ts">
  // One CRM to-do: completion with an optional result, due marker, links.
  import { api, crmTaskKindLabel, taskWhen, type CrmTask } from '../api';
  import Modal from '../Modal.svelte';
  import { session } from '../session.svelte';

  let {
    task,
    today,
    showLinks = true,
    onchange,
    onedit,
    onnext
  }: {
    task: CrmTask;
    today: string;
    /** Show the company and deal (off inside their own cards). */
    showLinks?: boolean;
    onchange: () => void;
    onedit: (t: CrmTask) => void;
    /** Plan the next step after completion. */
    onnext?: (t: CrmTask) => void;
  } = $props();

  let doneOpen = $state(false);
  let result = $state('');
  let planNext = $state(true);
  let error = $state('');

  const overdue = $derived(!task.done_at && task.due_date < today);
  const isToday = $derived(!task.done_at && task.due_date === today);

  async function complete() {
    error = '';
    try {
      await api.patch(`/crm/tasks/${task.id}`, { done: true, result });
      doneOpen = false;
      onchange();
      if (planNext && onnext) onnext(task);
    } catch (e) {
      error = (e as Error).message;
    }
  }
  async function reopen() {
    await api.patch(`/crm/tasks/${task.id}`, { done: false });
    onchange();
  }

  const icon: Record<string, string> = {
    call: 'M5 4h4l2 5-2.5 1.5a11 11 0 0 0 5 5L15 13l5 2v4a2 2 0 0 1-2 2A16 16 0 0 1 3 6a2 2 0 0 1 2-2',
    meeting: 'M8 7a4 4 0 1 0 8 0 4 4 0 0 0-8 0M4 21v-1a6 6 0 0 1 6-6h4a6 6 0 0 1 6 6v1',
    email: 'M3 6h18v12H3zM3 7l9 6 9-6',
    todo: 'M9 11l3 3 8-8M20 12v7a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11'
  };
</script>

<div class="task" class:done={!!task.done_at}>
  {#if task.done_at}
    <button class="check on" onclick={reopen} disabled={!task.can_edit} title="Вернуть в работу" aria-label="Вернуть в работу">
      <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"><path d="M3 8.5l3 3 7-7" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/></svg>
    </button>
  {:else}
    <button class="check" onclick={() => { result = ''; planNext = !!onnext; doneOpen = true; }} disabled={!task.can_edit}
            title="Выполнено" aria-label="Отметить выполненным: {task.title}"></button>
  {/if}
  <div class="main">
    <button class="title" onclick={() => onedit(task)}>
      <svg class="k" viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-label={crmTaskKindLabel[task.kind]}><path d={icon[task.kind]} /></svg>
      <span>{task.title}</span>
    </button>
    <div class="meta">
      <span class="when" class:overdue class:today={isToday}>{task.done_at ? `выполнено ${new Date(task.done_at).toLocaleDateString('ru-RU')}` : taskWhen(task, today)}</span>
      {#if showLinks && task.deal_id}<a href="/crm/deals/{task.deal_id}">{task.deal_title}</a>{/if}
      {#if showLinks && task.company_id && !task.deal_id}<a href="/crm/companies/{task.company_id}">{task.company_name}</a>{/if}
      {#if showLinks && task.company_id && task.deal_id}<span class="faint">{task.company_name}</span>{/if}
      {#if task.contact_name}<span>{task.contact_name}{#if task.contact_phone}{' '}<a href="tel:{task.contact_phone}">{task.contact_phone}</a>{/if}</span>{/if}
      {#if task.assignee_id !== session.me?.id}<span class="who">→ {task.assignee_name}</span>{/if}
    </div>
    {#if task.notes && !task.done_at}<div class="notes">{task.notes}</div>{/if}
    {#if task.done_at && task.result}<div class="notes">Итог: {task.result}</div>{/if}
  </div>
</div>

<Modal bind:open={doneOpen} title="Выполнено: {task.title}" width={480}>
  <label class="field">Результат
    <textarea class="input" rows="3" bind:value={result} placeholder="О чём договорились — попадёт в историю взаимодействий"></textarea>
  </label>
  {#if onnext}<label class="check-line"><input type="checkbox" bind:checked={planNext} /> Запланировать следующий шаг</label>{/if}
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (doneOpen = false)}>Отмена</button>
    <button class="btn primary" onclick={complete}>Готово</button>
  {/snippet}
</Modal>

<style>
  .task { display: flex; gap: 10px; padding: 8px 4px; border-bottom: 1px solid var(--border); align-items: flex-start; }
  .task:last-child { border-bottom: 0; }
  .check {
    flex: none; width: 20px; height: 20px; border-radius: 50%; border: 1.5px solid var(--border-strong); background: var(--surface);
    cursor: pointer; margin-top: 1px; display: grid; place-items: center; color: #fff; padding: 0;
  }
  .check:hover:not(:disabled) { border-color: var(--ok); background: color-mix(in srgb, var(--ok) 12%, var(--surface)); }
  .check.on { background: var(--ok); border-color: var(--ok); }
  .check:disabled { cursor: default; opacity: .6; }
  .main { min-width: 0; flex: 1; display: grid; gap: 2px; }
  .title { display: flex; gap: 6px; align-items: center; border: 0; background: none; padding: 0; cursor: pointer; text-align: left; font: inherit; font-weight: 500; color: var(--text); }
  .title:hover span { text-decoration: underline; }
  .k { flex: none; color: var(--text-3); }
  .done .title { color: var(--text-3); text-decoration: line-through; }
  .meta { display: flex; flex-wrap: wrap; gap: 4px 12px; font-size: 13px; color: var(--text-2); }
  .when.today { color: var(--warn); font-weight: 600; }
  .when.overdue { color: var(--danger); font-weight: 600; }
  .who { color: var(--text-3); }
  .notes { font-size: 13px; color: var(--text-2); white-space: pre-wrap; overflow-wrap: anywhere; }
  .check-line { display: flex; gap: 6px; align-items: center; }
</style>
