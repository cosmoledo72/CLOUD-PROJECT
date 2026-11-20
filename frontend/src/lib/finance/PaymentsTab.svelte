<script lang="ts">
  import EmptyState from '#lib/EmptyState.svelte';
  import Banknote from '@lucide/svelte/icons/banknote';
  // «Финансы → Платежи»: income and expenses for a period with filters, totals,
  // CSV export; finance editors add and change payments.
  import { api, formatDate, formatMoney, plainMessage, type FinanceCategory, type Payment, type PaymentTotals } from '#lib/api.ts';
  import { session } from '#lib/session.svelte.ts';
  import ArrowDownLeft from '@lucide/svelte/icons/arrow-down-left';
  import ArrowUpRight from '@lucide/svelte/icons/arrow-up-right';
  import CalendarClock from '@lucide/svelte/icons/calendar-clock';
  import CalendarPlus from '@lucide/svelte/icons/calendar-plus';
  import Download from '@lucide/svelte/icons/download';
  import Plus from '@lucide/svelte/icons/plus';
  import Scale from '@lucide/svelte/icons/scale';
  import Search from '@lucide/svelte/icons/search';
  import Settings from '@lucide/svelte/icons/settings';
  import { plural } from '#lib/fieldLook.ts';
  import PickFilter from '#lib/ui/PickFilter.svelte';
  import StatTile from '#lib/ui/StatTile.svelte';
  import PaymentDialog from './PaymentDialog.svelte';
  import PeriodPicker from './PeriodPicker.svelte';
  import { periodRange, signed, type Period } from './period';

  let { onsettings }: { onsettings?: () => void } = $props();

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
  const periodText = $derived(from && to ? `${formatDate(from)} — ${formatDate(to)}` : from ? `с ${formatDate(from)}` : 'всё время');
  const facts = $derived(payments.filter((p) => p.status === 'fact'));
  const countWord = (n: number) => plural(n, 'платёж', 'платежа', 'платежей');
  const nIn = $derived(facts.filter((p) => p.kind === 'income').length);
  const nOut = $derived(facts.filter((p) => p.kind === 'expense').length);
  const planLate = $derived(payments.filter((p) => p.status === 'plan' && p.kind === 'income' && p.pay_date < today).reduce((a, p) => a + p.amount, 0));
  const diff = $derived(totals.income - totals.expense);
</script>

<div class="page-head">
  <h1>Платежи</h1>
  <div class="spacer"></div>
  {#if onsettings}<button class="btn" onclick={onsettings}><Settings size={16} />Статьи и сроки</button>{/if}
  {#if session.financeEdit}
    <button class="btn" onclick={() => add('income', 'plan')}><CalendarPlus size={16} />План оплаты</button>
    <button class="btn" onclick={() => add('expense')}><ArrowUpRight size={16} />Расход</button>
    <button class="btn primary" onclick={() => add('income')}><Plus size={16} />Поступление</button>
  {/if}
</div>

{#if error}<div class="error-box">{error}</div>{/if}

<div class="bar">
  <PeriodPicker bind:period bind:from bind:to />
  <span class="faint small">{periodText}</span>
  <div class="spacer"></div>
  <button class="btn" onclick={exportCsv} disabled={!payments.length} title="Выгрузить платежи в CSV (Excel)"><Download size={16} />CSV</button>
</div>

<div class="stat-grid">
  <StatTile label="Поступило" value={formatMoney(totals.income)} icon={ArrowDownLeft} tone="ok" valueTone="ok"
    hint={nIn ? `${nIn} ${countWord(nIn)}` : 'проведённые за период'} />
  <StatTile label="Израсходовано" value={formatMoney(totals.expense)} icon={ArrowUpRight} tone="danger" valueTone="danger"
    hint={nOut ? `${nOut} ${countWord(nOut)}` : 'проведённые за период'} />
  <StatTile label="Разница" value={formatMoney(diff)} icon={Scale} tone="accent" valueTone={diff < 0 ? 'danger' : diff > 0 ? 'ok' : undefined}
    hint="поступления минус расходы" />
  <StatTile label="Ожидается по плану" value={formatMoney(totals.income_plan)} icon={CalendarClock} tone="warn"
    hint={planLate ? `из них просрочено ${formatMoney(planLate)}` : totals.expense_plan ? `расходы по плану ${formatMoney(totals.expense_plan)}` : 'поступления по графику оплат'}
    hintTone={planLate ? 'danger' : undefined} />
</div>

<section class="card table">
  <div class="tbar">
    <label class="search"><Search size={16} /><input class="input" placeholder="Комментарий, контрагент, проект, № счёта" bind:value={q} /></label>
    <PickFilter label="Тип" bind:value={kind} options={[{ value: '', label: 'Все платежи' }, { value: 'income', label: 'Поступления' }, { value: 'expense', label: 'Расходы' }]} />
    <PickFilter label="Статус" bind:value={status} options={[{ value: '', label: 'Факт и план' }, { value: 'fact', label: 'Проведённые' }, { value: 'plan', label: 'Плановые' }]} />
    <PickFilter label="Статья" bind:value={categoryId} search={categories.length > 8}
      options={[{ value: '', label: 'Все статьи' }, ...categories.filter((c) => !kind || c.kind === kind).map((c) => ({ value: c.id, label: c.name, hint: c.kind === 'income' ? 'поступление' : 'расход' }))]} />
    <PickFilter label="Проект" bind:value={projectId} search
      options={[{ value: '', label: 'Все проекты' }, ...projects.map((p) => ({ value: p.id, label: p.name }))]} />
  </div>
  {#if loaded && !payments.length}
    <EmptyState icon={Banknote} title="Платежей за период нет" text={session.financeEdit ? 'Добавьте поступление или расход кнопками выше или выберите другой период.' : 'Выберите другой период.'} />
  {:else}
    <div class="scroll">
      <table>
        <thead><tr><th>Дата</th><th class="num">Сумма</th><th>Статья</th><th>Проект</th><th>Контрагент</th><th>Комментарий</th></tr></thead>
        <tbody>
          {#each payments as p (p.id)}
            <tr class:click={session.financeEdit} class:plan={p.status === 'plan'} onclick={() => edit(p)}>
              <td class="nowrap">{formatDate(p.pay_date)}
                {#if p.status === 'plan'}<div><span class="tag" class:late={p.pay_date < today}>{p.pay_date < today ? 'план, просрочен' : 'план'}</span></div>{/if}</td>
              <td class="num"><span class="dir {p.kind === 'income' ? 'in' : 'out'}" title={p.kind === 'income' ? 'Поступление' : 'Расход'}>{#if p.kind === 'income'}<ArrowDownLeft size={13} />{:else}<ArrowUpRight size={13} />{/if}</span><span class={p.kind === 'income' ? 'in' : 'out'}>{signed(p.amount, p.kind)}</span>
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
  .spacer { flex: 1; }
  .in { color: var(--ok); }
  .out { color: var(--danger); }
  .dir { display: inline-grid; place-items: center; width: 20px; height: 20px; border-radius: 6px; margin-right: 6px; vertical-align: -4px; }
  .dir.in { background: var(--ok-soft); }
  .dir.out { background: var(--danger-soft); }
  .table { padding: 0; overflow: hidden; }
  .tbar { display: flex; gap: 8px; padding: 12px; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .tbar :global(.btn.sm) { height: 36px; }
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
</style>
