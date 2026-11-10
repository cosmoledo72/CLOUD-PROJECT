<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import {
    api, formatDate, formatMoney,
    type Activity, type Company, type CompanyInput, type Contact, type Deal, type UserItem
  } from '$lib/api';
  import { contactName } from '$lib/api';
  import ActivityFeed from '$lib/crm/ActivityFeed.svelte';
  import SalesDocs from '$lib/crm/SalesDocs.svelte';
  import TaskBlock from '$lib/crm/TaskBlock.svelte';
  import CompanyForm from '$lib/crm/CompanyForm.svelte';
  import { emptyCompany, toInput, toPayload } from '$lib/crm/company';
  import Modal from '$lib/Modal.svelte';

  const id = $derived(page.params.id);
  let company = $state<Company | null>(null);
  let contacts = $state<Contact[]>([]);
  let deals = $state<Deal[]>([]);
  let projects = $state<{ id: string; name: string; status: string }[]>([]);
  let activities = $state<Activity[]>([]);
  let canEdit = $state(false);
  let users = $state<UserItem[]>([]);
  let groups = $state<{ id: string; name: string }[]>([]);
  let error = $state('');
  let tab = $state<'card' | 'contacts' | 'deals' | 'projects' | 'docs'>('card');

  async function load() {
    const r = await api.get<{ company: Company; contacts: Contact[]; deals: Deal[]; projects: typeof projects; activities: Activity[]; can_edit: boolean }>(`/crm/companies/${id}`);
    company = r.company;
    contacts = r.contacts;
    deals = r.deals;
    projects = r.projects;
    activities = r.activities;
    canEdit = r.can_edit;
  }
  $effect(() => {
    id;
    load().catch((e) => (error = e.message));
    api.get<{ users: UserItem[] }>('/users').then((r) => (users = r.users));
    api.get<{ groups: typeof groups }>('/groups').then((r) => (groups = r.groups));
  });

  async function guarded(fn: () => Promise<unknown>) {
    error = '';
    try {
      await fn();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  // --- Edit card ----------------------------------------------------------------------
  let editOpen = $state(false);
  let draft = $state<CompanyInput>(emptyCompany(null));
  let form = $state<CompanyForm>();
  function openEdit() {
    if (!company) return;
    draft = toInput(company);
    error = '';
    editOpen = true;
  }
  async function saveCard() {
    await guarded(async () => {
      await api.put(`/crm/companies/${id}`, toPayload(draft));
      editOpen = false;
      await load();
    });
  }
  async function remove() {
    if (!company || !confirm(`Удалить контрагента «${company.name}»? Контакты отвяжутся, сделки и проекты сохранятся без заказчика.`)) return;
    await guarded(async () => {
      await api.del(`/crm/companies/${id}`);
      goto('/crm/companies');
    });
  }

  // --- Contacts -----------------------------------------------------------------------
  let cOpen = $state(false);
  let cDraft = $state<Omit<Contact, 'company_name'>>(blankContact());
  function blankContact(): Omit<Contact, 'company_name'> {
    return { id: '', company_id: null, last_name: '', first_name: '', middle_name: '', position: '', phone: '', email: '', notes: '' };
  }
  function openContact(c?: Contact) {
    cDraft = c ? { ...c } : { ...blankContact(), company_id: id ?? null };
    error = '';
    cOpen = true;
  }
  async function saveContact() {
    await guarded(async () => {
      const body = { ...cDraft, company_id: id };
      if (cDraft.id) await api.put(`/crm/contacts/${cDraft.id}`, body);
      else await api.post('/crm/contacts', body);
      cOpen = false;
      await load();
    });
  }
  async function removeContact(c: Contact) {
    if (!confirm(`Удалить контакт «${contactName(c)}»?`)) return;
    await guarded(async () => {
      await api.del(`/crm/contacts/${c.id}`);
      await load();
    });
  }

  // --- New deal for this company ----------------------------------------------------------
  async function newDeal() {
    const title = prompt('Название сделки');
    if (!title?.trim()) return;
    await guarded(async () => {
      const r = await api.post<{ deal: Deal }>('/crm/deals', { title, company_id: id });
      goto(`/crm/deals/${r.deal.id}`);
    });
  }

  const crmPeople = $derived(users.filter((u) => u.is_active && u.role !== 'guest' && u.crm_role !== 'none').map((u) => ({ id: u.id, name: u.display_name })));
  const openDeals = $derived(deals.filter((d) => d.stage_kind === 'open'));
  const wonSum = $derived(deals.filter((d) => d.stage_kind === 'won').reduce((s, d) => s + d.amount, 0));
  const statusText: Record<string, string> = { active: 'Активный', paused: 'Приостановлен', closed: 'Закрыт' };
  const kindText: Record<string, string> = { open: 'в работе', won: 'выиграна', lost: 'проиграна' };

  const req = $derived(company ? [
    ['Полное наименование', company.full_name],
    ['ИНН', company.inn], ['КПП', company.kpp], ['ОГРН', company.ogrn],
    ['Руководитель', company.director],
    ['Юридический адрес', company.legal_address], ['Почтовый адрес', company.postal_address],
    ['Телефон', company.phone], ['Email', company.email], ['Сайт', company.website],
    ['Банк', company.bank_name], ['БИК', company.bik], ['Расчётный счёт', company.account], ['Корр. счёт', company.corr_account]
  ].filter(([, v]) => v) as [string, string][] : []);

  function copyRequisites() {
    if (!company) return;
    const text = [company.full_name || company.name, ...req.filter(([k]) => k !== 'Полное наименование').map(([k, v]) => `${k}: ${v}`)].join('\n');
    navigator.clipboard?.writeText(text);
  }
</script>

{#if company}
  <div class="page-head">
    <a href="/crm/companies" class="faint">Контрагенты</a><span class="faint">/</span>
    <h1>{company.name}</h1>
    {#each company.tags as t}<span class="badge">{t}</span>{/each}
    <div class="spacer"></div>
    <button class="btn" onclick={newDeal}>Новая сделка</button>
    {#if canEdit}
      <button class="btn" onclick={openEdit}>Редактировать</button>
      <button class="btn danger" onclick={remove}>Удалить</button>
    {/if}
  </div>
  {#if error && !editOpen && !cOpen}<div class="error-box">{error}</div>{/if}

  <div class="kpis">
    <div class="kpi"><span class="faint small">Ответственный</span><b>{company.responsible_name ?? '—'}</b></div>
    <div class="kpi"><span class="faint small">Сделки в работе</span><b>{openDeals.length} · {formatMoney(openDeals.reduce((s, d) => s + d.amount, 0))}</b></div>
    <div class="kpi"><span class="faint small">Выиграно</span><b>{formatMoney(wonSum)}</b></div>
    <div class="kpi"><span class="faint small">Проекты</span><b>{projects.length}</b></div>
  </div>

  <div class="cols">
    <div>
      <div class="tabs" role="tablist">
        <button role="tab" aria-selected={tab === 'card'} class:on={tab === 'card'} onclick={() => (tab = 'card')}>Реквизиты</button>
        <button role="tab" aria-selected={tab === 'contacts'} class:on={tab === 'contacts'} onclick={() => (tab = 'contacts')}>Контакты ({contacts.length})</button>
        <button role="tab" aria-selected={tab === 'deals'} class:on={tab === 'deals'} onclick={() => (tab = 'deals')}>Сделки ({deals.length})</button>
        <button role="tab" aria-selected={tab === 'projects'} class:on={tab === 'projects'} onclick={() => (tab = 'projects')}>Проекты ({projects.length})</button>
        <button role="tab" aria-selected={tab === 'docs'} class:on={tab === 'docs'} onclick={() => (tab = 'docs')}>Документы</button>
      </div>

      {#if tab === 'card'}
        <section class="card box">
          {#if req.length}
            <dl class="req">
              {#each req as [k, v]}<dt>{k}</dt><dd class:mono={['ИНН', 'КПП', 'ОГРН', 'БИК', 'Расчётный счёт', 'Корр. счёт'].includes(k)}>{v}</dd>{/each}
            </dl>
            <button class="btn sm copy" onclick={copyRequisites}>Скопировать реквизиты</button>
          {:else}
            <p class="faint">Реквизиты не заполнены.</p>
          {/if}
          {#if company.notes}<p class="notes">{company.notes}</p>{/if}
        </section>
      {:else if tab === 'contacts'}
        <section class="card box">
          {#if canEdit}<div class="row"><div class="spacer"></div><button class="btn sm" onclick={() => openContact()}>Добавить контакт</button></div>{/if}
          {#each contacts as c (c.id)}
            <div class="contact">
              <div>
                <b>{contactName(c)}</b>{#if c.position}<span class="muted"> · {c.position}</span>{/if}
                <div class="small muted">
                  {#if c.phone}<a href="tel:{c.phone}">{c.phone}</a>{/if}
                  {#if c.email}{c.phone ? ' · ' : ''}<a href="mailto:{c.email}">{c.email}</a>{/if}
                </div>
              </div>
              <div class="spacer"></div>
              {#if canEdit}
                <button class="btn ghost sm" onclick={() => openContact(c)}>Изменить</button>
                <button class="btn ghost sm danger" onclick={() => removeContact(c)}>Удалить</button>
              {/if}
            </div>
          {:else}
            <p class="faint">Контактных лиц нет.</p>
          {/each}
        </section>
      {:else if tab === 'deals'}
        <section class="card box">
          {#each deals as d (d.id)}
            <a class="line" href="/crm/deals/{d.id}">
              <span class="t">{d.title}</span>
              <span class="badge" class:ok={d.stage_kind === 'won'} class:danger={d.stage_kind === 'lost'}>{kindText[d.stage_kind]}</span>
              <span class="spacer"></span>
              <b>{formatMoney(d.amount)}</b>
            </a>
          {:else}
            <p class="faint">Сделок нет.</p>
          {/each}
        </section>
      {:else if tab === 'docs'}
        <section class="card box">
          <SalesDocs companyId={company.id} canEdit={canEdit} />
        </section>
      {:else}
        <section class="card box">
          {#each projects as p (p.id)}
            <a class="line" href="/projects/{p.id}"><span class="t">{p.name}</span><span class="spacer"></span><span class="muted">{statusText[p.status] ?? p.status}</span></a>
          {:else}
            <p class="faint">Проектов нет. Проект создаётся из выигранной сделки или привязывается на вкладке «Реквизиты» проекта.</p>
          {/each}
        </section>
      {/if}
    </div>

    <div class="side">
    <section class="card box">
      <TaskBlock companyId={company.id} people={crmPeople} onchange={load} />
    </section>
    <section class="card box">
      <h3>История взаимодействий</h3>
      <ActivityFeed people={users.filter((u) => u.is_active && u.role !== 'guest' && u.crm_role !== 'none').map((u) => ({ id: u.id, name: u.display_name }))} {activities} companyId={company.id} {contacts} onchange={load} />
    </section>
    </div>
  </div>
  <p class="faint small created">Создан {formatDate(company.created_at)}</p>
{:else if error}
  <div class="error-box">{error}</div>
{/if}

<Modal bind:open={editOpen} title="Контрагент" width={760}>
  <CompanyForm bind:this={form} bind:value={draft} excludeId={id} {users} {groups} />
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (editOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!draft.name.trim() || form?.hasErrors()} onclick={saveCard}>Сохранить</button>
  {/snippet}
</Modal>

<Modal bind:open={cOpen} title={cDraft.id ? 'Контактное лицо' : 'Новое контактное лицо'}>
  <div class="three">
    <label class="field">Фамилия <input class="input" bind:value={cDraft.last_name} /></label>
    <label class="field">Имя <input class="input" bind:value={cDraft.first_name} /></label>
    <label class="field">Отчество <input class="input" bind:value={cDraft.middle_name} /></label>
  </div>
  <label class="field">Должность <input class="input" bind:value={cDraft.position} /></label>
  <div class="two">
    <label class="field">Телефон <input class="input" type="tel" bind:value={cDraft.phone} /></label>
    <label class="field">Email <input class="input" type="email" bind:value={cDraft.email} /></label>
  </div>
  <label class="field">Примечание <textarea class="input" rows="2" bind:value={cDraft.notes}></textarea></label>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (cOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!cDraft.last_name.trim() && !cDraft.first_name.trim()} onclick={saveContact}>Сохранить</button>
  {/snippet}
</Modal>

<style>
  .error-box { margin-bottom: 10px; }
  .kpis { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 10px; margin-bottom: 14px; }
  .kpi { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 10px 14px; display: grid; gap: 2px; }
  .kpi b { font-variant-numeric: tabular-nums; }
  .cols { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 12px; align-items: start; }
  .tabs { display: flex; gap: 2px; margin-bottom: 8px; flex-wrap: wrap; }
  .tabs button { border: 0; background: none; padding: 6px 10px; border-bottom: 2px solid transparent; color: var(--text-2); cursor: pointer; }
  .tabs button.on { color: var(--accent); border-color: var(--accent); }
  .box { padding: 14px; display: grid; gap: 10px; grid-template-columns: minmax(0, 1fr); min-width: 0; }
  .box h3 { margin: 0; }
  .side { display: grid; gap: 12px; align-content: start; min-width: 0; }
  .box p { margin: 0; }
  .req { display: grid; grid-template-columns: 170px 1fr; gap: 6px 14px; margin: 0; }
  .req dt { color: var(--text-3); }
  .req dd { margin: 0; overflow-wrap: anywhere; }
  .copy { justify-self: start; }
  .notes { white-space: pre-wrap; background: var(--surface-2); padding: 8px 10px; border-radius: var(--radius); }
  .contact { display: flex; align-items: center; gap: 8px; padding: 6px 0; border-bottom: 1px solid var(--border); }
  .line { display: flex; align-items: center; gap: 8px; padding: 8px 4px; border-bottom: 1px solid var(--border); color: var(--text); }
  .line:hover { background: var(--surface-2); text-decoration: none; }
  .t { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .created { margin: 10px 0 0; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .three { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 10px; }
  @media (max-width: 960px) { .cols { grid-template-columns: 1fr; } .kpis { grid-template-columns: 1fr 1fr; } }
  @media (max-width: 560px) { .req { grid-template-columns: 1fr; } .two, .three { grid-template-columns: 1fr; } }
</style>
