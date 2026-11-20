<script lang="ts">
  // Month calendar of tasks by due date (tasks with only a start date are
  // shown on their start). Weeks start on Monday.
  import { untrack } from 'svelte';
  import type { Task } from '#lib/api.ts';
  import { isOverdue } from './dates';
  import { labels } from './labels.svelte';
  import ChevronLeft from '@lucide/svelte/icons/chevron-left';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';

  let { tasks, today, onopen, onundated }: {
    tasks: Task[];
    today: string;
    onopen: (t: Task) => void;
    /** Show the tasks without dates (switches to the list). */
    onundated?: () => void;
  } = $props();

  const WEEKDAYS = ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс'];
  const pad = (n: number) => String(n).padStart(2, '0');
  const iso = (d: Date) => `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}`;

  // Visible month as [year, month0]; starts at the current month.
  const monthOf = (d: string): [number, number] => [Number(d.slice(0, 4)), Number(d.slice(5, 7)) - 1];
  let ym = $state<[number, number]>(untrack(() => monthOf(today)));

  const weeks = $derived.by(() => {
    const first = new Date(Date.UTC(ym[0], ym[1], 1));
    const shift = (first.getUTCDay() + 6) % 7; // Monday = 0
    const start = new Date(first.getTime() - shift * 86_400_000);
    const out: { date: string; inMonth: boolean; weekend: boolean }[][] = [];
    for (let w = 0; w < 6; w++) {
      const row = [];
      for (let d = 0; d < 7; d++) {
        const dt = new Date(start.getTime() + (w * 7 + d) * 86_400_000);
        row.push({ date: iso(dt), inMonth: dt.getUTCMonth() === ym[1], weekend: d >= 5 });
      }
      // Drop a trailing week that lies entirely in the next month.
      if (w >= 4 && !row.some((c) => c.inMonth)) break;
      out.push(row);
    }
    return out;
  });

  const byDay = $derived.by(() => {
    const m = new Map<string, Task[]>();
    for (const t of tasks) {
      const d = t.due_date ?? t.start_date;
      if (!d) continue;
      const list = m.get(d) ?? [];
      list.push(t);
      m.set(d, list);
    }
    for (const list of m.values()) list.sort((a, b) => Number(a.status === 'done') - Number(b.status === 'done') || b.priority - a.priority);
    return m;
  });
  const undated = $derived(tasks.filter((t) => !t.due_date && !t.start_date).length);

  const monthTitle = $derived(
    new Date(Date.UTC(ym[0], ym[1], 1)).toLocaleDateString('ru-RU', { month: 'long', year: 'numeric', timeZone: 'UTC' })
  );
  function step(n: number) {
    const m = ym[1] + n;
    ym = [ym[0] + Math.floor(m / 12), ((m % 12) + 12) % 12];
  }
  function goToday() {
    ym = monthOf(today);
  }
  const dotColor = (t: Task) => (t.label_ids.length ? labels.byId(t.label_ids[0])?.color : undefined);
</script>

<div class="cal-head">
  <button class="btn sm" onclick={() => step(-1)} aria-label="Предыдущий месяц"><ChevronLeft size={16} strokeWidth={2} /></button>
  <h2 class="title">{monthTitle}</h2>
  <button class="btn sm" onclick={() => step(1)} aria-label="Следующий месяц"><ChevronRight size={16} strokeWidth={2} /></button>
  <button class="btn sm" onclick={goToday}>Сегодня</button>
  <div class="spacer"></div>
  {#if undated}<button class="btn ghost sm" onclick={() => onundated?.()}>Без срока: {undated}</button>{/if}
</div>

<div class="cal card" role="grid" aria-label="Календарь задач, {monthTitle}">
  <div class="wd" role="row">
    {#each WEEKDAYS as w, i}<span role="columnheader" class:we={i >= 5}>{w}</span>{/each}
  </div>
  {#each weeks as week, wi (wi)}
    <div class="week" role="row">
      {#each week as c (c.date)}
        {@const list = byDay.get(c.date) ?? []}
        <div class="day" class:out={!c.inMonth} class:we={c.weekend} class:today={c.date === today} role="gridcell" aria-label={c.date}>
          <span class="dnum">{Number(c.date.slice(8))}</span>
          <div class="items">
            {#each list as t (t.id)}
              <button class="item p{t.priority}" class:done={t.status === 'done'} class:late={isOverdue(t, today)} onclick={() => onopen(t)}
                      title="#{t.number} {t.title} · {t.project_name}{t.assignee_name ? ` · ${t.assignee_name}` : ''}">
                {#if dotColor(t)}<span class="ldot" style="background:{dotColor(t)}"></span>{/if}
                <span class="num">#{t.number}</span><span class="tt">{t.title}</span>
              </button>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {/each}
</div>

<style>
  .cal-head { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin-bottom: 10px; }
  .title { margin: 0 6px; font-size: 16px; text-transform: capitalize; min-width: 150px; text-align: center; }
  .cal { padding: 0; overflow: hidden; }
  .wd, .week { display: grid; grid-template-columns: repeat(7, minmax(0, 1fr)); }
  .wd span { padding: 6px 8px; font-size: 11.5px; text-transform: uppercase; letter-spacing: .04em; color: var(--text-3); border-bottom: 1px solid var(--border); }
  .wd span.we { color: var(--danger); opacity: .75; }
  .day { min-height: 112px; border-right: 1px solid var(--border); border-bottom: 1px solid var(--border); padding: 4px; display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .day:nth-child(7n) { border-right: 0; }
  .day.we { background: color-mix(in srgb, var(--surface-2) 55%, transparent); }
  .day.out { background: var(--surface-2); }
  .day.out .dnum { color: var(--text-3); opacity: .6; }
  .dnum { font-size: 12px; color: var(--text-2); padding: 0 4px; align-self: flex-end; font-variant-numeric: tabular-nums; }
  .day.today .dnum { background: var(--accent); color: var(--on-accent); border-radius: 999px; padding: 0 7px; font-weight: 600; }
  .items { display: grid; gap: 2px; max-height: 150px; overflow-y: auto; }
  .item {
    display: flex; align-items: center; gap: 4px; width: 100%; min-width: 0; border: 0; border-left: 3px solid var(--accent);
    background: color-mix(in srgb, var(--accent) 9%, var(--surface)); color: var(--text); border-radius: 4px;
    padding: 2px 5px; font: inherit; font-size: 12px; text-align: left; cursor: pointer;
  }
  .item:hover { background: color-mix(in srgb, var(--accent) 16%, var(--surface)); }
  .item.p3 { border-left-color: var(--warn); }
  .item.p4 { border-left-color: var(--danger); }
  .item.late { background: var(--danger-soft); color: var(--danger); }
  .item.done { opacity: .55; text-decoration: line-through; border-left-color: var(--ok); }
  .num { color: var(--text-3); flex: none; font-variant-numeric: tabular-nums; }
  .item.late .num { color: inherit; }
  .tt { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ldot { width: 7px; height: 7px; border-radius: 50%; flex: none; }
  @media (max-width: 760px) {
    .day { min-height: 70px; padding: 2px; }
    .num { display: none; }
    .item { font-size: 11px; padding: 1px 3px; }
    .title { min-width: 0; font-size: 15px; }
  }
</style>
