<script lang="ts">
  // Working calendar: the year by months with weekends, holidays, transferred
  // days off, working weekends and shortened days. The official calendar ships
  // with the portal; an administrator can change any day (a corporate day
  // off) or fill a year without official data with the Labour Code holidays.
  import { api } from '#lib/api.ts';
  import Modal from '#lib/Modal.svelte';
  import { toDay, workCalendar, type CalendarDay, type DayKind } from '#lib/workCalendar.svelte.ts';
  import CalendarDays from '@lucide/svelte/icons/calendar-days';
  import ChevronLeft from '@lucide/svelte/icons/chevron-left';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';

  interface YearData {
    days: CalendarDay[];
    months: number[];
    work_days: number;
    rest_days: number;
    short_days: number;
    hours_40: number;
    official: boolean;
    official_years: number[];
  }

  let year = $state(new Date().getFullYear());
  let data = $state<YearData | null>(null);
  let error = $state('');
  let notice = $state('');
  let tick = $state(0);

  $effect(() => {
    void tick;
    const y = year;
    api.get<YearData>(`/calendar?year=${y}`).then((r) => { if (y === year) { data = r; error = ''; } }).catch((e) => (error = e.message));
  });

  const MONTHS = ['Январь', 'Февраль', 'Март', 'Апрель', 'Май', 'Июнь', 'Июль', 'Август', 'Сентябрь', 'Октябрь', 'Ноябрь', 'Декабрь'];
  const WD = ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс'];
  const KIND_LABEL: Record<DayKind, string> = {
    holiday: 'Праздник',
    off: 'Выходной (перенос)',
    work: 'Рабочий выходной',
    short: 'Сокращённый день'
  };

  const special = $derived(new Map((data?.days ?? []).map((d) => [d.date, d])));
  const iso = (y: number, m: number, d: number) => `${y}-${String(m + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
  const isWeekend = (y: number, m: number, d: number) => {
    const w = new Date(Date.UTC(y, m, d)).getUTCDay();
    return w === 0 || w === 6;
  };

  type Cell = { day: number; date: string; cls: string; title: string; custom: boolean } | null;
  function month(m: number): Cell[] {
    const first = (new Date(Date.UTC(year, m, 1)).getUTCDay() + 6) % 7;
    const n = new Date(Date.UTC(year, m + 1, 0)).getUTCDate();
    const cells: Cell[] = Array.from({ length: first }, () => null);
    for (let d = 1; d <= n; d++) {
      const date = iso(year, m, d);
      const s = special.get(date);
      const we = isWeekend(year, m, d);
      let cls = we ? 'rest' : 'work';
      let title = we ? 'Выходной' : 'Рабочий день';
      if (s) {
        cls = { holiday: 'holiday', off: 'off', work: 'work we-work', short: we ? 'work we-work short' : 'work short' }[s.kind];
        title = s.title || KIND_LABEL[s.kind];
      }
      cells.push({ day: d, date, cls, title, custom: !!s?.custom });
    }
    return cells;
  }

  // --- Day editor ------------------------------------------------------------
  let editOpen = $state(false);
  let editDate = $state('');
  let editKind = $state<DayKind | 'default'>('default');
  let editTitle = $state('');
  let busy = $state(false);
  const editInfo = $derived(special.get(editDate));
  const editWeekend = $derived(editDate ? (() => { const [y, m, d] = editDate.split('-').map(Number); return isWeekend(y, m - 1, d); })() : false);

  function openDay(c: NonNullable<Cell>) {
    editDate = c.date;
    const s = special.get(c.date);
    editKind = s?.kind ?? 'default';
    editTitle = s?.title ?? '';
    editOpen = true;
  }

  async function saveDay() {
    busy = true;
    error = '';
    try {
      if (editKind === 'default') await api.del(`/admin/calendar/days/${editDate}`);
      else await api.put(`/admin/calendar/days/${editDate}`, { kind: editKind, title: editTitle });
      editOpen = false;
      changed();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  async function resetDay() {
    busy = true;
    try {
      await api.del(`/admin/calendar/days/${editDate}`);
      editOpen = false;
      changed();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  async function fillHolidays() {
    error = '';
    try {
      const r = await api.post<{ added: number }>(`/admin/calendar/${year}/holidays`);
      notice = r.added
        ? `Добавлены праздники по Трудовому кодексу (${r.added} дн.). Переносы январских выходных Правительство утверждает отдельно — добавьте их, когда выйдет постановление.`
        : 'Праздники этого года уже отмечены.';
      changed();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  function changed() {
    workCalendar.invalidate();
    tick++;
  }

  const editLabel = $derived(
    editDate ? new Date(toDay(editDate) * 86_400_000).toLocaleDateString('ru-RU', { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric', timeZone: 'UTC' }) : ''
  );
  const customCount = $derived((data?.days ?? []).filter((d) => d.custom).length);
</script>

<div class="card intro">
  <CalendarDays size={20} strokeWidth={1.75} />
  <p class="muted">
    Производственный календарь РФ: выходные, праздники и переносы по постановлениям Правительства. В проектах со сроками
    «в рабочих днях» длительность задач и задержки связей считаются по нему, а автосдвиг не ставит задачи на нерабочие дни.
    Нажмите на день, чтобы изменить его для вашего портала — например, объявить корпоративный выходной.
  </p>
</div>

{#if error}<div class="error-box">{error}</div>{/if}
{#if notice}<div class="info-box" role="status">{notice}</div>{/if}

<div class="bar">
  <div class="year">
    <button class="btn sm ybtn" onclick={() => { year--; notice = ''; }} aria-label="Предыдущий год"><ChevronLeft size={16} /></button>
    <b>{year}</b>
    <button class="btn sm ybtn" onclick={() => { year++; notice = ''; }} aria-label="Следующий год"><ChevronRight size={16} /></button>
  </div>
  {#if data}
    {#if data.official}
      <span class="badge ok">Официальный календарь</span>
    {:else}
      <span class="badge warn">Официальных данных за {year} год нет</span>
      <button class="btn sm" onclick={fillHolidays}>Отметить праздники по ТК РФ</button>
    {/if}
    {#if customCount}<span class="faint small">изменено вручную: {customCount} дн.</span>{/if}
  {/if}
</div>

{#if data}
  <div class="kpis">
    <div class="card kpi"><span>Рабочих дней</span><b>{data.work_days}</b></div>
    <div class="card kpi"><span>Выходных и праздничных</span><b>{data.rest_days}</b></div>
    <div class="card kpi"><span>Сокращённых дней</span><b>{data.short_days}</b></div>
    <div class="card kpi"><span>Норма часов (40-часовая неделя)</span><b>{data.hours_40}</b></div>
  </div>

  <div class="legend small">
    <span><i class="sw work"></i>Рабочий</span>
    <span><i class="sw rest"></i>Выходной</span>
    <span><i class="sw holiday"></i>Праздник</span>
    <span><i class="sw off"></i>Перенесённый выходной</span>
    <span><i class="sw work we-work"></i>Рабочий выходной</span>
    <span><i class="sw work short"></i>Сокращённый (на час короче)</span>
    <span><i class="dot"></i>Изменено на портале</span>
  </div>

  <div class="months">
    {#each MONTHS as name, m (m)}
      <section class="card month">
        <header><b>{name}</b><span class="faint small">{data.months[m]} раб. дн.</span></header>
        <div class="grid">
          {#each WD as w, i (w)}<span class="wd" class:we={i > 4}>{w}</span>{/each}
          {#each month(m) as c, i (i)}
            {#if c}
              <button class="d {c.cls}" title={c.title} onclick={() => openDay(c)}>{c.day}{#if c.custom}<i class="dot"></i>{/if}</button>
            {:else}
              <span></span>
            {/if}
          {/each}
        </div>
      </section>
    {/each}
  </div>
{/if}

<Modal bind:open={editOpen} title="День календаря" width={440}>
  <p class="when">{editLabel}</p>
  {#if editInfo?.custom}
    <p class="faint small">Изменён на портале. {editInfo.official_kind ? `По официальному календарю: ${KIND_LABEL[editInfo.official_kind].toLowerCase()}.` : `По официальному календарю — обычный ${editWeekend ? 'выходной' : 'рабочий день'}.`}</p>
  {/if}
  <div class="kinds" role="radiogroup" aria-label="Тип дня">
    <label class="opt"><input type="radio" bind:group={editKind} value="default" />Обычный {editWeekend ? 'выходной' : 'рабочий день'}</label>
    <label class="opt"><input type="radio" bind:group={editKind} value={editWeekend ? 'work' : 'off'} />{editWeekend ? 'Рабочий день (рабочая суббота)' : 'Выходной (перенос, корпоративный)'}</label>
    <label class="opt"><input type="radio" bind:group={editKind} value="holiday" />Праздник</label>
    <label class="opt"><input type="radio" bind:group={editKind} value="short" />Сокращённый рабочий день</label>
  </div>
  {#if editKind !== 'default'}
    <label class="field">Пояснение <input class="input" bind:value={editTitle} maxlength="200" placeholder="Например: перенос с 3 января" /></label>
  {/if}
  {#snippet footer()}
    {#if editInfo?.custom}<button class="btn ghost" onclick={resetDay} disabled={busy}>Как в официальном</button><span class="spacer"></span>{/if}
    <button class="btn" onclick={() => (editOpen = false)}>Отмена</button>
    <button class="btn primary" onclick={saveDay} disabled={busy}>Сохранить</button>
  {/snippet}
</Modal>

<style>
  .intro { display: flex; gap: 12px; align-items: flex-start; padding: 14px 16px; margin-bottom: 14px; color: var(--accent); }
  .intro p { margin: 0; }
  .bar { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; margin-bottom: 14px; }
  .year { display: inline-flex; align-items: center; gap: 6px; }
  .ybtn { width: 30px; padding: 0; justify-content: center; }
  .info-box { background: var(--accent-soft); color: var(--accent); padding: 9px 12px; border-radius: var(--radius); font-size: 13px; margin-bottom: 12px; }
  .year b { font-size: 18px; min-width: 52px; text-align: center; font-variant-numeric: tabular-nums; }
  .badge.ok { background: var(--ok-soft); color: var(--ok); }
  .badge.warn { background: var(--warn-soft); color: var(--warn); }
  .kpis { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 10px; margin-bottom: 12px; }
  .kpi { padding: 11px 14px; display: grid; gap: 2px; }
  .kpi span { font-size: 12.5px; color: var(--text-3); }
  .kpi b { font-size: 20px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .legend { display: flex; flex-wrap: wrap; gap: 6px 16px; color: var(--text-2); margin-bottom: 12px; }
  .legend span { display: inline-flex; align-items: center; gap: 6px; }
  .sw { width: 14px; height: 14px; border-radius: 4px; display: inline-block; border: 1px solid var(--border); }
  .months { display: grid; grid-template-columns: repeat(auto-fill, minmax(232px, 1fr)); gap: 12px; }
  .month { padding: 12px; }
  .month header { display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 8px; }
  .grid { display: grid; grid-template-columns: repeat(7, 1fr); gap: 3px; }
  .wd { font-size: 11px; color: var(--text-3); text-align: center; padding-bottom: 2px; }
  .wd.we { color: var(--danger); }
  .d {
    position: relative; height: 28px; border: 1px solid transparent; border-radius: 6px; background: none;
    font-size: 12.5px; font-variant-numeric: tabular-nums; color: var(--text); cursor: pointer; padding: 0;
  }
  .d:hover { border-color: var(--border-strong); }
  .d:focus-visible { outline: none; box-shadow: var(--ring); }
  .work { background: var(--surface); }
  .rest { background: var(--surface-3); color: var(--text-3); }
  .holiday { background: var(--danger-soft); color: var(--danger); font-weight: 600; }
  .off { background: color-mix(in srgb, var(--danger-soft) 55%, var(--surface-3)); color: var(--danger); }
  .we-work { border-color: var(--accent); color: var(--accent); font-weight: 600; }
  .short { text-decoration: underline dotted; text-underline-offset: 3px; }
  .sw.short { background: linear-gradient(to top, var(--text-3) 0 2px, var(--surface) 2px); }
  .dot { position: absolute; top: 3px; right: 3px; width: 5px; height: 5px; border-radius: 50%; background: var(--warn); }
  .legend .dot { position: static; display: inline-block; width: 7px; height: 7px; }
  .when { margin: 0 0 6px; font-weight: 600; }
  .when::first-letter { text-transform: uppercase; }
  .kinds { display: grid; gap: 6px; margin: 6px 0 4px; }
  .opt { display: flex; gap: 8px; align-items: center; cursor: pointer; }
  .spacer { flex: 1; }
</style>
