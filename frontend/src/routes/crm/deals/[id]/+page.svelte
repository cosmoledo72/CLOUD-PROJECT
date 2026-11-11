<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { nameDef,
    api, formatDateTime, formatMoney,
    type Activity, type CompanyListItem, type Contact, type Deal, type FieldDef, type FieldValues, type Stage, type UserItem
  } from '$lib/api';
  import { contactName } from '$lib/api';
  import ActivityFeed from '$lib/crm/ActivityFeed.svelte';
  import SalesDocs from '$lib/crm/SalesDocs.svelte';
  import TaskBlock from '$lib/crm/TaskBlock.svelte';
  import Modal from '$lib/Modal.svelte';
  import RequisitesForm from '$lib/RequisitesForm.svelte';
  import { session } from '$lib/session.svelte';

  const id = $derived(page.params.id);
  let deal = $state<Deal | null>(null);
  let activities = $state<Activity[]>([]);
  let canEdit = $state(false);
  let stages = $state<Stage[]>([]);
  let companies = $state<CompanyListItem[]>([]);
  let contacts = $state<Contact[]>([]);
  let users = $state<UserItem[]>([]);
  let error = $state('');
  let notice = $state('');

  // Editable copy of the deal fields.
  let f = $state({ title: '', company_id: '', contact_id: '', amount: '', expected_close: '', responsible_id: '', description: '', lost_reason: '' });

  async function load() {
    const r = await api.get<{ deal: Deal; activities: Activity[]; can_edit: boolean }>(`/crm/deals/${id}`);
    deal = r.deal;
    activities = r.activities;
    canEdit = r.can_edit;
    f = {
      title: r.deal.title, company_id: r.deal.company_id ?? '', contact_id: r.deal.contact_id ?? '',
      amount: String(r.deal.amount), expected_close: r.deal.expected_close ?? '', responsible_id: r.deal.responsible_id ?? '',
      description: r.deal.description, lost_reason: r.deal.lost_reason
    };
  }
  $effect(() => {
    id;
    Promise.all([
      load(),
      api.get<{ stages: Stage[] }>('/crm/stages').then((r) => (stages = r.stages)),
      api.get<{ companies: CompanyListItem[] }>('/crm/companies').then((r) => (companies = r.companies)),
      api.get<{ users: UserItem[] }>('/users').then((r) => (users = r.users))
    ]).catch((e) => (error = e.message));
  });
  $effect(() => {
    const cid = f.company_id;
    if (!cid) {
      contacts = [];
      f.contact_id = '';
      return;
    }
    api.get<{ contacts: Contact[] }>(`/crm/contacts?company_id=${cid}`).then((r) => {
      contacts = r.contacts;
      // A contact of the previous company cannot stay on the deal.
      if (f.contact_id && !contacts.some((c) => c.id === f.contact_id)) f.contact_id = '';
    });
  });

  const dirty = $derived(
    !!deal &&
      (f.title !== deal.title || f.company_id !== (deal.company_id ?? '') || f.contact_id !== (deal.contact_id ?? '') ||
        Number(f.amount.replace(/\s/g, '').replace(',', '.')) !== deal.amount || f.expected_close !== (deal.expected_close ?? '') ||
        f.responsible_id !== (deal.responsible_id ?? '') || f.description !== deal.description || f.lost_reason !== deal.lost_reason)
  );

  async function guarded(fn: () => Promise<unknown>, ok = '') {
    error = '';
    notice = '';
    try {
      await fn();
      notice = ok;
    } catch (e) {
      error = (e as Error).message;
    }
  }

  const save = () => guarded(async () => {
    await api.patch(`/crm/deals/${id}`, {
      title: f.title,
      company_id: f.company_id || null,
      contact_id: f.contact_id || null,
      amount: Number(f.amount.replace(/\s/g, '').replace(',', '.')) || 0,
      expected_close: f.expected_close || null,
      responsible_id: f.responsible_id || null,
      description: f.description,
      lost_reason: f.lost_reason
    });
    await load();
  }, 'Сохранено.');

  const setStage = (s: Stage) => guarded(async () => {
    let reason: string | undefined;
    if (s.kind === 'lost') reason = prompt('Причина проигрыша (необязательно)') ?? undefined;
    await api.patch(`/crm/deals/${id}`, { stage_id: s.id, ...(reason ? { lost_reason: reason } : {}) });
    await load();
  });

  async function remove() {
    if (!deal || !confirm(`Удалить сделку «${deal.title}» вместе с историей?`)) return;
    await guarded(async () => {
      await api.del(`/crm/deals/${id}`);
      goto('/crm/deals');
    });
  }

  // --- Project from deal ---------------------------------------------------------
  let projOpen = $state(false);
  let projName = $state('');
  let projValues = $state<FieldValues>({});
  let defs = $state<FieldDef[]>([]);
  // «Наименование объекта» is the project name: one input instead of two.
  const projNd = $derived(nameDef(defs));
  let projUsers = $state<UserItem[]>([]);
  async function openProject() {
    const [d, fd] = await Promise.all([
      api.get<{ name: string; fields: FieldValues }>(`/crm/deals/${id}/project`),
      api.get<{ fields: FieldDef[] }>('/project-fields')
    ]);
    projName = d.name;
    projValues = { ...d.fields };
    defs = fd.fields.filter((x) => !x.is_archived);
    projUsers = users;
    projOpen = true;
  }
  async function createProject() {
    const fields = Object.fromEntries(Object.entries(projValues).filter(([, v]) => v !== '' && v !== null && v !== false));
    await guarded(async () => {
      const r = await api.post<{ project_id: string }>(`/crm/deals/${id}/project`, { name: projName, fields });
      projOpen = false;
      goto(`/projects/${r.project_id}`);
    });
  }

  const crmPeople = $derived(users.filter((u) => u.is_active && u.role !== 'guest' && u.crm_role !== 'none').map((u) => ({ id: u.id, name: u.display_name })));
  const stageOf = $derived(stages.find((s) => s.id === deal?.stage_id));
</script>

{#if deal}
  <div class="page-head">
    <a href="/crm/deals" class="faint">Сделки</a><span class="faint">/</span>
    <h1>{deal.title}</h1>
    <div class="spacer"></div>
    {#if deal.project_id}
      <a class="btn" href="/projects/{deal.project_id}">Проект: {deal.project_name}</a>
    {:else if canEdit && session.has('projects')}
      <button class="btn" class:primary={deal.stage_kind === 'won'} onclick={openProject}>Создать проект</button>
    {/if}
    {#if canEdit}<button class="btn danger" onclick={remove}>Удалить</button>{/if}
  </div>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#if notice}<div class="ok-box">{notice}</div>{/if}

  <div class="stages" role="radiogroup" aria-label="Этап сделки">
    {#each stages as s (s.id)}
      <button class="stage k-{s.kind}" class:on={s.id === deal.stage_id} style="--c:{s.color}" disabled={!canEdit}
              role="radio" aria-checked={s.id === deal.stage_id} onclick={() => setStage(s)}>{s.name}</button>
    {/each}
  </div>

  <div class="cols">
    <section class="card box">
      <h3>Сделка</h3>
      <div class="grid">
        <label class="field wide">Название <input class="input" bind:value={f.title} disabled={!canEdit} /></label>
        <label class="field">Контрагент
          <select class="input" bind:value={f.company_id} disabled={!canEdit}>
            <option value="">—</option>
            {#each companies as c}<option value={c.id}>{c.name}</option>{/each}
          </select>
          {#if deal.company_id}<a class="small" href="/crm/companies/{deal.company_id}">Открыть карточку</a>{/if}
        </label>
        <label class="field">Контактное лицо
          <select class="input" bind:value={f.contact_id} disabled={!canEdit || !contacts.length}>
            <option value="">—</option>
            {#each contacts as c}<option value={c.id}>{contactName(c)}</option>{/each}
          </select>
        </label>
        <label class="field">Сумма, ₽ <input class="input" inputmode="decimal" bind:value={f.amount} disabled={!canEdit} /></label>
        <label class="field">Ожидаемое закрытие <input class="input" type="date" bind:value={f.expected_close} disabled={!canEdit} /></label>
        <label class="field">Ответственный
          <select class="input" bind:value={f.responsible_id} disabled={!canEdit}>
            <option value="">—</option>
            {#each users.filter((u) => u.is_active && u.crm_role !== 'none') as u}<option value={u.id}>{u.display_name}</option>{/each}
          </select>
        </label>
        <div class="field">
          <span>Вероятность</span>
          <span class="val">{stageOf?.probability ?? 0}% · {formatMoney(Math.round((deal.amount * (stageOf?.probability ?? 0)) / 100))}</span>
        </div>
        <label class="field wide">Описание <textarea class="input" rows="4" bind:value={f.description} disabled={!canEdit}></textarea></label>
        {#if deal.stage_kind === 'lost'}
          <label class="field wide">Причина проигрыша <input class="input" bind:value={f.lost_reason} disabled={!canEdit} /></label>
        {/if}
      </div>
      <div class="row meta faint small">
        <span>Создана {formatDateTime(deal.created_at)}</span>
        {#if deal.closed_at}<span>· закрыта {formatDateTime(deal.closed_at)}</span>{/if}
        <div class="spacer"></div>
        {#if canEdit}<button class="btn primary" disabled={!dirty} onclick={save}>Сохранить</button>{/if}
      </div>
    </section>

    <div class="side">
    <section class="card box">
      <TaskBlock dealId={deal.id} people={crmPeople} onchange={load} />
    </section>
    <section class="card box">
      <SalesDocs dealId={deal.id} {canEdit} onchange={load} />
    </section>
    <section class="card box">
      <h3>История</h3>
      <ActivityFeed people={users.filter((u) => u.is_active && u.role !== 'guest' && u.crm_role !== 'none').map((u) => ({ id: u.id, name: u.display_name }))} {activities} dealId={deal.id} companyId={deal.company_id} {contacts} onchange={load} />
    </section>
    </div>
  </div>
{:else if error}
  <div class="error-box">{error}</div>
{/if}

<Modal bind:open={projOpen} title="Проект по сделке" width={640}>
  {#if projNd}
    <label class="field">{projNd.label}<span class="faint small">— это и название проекта</span>
      <textarea class="input" rows="2" bind:value={projName}></textarea>
    </label>
  {:else}
    <label class="field">Название проекта <input class="input" bind:value={projName} /></label>
  {/if}
  <p class="faint small">Заказчик, ИНН и стоимость подставлены из сделки. Заполните обязательные реквизиты.</p>
  <RequisitesForm {defs} bind:values={projValues} users={projUsers} exclude={projNd ? [projNd.key] : []} />
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (projOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!projName.trim()} onclick={createProject}>Создать проект</button>
  {/snippet}
</Modal>

<style>
  .error-box, .ok-box { margin-bottom: 10px; }
  .stages { display: flex; gap: 4px; flex-wrap: wrap; margin-bottom: 14px; }
  .stage {
    border: 1px solid var(--border-strong); background: var(--surface); color: var(--text-2); cursor: pointer;
    padding: 5px 12px; border-radius: 999px; font-size: 13px;
  }
  .stage.on { background: var(--c); border-color: var(--c); color: #fff; font-weight: 500; }
  .stage:disabled { cursor: default; }
  .cols { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 12px; align-items: start; }
  .box { padding: 16px; display: grid; gap: 12px; grid-template-columns: minmax(0, 1fr); min-width: 0; }
  .box h3 { margin: 0; }
  .side { display: grid; gap: 12px; align-content: start; min-width: 0; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px 12px; }
  .wide { grid-column: 1 / -1; }
  .val { color: var(--text); padding-top: 6px; }
  .meta { flex-wrap: wrap; }
  @media (max-width: 960px) { .cols { grid-template-columns: 1fr; } }
  @media (max-width: 560px) { .grid { grid-template-columns: 1fr; } }
</style>
