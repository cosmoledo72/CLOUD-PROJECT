<script lang="ts">
  // «CRM → Отчёты» (B11): key figures, funnel, won and lost by month, managers
  // and reasons for lost deals on one page; any number opens the deals behind
  // it. Managers see every deal and may pick one person; others see their own.
  import { onMount } from 'svelte';
  import Download from '@lucide/svelte/icons/download';
  import Hourglass from '@lucide/svelte/icons/hourglass';
  import Percent from '@lucide/svelte/icons/percent';
  import Scale from '@lucide/svelte/icons/scale';
  import Sparkles from '@lucide/svelte/icons/sparkles';
  import Trophy from '@lucide/svelte/icons/trophy';
  import StatTile from '$lib/ui/StatTile.svelte';
  import { api, formatDate, formatMoney, plainMessage, type UserItem } from '$lib/api';
  import { moneyShort } from '$lib/fieldLook';
  import Avatar from '$lib/Avatar.svelte';
  import Modal from '$lib/Modal.svelte';
  import PeriodPicker from '$lib/finance/PeriodPicker.svelte';
  import { MONTHS_SHORT, periodRange, type Period } from '$lib/finance/period';

  type Kpi = {
    new: number; new_amount: number; new_prev: number; won: number; won_amount: number; won_prev: number; won_amount_prev: number;
    avg_check: number | null; lost: number; lost_amount: number; win_rate: number | null; win_rate_prev: number | null;
    cycle_days: number | null; cycle_days_prev: number | null; open: number; open_amount: number; weighted: number;
  };
  type Step = { stage_id: string | null; name: string; color: string; kind: 'open' | 'won'; count: number; amount: number };
  type Month = { month: string; won: number; won_amount: number; lost: number; lost_amount: number };
  type Manager = { user_id: string | null; name: string; new: number; won: number; won_amount: number; lost: number; open: number;
    win_rate: number | null; received?: number; debt?: number };
  type Reason = { key: string; reason: string; count: number; amount: number; empty: boolean };
  type Report = { kpi: Kpi; funnel: Step[]; months: Month[]; managers: Manager[]; totals: Manager; reasons: Reason[];
    money: boolean; can_filter: boolean; prev_from: string; prev_to: string };
  type DealItem = { id: string; title: string; company_name: string | null; responsible_name: string | null; amount: number;
    stage_name: string; stage_kind: string; created: string; closed: string | null; lost_reason: string };

  let period = $state<Period>('quarter');
  let from = $state(periodRange('quarter')[0]);
  let to = $state(periodRange('quarter')[1]);
  let who = $state('');
  let users = $state<UserItem[]>([]);
  let data = $state<Report | null>(null);
  let error = $state('');
  let loading = $state(true);

  const query = $derived(`from=${from}&to=${to}${who ? `&responsible_id=${who}` : ''}`);
  $effect(() => {
    if (!from || !to) return;
    const q = query;
    loading = true;
    api.get<Report>(`/crm/reports?${q}`)
      .then((r) => { if (q === query) { data = r; error = ''; } })
      .catch((e) => (error = plainMessage((e as Error).message)))
      .finally(() => (loading = false));
  });
  onMount(() => {
    api.get<{ users: UserItem[] }>('/users')
      .then((r) => (users = r.users.filter((u) => u.is_active && u.crm_role !== 'none' && u.role !== 'guest')))
      .catch(() => {});
  });

  const k = $derived(data?.kpi);
  const periodLabel = $derived(from && to ? `${formatDate(from)} — ${formatDate(to)}` : '');
  const funnelTop = $derived(Math.max(1, data?.funnel[0]?.count ?? 1));
  const monthPeak = $derived(Math.max(1, ...(data?.months ?? []).map((m) => Math.max(m.won_amount, m.lost_amount))));
  const reasonPeak = $derived(Math.max(1, ...(data?.reasons ?? []).map((r) => r.count)));
  const empty = $derived(!!data && !data.kpi.new && !data.kpi.won && !data.kpi.lost && !data.kpi.open);

  const pct = (v: number | null) => (v == null ? '—' : `${v.toLocaleString('ru-RU', { maximumFractionDigits: 1 })} %`);
  const days = (n: number) => {
    const a = Math.abs(n) % 100, b = a % 10;
    return `${n} ${a > 10 && a < 20 ? 'дней' : b === 1 ? 'день' : b >= 2 && b <= 4 ? 'дня' : 'дней'}`;
  };
  const dealsWord = (n: number) => {
    const a = n % 100, b = n % 10;
    return a > 10 && a < 20 ? 'сделок' : b === 1 ? 'сделка' : b >= 2 && b <= 4 ? 'сделки' : 'сделок';
  };
  const monthLabel = (m: string) => `${MONTHS_SHORT[+m.slice(5, 7) - 1]}${m.slice(5, 7) === '01' ? ` ${m.slice(2, 4)}` : ''}`;
  function delta(cur: number | null, prev: number | null, unit = '') {
    if (cur == null || prev == null) return null;
    const d = Math.round((cur - prev) * 10) / 10;
    if (!d) return { text: 'как в прошлом периоде', cls: '' };
    return { text: `${d > 0 ? '▲' : '▼'} ${Math.abs(d).toLocaleString('ru-RU')}${unit} к прошлому периоду`, cls: d > 0 ? 'up' : 'dn' };
  }

  // --- Drill-down -------------------------------------------------------------------
  let drillOpen = $state(false);
  let drillTitle = $state('');
  let drill = $state<{ deals: DealItem[]; count: number; amount: number } | null>(null);
  function open(title: string, params: Record<string, string | null | undefined>) {
    drillTitle = title;
    drill = null;
    drillOpen = true;
    const extra = Object.entries(params).filter(([, v]) => v != null).map(([key, v]) => `&${key}=${encodeURIComponent(v as string)}`).join('');
    api.get<{ deals: DealItem[]; count: number; amount: number }>(`/crm/reports/deals?${query}${extra}`)
      .then((r) => (drill = r))
      .catch((e) => { drillOpen = false; error = plainMessage((e as Error).message); });
  }
  const mgr = (m: Manager) => m.user_id ?? 'none';
</script>

<svelte:head><title>Отчёты CRM</title></svelte:head>

<div class="bar">
  <PeriodPicker bind:period bind:from bind:to allowAll={false} />
  <span class="faint small">{periodLabel}</span>
  <div class="spacer"></div>
  {#if data?.can_filter}
    <select class="input who" bind:value={who} aria-label="Ответственный">
      <option value="">Все ответственные</option>
      {#each users as u (u.id)}<option value={u.id}>{u.display_name}</option>{/each}
      <option value="none">Без ответственного</option>
    </select>
  {/if}
  <a class="btn" href="/api/crm/reports.xlsx?{query}" download><Download size={16} />XLSX</a>
</div>

{#if error}<div class="error-box">{error}</div>{/if}
{#if data && !data.can_filter}<p class="faint small note">Показаны сделки, где вы ответственный.</p>{/if}

{#if data && k}
  {@const dNew = delta(k.new, k.new_prev)}
  {@const dRate = delta(k.win_rate, k.win_rate_prev, ' п. п.')}
  <div class="stat-grid" class:dim={loading}>
    <StatTile label="Новых сделок" value={k.new} icon={Sparkles} tone="accent" onclick={() => open('Новые сделки', { set: 'new' })}
      hint={dNew?.text ?? 'созданы за период'} hintTone={dNew?.cls === 'up' ? 'ok' : dNew?.cls === 'dn' ? 'danger' : undefined} />
    <StatTile label="Выиграно · {k.won} {dealsWord(k.won)}" value={moneyShort(k.won_amount)} icon={Trophy} tone="ok" onclick={() => open('Выигранные сделки', { set: 'won' })}
      hint={k.avg_check != null ? `средний чек ${moneyShort(k.avg_check)}` : 'за период не было'} />
    <StatTile label="Конверсия в договор" value={pct(k.win_rate)} icon={Percent} tone="violet" onclick={() => open('Проигранные сделки', { set: 'lost' })}
      hint={dRate?.text ?? `выиграно ${k.won} из ${k.won + k.lost} закрытых`} hintTone={dRate?.cls === 'up' ? 'ok' : dRate?.cls === 'dn' ? 'danger' : undefined} />
    <StatTile label="Средний цикл сделки" value={k.cycle_days != null ? days(k.cycle_days) : '—'} icon={Hourglass} tone="warn" hint="от создания до выигрыша" />
    <StatTile label="В работе с учётом вероятности" value={moneyShort(k.weighted)} icon={Scale} tone="neutral" onclick={() => open('Открытые сделки', { set: 'open' })}
      hint="{k.open} открытых на {moneyShort(k.open_amount)}" />
  </div>

  {#if empty}
    <div class="card emptybox"><p class="muted">За этот период сделок нет. Выберите другой период{data.can_filter ? ' или ответственного' : ''}.</p></div>
  {:else}
    <div class="g2" class:dim={loading}>
      <section class="card">
        <div class="sec"><h2>Воронка</h2><span class="faint small">сделки, созданные за период, дошедшие до этапа</span></div>
        <div class="fun">
          {#each data.funnel as s, i (s.stage_id ?? s.name)}
            {#if i > 0}
              {@const p = data.funnel[i - 1].count}
              <div class="cv faint">↓ {p ? Math.round((s.count / p) * 100) : 0} %</div>
            {/if}
            <button class="fr" onclick={() => open(`Дошли до этапа «${s.name}»`, { set: 'funnel', stage_id: s.stage_id })} disabled={!s.count || !s.stage_id}>
              <span class="fn">{s.name}</span>
              <span class="track"><span class="fb" class:won={s.kind === 'won'} style="width:{Math.max(2, (s.count / funnelTop) * 100)}%">{s.count}</span></span>
              <span class="fm">{moneyShort(s.amount)}</span>
            </button>
          {/each}
        </div>
      </section>
      <section class="card">
        <div class="sec"><h2>Выиграно и проиграно по месяцам</h2>
          <span class="legend"><i class="lw"></i>выиграно <i class="ll"></i>проиграно</span></div>
        <div class="bars">
          {#each data.months as m (m.month)}
            <div class="col" title="{monthLabel(m.month)}: выиграно {m.won} на {formatMoney(m.won_amount)}, проиграно {m.lost} на {formatMoney(m.lost_amount)}">
              <div class="pair">
                <div class="b w" style="height:{(m.won_amount / monthPeak) * 100}%"></div>
                <div class="b l" style="height:{(m.lost_amount / monthPeak) * 100}%"></div>
              </div>
              <div class="m faint">{monthLabel(m.month)}</div>
            </div>
          {/each}
        </div>
      </section>
    </div>

    <div class="stack" class:dim={loading}>
      <section class="card mg">
        <div class="sec"><h2>Ответственные</h2></div>
        <div class="tw">
          <table>
            <thead><tr>
              <th>Ответственный</th><th class="num">Новых</th><th class="num">Выиграно</th><th class="num">Сумма выигранных</th>
              <th class="num">Конверсия</th><th class="num">Открытых</th>
              {#if data.money}<th class="num">Поступило</th><th class="num">Долг клиентов</th>{/if}
            </tr></thead>
            <tbody>
              {#each data.managers as m (mgr(m))}
                <tr>
                  <td><span class="who-cell">{#if m.user_id}<Avatar id={m.user_id} name={m.name} size={22} />{/if}{m.name}</span></td>
                  <td class="num"><button class="lnk" disabled={!m.new} onclick={() => open(`Новые сделки: ${m.name}`, { set: 'new', manager: mgr(m) })}>{m.new}</button></td>
                  <td class="num"><button class="lnk" disabled={!m.won} onclick={() => open(`Выигранные: ${m.name}`, { set: 'won', manager: mgr(m) })}>{m.won}</button></td>
                  <td class="num">{formatMoney(m.won_amount)}</td>
                  <td class="num">{pct(m.win_rate)}</td>
                  <td class="num"><button class="lnk" disabled={!m.open} onclick={() => open(`Открытые сделки: ${m.name}`, { set: 'open', manager: mgr(m) })}>{m.open}</button></td>
                  {#if data.money}
                    <td class="num">{formatMoney(m.received ?? 0)}</td>
                    <td class="num">{#if m.debt}<a href="/finance?tab=receivables">{formatMoney(m.debt)}</a>{:else}—{/if}</td>
                  {/if}
                </tr>
              {/each}
            </tbody>
            {#if data.managers.length > 1}
              <tfoot><tr>
                <td>Итого</td><td class="num">{data.totals.new}</td><td class="num">{data.totals.won}</td>
                <td class="num">{formatMoney(data.totals.won_amount)}</td><td class="num">{pct(data.totals.win_rate)}</td><td class="num">{data.totals.open}</td>
                {#if data.money}<td class="num">{formatMoney(data.totals.received ?? 0)}</td><td class="num">{formatMoney(data.totals.debt ?? 0)}</td>{/if}
              </tr></tfoot>
            {/if}
          </table>
        </div>
        {#if data.money}<p class="faint small foot">«Поступило» — оплаты за период по счетам и проектам сделок; «Долг клиентов» — неоплаченные счета на сегодня.</p>{/if}
      </section>
      <section class="card">
        <div class="sec"><h2>Причины отказов</h2><span class="faint small">{k.lost} {dealsWord(k.lost)}</span></div>
        {#if !data.reasons.length}
          <p class="muted pad">Проигранных сделок за период нет.</p>
        {:else}
          <div class="rs">
            {#each data.reasons as r (r.key)}
              <button class="rr" onclick={() => open(`Причина: ${r.reason}`, { set: 'reason', reason: r.key })} title="{r.count} на {formatMoney(r.amount)}">
                <span class="rn" class:faint={r.empty}>{r.reason}</span><span class="rc">{r.count}</span>
                <span class="rt"><span class="rb" class:none={r.empty} style="width:{(r.count / reasonPeak) * 100}%"></span></span>
              </button>
            {/each}
          </div>
        {/if}
      </section>
    </div>
  {/if}
{:else if !error}
  <p class="muted">Загрузка…</p>
{/if}

<Modal title={drillTitle} bind:open={drillOpen} width={760}>
  {#if !drill}
    <p class="muted">Загрузка…</p>
  {:else if !drill.count}
    <p class="muted">Сделок нет.</p>
  {:else}
    <p class="faint small dsum">{drill.count} {dealsWord(drill.count)} на {formatMoney(drill.amount)}{drill.count > drill.deals.length ? ` · показаны первые ${drill.deals.length}` : ''}</p>
    <ul class="dl">
      {#each drill.deals as d (d.id)}
        <li>
          <a href="/crm/deals/{d.id}">{d.title}</a>
          <span class="amt">{formatMoney(d.amount)}</span>
          <span class="faint small meta">
            {[d.company_name, d.responsible_name, d.stage_name, d.closed ? `закрыта ${formatDate(d.closed)}` : `создана ${formatDate(d.created)}`]
              .filter(Boolean).join(' · ')}{d.stage_kind === 'lost' && d.lost_reason ? ` · ${d.lost_reason}` : ''}
          </span>
        </li>
      {/each}
    </ul>
  {/if}
</Modal>

<style>
  .bar { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; margin-bottom: 14px; }
  .spacer { flex: 1; }
  .who { width: auto; min-width: 200px; }
  .note { margin: -6px 0 10px; }
  .small { font-size: 12.5px; }
  .dim { opacity: .6; transition: opacity .15s; }
  .g2 { display: grid; grid-template-columns: minmax(0, 1.15fr) minmax(0, 1fr); gap: 14px; margin-bottom: 14px; }
  .stack { display: grid; gap: 14px; margin-bottom: 14px; }
  .card { min-width: 0; }
  .sec { display: flex; gap: 12px; align-items: baseline; justify-content: space-between; padding: 12px 16px; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  h2 { font-size: 14px; margin: 0; }
  .fun { padding: 12px 16px 14px; display: grid; gap: 4px; }
  .fr { display: grid; grid-template-columns: minmax(90px, 200px) 1fr auto; gap: 10px; align-items: center; background: none; border: 0; padding: 3px 4px;
    margin: 0 -4px; border-radius: 6px; font: inherit; color: inherit; text-align: left; cursor: pointer; }
  .fr:hover:not(:disabled) { background: var(--surface-2); }
  .fr:disabled { cursor: default; }
  .fn { font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .track { display: block; }
  .fb { display: flex; align-items: center; height: 24px; padding: 0 8px; border-radius: 6px; background: var(--accent); color: #fff; font-size: 12px; font-weight: 600;
    min-width: 26px; box-sizing: border-box; }
  .fb.won { background: var(--ok); }
  .fm { font-size: 12.5px; color: var(--text-2); font-variant-numeric: tabular-nums; white-space: nowrap; min-width: 84px; text-align: right; }
  .cv { font-size: 11.5px; padding-left: min(210px, 30%); }
  .legend { display: inline-flex; gap: 6px; align-items: center; font-size: 12px; color: var(--text-2); }
  .legend i { width: 10px; height: 10px; border-radius: 3px; display: inline-block; margin-left: 6px; }
  .lw, .b.w { background: var(--accent); }
  .ll, .b.l { background: var(--border-strong, #cfd6e0); }
  .bars { display: flex; gap: 8px; align-items: flex-end; height: 200px; padding: 12px 16px 10px; overflow-x: auto; }
  .col { flex: 1; min-width: 28px; height: 100%; display: flex; flex-direction: column; }
  .pair { flex: 1; display: flex; gap: 3px; align-items: flex-end; justify-content: center; border-bottom: 1px solid var(--border); }
  .b { width: 38%; max-width: 16px; border-radius: 3px 3px 0 0; min-height: 1px; }
  .m { font-size: 11px; text-align: center; margin-top: 4px; white-space: nowrap; }
  .tw { overflow-x: auto; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  th { text-align: left; font-weight: 500; color: var(--text-3); padding: 8px 12px; border-bottom: 1px solid var(--border); white-space: nowrap; }
  td { padding: 7px 12px; border-bottom: 1px solid var(--border); }
  tfoot td { font-weight: 600; border-bottom: 0; background: var(--surface-2); }
  .num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .who-cell { display: inline-flex; gap: 8px; align-items: center; white-space: nowrap; }
  .lnk { background: none; border: 0; padding: 0; font: inherit; color: var(--accent); cursor: pointer; font-variant-numeric: tabular-nums; }
  .lnk:disabled { color: var(--text-3); cursor: default; }
  .foot { margin: 0; padding: 8px 16px 12px; }
  .rs { padding: 10px 16px 14px; display: grid; gap: 2px 32px; grid-template-columns: repeat(auto-fill, minmax(min(100%, 380px), 1fr)); }
  .rr { display: grid; grid-template-columns: minmax(0, 1fr) 28px minmax(60px, 34%); gap: 10px; align-items: center; background: none; border: 0; padding: 5px 4px;
    margin: 0 -4px; border-radius: 6px; font: inherit; font-size: 13px; color: inherit; text-align: left; cursor: pointer; }
  .rr:hover { background: var(--surface-2); }
  .rn { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rc { text-align: right; font-variant-numeric: tabular-nums; }
  .rt { display: block; }
  .rb { display: block; height: 8px; border-radius: 4px; background: color-mix(in srgb, var(--danger) 45%, var(--surface)); }
  .rb.none { background: var(--border-strong, #cfd6e0); }
  .pad { padding: 12px 16px; margin: 0; }
  .emptybox { padding: 24px; }
  .emptybox p { margin: 0; }
  .dsum { margin: 0 0 8px; }
  .dl { list-style: none; margin: 0; padding: 0; display: grid; max-height: 60vh; overflow: auto; }
  .dl li { display: grid; grid-template-columns: 1fr auto; gap: 2px 12px; padding: 8px 0; border-bottom: 1px solid var(--border); }
  .dl li:last-child { border-bottom: 0; }
  .amt { font-variant-numeric: tabular-nums; white-space: nowrap; }
  .meta { grid-column: 1 / -1; }
  @media (max-width: 900px) {
    .g2 { grid-template-columns: minmax(0, 1fr); }
  }
  @media (max-width: 640px) {
    .who { min-width: 0; flex: 1 1 100%; order: 3; }
        .fr { grid-template-columns: minmax(0, 1fr) auto; row-gap: 4px; }
    .fn { white-space: normal; }
    .track { grid-column: 1 / -1; grid-row: 2; }
    .fm { min-width: 0; }
    .cv { padding-left: 0; }
  }
</style>
