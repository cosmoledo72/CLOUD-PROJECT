<script lang="ts">
  // SVG Gantt chart: task list on the left, timeline on the right.
  // Bars can be dragged (move) or resized from the right edge; dates snap to
  // whole days. Dependencies are drawn as elbow arrows. Optional overlays:
  // critical path (red outline), float in tooltips and the baseline plan
  // (thin grey bar under each task).
  import { formatDate, localToday, type BaselineEntry, type Dependency, type Task } from './api';

  let { tasks, dependencies, canEdit = false, canEditTask, groupOf, critical = [], slack = {}, baseline = [], onchange, onselect }: {
    tasks: Task[];
    dependencies: Dependency[];
    canEdit?: boolean;
    /** Per-task edit right (overrides `canEdit`), e.g. across several projects. */
    canEditTask?: (t: Task) => boolean;
    /** Group label of a task; a header row is drawn whenever it changes (tasks must be sorted by it). */
    groupOf?: (t: Task) => string;
    critical?: string[];
    slack?: Record<string, number>;
    baseline?: BaselineEntry[];
    onchange?: (id: string, start: string, due: string) => void;
    onselect?: (task: Task) => void;
  } = $props();

  const editable = (t: Task) => (canEditTask ? canEditTask(t) : canEdit);
  type GRow = { kind: 'group'; key: string; label: string } | { kind: 'task'; key: string; t: Task };
  const rows = $derived.by(() => {
    const out: GRow[] = [];
    let last: string | null = null;
    for (const t of tasks) {
      if (groupOf) {
        const g = groupOf(t);
        if (g !== last) out.push({ kind: 'group', key: `g:${g}:${out.length}`, label: g });
        last = g;
      }
      out.push({ kind: 'task', key: t.id, t });
    }
    return out;
  });
  let showCritical = $state(true);
  let showBaseline = $state(true);
  const criticalSet = $derived(new Set(critical));
  const isCritical = (t: Task) => showCritical && criticalSet.has(t.id) && t.status !== 'done';
  const baseMap = $derived(new Map(baseline.map((b) => [b.task_id, b])));

  function baseSpan(id: string): [number, number] | null {
    const b = baseMap.get(id);
    if (!b) return null;
    const s = b.start_date ? toDay(b.start_date) : b.due_date ? toDay(b.due_date) : null;
    const e = b.due_date ? toDay(b.due_date) : s;
    return s === null || e === null ? null : [s, Math.max(s, e)];
  }

  function tooltip(t: Task): string {
    const lines = [t.title, `${formatDate(t.start_date ?? t.due_date)} — ${formatDate(t.due_date ?? t.start_date)}`];
    const f = slack[t.id];
    if (f !== undefined && t.status !== 'done') {
      lines.push(f < 0 ? `Нарушена связь: не хватает ${-f} дн.` : f === 0 ? 'Критическая задача: резерва нет' : `Резерв: ${f} дн.`);
    }
    const b = baseSpan(t.id);
    const cur = span(t);
    if (b && cur && b[1] !== cur[1]) {
      const d = cur[1] - b[1];
      lines.push(`Относительно базового плана: ${d > 0 ? '+' : ''}${d} дн.`);
    }
    return lines.join('\n');
  }

  const ROW = 34;
  const HEAD = 48;
  const DAY_MS = 86_400_000;

  type Zoom = 'day' | 'week' | 'month';
  let zoom = $state<Zoom>('day');
  const dayW = $derived({ day: 30, week: 12, month: 4 }[zoom]);

  // --- Date helpers (UTC day numbers avoid DST issues) --------------------
  const toDay = (s: string) => {
    const [y, m, d] = s.split('-').map(Number);
    return Date.UTC(y, m - 1, d) / DAY_MS;
  };
  const fromDay = (n: number) => new Date(n * DAY_MS).toISOString().slice(0, 10);
  // Today in the user's local calendar (not UTC), so the marker is right east of Greenwich too.
  const today = toDay(localToday());

  // Effective [start, end] of a task in day numbers, or null when undated.
  function span(t: Task): [number, number] | null {
    const s = t.start_date ? toDay(t.start_date) : t.due_date ? toDay(t.due_date) : null;
    const e = t.due_date ? toDay(t.due_date) : s;
    return s === null || e === null ? null : [s, Math.max(s, e)];
  }

  // Live drag override.
  let drag = $state<{ id: string; mode: 'move' | 'resize'; x0: number; s: number; e: number; ds: number } | null>(null);
  const spanOf = (t: Task): [number, number] | null => {
    if (drag && drag.id === t.id) {
      return drag.mode === 'move' ? [drag.s + drag.ds, drag.e + drag.ds] : [drag.s, Math.max(drag.s, drag.e + drag.ds)];
    }
    return span(t);
  };

  const range = $derived.by(() => {
    const spans = tasks.map(span).filter((x): x is [number, number] => x !== null);
    const min = spans.length ? Math.min(...spans.map((s) => s[0]), today) : today;
    const max = spans.length ? Math.max(...spans.map((s) => s[1]), today) : today + 30;
    const pad = zoom === 'month' ? 30 : 7;
    return { start: min - pad, end: max + pad * 2 };
  });
  const days = $derived(range.end - range.start + 1);
  const width = $derived(days * dayW);
  const height = $derived(HEAD + Math.max(rows.length, 1) * ROW);
  const x = (d: number) => (d - range.start) * dayW;

  const index = $derived(new Map(rows.map((r, i) => [r.key, i])));

  // --- Header ticks -------------------------------------------------------
  const months = $derived.by(() => {
    const out: { x: number; w: number; label: string }[] = [];
    let d = range.start;
    while (d <= range.end) {
      const dt = new Date(d * DAY_MS);
      const next = Date.UTC(dt.getUTCFullYear(), dt.getUTCMonth() + 1, 1) / DAY_MS;
      const end = Math.min(next, range.end + 1);
      out.push({
        x: x(d),
        w: (end - d) * dayW,
        label: dt.toLocaleDateString('ru-RU', { month: zoom === 'month' ? 'short' : 'long', year: 'numeric', timeZone: 'UTC' })
      });
      d = next;
    }
    return out;
  });
  const dayTicks = $derived(Array.from({ length: days }, (_, i) => range.start + i));
  const weekday = (d: number) => new Date(d * DAY_MS).getUTCDay();

  // --- Dependencies --------------------------------------------------------
  const arrows = $derived.by(() =>
    dependencies.flatMap((dep) => {
      const a = tasks.find((t) => t.id === dep.depends_on_id);
      const b = tasks.find((t) => t.id === dep.task_id);
      if (!a || !b) return [];
      const sa = spanOf(a), sb = spanOf(b);
      if (!sa || !sb) return [];
      const ia = index.get(a.id)!, ib = index.get(b.id)!;
      const fromEnd = dep.kind === 'FS' || dep.kind === 'FF';
      const toEnd = dep.kind === 'FF' || dep.kind === 'SF';
      const x1 = fromEnd ? x(sa[1] + 1) : x(sa[0]);
      const x2 = toEnd ? x(sb[1] + 1) : x(sb[0]);
      const y1 = HEAD + ia * ROW + ROW / 2;
      const y2 = HEAD + ib * ROW + ROW / 2;
      const mid = Math.max(x1 + 8, Math.min(x2 - 8, x1 + 8));
      const path = x2 - 8 > x1
        ? `M${x1},${y1} H${mid} V${y2} H${x2}`
        : `M${x1},${y1} H${x1 + 8} V${y1 + (y2 > y1 ? ROW / 2 : -ROW / 2)} H${x2 - 10} V${y2} H${x2}`;
      // Violated when the successor starts before the predecessor finishes.
      const violated = dep.kind === 'FS' && sb[0] <= sa[1] + dep.lag_days;
      return [{ key: `${dep.task_id}-${dep.depends_on_id}`, path, violated }];
    })
  );

  // --- Dragging ------------------------------------------------------------
  function startDrag(e: PointerEvent, t: Task, mode: 'move' | 'resize') {
    if (!editable(t) || t.status === 'done') return;
    const sp = span(t);
    if (!sp) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    drag = { id: t.id, mode: t.is_milestone ? 'move' : mode, x0: e.clientX, s: sp[0], e: sp[1], ds: 0 };
  }

  function moveDrag(e: PointerEvent) {
    if (drag) drag.ds = Math.round((e.clientX - drag.x0) / dayW);
  }

  function endDrag() {
    if (!drag) return;
    const d = drag;
    drag = null;
    if (d.ds === 0) {
      const t = tasks.find((t) => t.id === d.id);
      if (t) onselect?.(t);
      return;
    }
    const [s, e] = d.mode === 'move' ? [d.s + d.ds, d.e + d.ds] : [d.s, Math.max(d.s, d.e + d.ds)];
    onchange?.(d.id, fromDay(s), fromDay(e));
  }

  let scroller: HTMLDivElement;
  function scrollToToday() {
    if (scroller) scroller.scrollTo({ left: Math.max(0, x(today) - scroller.clientWidth / 3), behavior: 'smooth' });
  }
  $effect(() => {
    zoom;
    queueMicrotask(scrollToToday);
  });

  const barColor = (t: Task) =>
    t.status === 'done' ? 'var(--ok)' : t.due_date && toDay(t.due_date) < today ? 'var(--danger)' : 'var(--accent)';
</script>

<div class="toolbar row">
  <div class="seg" role="group" aria-label="Масштаб">
    {#each [['day', 'Дни'], ['week', 'Недели'], ['month', 'Месяцы']] as [z, label]}
      <button class="btn sm" class:on={zoom === z} onclick={() => (zoom = z as Zoom)}>{label}</button>
    {/each}
  </div>
  <button class="btn sm" onclick={scrollToToday}>Сегодня</button>
  {#if critical.length || Object.keys(slack).length}<label class="check small"><input type="checkbox" bind:checked={showCritical} />Критический путь</label>{/if}
  {#if baseline.length}<label class="check small"><input type="checkbox" bind:checked={showBaseline} />Базовый план</label>{/if}
  {#if canEdit || canEditTask}<span class="faint hint">Перетащите полосу, чтобы сдвинуть сроки; правый край — изменить длительность.</span>{/if}
</div>

<div class="gantt card">
  <div class="left">
    <div class="lhead" style="height:{HEAD}px">Задача</div>
    {#each rows as r (r.key)}
      {#if r.kind === 'group'}
        <div class="lrow grp" style="height:{ROW}px" title={r.label}><span class="ttl">{r.label}</span></div>
      {:else}
        {@const t = r.t}
        <button class="lrow" class:crit={isCritical(t)} style="height:{ROW}px" onclick={() => onselect?.(t)} title={tooltip(t)}>
          <span class="status s-{t.status}"></span>
          <span class="ttl" class:done={t.status === 'done'}>{t.is_milestone ? '◆ ' : ''}{t.title}</span>
          {#if t.assignee_name}<span class="who faint">{t.assignee_name}</span>{/if}
        </button>
      {/if}
    {/each}
    {#if tasks.length === 0}<div class="lrow faint" style="height:{ROW}px">Задач пока нет</div>{/if}
  </div>

  <div class="right" bind:this={scroller}>
    <svg {width} {height} role="img" aria-label="Диаграмма Ганта" onpointermove={moveDrag} onpointerup={endDrag} onpointercancel={endDrag}>
      <!-- Weekend shading and grid -->
      {#each dayTicks as d}
        {#if dayW >= 12 && (weekday(d) === 0 || weekday(d) === 6)}
          <rect x={x(d)} y={HEAD} width={dayW} height={height - HEAD} class="weekend" />
        {/if}
        {#if zoom === 'day' || (zoom === 'week' && weekday(d) === 1)}
          <line x1={x(d)} x2={x(d)} y1={24} y2={height} class="grid" />
        {/if}
      {/each}
      {#each rows as r, i (r.key)}
        {#if r.kind === 'group'}<rect x="0" y={HEAD + i * ROW} width={width} height={ROW} class="grp-band" />{/if}
        <line x1="0" x2={width} y1={HEAD + (i + 1) * ROW} y2={HEAD + (i + 1) * ROW} class="grid" />
      {/each}

      <!-- Header -->
      {#each months as m}
        <text x={m.x + 6} y="16" class="month">{m.label}</text>
        <line x1={m.x} x2={m.x} y1="0" y2={HEAD} class="grid strong" />
      {/each}
      {#each dayTicks as d}
        {#if zoom === 'day'}
          <text x={x(d) + dayW / 2} y="40" class="day" class:we={weekday(d) === 0 || weekday(d) === 6} text-anchor="middle">{new Date(d * DAY_MS).getUTCDate()}</text>
        {:else if zoom === 'week' && weekday(d) === 1}
          <text x={x(d) + 3} y="40" class="day">{new Date(d * DAY_MS).getUTCDate()}</text>
        {/if}
      {/each}
      <line x1="0" x2={width} y1={HEAD} y2={HEAD} class="grid strong" />

      <!-- Today -->
      <line x1={x(today) + dayW / 2} x2={x(today) + dayW / 2} y1={24} y2={height} class="today" />

      <!-- Dependencies -->
      <defs>
        <marker id="arr" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
          <path d="M0,0 L8,4 L0,8 z" class="arrowhead" />
        </marker>
        <marker id="arr-bad" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
          <path d="M0,0 L8,4 L0,8 z" class="arrowhead bad" />
        </marker>
      </defs>
      {#each arrows as a (a.key)}
        <path d={a.path} class="dep" class:bad={a.violated} marker-end={a.violated ? 'url(#arr-bad)' : 'url(#arr)'} />
      {/each}

      <!-- Baseline (approved plan) -->
      {#if showBaseline}
        {#each tasks as t (t.id)}
          {@const i = index.get(t.id) ?? 0}
          {@const b = baseSpan(t.id)}
          {#if b}
            <rect x={x(b[0])} y={HEAD + i * ROW + ROW - 8} width={Math.max(4, (b[1] - b[0] + 1) * dayW)} height="4" rx="1" class="baseline" />
          {/if}
        {/each}
      {/if}

      <!-- Bars -->
      {#each tasks as t (t.id)}
        {@const i = index.get(t.id) ?? 0}
        {@const sp = spanOf(t)}
        {#if sp}
          {@const y = HEAD + i * ROW}
          {#if t.is_milestone}
            {@const cx = x(sp[0]) + dayW / 2}
            <g class="bar" class:dragging={drag?.id === t.id} class:crit={isCritical(t)} role="button" tabindex="-1" aria-label={t.title}
               onpointerdown={(e) => startDrag(e, t, 'move')}>
              <title>{tooltip(t)}</title>
              <path d="M{cx},{y + 7} L{cx + 9},{y + ROW / 2 - 1} L{cx},{y + ROW - 9} L{cx - 9},{y + ROW / 2 - 1} Z" fill={barColor(t)} class="shape" />
              <text x={cx + 14} y={y + ROW / 2 + 2} class="label">{t.title}</text>
            </g>
          {:else}
            {@const bx = x(sp[0])}
            {@const bw = (sp[1] - sp[0] + 1) * dayW}
            <g class="bar" class:dragging={drag?.id === t.id} class:crit={isCritical(t)} role="button" tabindex="-1" aria-label={t.title}
               onpointerdown={(e) => startDrag(e, t, 'move')}>
              <title>{tooltip(t)}</title>
              <rect x={bx} y={y + 6} width={bw} height={ROW - 16} rx="3" fill={barColor(t)} opacity="0.28" class="shape" />
              <rect x={bx} y={y + 6} width={(bw * t.progress) / 100} height={ROW - 16} rx="3" fill={barColor(t)} />
              {#if editable(t) && t.status !== 'done'}
                <rect x={bx + bw - 6} y={y + 6} width="6" height={ROW - 16} class="handle" role="presentation"
                      onpointerdown={(e) => startDrag(e, t, 'resize')} />
              {/if}
              <text x={bx + bw + 6} y={y + ROW / 2 + 2} class="label">{t.title}{#if showCritical && slack[t.id] > 0 && t.status !== 'done'}<tspan class="float">{` · резерв ${slack[t.id]} дн.`}</tspan>{/if}</text>
            </g>
          {/if}
        {/if}
      {/each}
    </svg>
  </div>
</div>

<style>
  .toolbar { margin-bottom: 8px; flex-wrap: wrap; }
  .hint { font-size: 12px; }
  .gantt { display: grid; grid-template-columns: minmax(180px, 280px) 1fr; overflow: hidden; }
  .left { border-right: 1px solid var(--border); background: var(--surface); }
  .lhead { display: flex; align-items: flex-end; padding: 0 12px 8px; font-size: 12px; color: var(--text-3); text-transform: uppercase; letter-spacing: .04em; border-bottom: 1px solid var(--border-strong); }
  .lrow {
    display: flex; align-items: center; gap: 8px; width: 100%; padding: 0 12px;
    border: 0; border-bottom: 1px solid var(--border); background: none; text-align: left; cursor: pointer;
  }
  .lrow:hover { background: var(--surface-2); }
  .lrow.grp { cursor: default; background: var(--surface-2); font-weight: 600; font-size: 12.5px; color: var(--text-2); }
  .grp-band { fill: var(--surface-2); }
  .ttl { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ttl.done { text-decoration: line-through; color: var(--text-3); }
  .who { font-size: 12px; max-width: 90px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .status { width: 8px; height: 8px; border-radius: 50%; flex: none; background: var(--text-3); }
  .s-in_progress { background: var(--accent); }
  .s-review { background: var(--warn); }
  .s-done { background: var(--ok); }
  .right { overflow-x: auto; overflow-y: hidden; background: var(--surface); }
  svg { display: block; user-select: none; touch-action: pan-y; }
  .grid { stroke: var(--border); stroke-width: 1; }
  .grid.strong { stroke: var(--border-strong); }
  .weekend { fill: var(--surface-2); }
  .month { font-size: 12px; font-weight: 600; fill: var(--text-2); }
  .day { font-size: 11px; fill: var(--text-3); }
  .day.we { fill: var(--danger); opacity: .7; }
  .today { stroke: var(--danger); stroke-width: 1.5; stroke-dasharray: 3 3; }
  .dep { fill: none; stroke: var(--text-3); stroke-width: 1.3; }
  .dep.bad { stroke: var(--danger); }
  .arrowhead { fill: var(--text-3); }
  .arrowhead.bad { fill: var(--danger); }
  .bar { cursor: grab; }
  .bar.dragging { cursor: grabbing; opacity: .85; }
  .handle { fill: transparent; cursor: ew-resize; }
  .bar:hover .handle { fill: rgb(0 0 0 / 18%); }
  .label { font-size: 12px; fill: var(--text-2); pointer-events: none; paint-order: stroke; stroke: var(--surface); stroke-width: 3px; }
  .float { fill: var(--text-3); }
  .bar.crit .shape { stroke: var(--danger); stroke-width: 2; opacity: 1; fill-opacity: .28; }
  .bar.crit path.shape { fill-opacity: 1; }
  .baseline { fill: var(--text-3); opacity: .55; }
  .lrow.crit { box-shadow: inset 3px 0 0 var(--danger); }
  @media (max-width: 720px) {
    .gantt { grid-template-columns: 130px 1fr; }
    .who { display: none; }
    .lrow { padding: 0 8px; font-size: 13px; }
  }
</style>
