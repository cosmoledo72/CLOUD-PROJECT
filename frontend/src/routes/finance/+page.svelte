<script lang="ts">
  import { api, docKindLabel, docStatusLabel, formatDate, formatMoney, localToday, type DocKind, type DocStatus, type SalesDoc } from '$lib/api';
  import Search from '@lucide/svelte/icons/search';
  import Receipt from '@lucide/svelte/icons/receipt';
  import FileCheck2 from '@lucide/svelte/icons/file-check-2';
  import FileSignature from '@lucide/svelte/icons/file-signature';
  import Scale from '@lucide/svelte/icons/scale';
  import Download from '@lucide/svelte/icons/download';

  type Period = 'month' | 'quarter' | 'year' | 'prev_year' | 'all' | 'custom';
  let period = $state<Period>('year');
  let from = $state('');
  let to = $state('');
  let kind = $state<'' | DocKind>('');
  let status = $state<'' | DocStatus>('');
  let q = $state('');
  let docs = $state<SalesDoc[]>([]);
  let loaded = $state(false);
  let error = $state('');

  const pad = (n: number) => String(n).padStart(2, '0');
  function range(p: Period): [string, string] {
    const t = localToday();
    const y = +t.slice(0, 4);
    const m = +t.slice(5, 7);
    if (p === 'month') return [`${y}-${pad(m)}-01`, t];
    if (p === 'quarter') return [`${y}-${pad(Math.floor((m - 1) / 3) * 3 + 1)}-01`, t];
    if (p === 'year') return [`${y}-01-01`, t];
    if (p === 'prev_year') return [`${y - 1}-01-01`, `${y - 1}-12-31`];
    return ['', ''];
  }
  function setPeriod(p: Period) {
    period = p;
    if (p !== 'custom') [from, to] = range(p);
  }
  setPeriod('year');

  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const params = new URLSearchParams();
    if (from) params.set('from', from);
    if (to) params.set('to', to);
    if (kind) params.set('kind', kind);
    if (status) params.set('status', status);
    if (q.trim()) params.set('q', q.trim());
    const qs = params.toString();
    clearTimeout(timer);
    timer = setTimeout(() => {
      api.get<{ documents: SalesDoc[] }>(`/crm/documents?${qs}`)
        .then((r) => { docs = r.documents; loaded = true; error = ''; })
        .catch((e) => (error = e.message));
    }, 200);
  });

  const live = $derived(docs.filter((d) => d.status !== 'cancelled'));
  const sum = (k: DocKind, onlySigned = false) =>
    live.filter((d) => d.kind === k && (!onlySigned || d.status === 'signed')).reduce((a, d) => a + d.amount, 0);
  const tiles = $derived.by(() => {
    const invoiced = sum('invoice');
    const acts = sum('act');
    return [
      { key: 'contract', icon: FileSignature, label: 'Договоры', value: sum('contract'), hint: `подписано на ${formatMoney(sum('contract', true))}` },
      { key: 'invoice', icon: Receipt, label: 'Выставлено счетов', value: invoiced, hint: `${live.filter((d) => d.kind === 'invoice').length} шт.` },
      { key: 'act', icon: FileCheck2, label: 'Закрыто актами', value: acts, hint: `подписано на ${formatMoney(sum('act', true))}` },
      { key: 'gap', icon: Scale, label: 'Счета без актов', value: Math.max(invoiced - acts, 0), hint: 'выставлено, но ещё не закрыто' }
    ];
  });
  const vatTotal = $derived(live.filter((d) => d.kind === 'invoice').reduce((a, d) => a + d.vat_amount, 0));

  // Monthly invoices vs acts for the chart.
  const months = $derived.by(() => {
    const map = new Map<string, { inv: number; act: number }>();
    for (const d of live) {
      if (d.kind !== 'invoice' && d.kind !== 'act') continue;
      const k = d.doc_date.slice(0, 7);
      const v = map.get(k) ?? { inv: 0, act: 0 };
      if (d.kind === 'invoice') v.inv += d.amount;
      else v.act += d.amount;
      map.set(k, v);
    }
    return [...map.entries()].sort(([a], [b]) => a.localeCompare(b)).slice(-12);
  });
  const peak = $derived(Math.max(1, ...months.map(([, v]) => Math.max(v.inv, v.act))));
  const MONTHS = ['янв', 'фев', 'мар', 'апр', 'май', 'июн', 'июл', 'авг', 'сен', 'окт', 'ноя', 'дек'];
  const monthLabel = (k: string) => `${MONTHS[+k.slice(5, 7) - 1]} ${k.slice(2, 4)}`;

  async function setStatus(d: SalesDoc, s: DocStatus) {
    try {
      await api.patch(`/crm/documents/${d.id}`, { status: s });
      d.status = s;
    } catch (e) {
      error = (e as Error).message;
    }
  }

  function exportCsv() {
    const head = ['Дата', 'Вид', 'Номер', 'Контрагент', 'Сделка', 'Предмет', 'Сумма', 'НДС', 'Статус'];
    const esc = (v: string) => `"${v.replace(/"/g, '""')}"`;
    const lines = docs.map((d) => [d.doc_date, docKindLabel[d.kind], d.number, d.company_name, d.deal_title ?? '', d.subject,
      d.amount.toFixed(2).replace('.', ','), d.vat_amount.toFixed(2).replace('.', ','), docStatusLabel[d.status]].map((v) => esc(String(v))).join(';'));
    const blob = new Blob(['﻿' + [head.map(esc).join(';'), ...lines].join('\r\n')], { type: 'text/csv;charset=utf-8' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `финансы_${from || 'все'}_${to || ''}.csv`;
    a.click();
    URL.revokeObjectURL(a.href);
  }
  const statusClass: Record<DocStatus, string> = { draft: 'grey', sent: 'warn', signed: 'ok', cancelled: 'danger' };
</script>

<svelte:head><title>Финансы</title></svelte:head>

<div class="page">

<div class="page-head">
  <h1>Финансы</h1>
  <div class="spacer"></div>
  <button class="btn" onclick={exportCsv} disabled={!docs.length}><Download size={15} /> Выгрузить CSV</button>
</div>
{#if error}<div class="error-box">{error}</div>{/if}

<div class="filters">
  <div class="seg" role="radiogroup" aria-label="Период">
    {#each [['month', 'Месяц'], ['quarter', 'Квартал'], ['year', 'Год'], ['prev_year', 'Прошлый год'], ['all', 'Всё время'], ['custom', 'Период…']] as [k, l]}
      <button role="radio" aria-checked={period === k} class:on={period === k} onclick={() => setPeriod(k as Period)}>{l}</button>
    {/each}
  </div>
  {#if period === 'custom'}
    <input class="input date" type="date" bind:value={from} aria-label="С" />
    <span class="faint">—</span>
    <input class="input date" type="date" bind:value={to} aria-label="По" />
  {/if}
</div>

<div class="tiles">
  {#each tiles as t (t.key)}
    {@const Icon = t.icon}
    <div class="card tile t-{t.key}">
      <div class="ic"><Icon size={18} /></div>
      <div class="label">{t.label}</div>
      <div class="value">{formatMoney(t.value)}</div>
      <div class="faint small">{t.hint}</div>
    </div>
  {/each}
</div>

{#if months.length}
  <section class="card chart">
    <div class="chart-head">
      <h2>Счета и акты по месяцам</h2>
      <span class="legend"><i class="inv"></i>Счета <i class="act"></i>Акты</span>
      <span class="faint small">НДС в счетах: {formatMoney(vatTotal)}</span>
    </div>
    <div class="bars">
      {#each months as [k, v] (k)}
        <div class="col" title="{monthLabel(k)}: счета {formatMoney(v.inv)}, акты {formatMoney(v.act)}">
          <div class="pair">
            <div class="bar inv" style="height: {(v.inv / peak) * 100}%"></div>
            <div class="bar act" style="height: {(v.act / peak) * 100}%"></div>
          </div>
          <div class="m faint">{monthLabel(k)}</div>
        </div>
      {/each}
    </div>
  </section>
{/if}

<section class="card table">
  <div class="tbar">
    <label class="search"><Search size={16} /><input class="input" placeholder="Номер, контрагент, предмет" bind:value={q} /></label>
    <select class="input" bind:value={kind} aria-label="Вид документа">
      <option value="">Все виды</option>
      {#each Object.entries(docKindLabel) as [k, l]}<option value={k}>{l}</option>{/each}
    </select>
    <select class="input" bind:value={status} aria-label="Статус">
      <option value="">Все статусы</option>
      {#each Object.entries(docStatusLabel) as [k, l]}<option value={k}>{l}</option>{/each}
    </select>
  </div>
  {#if loaded && !docs.length}
    <p class="empty muted">Документов за выбранный период нет. Счета, акты и договоры создаются в карточке сделки.</p>
  {:else}
    <div class="scroll">
      <table>
        <thead>
          <tr><th>Дата</th><th>Документ</th><th>Контрагент / сделка</th><th class="num">Сумма</th><th>Статус</th></tr>
        </thead>
        <tbody>
          {#each docs as d (d.id)}
            <tr class:cancelled={d.status === 'cancelled'}>
              <td class="nowrap">{formatDate(d.doc_date)}</td>
              <td>
                <span class="kind k-{d.kind}">{docKindLabel[d.kind]}</span>
                {#if d.file_id}<a href="/editor/{d.file_id}" target="_blank" rel="noopener">№ {d.number}</a>{:else}№ {d.number}{/if}
                {#if d.subject}<div class="faint small sub">{d.subject}</div>{/if}
              </td>
              <td>
                {#if d.company_id}<a href="/crm/companies/{d.company_id}">{d.company_name}</a>{:else}{d.company_name}{/if}
                {#if d.deal_id}<div class="small"><a class="muted" href="/crm/deals/{d.deal_id}">{d.deal_title}</a></div>{/if}
              </td>
              <td class="num">{formatMoney(d.amount)}{#if d.vat_amount}<div class="faint small">НДС {formatMoney(d.vat_amount)}</div>{/if}</td>
              <td>
                <select class="pill {statusClass[d.status]}" value={d.status} onchange={(e) => setStatus(d, e.currentTarget.value as DocStatus)} aria-label="Статус документа">
                  {#each Object.entries(docStatusLabel) as [k, l]}<option value={k}>{l}</option>{/each}
                </select>
              </td>
            </tr>
          {/each}
        </tbody>
        {#if docs.length}
          <tfoot><tr>
            {#if kind}
              <td colspan="3">Итого: {docKindLabel[kind].toLowerCase()}, без аннулированных ({live.length})</td><td class="num">{formatMoney(live.reduce((a, d) => a + d.amount, 0))}</td><td></td>
            {:else}
              <td colspan="5" class="muted">Документов: {docs.length}{docs.length !== live.length ? `, из них аннулировано ${docs.length - live.length}` : ''}. Выберите вид документа, чтобы увидеть итог.</td>
            {/if}
          </tr></tfoot>
        {/if}
      </table>
    </div>
  {/if}
</section>
</div>

<style>
  .filters { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; margin-bottom: 14px; }
  .seg { display: inline-flex; border: 1px solid var(--border-strong); border-radius: var(--radius); overflow: hidden; flex-wrap: wrap; }
  .seg button { border: 0; background: var(--surface); padding: 5px 12px; cursor: pointer; font: inherit; color: var(--text-2); }
  .seg button.on { background: var(--accent); color: var(--on-accent); }
  .date { width: 150px; }
  .page-head .btn { display: inline-flex; gap: 6px; align-items: center; }
  .tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(210px, 1fr)); gap: 12px; margin-bottom: 14px; }
  .tile { padding: 14px 16px; position: relative; }
  .tile .ic { position: absolute; right: 14px; top: 14px; width: 32px; height: 32px; border-radius: 9px; display: grid; place-items: center; }
  .t-contract .ic { background: color-mix(in srgb, #7c3aed 14%, transparent); color: #7c3aed; }
  .t-invoice .ic { background: var(--accent-soft); color: var(--accent); }
  .t-act .ic { background: var(--ok-soft); color: var(--ok); }
  .t-gap .ic { background: var(--warn-soft); color: var(--warn); }
  .label { color: var(--text-2); font-size: 13px; }
  .value { font-size: 22px; font-weight: 650; margin: 6px 0 2px; font-variant-numeric: tabular-nums; }
  .chart { padding: 14px 16px; margin-bottom: 14px; }
  .chart-head { display: flex; gap: 16px; align-items: center; flex-wrap: wrap; margin-bottom: 10px; }
  .chart-head h2 { font-size: 14px; margin: 0; }
  .legend { display: inline-flex; gap: 6px; align-items: center; font-size: 12px; color: var(--text-2); }
  .legend i { width: 10px; height: 10px; border-radius: 3px; display: inline-block; margin-left: 6px; }
  .legend i.inv, .bar.inv { background: var(--accent); }
  .legend i.act, .bar.act { background: var(--ok); }
  .bars { display: flex; gap: 10px; align-items: flex-end; height: 160px; overflow-x: auto; }
  .col { flex: 1; min-width: 44px; height: 100%; display: flex; flex-direction: column; }
  .pair { flex: 1; display: flex; gap: 3px; align-items: flex-end; justify-content: center; border-bottom: 1px solid var(--border); }
  .bar { width: 14px; border-radius: 4px 4px 0 0; min-height: 2px; }
  .m { font-size: 11px; text-align: center; margin-top: 4px; white-space: nowrap; }
  .table { padding: 0; overflow: hidden; }
  .tbar { display: flex; gap: 10px; padding: 12px; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .tbar select { width: auto; }
  .search { position: relative; flex: 1; min-width: 200px; }
  .search :global(svg) { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); color: var(--text-3); }
  .search input { padding-left: 34px; width: 100%; }
  .scroll { overflow-x: auto; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  th { text-align: left; font-weight: 500; color: var(--text-3); padding: 8px 12px; border-bottom: 1px solid var(--border); white-space: nowrap; }
  td { padding: 9px 12px; border-bottom: 1px solid var(--border); vertical-align: top; }
  tfoot td { font-weight: 600; border-bottom: 0; background: var(--surface-2); }
  .num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .nowrap { white-space: nowrap; }
  .sub { max-width: 340px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  tr.cancelled td { color: var(--text-3); }
  tr.cancelled .num { text-decoration: line-through; }
  .kind { font-size: 11px; padding: 1px 7px; border-radius: 999px; margin-right: 6px; background: var(--surface-2); color: var(--text-2); }
  .k-invoice { background: var(--accent-soft); color: var(--accent); }
  .k-act { background: var(--ok-soft); color: var(--ok); }
  .k-contract { background: color-mix(in srgb, #7c3aed 12%, transparent); color: #7c3aed; }
  .pill { border: 0; border-radius: 999px; padding: 2px 8px; font: inherit; font-size: 12px; cursor: pointer; }
  .pill.grey { background: var(--surface-2); color: var(--text-2); }
  .pill.warn { background: var(--warn-soft); color: var(--warn); }
  .pill.ok { background: var(--ok-soft); color: var(--ok); }
  .pill.danger { background: var(--danger-soft); color: var(--danger); }
  .empty { padding: 26px; text-align: center; margin: 0; }
</style>
