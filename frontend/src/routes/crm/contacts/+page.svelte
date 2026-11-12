<script lang="ts">
  import { onIntent } from '$lib/intent';
  import EmptyState from '$lib/EmptyState.svelte';
  import ContactIcon from '@lucide/svelte/icons/contact';
  import SearchX from '@lucide/svelte/icons/search-x';
  import { api, contactName, type CompanyListItem, type Contact } from '$lib/api';
  import Modal from '$lib/Modal.svelte';

  let contacts = $state<Contact[]>([]);
  let companies = $state<CompanyListItem[]>([]);
  let query = $state('');
  onIntent('q', (v) => (query = v));
  let error = $state('');
  let loaded = $state(false);

  async function load() {
    const p = new URLSearchParams();
    if (query.trim()) p.set('q', query.trim());
    contacts = (await api.get<{ contacts: Contact[] }>(`/crm/contacts?${p}`)).contacts;
    loaded = true;
  }
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    query;
    clearTimeout(timer);
    timer = setTimeout(() => load().catch((e) => (error = e.message)), 250);
  });
  $effect(() => {
    api.get<{ companies: CompanyListItem[] }>('/crm/companies').then((r) => (companies = r.companies));
  });

  function blank(): Omit<Contact, 'company_name'> {
    return { id: '', company_id: null, last_name: '', first_name: '', middle_name: '', position: '', phone: '', email: '', notes: '' };
  }
  let open = $state(false);
  let draft = $state<Omit<Contact, 'company_name'>>(blank());
  function edit(c?: Contact) {
    draft = c ? { ...c } : blank();
    error = '';
    open = true;
  }
  async function save() {
    error = '';
    try {
      const body = { ...draft, company_id: draft.company_id || null };
      if (draft.id) await api.put(`/crm/contacts/${draft.id}`, body);
      else await api.post('/crm/contacts', body);
      open = false;
      await load();
    } catch (e) {
      error = (e as Error).message;
    }
  }
  async function remove() {
    if (!draft.id || !confirm(`Удалить контакт «${contactName(draft)}»?`)) return;
    error = '';
    try {
      await api.del(`/crm/contacts/${draft.id}`);
      open = false;
      await load();
    } catch (e) {
      error = (e as Error).message;
    }
  }
</script>

<div class="page-head">
  <h1>Контакты</h1>
  <div class="spacer"></div>
  <input class="input search" placeholder="ФИО, телефон, email" bind:value={query} aria-label="Поиск" />
  <button class="btn primary" onclick={() => edit()}>Новый контакт</button>
</div>
{#if error && !open}<div class="error-box">{error}</div>{/if}

{#if loaded && contacts.length === 0}
  <div class="card">{#if query}<EmptyState icon={SearchX} title="Ничего не найдено" />{:else}<EmptyState icon={ContactIcon} title="Контактных лиц пока нет" text="Добавьте сотрудников заказчиков и подрядчиков, с кем ведёте переписку." />{/if}</div>
{:else}
  <div class="card scroll">
    <table class="list">
      <thead><tr><th>ФИО</th><th>Должность</th><th>Контрагент</th><th class="hide-sm">Телефон</th><th class="hide-sm">Email</th></tr></thead>
      <tbody>
        {#each contacts as c (c.id)}
          <tr class="click" onclick={() => edit(c)}>
            <td><b>{contactName(c) || '—'}</b></td>
            <td class="muted">{c.position}</td>
            <td>
              {#if c.company_id}
                <a href="/crm/companies/{c.company_id}" onclick={(e) => e.stopPropagation()}>{c.company_name}</a>
              {:else}<span class="faint">—</span>{/if}
            </td>
            <td class="hide-sm"><a href="tel:{c.phone}" onclick={(e) => e.stopPropagation()}>{c.phone}</a></td>
            <td class="hide-sm"><a href="mailto:{c.email}" onclick={(e) => e.stopPropagation()}>{c.email}</a></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<Modal bind:open title={draft.id ? 'Контактное лицо' : 'Новое контактное лицо'}>
  <div class="three">
    <label class="field">Фамилия <input class="input" bind:value={draft.last_name} /></label>
    <label class="field">Имя <input class="input" bind:value={draft.first_name} /></label>
    <label class="field">Отчество <input class="input" bind:value={draft.middle_name} /></label>
  </div>
  <div class="two">
    <label class="field">Контрагент
      <select class="input" bind:value={draft.company_id}>
        <option value={null}>— без компании —</option>
        {#each companies as co}<option value={co.id}>{co.name}</option>{/each}
      </select>
    </label>
    <label class="field">Должность <input class="input" bind:value={draft.position} /></label>
    <label class="field">Телефон <input class="input" type="tel" bind:value={draft.phone} /></label>
    <label class="field">Email <input class="input" type="email" bind:value={draft.email} /></label>
  </div>
  <label class="field">Примечание <textarea class="input" rows="2" bind:value={draft.notes}></textarea></label>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    {#if draft.id}<button class="btn danger" onclick={remove}>Удалить</button>{/if}
    <div class="spacer"></div>
    <button class="btn" onclick={() => (open = false)}>Отмена</button>
    <button class="btn primary" disabled={!draft.last_name.trim() && !draft.first_name.trim()} onclick={save}>Сохранить</button>
  {/snippet}
</Modal>

<style>
  .search { width: 280px; }
  .error-box { margin-bottom: 10px; }
  .scroll { overflow-x: auto; }
  .click { cursor: pointer; }
  .click:hover { background: var(--surface-2); }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .three { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 10px; }
  @media (max-width: 760px) { .hide-sm { display: none; } .search { width: 100%; } }
  @media (max-width: 560px) { .two, .three { grid-template-columns: 1fr; } }
</style>
