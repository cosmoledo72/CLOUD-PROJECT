<script lang="ts">
  // Create or edit a CRM to-do. The company/deal can be fixed by the caller
  // (deal or company card) or picked from the directory (to-do list).
  import { api, contactName, crmTaskKindLabel, type CompanyRef, type Contact, type CrmTask, type CrmTaskKind } from '../api';
  import Modal from '../Modal.svelte';
  import { session } from '../session.svelte';
  import CompanyPicker from './CompanyPicker.svelte';

  let {
    open = $bindable(false),
    task = null,
    companyId = null,
    dealId = null,
    people = [],
    today,
    onsaved
  }: {
    open?: boolean;
    /** The task to edit; a new one when null. */
    task?: CrmTask | null;
    companyId?: string | null;
    dealId?: string | null;
    people?: { id: string; name: string }[];
    today: string;
    onsaved: () => void;
  } = $props();

  type Form = { kind: CrmTaskKind; title: string; due_date: string; due_time: string; assignee_id: string; contact_id: string; notes: string };
  let f = $state<Form>({ kind: 'call', title: '', due_date: '', due_time: '', assignee_id: '', contact_id: '', notes: '' });
  let company = $state<CompanyRef | null>(null);
  let contacts = $state<Contact[]>([]);
  let error = $state('');
  let busy = $state(false);

  // Fill the form each time the dialog opens.
  let wasOpen = false;
  $effect(() => {
    if (open && !wasOpen) {
      error = '';
      if (task) {
        f = {
          kind: task.kind, title: task.title, due_date: task.due_date, due_time: task.due_time?.slice(0, 5) ?? '',
          assignee_id: task.assignee_id, contact_id: task.contact_id ?? '', notes: task.notes
        };
        company = task.company_id ? { id: task.company_id, name: task.company_name ?? '', inn: null } : null;
      } else {
        f = { kind: 'call', title: '', due_date: today, due_time: '', assignee_id: session.me?.id ?? '', contact_id: '', notes: '' };
        company = null;
      }
    }
    wasOpen = open;
  });

  const fixed = $derived(!!(task || companyId || dealId));
  const effectiveCompany = $derived(task?.company_id ?? companyId ?? (company?.id || null));
  $effect(() => {
    const cid = effectiveCompany;
    if (!cid || !open) {
      contacts = [];
      return;
    }
    api.get<{ contacts: Contact[] }>(`/crm/contacts?company_id=${cid}`).then((r) => (contacts = r.contacts)).catch(() => (contacts = []));
  });

  // Typical titles for a quick start.
  const suggestions: Record<CrmTaskKind, string[]> = {
    call: ['Перезвонить', 'Уточнить условия', 'Узнать решение по КП'],
    meeting: ['Встреча на объекте', 'Переговоры по договору'],
    email: ['Отправить КП', 'Отправить договор', 'Отправить акт'],
    todo: ['Подготовить КП', 'Подготовить договор']
  };

  function shift(days: number) {
    const d = new Date(today + 'T00:00:00');
    d.setDate(d.getDate() + days);
    f.due_date = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  }

  async function save() {
    error = '';
    busy = true;
    try {
      const body = {
        kind: f.kind, title: f.title, notes: f.notes, due_date: f.due_date, due_time: f.due_time || null,
        assignee_id: f.assignee_id || null, contact_id: f.contact_id || null
      };
      if (task) {
        await api.patch(`/crm/tasks/${task.id}`, body);
      } else {
        await api.post('/crm/tasks', { ...body, company_id: companyId ?? (company?.id || null), deal_id: dealId });
      }
      open = false;
      onsaved();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!task || !confirm(`Удалить «${task.title}»?`)) return;
    try {
      await api.del(`/crm/tasks/${task.id}`);
      open = false;
      onsaved();
    } catch (e) {
      error = (e as Error).message;
    }
  }
</script>

<Modal bind:open title={task ? 'Дело' : 'Новое дело'} width={560}>
  <div class="kinds" role="radiogroup" aria-label="Тип">
    {#each Object.entries(crmTaskKindLabel) as [k, l]}
      <button type="button" role="radio" aria-checked={f.kind === k} class="chip" class:on={f.kind === k} onclick={() => (f.kind = k as CrmTaskKind)}>{l}</button>
    {/each}
  </div>
  <label class="field">Что сделать
    <input class="input" bind:value={f.title} maxlength="300" placeholder="Например: перезвонить и узнать решение по КП" />
  </label>
  {#if !f.title}
    <div class="sugg">{#each suggestions[f.kind] as s}<button type="button" class="btn ghost sm" onclick={() => (f.title = s)}>{s}</button>{/each}</div>
  {/if}
  <div class="when">
    <label class="field">Дата <input class="input" type="date" bind:value={f.due_date} required /></label>
    <label class="field">Время <input class="input" type="time" bind:value={f.due_time} /></label>
    <div class="quick">
      <button type="button" class="btn ghost sm" onclick={() => shift(0)}>Сегодня</button>
      <button type="button" class="btn ghost sm" onclick={() => shift(1)}>Завтра</button>
      <button type="button" class="btn ghost sm" onclick={() => shift(7)}>Через неделю</button>
    </div>
  </div>
  <p class="faint small">С указанным временем напомним за 15 минут, без времени — утром в день срока.</p>
  {#if !fixed}
    <div class="field"><span>Контрагент</span><CompanyPicker bind:value={company} /></div>
  {:else if task?.company_name || task?.deal_title}
    <p class="small muted">{task?.deal_title ? `Сделка «${task.deal_title}»` : ''}{task?.deal_title && task?.company_name ? ', ' : ''}{task?.company_name ?? ''}</p>
  {/if}
  <div class="two">
    {#if contacts.length}
      <label class="field">Контактное лицо
        <select class="input" bind:value={f.contact_id}>
          <option value="">—</option>
          {#each contacts as c}<option value={c.id}>{contactName(c)}{c.phone ? ` · ${c.phone}` : ''}</option>{/each}
        </select>
      </label>
    {/if}
    <label class="field">Исполнитель
      <select class="input" bind:value={f.assignee_id}>
        {#each people as p}<option value={p.id}>{p.id === session.me?.id ? `${p.name} (я)` : p.name}</option>{/each}
      </select>
    </label>
  </div>
  <label class="field">Комментарий <textarea class="input" rows="2" bind:value={f.notes}></textarea></label>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    {#if task?.can_edit}<button class="btn danger" onclick={remove}>Удалить</button>{/if}
    <div class="spacer"></div>
    <button class="btn" onclick={() => (open = false)}>Отмена</button>
    <button class="btn primary" disabled={busy || !f.title.trim() || !f.due_date || (task !== null && !task.can_edit)} onclick={save}>
      {task ? 'Сохранить' : 'Запланировать'}
    </button>
  {/snippet}
</Modal>

<style>
  .kinds { display: flex; gap: 6px; flex-wrap: wrap; }
  .chip { border: 1px solid var(--border-strong); background: var(--surface); border-radius: 999px; padding: 4px 12px; cursor: pointer; color: var(--text-2); font: inherit; font-size: 13px; }
  .chip.on { background: var(--accent); border-color: var(--accent); color: #fff; }
  .sugg { display: flex; gap: 4px; flex-wrap: wrap; margin-top: -6px; }
  .when { display: grid; grid-template-columns: 1fr 120px; gap: 10px; align-items: end; }
  .quick { grid-column: 1 / -1; display: flex; gap: 4px; margin-top: -4px; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  p { margin: 0; }
  @media (max-width: 560px) { .two { grid-template-columns: 1fr; } }
</style>
