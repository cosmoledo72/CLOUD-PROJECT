<script lang="ts">
  import Plus from '@lucide/svelte/icons/plus';
  import EmptyState from '$lib/EmptyState.svelte';
  import Building2 from '@lucide/svelte/icons/building-2';
  import SearchX from '@lucide/svelte/icons/search-x';
  import { goto } from '$app/navigation';
  import { api, formatDate, formatMoney, upload, type CompanyInput, type CompanyListItem, type UserItem } from '$lib/api';
  import CompanyForm from '$lib/crm/CompanyForm.svelte';
  import { emptyCompany, toPayload } from '$lib/crm/company';
  import Modal from '$lib/Modal.svelte';
  import { session } from '$lib/session.svelte';

  let companies = $state<CompanyListItem[]>([]);
  let users = $state<UserItem[]>([]);
  let groups = $state<{ id: string; name: string }[]>([]);
  let error = $state('');
  let query = $state('');
  let tag = $state('');
  let mineOnly = $state(false);
  let loaded = $state(false);

  async function load() {
    const p = new URLSearchParams();
    if (query.trim()) p.set('q', query.trim());
    if (tag) p.set('tag', tag);
    if (mineOnly && session.me) p.set('responsible_id', session.me.id);
    companies = (await api.get<{ companies: CompanyListItem[] }>(`/crm/companies?${p}`)).companies;
    loaded = true;
  }
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    [query, tag, mineOnly];
    clearTimeout(timer);
    timer = setTimeout(() => load().catch((e) => (error = e.message)), 250);
  });
  $effect(() => {
    api.get<{ users: UserItem[] }>('/users').then((r) => (users = r.users));
    api.get<{ groups: { id: string; name: string }[] }>('/groups').then((r) => (groups = r.groups));
  });

  const allTags = $derived([...new Set(companies.flatMap((c) => c.tags))].sort());

  // --- Create ---------------------------------------------------------------------
  let createOpen = $state(false);
  let draft = $state<CompanyInput>(emptyCompany(null));
  let form = $state<CompanyForm>();
  function openCreate() {
    draft = emptyCompany(session.me?.id ?? null);
    error = '';
    createOpen = true;
  }
  async function create() {
    error = '';
    try {
      const r = await api.post<{ id: string }>('/crm/companies', toPayload(draft));
      createOpen = false;
      goto(`/crm/companies/${r.id}`);
    } catch (e) {
      error = (e as Error).message;
    }
  }

  // --- Import ---------------------------------------------------------------------
  let importOpen = $state(false);
  let importFile = $state<File | null>(null);
  let importResult = $state<{ created: number; total: number; skipped: { row: number; name: string; reason: string }[] } | null>(null);
  let importBusy = $state(false);
  async function runImport() {
    if (!importFile) return;
    error = '';
    importBusy = true;
    try {
      importResult = await upload(`/crm/companies/import`, importFile);
      await load();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      importBusy = false;
    }
  }
  function downloadSample() {
    const csv = '﻿Наименование;Полное наименование;ИНН;КПП;ОГРН;Юридический адрес;Телефон;Email;Руководитель;Банк;БИК;Р/с;К/с;Метки\r\n' +
      'ООО «Пример»;Общество с ограниченной ответственностью «Пример»;7707083893;773601001;1027700132195;г. Москва, ул. Примерная, д. 1;+7 495 000-00-00;info@example.com;Иванов И. И.;ПАО Сбербанк;044525225;40702810938000000001;30101810400000000225;заказчик\r\n';
    const a = document.createElement('a');
    a.href = URL.createObjectURL(new Blob([csv], { type: 'text/csv;charset=utf-8' }));
    a.download = 'контрагенты-шаблон.csv';
    a.click();
    URL.revokeObjectURL(a.href);
  }
</script>

<div class="page-head">
  <h1>Контрагенты</h1>
  <div class="spacer"></div>
  <input class="input search" placeholder="Название, ИНН, телефон, email" bind:value={query} aria-label="Поиск" />
  {#if allTags.length}
    <select class="input tagsel" bind:value={tag} aria-label="Метка">
      <option value="">Все метки</option>
      {#each allTags as t}<option value={t}>{t}</option>{/each}
    </select>
  {/if}
  <label class="check small"><input type="checkbox" bind:checked={mineOnly} />Мои</label>
  <button class="btn" onclick={() => { importResult = null; importFile = null; importOpen = true; }}>Импорт</button>
  <button class="btn primary" onclick={openCreate}><Plus size={16} />Новый контрагент</button>
</div>
{#if error && !createOpen && !importOpen}<div class="error-box">{error}</div>{/if}

{#if loaded && companies.length === 0}
  <div class="card">
    {#if query || tag}<EmptyState icon={SearchX} title="Ничего не найдено" text="Измените условия поиска." />
    {:else}<EmptyState icon={Building2} title="Контрагентов пока нет" text="Добавьте заказчиков и подрядчиков вручную или загрузите список из Excel." />{/if}
  </div>
{:else}
  <div class="card scroll">
    <table class="list">
      <thead>
        <tr><th>Контрагент</th><th>ИНН / КПП</th><th class="hide-sm">Контакты</th><th class="hide-sm">Ответственный</th><th>Сделки в работе</th><th class="hide-sm">Проекты</th><th class="hide-sm">Последний контакт</th></tr>
      </thead>
      <tbody>
        {#each companies as c (c.id)}
          <tr>
            <td>
              <a class="name" href="/crm/companies/{c.id}">{c.name}</a>
              {#if c.visibility !== 'all'}<span class="badge grey" title="Ограниченная видимость">{c.visibility === 'group' ? 'группа' : 'личный'}</span>{/if}
              {#each c.tags as t}<span class="badge">{t}</span>{/each}
            </td>
            <td class="mono muted">{c.inn ?? '—'}{c.kpp ? ` / ${c.kpp}` : ''}</td>
            <td class="hide-sm muted small">{c.phone}{c.phone && c.email ? ' · ' : ''}{c.email}</td>
            <td class="hide-sm muted">{c.responsible_name ?? '—'}</td>
            <td>{#if c.deals_open}{c.deals_open} · {formatMoney(c.deals_amount)}{:else}<span class="faint">—</span>{/if}</td>
            <td class="hide-sm">{c.projects || ''}</td>
            <td class="hide-sm muted">{c.last_activity_at ? formatDate(c.last_activity_at) : '—'}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<Modal bind:open={createOpen} title="Новый контрагент" width={760}>
  <CompanyForm bind:this={form} bind:value={draft} {users} {groups} />
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (createOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!draft.name.trim() || form?.hasErrors()} onclick={create}>Создать</button>
  {/snippet}
</Modal>

<Modal bind:open={importOpen} title="Импорт контрагентов" width={600}>
  <p class="muted">
    Файл .xlsx или .csv. В первой строке — названия столбцов: «Наименование» (обязательно), «ИНН», «КПП», «ОГРН»,
    «Юридический адрес», «Телефон», «Email», «Руководитель», «Банк», «БИК», «Р/с», «К/с», «Метки».
    Строки с ошибками в реквизитах и дубликаты по ИНН/КПП пропускаются.
  </p>
  <button class="btn sm link" onclick={downloadSample}>Скачать шаблон CSV</button>
  <input class="input file" type="file" accept=".xlsx,.csv" onchange={(e) => (importFile = e.currentTarget.files?.[0] ?? null)} />
  {#if importResult}
    <div class="ok-box">Добавлено: {importResult.created} из {importResult.total}.</div>
    {#if importResult.skipped.length}
      <div class="skipped">
        <b>Пропущено ({importResult.skipped.length}):</b>
        <table class="list">
          <tbody>
            {#each importResult.skipped as s}<tr><td class="faint">стр. {s.row}</td><td>{s.name || '—'}</td><td class="muted">{s.reason}</td></tr>{/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/if}
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (importOpen = false)}>Закрыть</button>
    <button class="btn primary" disabled={!importFile || importBusy} onclick={runImport}>{importBusy ? 'Загрузка…' : 'Импортировать'}</button>
  {/snippet}
</Modal>

<style>
  .search { width: 280px; }
  .tagsel { width: auto; }
  .error-box { margin-bottom: 10px; }
  .scroll { overflow-x: auto; }
  .name { font-weight: 500; color: var(--text); margin-right: 6px; }
  td .badge { margin-right: 4px; }
  .file { padding: 4px 6px; height: auto; }
  .link { justify-self: start; }
  .skipped { max-height: 240px; overflow-y: auto; font-size: 13px; }
  p { margin: 0; }
  @media (max-width: 760px) { .hide-sm { display: none; } .search { width: 100%; } }
</style>
