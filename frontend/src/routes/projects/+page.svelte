<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, formatDate, formatFieldValue, nameDef, type FieldDef, type FieldValues, type Project, type UserItem } from '$lib/api';
  import Modal from '$lib/Modal.svelte';
  import RequisitesForm from '$lib/RequisitesForm.svelte';
  import { session } from '$lib/session.svelte';
  import LayoutGrid from '@lucide/svelte/icons/layout-grid';
  import List from '@lucide/svelte/icons/list';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import ArrowDown from '@lucide/svelte/icons/arrow-down';

  let projects = $state<Project[]>([]);
  let defs = $state<FieldDef[]>([]);
  let users = $state<UserItem[]>([]);
  let createOpen = $state(false);
  let name = $state('');
  let description = $state('');
  let values = $state<FieldValues>({});
  let allFields = $state(false);
  let error = $state('');
  let query = $state('');

  async function load() {
    const [p, f, u] = await Promise.all([
      api.get<{ projects: Project[] }>('/projects'),
      api.get<{ fields: FieldDef[] }>('/project-fields'),
      api.get<{ users: UserItem[] }>('/users')
    ]);
    projects = p.projects;
    defs = f.fields.filter((d) => !d.is_archived);
    users = u.users;
  }
  $effect(() => {
    load();
  });

  const listDefs = $derived(defs.filter((d) => d.show_in_list));
  const hasRequired = $derived(defs.some((d) => d.is_required));
  // «Наименование объекта» is the project name: one input instead of two.
  const nd = $derived(nameDef(defs));

  // Search by name and by any requisite value (project code, client, ...).
  const shown = $derived(
    projects.filter((p) => {
      const q = query.trim().toLowerCase();
      if (!q) return true;
      const hay = [p.name, p.description, ...Object.values(p.fields ?? {}).map((v) => String(v ?? ''))].join(' ').toLowerCase();
      return hay.includes(q);
    })
  );

  function openCreate() {
    name = '';
    description = '';
    values = {};
    allFields = false;
    error = '';
    createOpen = true;
  }

  async function create() {
    error = '';
    // Send only filled-in values.
    const fields = Object.fromEntries(Object.entries(values).filter(([, v]) => v !== '' && v !== null && v !== false));
    try {
      const r = await api.post<{ id: string }>('/projects', { name, description, fields });
      goto(`/projects/${r.id}`);
    } catch (e) {
      error = (e as Error).message;
    }
  }

  const statusText = { active: 'Активный', paused: 'Приостановлен', closed: 'Закрыт' };

  // --- View: rows (default) or cards; remembered per browser -----------------------
  type View = 'rows' | 'cards';
  function stored<T extends string>(key: string, allowed: readonly T[], dflt: T): T {
    try {
      const v = localStorage.getItem(key);
      return (allowed as readonly string[]).includes(v ?? '') ? (v as T) : dflt;
    } catch {
      return dflt;
    }
  }
  function store(key: string, v: string) {
    try { localStorage.setItem(key, v); } catch { /* storage unavailable */ }
  }
  let view = $state<View>(stored('wh_projects_view', ['rows', 'cards'] as const, 'rows'));
  function setView(v: View) {
    view = v;
    store('wh_projects_view', v);
  }
  let status = $state<'all' | Project['status']>(stored('wh_projects_status', ['all', 'active', 'paused', 'closed'] as const, 'all'));
  $effect(() => store('wh_projects_status', status));

  // --- Sorting for the rows view ----------------------------------------------------
  type SortKey = 'name' | 'code' | 'status' | 'progress' | 'start' | 'end' | `f:${string}`;
  let sortKey = $state<SortKey>('name');
  let sortDir = $state<1 | -1>(1);
  function sortBy(k: SortKey) {
    if (sortKey === k) sortDir = sortDir === 1 ? -1 : 1;
    else { sortKey = k; sortDir = 1; }
  }
  const progress = (p: Project) => (p.task_total ? p.task_done / p.task_total : 0);
  const rowDefs = $derived(listDefs.filter((d) => d.key !== 'code'));
  const coll = new Intl.Collator('ru', { numeric: true, sensitivity: 'base' });
  const rows = $derived.by(() => {
    const list = shown.filter((p) => status === 'all' || p.status === status);
    const val = (p: Project): string | number => {
      switch (sortKey) {
        case 'name': return p.name;
        case 'code': return String(p.fields?.code ?? '');
        case 'status': return ['active', 'paused', 'closed'].indexOf(p.status);
        case 'progress': return progress(p);
        case 'start': return p.start_date ?? '';
        case 'end': return p.end_date ?? '';
        default: {
          const d = defs.find((x) => x.key === sortKey.slice(2));
          const v = p.fields?.[sortKey.slice(2)];
          return d && v != null ? (typeof v === 'number' ? v : formatFieldValue(d, v, users)) : '';
        }
      }
    };
    return [...list].sort((a, b) => {
      const x = val(a);
      const y = val(b);
      // Empty values always go last.
      if (x === '' && y !== '') return 1;
      if (y === '' && x !== '') return -1;
      const c = typeof x === 'number' && typeof y === 'number' ? x - y : coll.compare(String(x), String(y));
      return c * sortDir;
    });
  });
  const hasCode = $derived(projects.some((p) => p.fields?.code));
  const late = (p: Project) => p.status === 'active' && !!p.end_date && p.end_date < new Date().toISOString().slice(0, 10) && p.task_done < p.task_total;
</script>

<div class="page">
  <div class="page-head">
    <h1>Проекты</h1>
    <div class="spacer"></div>
    {#if projects.length > 3}<input class="input search" placeholder="Поиск: название, шифр, заказчик" bind:value={query} aria-label="Поиск проектов" />{/if}
    {#if projects.length}
      <select class="input status-sel" bind:value={status} aria-label="Статус">
        <option value="all">Все статусы</option><option value="active">Активные</option><option value="paused">Приостановленные</option><option value="closed">Закрытые</option>
      </select>
      <div class="seg" role="radiogroup" aria-label="Вид">
        <button role="radio" aria-checked={view === 'rows'} class:on={view === 'rows'} title="Строками" onclick={() => setView('rows')}><List size={16} /></button>
        <button role="radio" aria-checked={view === 'cards'} class:on={view === 'cards'} title="Карточками" onclick={() => setView('cards')}><LayoutGrid size={16} /></button>
      </div>
    {/if}
    {#if session.me?.role !== 'guest'}<button class="btn primary" onclick={openCreate}>Новый проект</button>{/if}
  </div>

  {#if projects.length === 0}
    <div class="card empty"><p class="muted">Проектов пока нет.</p></div>
  {:else if !rows.length}
    <div class="card empty"><p class="muted">Ничего не найдено.</p></div>
  {:else if view === 'rows'}
    {#snippet th(k: SortKey, label: string, cls = '')}
      <th class={cls} aria-sort={sortKey === k ? (sortDir === 1 ? 'ascending' : 'descending') : 'none'}>
        <button class="sort" onclick={() => sortBy(k)}>{label}
          {#if sortKey === k}{#if sortDir === 1}<ArrowUp size={12} />{:else}<ArrowDown size={12} />{/if}{/if}
        </button>
      </th>
    {/snippet}
    <div class="card table">
      <table class="rows">
        <thead>
          <tr>
            {#if hasCode}{@render th('code', 'Шифр', 'c-code')}{/if}
            {@render th('name', 'Проект')}
            {#each rowDefs as d (d.id)}{@render th(`f:${d.key}`, d.label, 'hide-md')}{/each}
            {@render th('status', 'Статус')}
            {@render th('progress', 'Выполнено', 'c-prog')}
            {@render th('end', 'Сроки', 'hide-sm')}
          </tr>
        </thead>
        <tbody>
          {#each rows as p (p.id)}
            {@const pct = Math.round(progress(p) * 100)}
            <tr onclick={(e) => { if (!(e.target as HTMLElement).closest('a')) goto(`/projects/${p.id}`); }}>
              {#if hasCode}<td class="c-code">{#if p.fields?.code}<span class="code mono">{p.fields.code}</span>{/if}</td>{/if}
              <td class="c-name">
                <a href="/projects/{p.id}" title={p.description || p.name}>{p.name}</a>
              </td>
              {#each rowDefs as d (d.id)}
                <td class="hide-md small req-cell">{p.fields?.[d.key] != null ? formatFieldValue(d, p.fields[d.key], users) : ''}</td>
              {/each}
              <td><span class="badge st-{p.status}">{statusText[p.status]}</span></td>
              <td class="c-prog">
                <div class="prog-row">
                  <div class="progress" aria-label="Выполнено {pct}%"><div style="width:{pct}%"></div></div>
                  <span class="small muted nums">{p.task_done}/{p.task_total}</span>
                </div>
              </td>
              <td class="hide-sm small nums dates">
                {#if p.start_date || p.end_date}{formatDate(p.start_date)} —<br /><span class:late={late(p)}>{formatDate(p.end_date)}</span>{/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <div class="grid">
      {#each rows as p (p.id)}
        {@const pct = p.task_total ? Math.round((p.task_done / p.task_total) * 100) : 0}
        <a class="card proj" href="/projects/{p.id}">
          <div class="row top">
            {#if p.fields?.code}<span class="code mono">{p.fields.code}</span>{/if}
            <div class="spacer"></div>
            <span class="badge st-{p.status}">{statusText[p.status]}</span>
          </div>
          <h3>{p.name}</h3>
          {#if p.description}<p class="muted desc">{p.description}</p>{/if}
          {#if listDefs.some((d) => d.key !== 'code' && p.fields?.[d.key] != null)}
            <dl class="req">
              {#each listDefs.filter((d) => d.key !== 'code' && p.fields?.[d.key] != null) as d (d.id)}
                <dt>{d.label}</dt><dd>{formatFieldValue(d, p.fields[d.key], users)}</dd>
              {/each}
            </dl>
          {/if}
          <div class="progress" aria-label="Выполнено {pct}%"><div style="width:{pct}%"></div></div>
          <div class="row meta faint">
            <span>{p.task_done} / {p.task_total} задач</span>
            <div class="spacer"></div>
            {#if p.start_date || p.end_date}<span>{formatDate(p.start_date)} — {formatDate(p.end_date)}</span>{/if}
          </div>
        </a>
      {/each}
    </div>
  {/if}
</div>

<Modal bind:open={createOpen} title="Новый проект" width={defs.length ? 640 : 480}>
  {#if nd}
    <label class="field">{nd.label}<span class="faint small">— это и название проекта</span>
      <textarea class="input" rows="2" bind:value={name}></textarea>
    </label>
  {:else}
    <label class="field">Название <input class="input" bind:value={name} /></label>
  {/if}
  <label class="field">Описание <textarea class="input" rows="2" bind:value={description}></textarea></label>
  {#if defs.length}
    <div class="req-head row">
      <h3>Реквизиты</h3>
      <div class="spacer"></div>
      {#if hasRequired}
        <label class="check small"><input type="checkbox" bind:checked={allFields} />Заполнить все</label>
      {/if}
    </div>
    <RequisitesForm {defs} bind:values {users} requiredOnly={hasRequired && !allFields} exclude={nd ? [nd.key] : []} />
    {#if !allFields && hasRequired}<p class="faint small">Остальные реквизиты можно заполнить позже на вкладке «Реквизиты» проекта.</p>{/if}
  {/if}
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (createOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!name.trim()} onclick={create}>Создать</button>
  {/snippet}
</Modal>

<style>
  .search { width: 280px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 12px; }
  .proj { padding: 16px; color: var(--text); display: grid; gap: 8px; align-content: start; }
  .proj:hover { text-decoration: none; border-color: var(--accent); }
  .proj h3 { margin: 0; }
  .top { min-height: 22px; }
  .code { background: var(--surface-2); border: 1px solid var(--border); padding: 1px 6px; border-radius: 4px; color: var(--text-2); }
  .desc { margin: 0; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .req { display: grid; grid-template-columns: auto 1fr; gap: 2px 10px; margin: 0; font-size: 12.5px; }
  .req dt { color: var(--text-3); }
  .req dd { margin: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .progress { height: 6px; border-radius: 3px; background: var(--surface-2); overflow: hidden; }
  .progress div { height: 100%; background: var(--ok); }
  .meta { font-size: 12px; }
  .st-paused { background: var(--warn-soft); color: var(--warn); }
  .st-closed { background: var(--surface-2); color: var(--text-3); }
  .empty { padding: 40px; text-align: center; }
  .req-head h3 { margin: 0; }
  p { margin: 0; }
  .status-sel { width: auto; }
  .seg { display: inline-flex; border: 1px solid var(--border-strong); border-radius: var(--radius); overflow: hidden; height: 34px; }
  .seg button { border: 0; background: var(--surface); padding: 0 10px; cursor: pointer; color: var(--text-2); display: grid; place-items: center; }
  .seg button.on { background: var(--accent); color: var(--on-accent); }
  .table { padding: 0; overflow-x: auto; }
  .rows { width: 100%; border-collapse: collapse; font-size: 13.5px; }
  .rows th { text-align: left; font-weight: 500; color: var(--text-3); padding: 0; border-bottom: 1px solid var(--border); }
  .rows th .sort { text-align: left; }
  .sort { border: 0; background: none; font: inherit; font-size: 12.5px; color: inherit; padding: 9px 10px; cursor: pointer; display: inline-flex; gap: 4px; align-items: center; }
  .sort:hover { color: var(--text); }
  .rows td { padding: 9px 10px; border-bottom: 1px solid var(--border); vertical-align: middle; }
  .rows tbody tr { cursor: pointer; }
  .rows tbody tr:hover { background: var(--surface-2); }
  .rows tbody tr:last-child td { border-bottom: 0; }
  .c-name a { color: var(--text); font-weight: 500; }
  .c-name { min-width: 180px; line-height: 1.35; }
  .rows .code { font-size: 12px; }
  .c-code { width: 1%; white-space: nowrap; }
  .c-prog { width: 150px; min-width: 120px; }
  .one { max-width: 340px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .req-cell { max-width: 180px; line-height: 1.35; }
  .dates { line-height: 1.35; color: var(--text-2); }
  .prog-row { display: flex; align-items: center; gap: 8px; }
  .prog-row .progress { flex: 1; }
  .nums { font-variant-numeric: tabular-nums; white-space: nowrap; }
  .late { color: var(--danger); font-weight: 500; }
  @media (max-width: 1100px) { .hide-md { display: none; } }
  @media (max-width: 720px) { .search { width: 100%; } .hide-sm { display: none; } .c-prog { width: 110px; } }
</style>
