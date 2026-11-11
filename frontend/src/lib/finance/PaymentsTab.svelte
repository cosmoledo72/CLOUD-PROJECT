<script lang="ts">
  // «Финансы → Платежи»: income and expenses for a period with filters, totals,
  // CSV export; finance editors add and change payments.
  import { api, formatDate, formatMoney, plainMessage, type FinanceCategory, type Payment, type PaymentTotals } from '$lib/api';
  import { session } from '$lib/session.svelte';
  import Download from '@lucide/svelte/icons/download';
  import Plus from '@lucide/svelte/icons/plus';
  import Search from '@lucide/svelte/icons/search';
  import PaymentDialog from './PaymentDialog.svelte';
  import PeriodPicker from './PeriodPicker.svelte';
  import { periodRange, signed, type Period } from './period';

  let period = $state<Period>('month');
  let from = $state(periodRange('month')[0]);
  let to = $state(periodRange('month')[1]);
  let kind = $state('');
  let status = $state('');
  let categoryId = $state('');
  let projectId = $state('');
  let q = $state('');
  let payments = $state<Payment[]>([]);
  let totals = $state<PaymentTotals>({ income: 0, expense: 0, income_plan: 0, expense_plan: 0 });
  let categories = $state<FinanceCategory[]>([]);
  let projects = $state<{ id: string; name: string }[]>([]);
  let loaded = $state(false);
  let error = $state('');
  let reloadTick = $state(0);

  $effect(() => {
    api.get<{ categories: FinanceCategory[] }>('/finance/categories').then((r) => (categories = r.categories)).catch(() => {});
    api.get<{ projects: typeof projects }>('/projects').then((r) => (projects = r.projects)).catch(() => {});
  });

  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void reloadTick;
    const p = new URLSearchParams();
    if (from) p.set('from', from);
    // The year period ends on 31.12 so planned payments of the year are included.
    if (to) p.set('to', to);
    if (kind) p.set('kind', kind);
    if (status) p.set('status', status);
    if (categoryId) p.set('category_id', categoryId);
    if (projectId) p.set('project_id', projectId);
    if (q.trim()) p.set('q', q.trim());
    const qs = p.toString();
    clearTimeout(timer);
    timer = setTimeout(() => {
      api.get<{ payments: Payment[]; totals: PaymentTotals }>(`/finance/payments?${qs}`)
        .then((r) => { payments = r.payments; totals = r.totals; loaded = true; error = ''; })
        .catch((e) => (error = plainMessage(e.message)));
    }, 200);
  });

  let dialogOpen = $state(false);
  let editing = $state<Payment | null>(null);
  let preset = $state<{ kind?: 'income' | 'expense'; status?: 'plan' | 'fact' }>({});
  function add(k: 'income' | 'expense', s: 'plan' | 'fact' = 'fact') {
    editing = null;
    preset = { kind: k, status: s };
    dialogOpen = true;
  }
  function edit(p: Payment) {
    if (!session.financeEdit) return;
    editing = p;
    dialogOpen = true;
  }

  function exportCsv() {
    const head = ['Дата', 'Тип', 'Статус', 'Сумма', 'НДС', 'Статья', 'Проект', 'Контрагент', 'Счёт', 'Комментарий'];
    const esc = (v: string) => `"${v.replace(/"/g, '""')}"`;
    const lines = payments.map((p) => [p.pay_date, p.kind === 'income' ? 'Поступление' : 'Расход', p.status === 'plan' ? 'План' : 'Факт',
      p.amount.toFixed(2).replace('.', ','), p.vat_amount.toFixed(2).replace('.', ','), p.category_name ?? '', p.project_name ?? '',
      p.company_name, p.invoice_number ?? '', p.description].map((v) => esc(String(v))).join(';'));
    const blob = new Blob(['﻿' + [head.map(esc).join(';'), ...lines].join('\r\n')], { type: 'text/csv;charset=utf-8' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `платежи_${from || 'все'}_${to || ''}.csv`;
    a.click();
    URL.revokeObjectURL(a.href);
  }
  const today = new Date().toISOString().slice(0, 10);
</script>

{#if error}<div class="error-box">{error}</div>{/if}

<div class="bar">
  <PeriodPicker bind:period bind:from bind:to />
  <div class="spacer"></div>
  {#if session.financeEdit}
    <button class="btn" onclick={() => add('income', 'plan')}><Plus size={15} /> План оплаты</button>
    <button class="btn add-in" onclick={() => add('income')}><Plus size={15} /> Поступление</button>
    <button class="btn add-out" onclick={() => add('expense')}><Plus size={15} /> Расход</button>
  {/if}
</div>

<div class="sum">
  <div class="card tile"><span>Поступило</span><b class="in">{formatMoney(totals.income)}</b></div>
  <div class="card tile"><span>Израсходовано</span><b class="out">{formatMoney(totals.expense)}</b></div>
  <div class="card tile"><span>Разница</span><b class:out={totals.income - totals.expense < 0}>{formatMoney(totals.income - totals.expense)}</b></div>
  <div class="card tile"><span>Ожидается по плану</span><b class="plan">{formatMoney(totals.income_plan)}</b>
    {#if totals.expense_plan}<small class="faint">расходы по плану {formatMoney(totals.expense_plan)}</small>{/if}</div>
</div>

<section class="card table">
  <div class="tbar">
    <label class="search"><Search size={16} /><input class="input" placeholder="Комментарий, контрагент, проект, № счёта" bind:value={q} /></label>
    <select class="input" bind:value={kind} aria-label="Тип"><option value="">Все платежи</option><option value="income">Поступления</option><option value="expense">Расходы</option></select>
    <select class="input" bind:value={status} aria-label="Статус"><option value="">Факт и план</option><option value="fact">Проведённые</option><option value="plan">Плановые</option></select>
    <select class="input" bind:value={categoryId} aria-label="Статья">
      <option value="">Все статьи</option>
      {#each categories.filter((c) => !kind || c.kind === kind) as c (c.id)}<option value={c.id}>{c.kind === 'income' ? '＋' : '－'} {c.name}</option>{/each}
    </select>
    <select class="input proj" bind:value={projectId} aria-label="Проект">
      <option value="">Все проекты</option>
      {#each projects as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
    </select>
    <button class="btn" onclick={exportCsv} disabled={!payments.length} title="Выгрузить в CSV"><Download size={15} /></button>
  </div>
  {#if loaded && !payments.length}
    <p class="empty muted">Платежей за выбранный период нет.{session.financeEdit ? ' Добавьте поступление или расход кнопками выше.' : ''}</p>
  {:else}
    <div class="scroll">
      <table>
        <thead><tr><th>Дата</th><th class="num">Сумма</th><th>Статья</th><th>Проект</th><th>Контрагент</th><th>Комментарий</th></tr></thead>
        <tbody>
          {#each payments as p (p.id)}
            <tr class:click={session.financeEdit} class:plan={p.status === 'plan'} onclick={() => edit(p)}>
              <td class="nowrap">{formatDate(p.pay_date)}
                {#if p.status === 'plan'}<div><span class="tag" class:late={p.pay_date < today}>{p.pay_date < today ? 'план, просрочен' : 'план'}</span></div>{/if}</td>
              <td class="num"><span class={p.kind === 'income' ? 'in' : 'out'}>{signed(p.amount, p.kind)}</span>
                {#if p.vat_amount}<div class="faint small">НДС {formatMoney(p.vat_amount)}</div>{/if}</td>
              <td>{p.category_name ?? '—'}</td>
              <td class="clip">{#if p.project_id}<a href="/projects/{p.project_id}?tab=finance" onclick={(e) => e.stopPropagation()}>{p.project_name}</a>{:else}<span class="faint">—</span>{/if}</td>
              <td class="clip">{p.company_name || '—'}{#if p.invoice_number}<div class="faint small">счёт № {p.invoice_number}</div>{/if}</td>
              <td class="clip desc">{p.description}{#if p.created_by_name}<div class="faint small">{p.created_by_name}</div>{/if}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>

<PaymentDialog bind:open={dialogOpen} payment={editing} {preset} onsaved={() => reloadTick++} />

<style>
  .bar { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; margin-bottom: 14px; }
  .bar .btn { display: inline-flex; gap: 6px; align-items: center; }
  .spacer { flex: 1; }
  .add-in { color: var(--ok); }
  .add-out { color: var(--danger); }
  .sum { display: grid; grid-template-columns: repeat(auto-fit, minmax(190px, 1fr)); gap: 10px; margin-bottom: 14px; }
  .tile { padding: 11px 14px; display: grid; gap: 2px; }
  .tile span { font-size: 12.5px; color: var(--text-3); }
  .tile b { font-size: 20px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .in { color: var(--ok); }
  .out { color: var(--danger); }
  .plan { color: var(--accent); }
  .table { padding: 0; overflow: hidden; }
  .tbar { display: flex; gap: 8px; padding: 12px; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .tbar select { width: auto; max-width: 200px; }
  .tbar .btn { display: inline-grid; place-items: center; }
  .search { position: relative; flex: 1; min-width: 200px; }
  .search :global(svg) { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); color: var(--text-3); }
  .search input { padding-left: 34px; width: 100%; }
  .scroll { overflow-x: auto; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  th { text-align: left; font-weight: 500; color: var(--text-3); padding: 8px 12px; border-bottom: 1px solid var(--border); white-space: nowrap; background: var(--surface-2); }
  td { padding: 9px 12px; border-bottom: 1px solid var(--border); vertical-align: top; }
  tbody tr:last-child td { border-bottom: 0; }
  tr.click { cursor: pointer; }
  tr.click:hover { background: var(--surface-2); }
  tr.plan td { background: color-mix(in srgb, var(--accent) 4%, transparent); }
  tr.plan .num span { opacity: .75; }
  .num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; font-weight: 500; }
  .nowrap { white-space: nowrap; }
  .clip { max-width: 220px; overflow-wrap: anywhere; }
  .clip a { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .desc { max-width: 300px; color: var(--text-2); }
  .tag { font-size: 11px; padding: 0 7px; border-radius: 999px; background: var(--accent-soft); color: var(--accent); font-weight: 500; }
  .tag.late { background: var(--danger-soft); color: var(--danger); }
  .empty { padding: 26px; text-align: center; margin: 0; }
</style>
