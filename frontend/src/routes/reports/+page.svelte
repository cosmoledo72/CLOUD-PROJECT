<script lang="ts">
  // «Отчёты»: hours (and, for finance, labour costs) over a period as a tree —
  // by projects, employees or tasks, down to the entries of each day; a chart
  // of hours per day or week; XLSX export; the «Ставки» tab with hour rates.
  import ChevronLeft from '@lucide/svelte/icons/chevron-left';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Download from '@lucide/svelte/icons/download';
  import Clock from '@lucide/svelte/icons/clock';
  import Banknote from '@lucide/svelte/icons/banknote';
  import BadgePercent from '@lucide/svelte/icons/badge-russian-ruble';
  import ListChecks from '@lucide/svelte/icons/list-checks';
  import Timer from '@lucide/svelte/icons/timer';
  import StatTile from '#lib/ui/StatTile.svelte';
  import { api, formatDate, localToday } from '#lib/api.ts';
  import EmptyState from '#lib/EmptyState.svelte';
  import { money, moneyShort } from '#lib/fieldLook.ts';
  import MultiPick from '#lib/time/MultiPick.svelte';
  import { hm, hours } from '#lib/time/timer.svelte.ts';
  import { fromDay, toDay } from '#lib/workCalendar.svelte.ts';

  interface Row {
    id: string;
    work_date: string;
    minutes: number;
    note: string;
    source: string;
    user_id: string;
    user_name: string;
    task_id: string;
    task_number: number;
    task_title: string;
    task_status: string;
    parent_id: string | null;
    parent_title: string | null;
    project_id: string;
    project_name: string;
    pay_mode: 'hourly' | 'fixed';
    hour_rate: number | null;
    cost: number | null;
  }
  interface Fixed {
    task_id: string;
    task_number: number;
    task_title: string;
    project_id: string;
    project_name: string;
    assignee_name: string | null;
    pay_amount: number;
    done_on: string;
  }
  interface Report {
    entries: Row[];
    fixed: Fixed[];
    truncated: boolean;
    sees_money: boolean;
    totals: { minutes: number; hourly_cost: number | null; fixed_cost: number | null };
    people: { id: string; display_name: string }[];
    projects: { id: string; name: string }[];
  }

  function stored<T>(key: string, dflt: T): T {
    try {
      const v = localStorage.getItem(key);
      return v ? (JSON.parse(v) as T) : dflt;
    } catch {
      return dflt;
    }
  }
  function store(key: string, v: unknown) {
    try { localStorage.setItem(key, JSON.stringify(v)); } catch { /* storage unavailable */ }
  }

  // --- Period ---------------------------------------------------------------------
  type Span = 'week' | 'month' | 'quarter' | 'year' | 'custom';
  const today = localToday();
  let span = $state<Span>(stored('wh_reports_span', 'month'));
  let anchor = $state(today);
  let customFrom = $state(fromDay(toDay(today) - 30));
  let customTo = $state(today);
  $effect(() => store('wh_reports_span', span));

  const MONTHS = ['январь', 'февраль', 'март', 'апрель', 'май', 'июнь', 'июль', 'август', 'сентябрь', 'октябрь', 'ноябрь', 'декабрь'];
  const ymd = (y: number, m: number, d: number) => new Date(Date.UTC(y, m, d)).toISOString().slice(0, 10);
  const period = $derived.by((): { from: string; to: string; label: string } => {
    const [y, m] = anchor.split('-').map(Number);
    if (span === 'week') {
      const n = toDay(anchor);
      const wd = (new Date(n * 86_400_000).getUTCDay() + 6) % 7;
      const from = fromDay(n - wd);
      const to = fromDay(n - wd + 6);
      return { from, to, label: `${formatDate(from).slice(0, 5)} — ${formatDate(to)}` };
    }
    if (span === 'month') return { from: ymd(y, m - 1, 1), to: ymd(y, m, 0), label: `${MONTHS[m - 1]} ${y}` };
    if (span === 'quarter') {
      const q = Math.floor((m - 1) / 3);
      return { from: ymd(y, q * 3, 1), to: ymd(y, q * 3 + 3, 0), label: `${['I', 'II', 'III', 'IV'][q]} квартал ${y}` };
    }
    if (span === 'year') return { from: ymd(y, 0, 1), to: ymd(y, 11, 31), label: `${y} год` };
    return { from: customFrom, to: customTo < customFrom ? customFrom : customTo, label: '' };
  });
  function shift(dir: 1 | -1) {
    const [y, m, d] = anchor.split('-').map(Number);
    if (span === 'week') anchor = fromDay(toDay(anchor) + 7 * dir);
    else if (span === 'month') anchor = ymd(y, m - 1 + dir, 1);
    else if (span === 'quarter') anchor = ymd(y, m - 1 + 3 * dir, 1);
    else if (span === 'year') anchor = ymd(y + dir, 0, 1);
    void d;
  }

  // --- Filters -----------------------------------------------------------------------
  let users = $state<string[]>(stored('wh_reports_users', []));
  let projects = $state<string[]>(stored('wh_reports_projects', []));
  let statuses = $state<string[]>(stored('wh_reports_statuses', []));
  $effect(() => store('wh_reports_users', users));
  $effect(() => store('wh_reports_projects', projects));
  $effect(() => store('wh_reports_statuses', statuses));
  const STATUSES = [
    { id: 'open', name: 'К выполнению' },
    { id: 'in_progress', name: 'В работе' },
    { id: 'review', name: 'На проверке' },
    { id: 'done', name: 'Выполнено' }
  ];
  type Group = 'project' | 'user' | 'task';
  let group = $state<Group>(stored('wh_reports_group', 'project'));
  $effect(() => store('wh_reports_group', group));

  const query = $derived.by(() => {
    const p = new URLSearchParams({ from: period.from, to: period.to });
    if (users.length) p.set('users', users.join(','));
    if (projects.length) p.set('projects', projects.join(','));
    if (statuses.length) p.set('statuses', statuses.join(','));
    return p.toString();
  });

  let data = $state<Report | null>(null);
  let loading = $state(false);
  let error = $state('');
  $effect(() => {
    const q = query;
    loading = true;
    error = '';
    api.get<Report>(`/time/report?${q}`)
      .then((r) => { if (q === query) data = r; })
      .catch((e) => (error = (e as Error).message))
      .finally(() => (loading = false));
  });

  const money_ = $derived(!!data?.sees_money);
  const rows = $derived(data?.entries ?? []);

  // --- Tree -----------------------------------------------------------------------------
  interface Node {
    id: string;
    label: string;
    sub?: string;
    href?: string;
    minutes: number;
    cost: number;
    fixed?: boolean;
    children: Node[];
    entries: Row[];
  }
  function node(id: string, label: string, extra: Partial<Node> = {}): Node {
    return { id, label, minutes: 0, cost: 0, children: [], entries: [], ...extra };
  }
  function child(parent: Node, id: string, make: () => Node): Node {
    let c = parent.children.find((x) => x.id === id);
    if (!c) {
      c = make();
      parent.children.push(c);
    }
    return c;
  }
  const taskHref = (r: Row, id: string = r.task_id) => `/projects/${r.project_id}?task=${id}`;
  /** Task level under a parent: top task → its subtask. */
  function putTask(under: Node, r: Row): Node {
    if (r.parent_id) {
      const top = child(under, under.id + '/' + r.parent_id, () => node(under.id + '/' + r.parent_id, r.parent_title ?? '—', { href: taskHref(r, r.parent_id!) }));
      return child(top, top.id + '/' + r.task_id, () => node(top.id + '/' + r.task_id, `#${r.task_number} ${r.task_title}`, { href: taskHref(r), fixed: r.pay_mode === 'fixed' }));
    }
    return child(under, under.id + '/' + r.task_id, () => node(under.id + '/' + r.task_id, `#${r.task_number} ${r.task_title}`, { href: taskHref(r), fixed: r.pay_mode === 'fixed' }));
  }
  const tree = $derived.by((): Node[] => {
    const root = node('', '');
    for (const r of rows) {
      let path: Node[];
      if (group === 'project') {
        const p = child(root, r.project_id, () => node(r.project_id, r.project_name, { href: `/projects/${r.project_id}` }));
        const t = putTask(p, r);
        path = [p, t];
        if (r.parent_id) path.splice(1, 0, p.children.find((x) => x.id === p.id + '/' + r.parent_id)!);
      } else if (group === 'user') {
        const u = child(root, r.user_id, () => node(r.user_id, r.user_name));
        const p = child(u, u.id + '/' + r.project_id, () => node(u.id + '/' + r.project_id, r.project_name, { href: `/projects/${r.project_id}` }));
        const t = putTask(p, r);
        path = [u, p, t];
        if (r.parent_id) path.splice(2, 0, p.children.find((x) => x.id === p.id + '/' + r.parent_id)!);
      } else {
        const t = child(root, r.task_id, () => node(r.task_id, `#${r.task_number} ${r.task_title}`, { sub: r.project_name, href: taskHref(r), fixed: r.pay_mode === 'fixed' }));
        path = [t];
      }
      for (const n of path) {
        n.minutes += r.minutes;
        n.cost += r.cost ?? 0;
      }
      path[path.length - 1].entries.push(r);
    }
    const sort = (list: Node[]) => {
      list.sort((a, b) => b.minutes - a.minutes || a.label.localeCompare(b.label, 'ru'));
      for (const n of list) {
        sort(n.children);
        n.entries.sort((a, b) => a.work_date.localeCompare(b.work_date) || a.user_name.localeCompare(b.user_name, 'ru'));
      }
    };
    sort(root.children);
    return root.children;
  });

  // Expanded nodes: the first level by default.
  let openIds = $state<Set<string>>(new Set());
  let touched = $state(false);
  $effect(() => {
    void group;
    touched = false;
  });
  const isOpen = (id: string, depth: number) => (touched ? openIds.has(id) : depth === 0);
  function toggle(id: string, depth: number) {
    if (!touched) {
      openIds = new Set(tree.map((n) => n.id));
      touched = true;
    }
    const next = new Set(openIds);
    if (isOpen(id, depth)) next.delete(id);
    else next.add(id);
    openIds = next;
  }
  function expandAll(on: boolean) {
    const all = new Set<string>();
    const walk = (list: Node[]) => list.forEach((n) => { all.add(n.id); walk(n.children); });
    if (on) walk(tree);
    openIds = all;
    touched = true;
  }

  // --- Chart: hours per day (≤ 62 days) or per week ------------------------------------------
  const chart = $derived.by(() => {
    const a = toDay(period.from);
    const b = toDay(period.to);
    const byWeek = b - a > 62;
    const buckets: { key: number; label: string; title: string; minutes: number; weekend: boolean }[] = [];
    if (byWeek) {
      let w = a - ((new Date(a * 86_400_000).getUTCDay() + 6) % 7);
      for (; w <= b; w += 7) buckets.push({ key: w, label: fromDay(Math.max(w, a)).slice(8, 10) + '.' + fromDay(Math.max(w, a)).slice(5, 7), title: `Неделя с ${formatDate(fromDay(w))}`, minutes: 0, weekend: false });
    } else {
      for (let d = a; d <= b; d++) {
        const wd = new Date(d * 86_400_000).getUTCDay();
        buckets.push({ key: d, label: fromDay(d).slice(8, 10), title: formatDate(fromDay(d)), minutes: 0, weekend: wd === 0 || wd === 6 });
      }
    }
    for (const r of rows) {
      const d = toDay(r.work_date);
      const k = byWeek ? d - ((new Date(d * 86_400_000).getUTCDay() + 6) % 7) : d;
      const bk = buckets.find((x) => x.key === k);
      if (bk) bk.minutes += r.minutes;
    }
    const max = Math.max(60, ...buckets.map((x) => x.minutes));
    return { buckets, max, byWeek };
  });

  const peopleCount = $derived(new Set(rows.map((r) => r.user_id)).size);
  const exportHref = $derived(`/api/time/report.xlsx?${query}`);

  // --- Rates tab ------------------------------------------------------------------------------
  let tab = $state<'hours' | 'rates'>('hours');
  interface Rate { user_id: string; display_name: string; hour_rate: number | null; is_active: boolean }
  let rates = $state<Rate[]>([]);
  let ratesEdit = $state(false);
  let rateDraft = $state<Record<string, string>>({});
  let applyUnrated = $state(true);
  let rateMsg = $state('');
  async function loadRates() {
    const r = await api.get<{ rates: Rate[]; can_edit: boolean }>('/time/rates');
    rates = r.rates;
    ratesEdit = r.can_edit;
    rateDraft = Object.fromEntries(r.rates.map((x) => [x.user_id, x.hour_rate != null ? String(x.hour_rate) : '']));
  }
  $effect(() => { if (tab === 'rates') loadRates().catch((e) => (error = (e as Error).message)); });
  async function saveRate(r: Rate) {
    const raw = (rateDraft[r.user_id] ?? '').replace(/\s/g, '').replace(',', '.');
    const v = raw === '' ? null : Number(raw);
    if (v !== null && (!Number.isFinite(v) || v < 0)) { rateMsg = 'Ставка — число от 0'; return; }
    if (v === r.hour_rate) return;
    const res = await api.put<{ applied: number }>(`/time/rates/${r.user_id}`, { hour_rate: v, apply_to_unrated: applyUnrated });
    rateMsg = `${r.display_name}: ставка ${v == null ? 'снята' : money(v) + '/ч'}${res.applied ? `, проставлена в ${res.applied} записях без ставки` : ''}.`;
    setTimeout(() => (rateMsg = ''), 5000);
    await loadRates();
  }
</script>

<div class="page">
  {#if money_}
    <div class="tabs-line" role="tablist" aria-label="Раздел отчётов">
      <button role="tab" aria-selected={tab === 'hours'} class:on={tab === 'hours'} onclick={() => (tab = 'hours')}><Timer size={17} />Трудозатраты</button>
      <button role="tab" aria-selected={tab === 'rates'} class:on={tab === 'rates'} onclick={() => (tab = 'rates')}><BadgePercent size={17} />Ставки</button>
    </div>
  {/if}
  <div class="page-head">
    <h1>{tab === 'rates' ? 'Ставки' : money_ ? 'Трудозатраты' : 'Отчёты'}</h1>
    <div class="spacer"></div>
    {#if tab === 'hours'}<a class="btn" href={exportHref} download><Download size={16} />XLSX</a>{/if}
  </div>

  {#if tab === 'hours'}
    <div class="bar">
      <div class="seg" role="radiogroup" aria-label="Период">
        {#each [['week', 'Неделя'], ['month', 'Месяц'], ['quarter', 'Квартал'], ['year', 'Год'], ['custom', 'Период']] as [k, l] (k)}
          <button role="radio" aria-checked={span === k} class:on={span === k} onclick={() => (span = k as Span)}>{l}</button>
        {/each}
      </div>
      {#if span === 'custom'}
        <input class="input dt" type="date" bind:value={customFrom} aria-label="С" /> — <input class="input dt" type="date" bind:value={customTo} aria-label="По" />
      {:else}
        <div class="nav">
          <button class="icon-btn" onclick={() => shift(-1)} aria-label="Раньше"><ChevronLeft size={18} /></button>
          <span class="plabel">{period.label}</span>
          <button class="icon-btn" onclick={() => shift(1)} aria-label="Позже"><ChevronRight size={18} /></button>
          {#if anchor !== today}<button class="lnk" onclick={() => (anchor = today)}>сегодня</button>{/if}
        </div>
      {/if}
    </div>
    <div class="bar">
      <MultiPick label="Все сотрудники" items={(data?.people ?? []).map((p) => ({ id: p.id, name: p.display_name }))} bind:value={users} />
      <MultiPick label="Все проекты" items={data?.projects ?? []} bind:value={projects} />
      <MultiPick label="Все статусы задач" items={STATUSES} bind:value={statuses} />
      <span class="spacer"></span>
      <div class="seg" role="radiogroup" aria-label="Группировка">
        {#each [['project', 'По проектам'], ['user', 'По сотрудникам'], ['task', 'По задачам']] as [k, l] (k)}
          <button role="radio" aria-checked={group === k} class:on={group === k} onclick={() => (group = k as Group)}>{l}</button>
        {/each}
      </div>
    </div>

    {#if error}<div class="error-box">{error}</div>{/if}

    {#if data}
      <div class="stat-grid">
        <StatTile label="Всего за период" value="{hours(data.totals.minutes)} ч" icon={Clock} tone="accent" hint={period.label || 'выбранный период'} />
        <StatTile label="Записей времени" value={rows.length} icon={ListChecks} tone="neutral"
          hint="{peopleCount} {peopleCount === 1 ? 'сотрудник' : peopleCount >= 2 && peopleCount <= 4 ? 'сотрудника' : 'сотрудников'}" />
        {#if money_}
          <StatTile label="Стоимость по часам" value={moneyShort(data.totals.hourly_cost ?? 0)} icon={Banknote} tone="ok" hint="часы × ставка сотрудника"
            title="Часы × ставка сотрудника на момент записи; задачи с фиксированной оплатой не входят" />
          <StatTile label="Фиксированная оплата" value={moneyShort(data.totals.fixed_cost ?? 0)} icon={BadgePercent} tone="warn" hint="задачи, выполненные за период"
            title="Фиксированная оплата задач, выполненных за период" />
        {/if}
      </div>

      {#if rows.length}
        <div class="card chart" aria-label="Часы по {chart.byWeek ? 'неделям' : 'дням'}">
          <div class="ch-head"><span>Часы по {chart.byWeek ? 'неделям' : 'дням'}</span><span class="faint">макс. {hours(chart.max)} ч</span></div>
          <div class="bars" style="--n:{chart.buckets.length}">
            {#each chart.buckets as b (b.key)}
              <div class="bc" class:we={b.weekend} title="{b.title}: {hm(b.minutes)}">
                <div class="b" style="height:{(b.minutes / chart.max) * 100}%"></div>
                <span class="bl">{b.label}</span>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      {#if data.truncated}<p class="warn-box">Показаны первые 20 000 записей — сузьте период или фильтры.</p>{/if}

      {#if !rows.length}
        <div class="card"><EmptyState icon={Clock} title={loading ? 'Загрузка…' : 'Нет записей времени'} text="За выбранный период и с этими фильтрами время не записано. Время записывается в карточке задачи — таймером или вручную." /></div>
      {:else}
        <div class="card tree" class:money={money_}>
          <div class="th">
            <span>{group === 'project' ? 'Проект / задача' : group === 'user' ? 'Сотрудник / проект / задача' : 'Задача'}</span>
            <span class="tools"><button class="lnk" onclick={() => expandAll(true)}>развернуть всё</button> · <button class="lnk" onclick={() => expandAll(false)}>свернуть</button></span>
            <span class="r">Часы</span>
            {#if money_}<span class="r">Стоимость</span>{/if}
          </div>
          {#snippet branch(list: Node[], depth: number)}
            {#each list as n (n.id)}
              {@const op = isOpen(n.id, depth)}
              <div class="tr lvl" style="--d:{depth}" class:top={depth === 0}>
                <button class="tg" onclick={() => toggle(n.id, depth)} aria-expanded={op} aria-label={op ? 'Свернуть' : 'Развернуть'}>
                  <span class="chev" class:open={op}><ChevronRight size={14} /></span>
                </button>
                <span class="nm">
                  {#if n.href}<a href={n.href}>{n.label}</a>{:else}{n.label}{/if}
                  {#if n.sub}<span class="sub">{n.sub}</span>{/if}
                  {#if n.fixed && money_}<span class="fx" title="Фиксированная оплата за объём">фикс.</span>{/if}
                </span>
                <span class="r nums">{hours(n.minutes)}</span>
                {#if money_}<span class="r nums">{n.cost ? money(n.cost) : n.fixed ? '—' : ''}</span>{/if}
              </div>
              {#if op}
                {@render branch(n.children, depth + 1)}
                {#each n.entries as e (e.id)}
                  <div class="tr ent" style="--d:{depth + 1}">
                    <span></span>
                    <span class="nm">
                      <span class="dt nums">{formatDate(e.work_date)}</span>
                      {#if group !== 'user'}<span class="who">{e.user_name}</span>{/if}
                      <span class="note">{e.note}</span>
                      {#if e.source === 'timer'}<span class="src">таймер</span>{/if}
                    </span>
                    <span class="r nums">{hours(e.minutes)}</span>
                    {#if money_}<span class="r nums faint">{e.cost != null ? money(e.cost) : ''}</span>{/if}
                  </div>
                {/each}
              {/if}
            {/each}
          {/snippet}
          {@render branch(tree, 0)}
          <div class="tr tot">
            <span></span><span class="nm">Итого</span>
            <span class="r nums">{hours(data.totals.minutes)}</span>
            {#if money_}<span class="r nums">{money(data.totals.hourly_cost ?? 0)}</span>{/if}
          </div>
        </div>
      {/if}

      {#if money_ && data.fixed.length}
        <h2 class="h2">Фиксированная оплата за объём</h2>
        <p class="faint small">Задачи с фиксированной оплатой, выполненные за период: сумма засчитывается в трудозатраты в день выполнения.</p>
        <div class="card table">
          <table class="ft">
            <thead><tr><th>Выполнена</th><th>Проект</th><th>Задача</th><th>Исполнитель</th><th class="r">Сумма</th></tr></thead>
            <tbody>
              {#each data.fixed as f (f.task_id)}
                <tr>
                  <td class="nums">{formatDate(f.done_on)}</td>
                  <td>{f.project_name}</td>
                  <td><a href="/projects/{f.project_id}?task={f.task_id}">#{f.task_number} {f.task_title}</a></td>
                  <td>{f.assignee_name ?? '—'}</td>
                  <td class="r nums">{money(f.pay_amount)}</td>
                </tr>
              {/each}
              <tr class="ftot"><td colspan="4">Итого</td><td class="r nums">{money(data.totals.fixed_cost ?? 0)}</td></tr>
            </tbody>
          </table>
        </div>
      {/if}
    {/if}
  {:else}
    <p class="muted">Стоимость часа сотрудника — для трудозатрат задач с оплатой по часам. Ставка запоминается в каждой записи времени: изменение ставки не пересчитывает прошлые записи.</p>
    {#if rateMsg}<div class="ok-box">{rateMsg}</div>{/if}
    {#if error}<div class="error-box">{error}</div>{/if}
    <div class="card table">
      <table class="ft">
        <thead><tr><th>Сотрудник</th><th class="r">Ставка, ₽/ч</th></tr></thead>
        <tbody>
          {#each rates as r (r.user_id)}
            <tr class:inactive={!r.is_active}>
              <td>{r.display_name}{#if !r.is_active} <span class="faint small">(отключён)</span>{/if}</td>
              <td class="r">
                {#if ratesEdit}
                  <input class="input rate" inputmode="decimal" placeholder="не задана" bind:value={rateDraft[r.user_id]}
                         onchange={() => saveRate(r)} onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()} aria-label="Ставка {r.display_name}" />
                {:else}
                  <span class="nums">{r.hour_rate != null ? money(r.hour_rate) : '—'}</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    {#if ratesEdit}
      <label class="check small apply"><input type="checkbox" bind:checked={applyUnrated} />Проставлять новую ставку в прошлые записи сотрудника, где ставки ещё нет</label>
    {/if}
  {/if}
</div>

<style>
  .bar { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin-bottom: 10px; }
  .bar .spacer { flex: 1; }
  .nav { display: flex; align-items: center; gap: 4px; }
  .plabel { min-width: 150px; text-align: center; font-weight: 600; }
  .plabel::first-letter { text-transform: uppercase; }
  .dt { width: auto; }
  .lnk { border: 0; background: none; color: var(--accent); cursor: pointer; font: inherit; font-size: 13px; padding: 0; }
  .page-head .btn { display: inline-flex; align-items: center; gap: 6px; }
  .nums { font-variant-numeric: tabular-nums; }

  .chart { padding: 12px 14px 8px; margin-bottom: 12px; }
  .ch-head { display: flex; justify-content: space-between; font-size: 12.5px; color: var(--text-2); font-weight: 500; margin-bottom: 8px; }
  .bars { display: grid; grid-template-columns: repeat(var(--n), minmax(0, 1fr)); gap: 3px; height: 120px; align-items: end; }
  .bc { height: 100%; display: grid; grid-template-rows: 1fr auto; align-items: end; min-width: 0; }
  .bc .b { background: var(--accent); border-radius: 3px 3px 0 0; min-height: 0; opacity: .85; }
  .bc.we { background: color-mix(in srgb, var(--text-3) 7%, transparent); border-radius: 3px; }
  .bc:hover .b { opacity: 1; }
  .bl { font-size: 10px; color: var(--text-3); text-align: center; padding-top: 3px; overflow: hidden; white-space: nowrap; }

  .tree { padding: 0; overflow: hidden; }
  .th, .tr { display: grid; grid-template-columns: 28px 1fr 90px; align-items: center; gap: 8px; padding: 0 14px 0 6px; }
  .tree.money .tr { grid-template-columns: 28px 1fr 90px 130px; }
  .th { grid-template-columns: 1fr auto 90px; padding: 9px 14px; background: var(--surface-2); border-bottom: 1px solid var(--border); font-size: 12.5px; color: var(--text-3); font-weight: 500; }
  .tree.money .th { grid-template-columns: 1fr auto 90px 130px; }
  .tools { font-weight: 400; }
  .r { text-align: right; }
  .tr { min-height: 36px; border-bottom: 1px solid var(--border); font-size: 13.5px; }
  .tr .nm { padding-left: calc(var(--d) * 22px); min-width: 0; display: flex; align-items: center; gap: 8px; }
  .tr.lvl .nm { font-weight: 500; }
  .tr.top { background: color-mix(in srgb, var(--surface-2) 55%, transparent); }
  .tr.top .nm { font-weight: 600; }
  .tr .nm a { color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tr .nm a:hover { color: var(--accent); }
  .sub { font-size: 12px; color: var(--text-3); font-weight: 400; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .fx { font-size: 11px; color: var(--warn); background: var(--warn-soft); border-radius: 999px; padding: 0 7px; font-weight: 500; }
  .tg { border: 0; background: none; padding: 4px; cursor: pointer; color: var(--text-3); display: grid; place-items: center; }
  .chev { display: grid; place-items: center; transition: transform .15s; }
  .chev.open { transform: rotate(90deg); }
  .tr.ent { font-size: 13px; color: var(--text-2); min-height: 30px; }
  .tr.ent .nm { padding-left: calc(var(--d) * 22px + 4px); }
  .dt { color: var(--text-3); white-space: nowrap; }
  .who { white-space: nowrap; }
  .note { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-3); }
  .src { font-size: 11px; color: var(--accent); }
  .tr.tot { font-weight: 600; border-bottom: 0; background: var(--surface-2); min-height: 40px; }
  .h2 { font-size: 16px; margin: 22px 0 4px; }
  .table { padding: 0; overflow-x: auto; }
  .ft { width: 100%; border-collapse: collapse; font-size: 13.5px; }
  .ft th { text-align: left; font-weight: 500; color: var(--text-3); font-size: 12.5px; padding: 9px 12px; background: var(--surface-2); border-bottom: 1px solid var(--border); }
  .ft td { padding: 8px 12px; border-bottom: 1px solid var(--border); }
  .ft th.r, .ft td.r { text-align: right; }
  .ftot td { font-weight: 600; border-bottom: 0; }
  .inactive td { color: var(--text-3); }
  .rate { width: 140px; text-align: right; display: inline-block; }
  .apply { margin-top: 10px; display: flex; gap: 8px; align-items: center; }
  .warn-box { background: var(--warn-soft); color: var(--warn); padding: 8px 12px; border-radius: 8px; }
  @media (max-width: 720px) {
    .tree .tools { display: none; }
    .tr .nm { padding-left: calc(var(--d) * 12px); }
    .tr .note { display: none; }
    .bl { display: none; }
  }
</style>
