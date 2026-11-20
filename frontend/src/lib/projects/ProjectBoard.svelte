<script lang="ts" module>
  import type { Project } from '#lib/api.ts';
  export interface BoardColumn {
    id: string;
    title: string;
    /** A person column (managers board): avatar from the id. */
    personId?: string;
    /** A label column: the label colour. */
    color?: string;
    items: Project[];
  }
</script>

<script lang="ts">
  // Read-only board of the project list: one column per manager (ГИП) or per
  // project label. A project with several managers or labels is shown in each
  // of its columns. Cards are compact: badge, name, labels, deadline, progress.
  import { formatDate } from '#lib/api.ts';
  import { avatarHue, initials, moneyShort } from '#lib/fieldLook.ts';
  import ProjectBadge from './ProjectBadge.svelte';
  import { projectLabels } from './projectLabels.svelte';
  import LabelChip from '#lib/tasks/LabelChip.svelte';


  let {
    columns,
    dueOf,
    urgencyOf,
    amountOf,
    showSum = false,
    hideLabelId = null
  }: {
    columns: BoardColumn[];
    dueOf: (p: Project) => string | null;
    urgencyOf: (p: Project) => 'late' | 'soon' | null;
    amountOf: (p: Project) => number;
    /** Show the column total of the contract amount. */
    showSum?: boolean;
    /** On the labels board the column's own label is not repeated on its cards. */
    hideLabelId?: boolean | null;
  } = $props();

  const statusText = { active: 'Активный', paused: 'Приостановлен', closed: 'Закрыт' };
  const pct = (p: Project) => (p.task_total ? Math.round((p.task_done / p.task_total) * 100) : 0);
</script>

<div class="board">
  {#each columns as c (c.id)}
    {@const late = c.items.filter((p) => urgencyOf(p) === 'late').length}
    {@const sum = c.items.filter((p) => p.status === 'active').reduce((s, p) => s + amountOf(p), 0)}
    <section class="col" aria-label={c.title}>
      <header style={c.color ? `--lc:${c.color}` : ''}>
        {#if c.personId}
          <span class="av" style="--h:{avatarHue(c.personId)}">{initials(c.title)}</span>
        {:else if c.color}
          <i class="ldot"></i>
        {/if}
        <span class="ttl" title={c.title}>{c.title}</span>
        <span class="n">{c.items.length}</span>
        <span class="spacer"></span>
        {#if late}<span class="late" title="Просрочено">{late}</span>{/if}
        {#if showSum && sum}<span class="sum" title="В работе по договорам">{moneyShort(sum)}</span>{/if}
      </header>
      <div class="list">
        {#each c.items as p (p.id)}
          {@const u = urgencyOf(p)}
          {@const due = dueOf(p)}
          {@const marks = projectLabels.of(p.label_ids).filter((l) => !(hideLabelId && l.id === c.id))}
          <a class="pc tone-{p.status === 'active' ? (u ?? 'ok') : p.status}" href="/projects/{p.id}">
            <div class="ttl-row">
              <ProjectBadge id={p.id} name={p.name} color={p.badge_color} icon={p.badge_icon} size={24} />
              <span class="nm">{p.name}</span>
            </div>
            {#if marks.length}<div class="marks">{#each marks as l (l.id)}<LabelChip label={l} small />{/each}</div>{/if}
            <div class="meta">
              {#if p.status !== 'active'}<span class="st st-{p.status}">{statusText[p.status]}</span>{/if}
              {#if due}<span class="due" class:late={u === 'late'} class:soon={u === 'soon'}>до {formatDate(due)}</span>{/if}
              <span class="spacer"></span>
              {#if p.task_total}
                <span class="lin" title="Выполнено {pct(p)}%"><i style="width:{pct(p)}%"></i></span>
                <span class="nums">{p.task_done}/{p.task_total}</span>
              {/if}
            </div>
          </a>
        {:else}
          <p class="empty">Нет проектов</p>
        {/each}
      </div>
    </section>
  {/each}
</div>

<style>
  .board { display: grid; grid-auto-flow: column; grid-auto-columns: minmax(280px, 300px); gap: 12px; overflow-x: auto;
           padding-bottom: 8px; align-items: start; }
  .col { background: var(--surface-2); border: 1px solid var(--border); border-radius: 12px; display: flex; flex-direction: column;
         max-height: calc(100vh - 300px); min-height: 120px; }
  header { display: flex; align-items: center; gap: 8px; padding: 10px 12px; font-weight: 600; font-size: 13.5px; border-bottom: 1px solid var(--border); }
  .ttl { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .n { font-weight: 400; color: var(--text-3); font-size: 12px; }
  .spacer { flex: 1; }
  .late { background: var(--danger-soft); color: var(--danger); border-radius: 999px; padding: 0 7px; font-size: 11.5px; font-weight: 600; }
  .sum { font-size: 12px; font-weight: 500; color: var(--text-2); font-variant-numeric: tabular-nums; }
  .av { width: 24px; height: 24px; border-radius: 50%; flex: none; display: inline-grid; place-items: center; font-size: 10px; font-weight: 600;
        color: #fff; background: hsl(var(--h) 42% 42%); }
  .ldot { width: 10px; height: 10px; border-radius: 50%; background: var(--lc); flex: none; }
  .list { padding: 8px; display: grid; gap: 8px; overflow-y: auto; }
  .pc { background: var(--surface); border: 1px solid var(--border); border-left: 3px solid var(--ok); border-radius: 8px; padding: 9px 10px;
        display: grid; gap: 7px; color: var(--text); }
  .pc:hover { text-decoration: none; border-color: var(--accent); }
  .pc.tone-soon { border-left-color: var(--warn); }
  .pc.tone-late { border-left-color: var(--danger); }
  .pc.tone-paused { border-left-color: var(--warn); }
  .pc.tone-closed { border-left-color: var(--border-strong); }
  .pc.tone-closed .nm { color: var(--text-2); }
  .ttl-row { display: flex; gap: 8px; align-items: flex-start; }
  .nm { font-weight: 600; font-size: 13.5px; line-height: 1.3; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  .marks { display: flex; flex-wrap: wrap; gap: 4px; }
  .meta { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-3); }
  .due { white-space: nowrap; }
  .due.late { color: var(--danger); font-weight: 500; }
  .due.soon { color: var(--warn); font-weight: 500; }
  .st { border-radius: 999px; padding: 0 7px; font-weight: 500; }
  .st-paused { background: var(--warn-soft); color: var(--warn); }
  .st-closed { background: var(--surface-2); color: var(--text-3); }
  .lin { width: 54px; height: 5px; border-radius: 3px; background: var(--surface-2); overflow: hidden; flex: none; }
  .lin i { display: block; height: 100%; background: var(--ok); }
  .nums { font-variant-numeric: tabular-nums; }
  .empty { margin: 0; padding: 14px 4px; text-align: center; color: var(--text-3); font-size: 12.5px; }
  @media (max-width: 720px) {
    .board { grid-auto-columns: 82vw; }
    .col { max-height: none; }
  }
</style>
