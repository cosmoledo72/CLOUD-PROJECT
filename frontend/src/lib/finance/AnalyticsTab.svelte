<script lang="ts">
  // «Финансы → Аналитика»: income, expense and profit for a period; bars by
  // month or week; expenses and income by category (donuts); projects by profit.
  import { api, formatMoney, plainMessage } from '#lib/api.ts';
  import { moneyShort } from '#lib/fieldLook.ts';
  import ArrowDownLeft from '@lucide/svelte/icons/arrow-down-left';
  import ArrowUpRight from '@lucide/svelte/icons/arrow-up-right';
  import CalendarClock from '@lucide/svelte/icons/calendar-clock';
  import TrendingUp from '@lucide/svelte/icons/trending-up';
  import StatTile from '#lib/ui/StatTile.svelte';
  import PeriodPicker from './PeriodPicker.svelte';
  import { MONTHS_SHORT, periodRange, type Period } from './period';

  type Summary = {
    totals: { income: number; expense: number; profit: number; income_plan: number; expense_plan: number };
    series: { start: string; income: number; expense: number }[];
    categories: { kind: 'income' | 'expense'; name: string; amount: number }[];
    projects: { id: string; name: string; income: number; expense: number; profit: number }[];
    plan: { overdue: number; next_30_days: number };
  };

  let period = $state<Period>('year');
  let from = $state(periodRange('year')[0]);
  let to = $state(periodRange('year')[1]);
  let step = $state<'month' | 'week'>('month');
  let data = $state<Summary | null>(null);
  let error = $state('');

  $effect(() => {
    if (!from || !to) return;
    const qs = `from=${from}&to=${to}&period=${step}`;
    api.get<Summary>(`/finance/summary?${qs}`).then((r) => { data = r; error = ''; }).catch((e) => (error = plainMessage(e.message)));
  });

  const peak = $derived(Math.max(1, ...(data?.series ?? []).map((s) => Math.max(s.income, s.expense))));
  function label(start: string) {
    const [y, m, d] = start.split('-');
    return step === 'month' ? `${MONTHS_SHORT[+m - 1]}${period === 'year' ? '' : ` ${y.slice(2)}`}` : `${d}.${m}`;
  }

  // Donut: palette of muted, distinguishable hues.
  const PALETTE = ['#1f6fb2', '#d9822b', '#2f9e6e', '#9b59b6', '#c0392b', '#16a5a5', '#b58b00', '#7f8c8d', '#e05a8a', '#5d6d7e'];
  function donut(items: { name: string; amount: number }[]) {
    const total = items.reduce((s, i) => s + i.amount, 0) || 1;
    let acc = 0;
    return items.map((it, i) => {
      const start = acc / total;
      acc += it.amount;
      return { ...it, color: PALETTE[i % PALETTE.length], start, end: acc / total, pct: Math.round((it.amount / total) * 100) };
    });
  }
  const R = 52;
  const C = 2 * Math.PI * R;
  const expenseCats = $derived(donut((data?.categories ?? []).filter((c) => c.kind === 'expense')));
  const incomeCats = $derived(donut((data?.categories ?? []).filter((c) => c.kind === 'income')));
  const maxProject = $derived(Math.max(1, ...(data?.projects ?? []).map((p) => Math.max(p.income, p.expense))));
</script>

<div class="page-head"><h1>Аналитика</h1></div>

{#if error}<div class="error-box">{error}</div>{/if}

<div class="bar">
  <PeriodPicker bind:period bind:from bind:to allowAll={false} />
  <div class="spacer"></div>
  <div class="seg" role="radiogroup" aria-label="Шаг графика">
    <button role="radio" aria-checked={step === 'month'} class:on={step === 'month'} onclick={() => (step = 'month')}>По месяцам</button>
    <button role="radio" aria-checked={step === 'week'} class:on={step === 'week'} onclick={() => (step = 'week')}>По неделям</button>
  </div>
</div>

{#if data}
  <div class="stat-grid">
    <StatTile label="Поступления" value={formatMoney(data.totals.income)} icon={ArrowDownLeft} tone="ok" valueTone="ok" hint="проведённые за период" />
    <StatTile label="Расходы" value={formatMoney(data.totals.expense)} icon={ArrowUpRight} tone="danger" valueTone="danger" hint="проведённые за период" />
    <StatTile label="Прибыль" value={formatMoney(data.totals.profit)} icon={TrendingUp} tone="accent"
      valueTone={data.totals.profit < 0 ? 'danger' : data.totals.profit > 0 ? 'ok' : undefined}
      hint={data.totals.income > 0 ? `рентабельность ${Math.round((data.totals.profit / data.totals.income) * 100)} %` : 'поступления минус расходы'} />
    <StatTile label="Ожидается за 30 дней" value={formatMoney(data.plan.next_30_days)} icon={CalendarClock} tone="warn"
      hint={data.plan.overdue ? `просрочено по плану ${formatMoney(data.plan.overdue)}` : 'по графику оплат'} hintTone={data.plan.overdue ? 'danger' : undefined} />
  </div>

  <section class="card chart">
    <div class="head"><h2>Поступления и расходы</h2><span class="legend"><i class="li"></i>Поступления <i class="lo"></i>Расходы</span></div>
    {#if data.series.every((s) => !s.income && !s.expense)}
      <p class="muted empty">За период нет проведённых платежей.</p>
    {:else}
      <div class="bars" class:dense={data.series.length > 16}>
        {#each data.series as s (s.start)}
          <div class="col" title="{label(s.start)}: поступления {formatMoney(s.income)}, расходы {formatMoney(s.expense)}, разница {formatMoney(s.income - s.expense)}">
            <div class="pair">
              <div class="b in" style="height:{(s.income / peak) * 100}%"></div>
              <div class="b out" style="height:{(s.expense / peak) * 100}%"></div>
            </div>
            <div class="m faint">{label(s.start)}</div>
          </div>
        {/each}
      </div>
    {/if}
  </section>

  <div class="two">
    {#each [['Расходы по статьям', expenseCats], ['Поступления по статьям', incomeCats]] as [title, items]}
      {@const list = items as ReturnType<typeof donut>}
      <section class="card donut">
        <h2>{title}</h2>
        {#if !list.length}
          <p class="muted empty">Нет данных за период.</p>
        {:else}
          <div class="d-body">
            <svg viewBox="0 0 140 140" width="140" height="140" role="img" aria-label={title as string}>
              <circle cx="70" cy="70" r={R} fill="none" stroke="var(--surface-2)" stroke-width="18" />
              {#each list as it (it.name)}
                <circle cx="70" cy="70" r={R} fill="none" stroke={it.color} stroke-width="18"
                        stroke-dasharray="{Math.max(0, (it.end - it.start) * C - (list.length > 1 ? 1.5 : 0))} {C}"
                        stroke-dashoffset={-it.start * C} transform="rotate(-90 70 70)"><title>{it.name}: {formatMoney(it.amount)}</title></circle>
              {/each}
              <text x="70" y="66" text-anchor="middle" class="t1">{moneyShort(list.reduce((s, i) => s + i.amount, 0))}</text>
              <text x="70" y="84" text-anchor="middle" class="t2">всего</text>
            </svg>
            <ul class="leg">
              {#each list as it (it.name)}
                <li><i style="background:{it.color}"></i><span class="n">{it.name}</span><span class="v">{formatMoney(it.amount)}</span><span class="p">{it.pct}%</span></li>
              {/each}
            </ul>
          </div>
        {/if}
      </section>
    {/each}
  </div>

  <section class="card projects">
    <h2>Проекты за период</h2>
    {#if !data.projects.length}
      <p class="muted empty">Платежей с привязкой к проектам за период нет.</p>
    {:else}
      <table>
        <thead><tr><th>Проект</th><th></th><th class="num">Поступления</th><th class="num">Расходы</th><th class="num">Результат</th></tr></thead>
        <tbody>
          {#each data.projects as p (p.id)}
            <tr>
              <td><a href="/projects/{p.id}?tab=finance">{p.name}</a></td>
              <td class="pb">
                <div class="hb in" style="width:{(p.income / maxProject) * 100}%"></div>
                <div class="hb out" style="width:{(p.expense / maxProject) * 100}%"></div>
              </td>
              <td class="num">{formatMoney(p.income)}</td>
              <td class="num">{formatMoney(p.expense)}</td>
              <td class="num" class:in={p.profit > 0} class:out={p.profit < 0}><b>{formatMoney(p.profit)}</b></td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>
{/if}

<style>
  .bar { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; margin-bottom: 14px; }
  .spacer { flex: 1; }
  .in { color: var(--ok); }
  .out { color: var(--danger); }
  h2 { font-size: 14px; margin: 0; }
  .chart, .donut, .projects { padding: 14px 16px; margin-bottom: 14px; }
  .head { display: flex; gap: 16px; align-items: center; margin-bottom: 10px; flex-wrap: wrap; }
  .legend { display: inline-flex; gap: 6px; align-items: center; font-size: 12px; color: var(--text-2); }
  .legend i { width: 10px; height: 10px; border-radius: 3px; display: inline-block; margin-left: 6px; }
  .li, .b.in, .hb.in { background: var(--ok); }
  .lo, .b.out, .hb.out { background: var(--danger); opacity: .8; }
  .bars { display: flex; gap: 8px; align-items: flex-end; height: 190px; overflow-x: auto; }
  .bars.dense { gap: 3px; }
  .col { flex: 1; min-width: 30px; height: 100%; display: flex; flex-direction: column; }
  .pair { flex: 1; display: flex; gap: 2px; align-items: flex-end; justify-content: center; border-bottom: 1px solid var(--border); }
  .b { width: 40%; max-width: 18px; border-radius: 3px 3px 0 0; min-height: 1px; }
  .m { font-size: 11px; text-align: center; margin-top: 4px; white-space: nowrap; }
  .two { display: grid; grid-template-columns: repeat(auto-fit, minmax(340px, 1fr)); gap: 14px; }
  .two .donut { margin-bottom: 14px; }
  .d-body { display: flex; gap: 18px; align-items: center; margin-top: 10px; flex-wrap: wrap; }
  .t1 { font-size: 13px; font-weight: 600; fill: var(--text); }
  .t2 { font-size: 10px; fill: var(--text-3); }
  .leg { list-style: none; margin: 0; padding: 0; flex: 1; min-width: 200px; display: grid; gap: 5px; font-size: 13px; }
  .leg li { display: grid; grid-template-columns: 10px 1fr auto 36px; gap: 8px; align-items: center; }
  .leg i { width: 10px; height: 10px; border-radius: 3px; }
  .leg .n { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .leg .v { font-variant-numeric: tabular-nums; }
  .leg .p { text-align: right; color: var(--text-3); font-variant-numeric: tabular-nums; }
  .empty { margin: 14px 0 4px; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; margin-top: 8px; }
  th { text-align: left; font-weight: 500; color: var(--text-3); padding: 6px 8px; border-bottom: 1px solid var(--border); }
  td { padding: 7px 8px; border-bottom: 1px solid var(--border); }
  tbody tr:last-child td { border-bottom: 0; }
  .num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .pb { width: 30%; min-width: 120px; }
  .hb { height: 5px; border-radius: 3px; margin: 2px 0; min-width: 1px; }
  .projects { overflow-x: auto; }
  @media (max-width: 640px) {
    .pb, th:nth-child(2) { display: none; }
    .two { grid-template-columns: 1fr; }
  }
</style>
