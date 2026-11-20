<script lang="ts">
  // Summary Gantt of the project list: one row per project on a shared time
  // scale (weeks, months or quarters). A bar runs from the first task to the
  // last one with the share of done tasks; milestones are diamonds, the
  // contract deadline (when a «Срок…» requisite differs) is a flag; the today
  // line shows where the bureau stands. Read-only: a click opens the project.
  import { formatDate, type Project } from '#lib/api.ts';
  import { avatarHue, initials } from '#lib/fieldLook.ts';
  import { fromDay, toDay } from '#lib/workCalendar.svelte.ts';
  import ProjectBadge from './ProjectBadge.svelte';

  type Group = { id: string; label: string; items: Project[] };
  let {
    groups,
    grouped = false,
    dueOf,
    urgencyOf,
    personName,
    today
  }: {
    groups: Group[];
    grouped?: boolean;
    dueOf: (p: Project) => string | null;
    urgencyOf: (p: Project) => 'late' | 'soon' | null;
    personName: (id: string) => string;
    today: string;
  } = $props();

  type Scale = 'week' | 'month' | 'quarter';
  const PX: Record<Scale, number> = { week: 18, month: 4.2, quarter: 1.6 };
  function storedScale(): Scale {
    try {
      const v = localStorage.getItem('wh_projects_gantt_scale');
      return v === 'week' || v === 'quarter' ? v : 'month';
    } catch {
      return 'month';
    }
  }
  let scale = $state<Scale>(storedScale());
  function setScale(s: Scale) {
    scale = s;
    try { localStorage.setItem('wh_projects_gantt_scale', s); } catch { /* storage unavailable */ }
  }

  const MONTHS = ['янв', 'фев', 'мар', 'апр', 'май', 'июн', 'июл', 'авг', 'сен', 'окт', 'ноя', 'дек'];
  const MONTHS_FULL = ['Январь', 'Февраль', 'Март', 'Апрель', 'Май', 'Июнь', 'Июль', 'Август', 'Сентябрь', 'Октябрь', 'Ноябрь', 'Декабрь'];
  const ROMAN = ['I', 'II', 'III', 'IV'];
  const t0 = $derived(toDay(today));

  /** Dates of a project's bar: tasks first, the contract deadline as a fallback end. */
  function span(p: Project): { s: number; e: number } | null {
    const due = dueOf(p);
    const ms = p.milestones.map((m) => toDay(m.date));
    const starts = [p.start_date, p.end_date].filter(Boolean).map((d) => toDay(d!));
    const s = starts.length ? Math.min(...starts) : ms.length ? Math.min(...ms) : null;
    const e = p.end_date ? toDay(p.end_date) : due ? toDay(due) : ms.length ? Math.max(...ms) : null;
    if (s == null || e == null) return null;
    return { s: Math.min(s, e), e: Math.max(s, e) };
  }

  const all = $derived(groups.flatMap((g) => g.items));
  /** Visible range: whole months around every bar, milestone, deadline and today. */
  const range = $derived.by(() => {
    let lo = t0 - 30;
    let hi = t0 + 60;
    for (const p of all) {
      const sp = span(p);
      if (sp) { lo = Math.min(lo, sp.s); hi = Math.max(hi, sp.e); }
      const due = dueOf(p);
      if (due) hi = Math.max(hi, toDay(due));
      for (const m of p.milestones) { lo = Math.min(lo, toDay(m.date)); hi = Math.max(hi, toDay(m.date)); }
    }
    const a = new Date(fromDay(lo - 7) + 'T00:00:00Z');
    const b = new Date(fromDay(hi + 21) + 'T00:00:00Z');
    const startMonth = scale === 'quarter' ? Math.floor(a.getUTCMonth() / 3) * 3 : a.getUTCMonth();
    const from = Date.UTC(a.getUTCFullYear(), startMonth, 1) / 86_400_000;
    const endMonth = scale === 'quarter' ? Math.floor(b.getUTCMonth() / 3) * 3 + 3 : b.getUTCMonth() + 1;
    const to = Date.UTC(b.getUTCFullYear(), endMonth, 1) / 86_400_000;
    return { from, to };
  });
  const px = $derived(PX[scale]);
  const width = $derived((range.to - range.from) * px);
  const x = (day: number) => (day - range.from) * px;

  type Cell = { left: number; width: number; text: string; title?: string };
  /** Header: an upper band (years / months) and a lower one (months / weeks / quarters). */
  const header = $derived.by(() => {
    const upper: Cell[] = [];
    const lower: Cell[] = [];
    const months: { s: number; e: number; y: number; m: number }[] = [];
    let d = new Date(fromDay(range.from) + 'T00:00:00Z');
    while (Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), 1) / 86_400_000 < range.to) {
      const y = d.getUTCFullYear();
      const m = d.getUTCMonth();
      const s = Date.UTC(y, m, 1) / 86_400_000;
      const e = Date.UTC(y, m + 1, 1) / 86_400_000;
      months.push({ s, e, y, m });
      d = new Date(Date.UTC(y, m + 1, 1));
    }
    const cell = (s: number, e: number, text: string, title?: string): Cell => ({ left: x(s), width: (e - s) * px, text, title });
    if (scale === 'week') {
      for (const mo of months) upper.push(cell(mo.s, mo.e, `${MONTHS_FULL[mo.m]} ${mo.y}`));
      // Weeks from the Monday on or before the range start.
      let w = range.from - ((new Date(fromDay(range.from) + 'T00:00:00Z').getUTCDay() + 6) % 7);
      for (; w < range.to; w += 7) {
        const s = Math.max(w, range.from);
        lower.push(cell(s, Math.min(w + 7, range.to), fromDay(w).slice(8, 10), `Неделя с ${formatDate(fromDay(w))}`));
      }
    } else if (scale === 'month') {
      const years = new Map<number, { s: number; e: number }>();
      for (const mo of months) {
        const y = years.get(mo.y);
        years.set(mo.y, { s: y ? y.s : mo.s, e: mo.e });
        lower.push(cell(mo.s, mo.e, MONTHS[mo.m]));
      }
      for (const [y, v] of years) upper.push(cell(v.s, v.e, String(y)));
    } else {
      const years = new Map<number, { s: number; e: number }>();
      for (const mo of months) {
        const y = years.get(mo.y);
        years.set(mo.y, { s: y ? y.s : mo.s, e: mo.e });
        if (mo.m % 3 === 0) {
          const e = Date.UTC(mo.y, mo.m + 3, 1) / 86_400_000;
          lower.push(cell(mo.s, Math.min(e, range.to), `${ROMAN[mo.m / 3]} кв.`));
        }
      }
      for (const [y, v] of years) upper.push(cell(v.s, v.e, String(y)));
    }
    return { upper, lower };
  });

  const pct = (p: Project) => (p.task_total ? Math.round((p.task_done / p.task_total) * 100) : 0);
  const tone = (p: Project) => (p.status === 'closed' ? 'closed' : p.status === 'paused' ? 'paused' : urgencyOf(p) === 'late' ? 'late' : 'ok');

  // Left column: narrower on phones.
  let narrow = $state(false);
  $effect(() => {
    const mq = matchMedia('(max-width: 720px)');
    narrow = mq.matches;
    const on = () => (narrow = mq.matches);
    mq.addEventListener('change', on);
    return () => mq.removeEventListener('change', on);
  });
  const LEFT = $derived(narrow ? 170 : 300);

  // Keep today in view: on mount and when the scale changes.
  let scroller = $state<HTMLDivElement | null>(null);
  $effect(() => {
    void scale;
    void range.from;
    if (!scroller) return;
    const el = scroller;
    queueMicrotask(() => { el.scrollLeft = Math.max(0, x(t0) - (el.clientWidth - LEFT) / 3); });
  });
</script>

<div class="bar-head">
  <div class="seg" role="radiogroup" aria-label="Масштаб">
    {#each [['week', 'Недели'], ['month', 'Месяцы'], ['quarter', 'Кварталы']] as [k, l] (k)}
      <button role="radio" aria-checked={scale === k} class:on={scale === k} onclick={() => setScale(k as Scale)}>{l}</button>
    {/each}
  </div>
  <span class="legend">
    <span><i class="lg lg-bar"></i>сроки задач</span>
    <span><i class="lg lg-dia"></i>веха</span>
    <span><i class="lg lg-flag"></i>срок по договору</span>
    <span><i class="lg lg-flag over"></i>график выходит за срок</span>
    <span><i class="lg lg-now"></i>сегодня</span>
  </span>
</div>

<div class="card sg" bind:this={scroller} style="--left:{LEFT}px">
  <div class="inner" style="width:{LEFT + width}px">
    <div class="head">
      <div class="lc hc">Проект</div>
      <div class="tl" style="width:{width}px">
        <div class="band up">{#each header.upper as c, i (i)}<span style="left:{c.left}px;width:{c.width}px">{c.text}</span>{/each}</div>
        <div class="band lo">{#each header.lower as c, i (i)}<span style="left:{c.left}px;width:{c.width}px" title={c.title}>{c.text}</span>{/each}</div>
      </div>
    </div>
    <div class="body">
      <div class="grid-lines" style="left:{LEFT}px;width:{width}px">
        {#each header.lower as c, i (i)}<i style="left:{c.left}px"></i>{/each}
      </div>
      {#if t0 >= range.from && t0 < range.to}
        <div class="today" style="left:{LEFT + x(t0) + px / 2}px" title="Сегодня, {formatDate(today)}"></div>
      {/if}
      {#each groups as g (g.id)}
        {#if grouped}
          <div class="grow"><div class="lc gl">{g.label} <span class="n">{g.items.length}</span></div></div>
        {/if}
        {#each g.items as p (p.id)}
          {@const sp = span(p)}
          {@const due = dueOf(p)}
          {@const flag = due && due !== p.end_date ? toDay(due) : null}
          <a class="row" href="/projects/{p.id}">
            <div class="lc">
              <ProjectBadge id={p.id} name={p.name} color={p.badge_color} icon={p.badge_icon} size={22} />
              <span class="nm" title={p.name}>{p.name}</span>
              <span class="mgrs">
                {#each p.manager_ids.slice(0, 2) as m (m)}
                  <span class="av" style="--h:{avatarHue(m)}" title="Руководитель: {personName(m)}">{initials(personName(m))}</span>
                {/each}
              </span>
            </div>
            <div class="tl" style="width:{width}px">
              {#if sp}
                {@const w = Math.max(4, (sp.e - sp.s + 1) * px)}
                <span class="bar tone-{tone(p)}" style="left:{x(sp.s)}px;width:{w}px"
                      title="{p.name}: {formatDate(fromDay(sp.s))} — {formatDate(fromDay(sp.e))}, выполнено {pct(p)}%">
                  <i style="width:{pct(p)}%"></i>
                </span>
                <span class="after" style="left:{x(sp.e) + Math.max(px, 4) + 6}px">{pct(p)}% · {formatDate(fromDay(sp.e))}</span>
              {:else}
                <span class="none" style="left:{Math.max(0, x(t0) + 8)}px">нет сроков задач</span>
              {/if}
              {#each p.milestones as m (m.date + m.title)}
                <span class="dia" class:done={m.done} style="left:{x(toDay(m.date)) + px / 2}px" title="Веха «{m.title}», {formatDate(m.date)}{m.done ? ' — выполнена' : ''}"></span>
              {/each}
              {#if flag != null}
                {@const over = urgencyOf(p) === 'late' || (!!sp && sp.e > flag && p.status !== 'closed')}
                <span class="flag" class:late={over} style="left:{x(flag) + px / 2}px"
                      title="Срок по договору: {formatDate(due)}{over ? (urgencyOf(p) === 'late' ? ' — просрочен' : ' — график задач выходит за срок') : ''}"></span>
              {/if}
            </div>
          </a>
        {/each}
      {/each}
    </div>
  </div>
</div>

<style>
  .bar-head { display: flex; align-items: center; gap: 16px; flex-wrap: wrap; margin-bottom: 10px; }
  .legend { display: flex; gap: 14px; flex-wrap: wrap; font-size: 12px; color: var(--text-3); }
  .legend > span { display: inline-flex; align-items: center; gap: 6px; }
  .lg { display: inline-block; flex: none; }
  .lg-bar { width: 22px; height: 8px; border-radius: 4px; background: color-mix(in srgb, var(--accent) 35%, transparent); }
  .lg-dia { width: 8px; height: 8px; transform: rotate(45deg); border: 1.5px solid var(--text-2); background: var(--surface); }
  .lg-flag { width: 2px; height: 12px; background: var(--text-2); }
  .lg-flag.over { background: var(--danger); }
  .lg-now { width: 2px; height: 12px; background: var(--accent); }

  .sg { padding: 0; overflow: auto; max-height: calc(100vh - 290px); position: relative; }
  .inner { position: relative; }
  .head { display: flex; position: sticky; top: 0; z-index: 3; background: var(--surface-2); border-bottom: 1px solid var(--border); }
  .lc { position: sticky; left: 0; z-index: 2; width: var(--left); flex: none; display: flex; align-items: center; gap: 8px;
        padding: 0 12px; background: var(--surface); border-right: 1px solid var(--border); min-width: 0; }
  .hc { background: var(--surface-2); font-size: 12.5px; color: var(--text-3); font-weight: 500; z-index: 4; }
  .tl { position: relative; flex: none; }
  .band { position: relative; height: 24px; }
  .band span { position: absolute; top: 0; height: 100%; display: flex; align-items: center; padding-left: 6px; font-size: 12px; color: var(--text-2);
               border-left: 1px solid var(--border); white-space: nowrap; overflow: hidden; box-sizing: border-box; }
  .band.up span { font-weight: 600; color: var(--text); }
  .band.lo { border-top: 1px solid var(--border); }
  .band.lo span { color: var(--text-3); font-size: 11.5px; }

  .body { position: relative; }
  .grid-lines { position: absolute; top: 0; bottom: 0; pointer-events: none; }
  .grid-lines i { position: absolute; top: 0; bottom: 0; width: 1px; background: color-mix(in srgb, var(--border) 60%, transparent); }
  .today { position: absolute; top: 0; bottom: 0; width: 2px; margin-left: -1px; background: var(--accent); opacity: .7; z-index: 1; pointer-events: none; }
  .grow { display: flex; align-items: stretch; height: 32px; border-bottom: 1px solid var(--border); background: var(--surface-2); }
  .grow .lc { background: var(--surface-2); font-weight: 600; font-size: 13px; color: var(--text-2); }
  .grow .n { font-weight: 400; color: var(--text-3); font-size: 12px; }
  .row { display: flex; align-items: stretch; gap: 0; height: 40px; border-bottom: 1px solid var(--border); color: var(--text); }
  .row:hover { text-decoration: none; background: color-mix(in srgb, var(--accent) 5%, transparent); }
  .row:hover .lc { background: var(--surface-2); }
  .nm { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13.5px; font-weight: 500; }
  .mgrs { display: flex; flex: none; }
  .mgrs .av + .av { margin-left: -6px; }
  .av { width: 22px; height: 22px; border-radius: 50%; display: inline-grid; place-items: center; font-size: 9.5px; font-weight: 600; color: #fff;
        background: hsl(var(--h) 42% 42%); border: 2px solid var(--surface); box-sizing: content-box; }

  .bar { position: absolute; top: 12px; height: 16px; border-radius: 5px; overflow: hidden;
         background: color-mix(in srgb, var(--accent) 28%, var(--surface)); border: 1px solid color-mix(in srgb, var(--accent) 55%, transparent); box-sizing: border-box; }
  .bar i { display: block; height: 100%; background: var(--accent); opacity: .85; }
  .bar.tone-late { background: color-mix(in srgb, var(--danger) 22%, var(--surface)); border-color: color-mix(in srgb, var(--danger) 60%, transparent); }
  .bar.tone-late i { background: var(--danger); }
  .bar.tone-paused { background: color-mix(in srgb, var(--warn) 22%, var(--surface)); border-color: color-mix(in srgb, var(--warn) 55%, transparent); }
  .bar.tone-paused i { background: var(--warn); }
  .bar.tone-closed { background: var(--surface-2); border-color: var(--border-strong); }
  .bar.tone-closed i { background: var(--text-3); opacity: .5; }
  .after { position: absolute; top: 12px; line-height: 16px; font-size: 11.5px; color: var(--text-3); white-space: nowrap; font-variant-numeric: tabular-nums; }
  .none { position: absolute; top: 12px; line-height: 16px; font-size: 12px; color: var(--text-3); font-style: italic; white-space: nowrap; }
  .dia { position: absolute; top: 14px; width: 10px; height: 10px; margin-left: -5px; transform: rotate(45deg); background: var(--surface);
         border: 2px solid var(--text-2); box-sizing: border-box; z-index: 1; }
  .dia.done { background: var(--ok); border-color: var(--ok); }
  .flag { position: absolute; top: 6px; height: 28px; width: 2px; margin-left: -1px; background: var(--text-2); z-index: 1; }
  .flag::after { content: ''; position: absolute; top: 0; left: 2px; border: 5px solid transparent; border-left: 7px solid var(--text-2); border-right: 0; }
  .flag.late { background: var(--danger); }
  .flag.late::after { border-left-color: var(--danger); }
  .seg button { padding: 5px 12px; }

  @media (max-width: 720px) {
    .sg { max-height: none; }
    .mgrs { display: none; }
  }
</style>
