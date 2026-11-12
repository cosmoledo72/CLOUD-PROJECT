<script lang="ts">
  import { onIntent } from '$lib/intent';
  import EmptyState from '$lib/EmptyState.svelte';
  import Handshake from '@lucide/svelte/icons/handshake';
  // Sales pipeline. Board: open stages fit the screen width; won/lost are drop
  // zones under the board. List: a sortable table with a stage filter.
  import { goto } from '$app/navigation';
  import { api, formatDate, formatMoney, localToday, taskWhen, type CompanyListItem, type Contact, type Deal, type Stage, type UserItem } from '$lib/api';
  import { contactName } from '$lib/api';
  import Modal from '$lib/Modal.svelte';
  import { session } from '$lib/session.svelte';
  import Kanban from '@lucide/svelte/icons/square-kanban';
  import List from '@lucide/svelte/icons/list';
  import Trophy from '@lucide/svelte/icons/trophy';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import ArrowDown from '@lucide/svelte/icons/arrow-down';

  let stages = $state<Stage[]>([]);
  let deals = $state<Deal[]>([]);
  let error = $state('');
  let query = $state('');
  let mineOnly = $state(false);

  async function load() {
    const p = new URLSearchParams();
    if (mineOnly && session.me) p.set('responsible_id', session.me.id);
    const r = await api.get<{ stages: Stage[]; deals: Deal[] }>(`/crm/deals?${p}`);
    stages = r.stages;
    deals = r.deals;
  }
  $effect(() => {
    mineOnly;
    load().catch((e) => (error = e.message));
  });

  const shown = $derived(
    deals.filter((d) => {
      const q = query.trim().toLowerCase();
      return !q || d.title.toLowerCase().includes(q) || (d.company_name ?? '').toLowerCase().includes(q);
    })
  );
  const byStage = (id: string) => shown.filter((d) => d.stage_id === id);
  const sum = (list: Deal[]) => list.reduce((s, d) => s + d.amount, 0);
  const openDeals = $derived(shown.filter((d) => d.stage_kind === 'open'));
  const weighted = $derived(openDeals.reduce((s, d) => s + (d.amount * d.probability) / 100, 0));
  const today = localToday();
  const overdue = (d: Deal) => d.stage_kind === 'open' && !!d.expected_close && d.expected_close < today;
  const openStages = $derived(stages.filter((st) => st.kind === 'open'));
  const wonStage = $derived(stages.find((st) => st.kind === 'won') ?? null);
  const lostStage = $derived(stages.find((st) => st.kind === 'lost') ?? null);
  const wonDeals = $derived(shown.filter((d) => d.stage_kind === 'won'));
  const lostDeals = $derived(shown.filter((d) => d.stage_kind === 'lost'));
  const stageOf = (id: string) => stages.find((st) => st.id === id);

  // --- View: board or list, remembered per browser ------------------------------
  type View = 'board' | 'list';
  function stored(key: string, dflt: string) {
    try { return localStorage.getItem(key) ?? dflt; } catch { return dflt; }
  }
  let view = $state<View>(stored('wh_deals_view', 'board') === 'list' ? 'list' : 'board');
  function setView(v: View) {
    view = v;
    try { localStorage.setItem('wh_deals_view', v); } catch { /* storage unavailable */ }
  }
  /** List filter: 'open' | 'won' | 'lost' | 'all' or a stage id. */
  let listFilter = $state('open');
  function showInList(f: string) {
    listFilter = f;
    setView('list');
  }

  // Funnel strip: open stages sized by amount (with a floor so empty stages stay visible).
  const funnel = $derived.by(() => {
    const items = openStages.map((st) => {
      const list = byStage(st.id);
      return { st, count: list.length, amount: sum(list) };
    });
    const total = items.reduce((a, i) => a + i.amount, 0);
    return items.map((i) => ({ ...i, grow: total ? 1 + (i.amount / total) * items.length * 2 : 1 }));
  });

  // --- List view: filtering and sorting --------------------------------------------
  type SortKey = 'title' | 'company' | 'stage' | 'amount' | 'close' | 'next' | 'resp';
  let sortKey = $state<SortKey>('stage');
  let sortDir = $state<1 | -1>(1);
  function sortBy(k: SortKey) {
    if (sortKey === k) sortDir = sortDir === 1 ? -1 : 1;
    else { sortKey = k; sortDir = k === 'amount' ? -1 : 1; }
  }
  const coll = new Intl.Collator('ru', { numeric: true, sensitivity: 'base' });
  const rows = $derived.by(() => {
    const f = listFilter;
    const list = shown.filter((d) =>
      f === 'all' ? true : f === 'open' || f === 'won' || f === 'lost' ? d.stage_kind === f : d.stage_id === f);
    const val = (d: Deal): string | number => {
      switch (sortKey) {
        case 'title': return d.title;
        case 'company': return d.company_name ?? '';
        case 'stage': return stageOf(d.stage_id)?.sort_order ?? 0;
        case 'amount': return d.amount;
        case 'close': return d.expected_close ?? '';
        case 'next': return d.next_task_date ?? '';
        case 'resp': return d.responsible_name ?? '';
      }
    };
    return [...list].sort((a, b) => {
      const x = val(a);
      const y = val(b);
      if (x === '' && y !== '') return 1;
      if (y === '' && x !== '') return -1;
      const c = typeof x === 'number' && typeof y === 'number' ? x - y : coll.compare(String(x), String(y));
      return (c || coll.compare(a.title, b.title)) * sortDir;
    });
  });
  function changeStage(d: Deal, stageId: string) {
    const st = stageOf(stageId);
    if (!st || st.id === d.stage_id) return;
    if (st.kind === 'lost') {
      pendingMove = { deal: d, stage: st };
      lostReason = '';
      lostOpen = true;
      return;
    }
    move(d, st);
  }

  const initials = (n: string | null) => (n ?? '?').split(/\s+/).map((p) => p[0]).slice(0, 2).join('').toUpperCase();

  // --- Drag and drop ---------------------------------------------------------------
  let dragId = $state<string | null>(null);
  let overStage = $state<string | null>(null);
  let lostOpen = $state(false);
  let lostReason = $state('');
  let pendingMove = $state<{ deal: Deal; stage: Stage } | null>(null);

  function onDrop(stage: Stage | null) {
    if (!stage) return;
    const deal = deals.find((d) => d.id === dragId);
    dragId = null;
    overStage = null;
    if (!deal || deal.stage_id === stage.id) return;
    if (!canMove(deal)) {
      error = 'Перемещать сделку может ответственный, её автор или менеджер.';
      return;
    }
    if (stage.kind === 'lost') {
      pendingMove = { deal, stage };
      lostReason = '';
      lostOpen = true;
      return;
    }
    move(deal, stage);
  }

  async function move(deal: Deal, stage: Stage, reason?: string) {
    error = '';
    const prev = deal.stage_id;
    deal.stage_id = stage.id; // optimistic
    try {
      const r = await api.patch<{ deal: Deal }>(`/crm/deals/${deal.id}`, { stage_id: stage.id, ...(reason ? { lost_reason: reason } : {}) });
      Object.assign(deal, r.deal);
    } catch (e) {
      deal.stage_id = prev;
      error = (e as Error).message;
    }
  }

  const canMove = (d: Deal) => session.crmManager || d.responsible_id === session.me?.id || d.created_by === session.me?.id;

  // --- New deal ---------------------------------------------------------------------
  let createOpen = $state(false);
  let companies = $state<CompanyListItem[]>([]);
  let contacts = $state<Contact[]>([]);
  let users = $state<UserItem[]>([]);
  let form = $state({ title: '', company_id: '', contact_id: '', amount: '', expected_close: '', responsible_id: '', stage_id: '' });

  async function openCreate(stageId = '') {
    [companies, users] = await Promise.all([
      api.get<{ companies: CompanyListItem[] }>('/crm/companies').then((r) => r.companies),
      api.get<{ users: UserItem[] }>('/users').then((r) => r.users)
    ]);
    form = { title: '', company_id: '', contact_id: '', amount: '', expected_close: '', responsible_id: session.me?.id ?? '', stage_id: stageId };
    contacts = [];
    createOpen = true;
  }
  onIntent('new', () => openCreate());
  $effect(() => {
    const cid = form.company_id;
    if (!createOpen) return;
    if (!cid) {
      contacts = [];
      return;
    }
    api.get<{ contacts: Contact[] }>(`/crm/contacts?company_id=${cid}`).then((r) => (contacts = r.contacts));
  });
  async function create() {
    error = '';
    try {
      const r = await api.post<{ deal: Deal }>('/crm/deals', {
        title: form.title,
        company_id: form.company_id || null,
        contact_id: form.contact_id || null,
        amount: form.amount ? Number(String(form.amount).replace(/\s/g, '').replace(',', '.')) : 0,
        expected_close: form.expected_close || null,
        responsible_id: form.responsible_id || null,
        stage_id: form.stage_id || null
      });
      createOpen = false;
      goto(`/crm/deals/${r.deal.id}`);
    } catch (e) {
      error = (e as Error).message;
    }
  }
</script>

<div class="page-head">
  <h1>Сделки</h1>
  <span class="muted summary">В работе: <b>{openDeals.length}</b> на <b>{formatMoney(sum(openDeals))}</b> · с учётом вероятности <b>{formatMoney(Math.round(weighted))}</b></span>
  <div class="spacer"></div>
  <input class="input search" placeholder="Поиск по сделке или контрагенту" bind:value={query} aria-label="Поиск" />
  <label class="check small"><input type="checkbox" bind:checked={mineOnly} />Только мои</label>
  <div class="seg" role="radiogroup" aria-label="Вид">
    <button role="radio" aria-checked={view === 'board'} class:on={view === 'board'} title="Доска" onclick={() => setView('board')}><Kanban size={16} /> Доска</button>
    <button role="radio" aria-checked={view === 'list'} class:on={view === 'list'} title="Список" onclick={() => setView('list')}><List size={16} /> Список</button>
  </div>
  <button class="btn primary" onclick={() => openCreate()}>Новая сделка</button>
</div>
{#if error}<div class="error-box">{error}</div>{/if}

<!-- Funnel: open stages sized by amount; closed deals of the last 30 days on the right. -->
<div class="funnel" aria-label="Воронка продаж">
  <div class="fsteps">
    {#each funnel as f, i (f.st.id)}
      <button class="fstep" class:active={view === 'list' && listFilter === f.st.id} style="--c:{f.st.color}; flex-grow:{f.grow}" title="{f.st.name}: {f.count} · {formatMoney(f.amount)}" onclick={() => showInList(f.st.id)}>
        <span class="fname">{f.st.name}</span>
        <span class="fval"><b>{f.count}</b> · {formatMoney(f.amount)}</span>
        {#if i < funnel.length - 1}<span class="chev" aria-hidden="true"></span>{/if}
      </button>
    {/each}
  </div>
  <div class="fclosed">
    <button class="fres won" class:active={view === 'list' && listFilter === 'won'} onclick={() => showInList('won')} title="Выиграно за 30 дней">
      <Trophy size={15} /><span><b>{wonDeals.length}</b> · {formatMoney(sum(wonDeals))}</span>
    </button>
    <button class="fres lost" class:active={view === 'list' && listFilter === 'lost'} onclick={() => showInList('lost')} title="Проиграно за 30 дней">
      <CircleX size={15} /><span><b>{lostDeals.length}</b></span>
    </button>
  </div>
</div>

{#snippet card(d: Deal)}
  <a
    class="deal card"
    class:dragging={dragId === d.id}
    href="/crm/deals/{d.id}"
    draggable={canMove(d)}
    ondragstart={(e) => { dragId = d.id; e.dataTransfer?.setData('text/plain', d.id); }}
    ondragend={() => { dragId = null; overStage = null; }}
  >
    <div class="t" title={d.title}>{d.title}</div>
    {#if d.company_name}<div class="muted small co">{d.company_name}</div>{/if}
    {#if d.next_task_title && d.next_task_date}
      <div class="next small" class:late={d.next_task_date < today} class:now={d.next_task_date === today} title="Следующее дело: {d.next_task_title}">
        ▸ {d.next_task_title} · {taskWhen({ due_date: d.next_task_date, due_time: null }, today)}
      </div>
    {:else}
      <div class="next small none" title="По сделке ничего не запланировано">▸ нет следующего шага</div>
    {/if}
    <div class="row foot">
      <b class="amt">{formatMoney(d.amount)}</b>
      <div class="spacer"></div>
      {#if d.expected_close}<span class="small date" class:late={overdue(d)} title="Ожидаемое закрытие">{formatDate(d.expected_close)}</span>{/if}
      {#if d.project_id}<span class="badge ok" title="Создан проект">проект</span>{/if}
      <span class="ava" title={d.responsible_name ?? ''}>{initials(d.responsible_name)}</span>
    </div>
  </a>
{/snippet}

{#if view === 'board'}
  <div class="board" role="list" style="--n:{openStages.length}">
    {#each openStages as st (st.id)}
      {@const list = byStage(st.id)}
      <section
        class="col"
        class:over={overStage === st.id}
        role="listitem"
        aria-label={st.name}
        ondragover={(e) => { e.preventDefault(); overStage = st.id; }}
        ondragleave={() => { if (overStage === st.id) overStage = null; }}
        ondrop={(e) => { e.preventDefault(); onDrop(st); }}
      >
        <header style="--c:{st.color}">
          <div class="row"><span class="dot"></span><b class="name" title={st.name}>{st.name}</b><span class="cnt">{list.length}</span></div>
          <div class="faint small">{formatMoney(sum(list))} · {st.probability}%</div>
        </header>
        <div class="cards">
          {#each list as d (d.id)}{@render card(d)}{/each}
          <button class="add" onclick={() => openCreate(st.id)}>+ Сделка</button>
        </div>
      </section>
    {/each}
  </div>

  <!-- Closing a deal: drop it here. Always visible, emphasised while dragging. -->
  <div class="closers" class:active={!!dragId}>
    {#each [wonStage, lostStage].filter((x): x is Stage => !!x) as st (st.id)}
      {#if st}
        <div
          class="closer {st.kind}"
          class:over={overStage === st.id}
          role="button"
          tabindex="0"
          aria-label="{st.name}: показать сделки"
          ondragover={(e) => { e.preventDefault(); overStage = st.id; }}
          ondragleave={() => { if (overStage === st.id) overStage = null; }}
          ondrop={(e) => { e.preventDefault(); onDrop(st); }}
          onclick={() => showInList(st.kind)}
          onkeydown={(e) => { if (e.key === 'Enter') showInList(st.kind); }}
        >
          {#if st.kind === 'won'}<Trophy size={18} />{:else}<CircleX size={18} />{/if}
          <span class="cl-name">{st.name}</span>
          <span class="cl-val">{st.kind === 'won' ? `${wonDeals.length} · ${formatMoney(sum(wonDeals))}` : lostDeals.length}</span>
          <span class="cl-hint">{dragId ? 'отпустите здесь' : 'за 30 дней'}</span>
        </div>
      {/if}
    {/each}
  </div>
  <p class="faint small hint">Перетащите карточку в другой столбец, чтобы сменить этап, или вниз — чтобы закрыть сделку.</p>
{:else}
  <div class="filters">
    {#each [['open', 'В работе', openDeals.length], ['won', 'Выиграны', wonDeals.length], ['lost', 'Проиграны', lostDeals.length], ['all', 'Все', shown.length]] as [k, l, n] (k)}
      <button class="chip" class:on={listFilter === k} onclick={() => (listFilter = String(k))}>{l} <span>{n}</span></button>
    {/each}
    <span class="sep"></span>
    {#each openStages as st (st.id)}
      <button class="chip st" class:on={listFilter === st.id} style="--c:{st.color}" onclick={() => (listFilter = st.id)}><i></i>{st.name} <span>{byStage(st.id).length}</span></button>
    {/each}
  </div>

  {#snippet th(k: SortKey, label: string, cls = '')}
    <th class={cls} aria-sort={sortKey === k ? (sortDir === 1 ? 'ascending' : 'descending') : 'none'}>
      <button class="sort" onclick={() => sortBy(k)}>{label}{#if sortKey === k}{#if sortDir === 1}<ArrowUp size={12} />{:else}<ArrowDown size={12} />{/if}{/if}</button>
    </th>
  {/snippet}
  <div class="card table">
    {#if rows.length}
      <table class="rows">
        <thead>
          <tr>
            {@render th('title', 'Сделка')}
            {@render th('company', 'Контрагент', 'hide-sm')}
            {@render th('stage', 'Этап')}
            {@render th('amount', 'Сумма', 'num')}
            {@render th('close', 'Закрытие', 'hide-sm')}
            {@render th('next', 'Следующий шаг', 'hide-md')}
            {@render th('resp', 'Ответственный', 'hide-md')}
          </tr>
        </thead>
        <tbody>
          {#each rows as d (d.id)}
            {@const st = stageOf(d.stage_id)}
            <tr onclick={(e) => { if (!(e.target as HTMLElement).closest('a, select')) goto(`/crm/deals/${d.id}`); }}>
              <td class="c-title">
                <a href="/crm/deals/{d.id}">{d.title}</a>
                {#if d.project_id}<span class="badge ok">проект</span>{/if}
              </td>
              <td class="hide-sm muted one">{#if d.company_id}<a class="muted" href="/crm/companies/{d.company_id}">{d.company_name}</a>{/if}</td>
              <td>
                {#if canMove(d)}
                  <select class="stage-pill" style="--c:{st?.color}" value={d.stage_id} onchange={(e) => changeStage(d, e.currentTarget.value)} aria-label="Этап">
                    {#each stages as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
                  </select>
                {:else}<span class="stage-pill ro" style="--c:{st?.color}">{st?.name}</span>{/if}
              </td>
              <td class="num"><b>{formatMoney(d.amount)}</b>{#if d.stage_kind === 'open'}<div class="faint small">{d.probability}%</div>{/if}</td>
              <td class="hide-sm small nums" class:late={overdue(d)}>{d.expected_close ? formatDate(d.expected_close) : ''}</td>
              <td class="hide-md small next-cell">
                {#if d.stage_kind !== 'open'}<span class="faint">{d.closed_at ? `закрыта ${formatDate(d.closed_at.slice(0, 10))}` : ''}</span>
                {:else if d.next_task_title && d.next_task_date}
                  <span class:late={d.next_task_date < today} class:now={d.next_task_date === today}>{taskWhen({ due_date: d.next_task_date, due_time: null }, today)}</span> · {d.next_task_title}
                {:else}<span class="faint none">нет</span>{/if}
              </td>
              <td class="hide-md small"><span class="resp"><span class="ava">{initials(d.responsible_name)}</span>{d.responsible_name ?? ''}</span></td>
            </tr>
          {/each}
        </tbody>
        <tfoot>
          <tr>
            <td>Итого: {rows.length}</td><td class="hide-sm"></td><td></td>
            <td class="num"><b>{formatMoney(sum(rows))}</b></td>
            <td class="hide-sm"></td><td class="hide-md"></td><td class="hide-md"></td>
          </tr>
        </tfoot>
      </table>
    {:else}
      <EmptyState icon={Handshake} title="Сделок нет" text="Создайте сделку или измените фильтры." />
    {/if}
  </div>
  <p class="faint small hint">Закрытые сделки показываются 30 дней после закрытия.</p>
{/if}

<Modal bind:open={createOpen} title="Новая сделка" width={560}>
  <label class="field">Название <input class="input" bind:value={form.title} placeholder="Например: Обследование корпуса 12" /></label>
  <div class="two">
    <label class="field">Контрагент
      <select class="input" bind:value={form.company_id}>
        <option value="">—</option>
        {#each companies as c}<option value={c.id}>{c.name}</option>{/each}
      </select>
    </label>
    <label class="field">Контактное лицо
      <select class="input" bind:value={form.contact_id} disabled={!contacts.length}>
        <option value="">—</option>
        {#each contacts as c}<option value={c.id}>{contactName(c)}</option>{/each}
      </select>
    </label>
    <label class="field">Сумма, ₽ <input class="input" inputmode="decimal" bind:value={form.amount} /></label>
    <label class="field">Ожидаемое закрытие <input class="input" type="date" bind:value={form.expected_close} /></label>
    <label class="field">Этап
      <select class="input" bind:value={form.stage_id}>
        <option value="">{stages.find((s) => s.kind === 'open')?.name ?? '—'}</option>
        {#each stages.filter((s) => s.kind === 'open').slice(1) as s}<option value={s.id}>{s.name}</option>{/each}
      </select>
    </label>
    <label class="field">Ответственный
      <select class="input" bind:value={form.responsible_id}>
        {#each users.filter((u) => u.is_active && u.crm_role !== 'none') as u}<option value={u.id}>{u.display_name}</option>{/each}
      </select>
    </label>
  </div>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (createOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!form.title.trim()} onclick={create}>Создать</button>
  {/snippet}
</Modal>

<Modal bind:open={lostOpen} title="Сделка проиграна">
  <label class="field">Причина (необязательно)
    <textarea class="input" rows="3" bind:value={lostReason} placeholder="Например: выбрали подрядчика с меньшей ценой"></textarea>
  </label>
  {#snippet footer()}
    <button class="btn" onclick={() => (lostOpen = false)}>Отмена</button>
    <button class="btn primary" onclick={() => { if (pendingMove) move(pendingMove.deal, pendingMove.stage, lostReason.trim()); lostOpen = false; }}>Перенести</button>
  {/snippet}
</Modal>

<style>
  .summary { font-size: 13px; }
  .search { width: 240px; }
  .error-box { margin-bottom: 10px; }

  /* Funnel strip */
  .funnel { display: flex; gap: 10px; margin-bottom: 12px; align-items: stretch; }
  .fsteps { flex: 1; display: flex; min-width: 0; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-lg); overflow: hidden; }
  .fstep { position: relative; flex: 1 1 0; min-width: 0; border: 0; background: none; font: inherit; color: var(--text); text-align: left; padding: 8px 18px 8px 12px; cursor: pointer; display: grid; gap: 1px; border-bottom: 3px solid var(--c); }
  .fstep:hover, .fstep.active { background: color-mix(in srgb, var(--c) 10%, transparent); }
  .fname { font-size: 12px; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fval { font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-variant-numeric: tabular-nums; }
  .chev { position: absolute; right: 2px; top: 50%; width: 10px; height: 10px; border-top: 1.5px solid var(--border-strong); border-right: 1.5px solid var(--border-strong); transform: translateY(-50%) rotate(45deg); }
  .fclosed { display: flex; gap: 6px; }
  .fres { display: inline-flex; align-items: center; gap: 6px; border: 1px solid var(--border); background: var(--surface); border-radius: var(--radius-lg); padding: 0 12px; font: inherit; font-size: 13px; cursor: pointer; white-space: nowrap; }
  .fres.won { color: var(--ok); }
  .fres.lost { color: var(--danger); }
  .fres span { color: var(--text); }
  .fres.active, .fres:hover { border-color: currentColor; }

  /* Board: open stages share the width; narrow screens scroll */
  .board { display: grid; grid-template-columns: repeat(var(--n), minmax(170px, 1fr)); gap: 10px; overflow-x: auto; padding-bottom: 4px; align-items: stretch; }
  .col { background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--radius-lg); display: flex; flex-direction: column; min-width: 0;
         max-height: calc(100vh - 330px); min-height: 220px; transition: outline-color .1s; outline: 2px dashed transparent; outline-offset: -3px; }
  .col.over { outline-color: var(--accent); }
  header { padding: 9px 10px 7px; border-bottom: 1px solid var(--border); border-top: 3px solid var(--c); border-radius: var(--radius-lg) var(--radius-lg) 0 0; }
  header .row { gap: 6px; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--c); flex: none; }
  .name { flex: 1; font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cnt { font-size: 11px; background: var(--surface); border: 1px solid var(--border); border-radius: 999px; padding: 0 7px; color: var(--text-2); }
  .cards { padding: 8px; display: grid; gap: 8px; align-content: start; overflow-y: auto; flex: 1; }
  .deal { padding: 9px 10px; display: grid; gap: 3px; color: var(--text); cursor: grab; min-width: 0; }
  .deal:hover { text-decoration: none; border-color: var(--accent); }
  .deal.dragging { opacity: .4; }
  .t { font-weight: 500; line-height: 1.3; font-size: 13.5px; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; overflow-wrap: anywhere; }
  .co { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .foot { gap: 6px; margin-top: 3px; min-width: 0; }
  .amt { font-size: 13px; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .date { white-space: nowrap; color: var(--text-2); }
  .late { color: var(--danger) !important; font-weight: 500; }
  .now { color: var(--warn); font-weight: 500; }
  .next { color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .next.none, .none { color: var(--text-3); font-style: italic; }
  .ava { width: 22px; height: 22px; border-radius: 50%; background: var(--accent-soft); color: var(--accent); font-size: 10px; font-weight: 600; display: grid; place-items: center; flex: none; font-style: normal; }
  .add { border: 1px dashed var(--border-strong); background: none; border-radius: var(--radius); padding: 6px; color: var(--text-3); cursor: pointer; font: inherit; font-size: 13px; }
  .add:hover { color: var(--accent); border-color: var(--accent); }

  /* Won / lost drop zones */
  .closers { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-top: 10px; }
  .closer { display: flex; align-items: center; gap: 8px; padding: 10px 14px; border: 1.5px dashed var(--border-strong); border-radius: var(--radius-lg); background: var(--surface); cursor: pointer; transition: all .12s; }
  .closer.won { color: var(--ok); }
  .closer.lost { color: var(--danger); }
  .cl-name { font-weight: 600; }
  .cl-val { color: var(--text); font-variant-numeric: tabular-nums; }
  .cl-hint { margin-left: auto; font-size: 12px; color: var(--text-3); }
  .closers.active .closer { padding: 18px 14px; border-color: currentColor; background: color-mix(in srgb, currentColor 6%, var(--surface)); }
  .closer.over { background: color-mix(in srgb, currentColor 16%, var(--surface)) !important; border-style: solid; }
  .closer:hover { border-color: currentColor; }
  .hint { margin: 8px 0 0; }

  /* List */
  .filters { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 10px; align-items: center; }
  .chip { border: 1px solid var(--border); background: var(--surface); border-radius: 999px; padding: 4px 11px; font: inherit; font-size: 13px; cursor: pointer; color: var(--text-2); display: inline-flex; align-items: center; gap: 6px; }
  .chip span { color: var(--text-3); font-size: 12px; }
  .chip.on { background: var(--accent-soft); color: var(--accent); border-color: var(--accent); }
  .chip.st i { width: 8px; height: 8px; border-radius: 50%; background: var(--c); }
  .sep { width: 1px; height: 20px; background: var(--border); margin: 0 4px; }
  .table { padding: 0; overflow-x: auto; }
  .rows { width: 100%; border-collapse: collapse; font-size: 13.5px; }
  .rows th { text-align: left; font-weight: 500; color: var(--text-3); padding: 0; border-bottom: 1px solid var(--border); white-space: nowrap; }
  .sort { border: 0; background: none; font: inherit; font-size: 12.5px; color: inherit; padding: 9px 10px; cursor: pointer; display: inline-flex; gap: 4px; align-items: center; }
  .sort:hover { color: var(--text); }
  .rows td { padding: 8px 10px; border-bottom: 1px solid var(--border); vertical-align: middle; }
  .rows tbody tr { cursor: pointer; }
  .rows tbody tr:hover { background: var(--surface-2); }
  .rows tfoot td { font-weight: 500; background: var(--surface-2); border-bottom: 0; }
  .c-title { min-width: 220px; }
  .c-title a { color: var(--text); font-weight: 500; margin-right: 6px; }
  .one { max-width: 220px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .num { text-align: right; white-space: nowrap; font-variant-numeric: tabular-nums; }
  th.num .sort { justify-content: flex-end; width: 100%; }
  .nums { white-space: nowrap; font-variant-numeric: tabular-nums; }
  .next-cell { max-width: 240px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); }
  .resp { display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; }
  .stage-pill { border: 0; border-radius: 999px; padding: 3px 8px; font: inherit; font-size: 12px; cursor: pointer;
                background: color-mix(in srgb, var(--c) 14%, transparent); color: color-mix(in srgb, var(--c) 75%, var(--text)); field-sizing: content; max-width: 240px; }
  .stage-pill.ro { cursor: default; display: inline-block; }
  .empty { padding: 26px; text-align: center; margin: 0; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 10px 12px; }
  @media (max-width: 1200px) { .hide-md { display: none; } }
  @media (max-width: 900px) {
    .funnel { flex-direction: column; }
    .fsteps { overflow-x: auto; }
    .fstep { min-width: 120px; }
    .col { max-height: none; }
  }
  @media (max-width: 720px) { .search { width: 100%; } .two { grid-template-columns: 1fr; } .hide-sm { display: none; } .closers { grid-template-columns: 1fr; } }
</style>
