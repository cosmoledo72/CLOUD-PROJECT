<script lang="ts">
  // Labels of a project: chips, and for project managers a menu to assign
  // labels and add a new one to the portal catalogue on the fly.
  import { api, type Project } from '#lib/api.ts';
  import LabelChip from '#lib/tasks/LabelChip.svelte';
  import { LABEL_COLORS } from '#lib/tasks/labels.svelte.ts';
  import Tag from '@lucide/svelte/icons/tag';
  import { projectLabels } from './projectLabels.svelte';

  let { project, canEdit = false, onchange }: {
    project: Pick<Project, 'id' | 'label_ids'>;
    canEdit?: boolean;
    onchange?: (ids: string[]) => void;
  } = $props();

  let menu = $state(false);
  let newName = $state('');
  let newColor = $state(LABEL_COLORS[4]);
  let error = $state('');
  let root = $state<HTMLDivElement>();

  $effect(() => {
    projectLabels.load().catch(() => {});
  });
  const mine = $derived(projectLabels.of(project.label_ids));

  async function save(ids: string[]) {
    error = '';
    try {
      const r = await api.put<{ label_ids: string[] }>(`/projects/${project.id}/labels`, { label_ids: ids });
      onchange?.(r.label_ids);
      projectLabels.load(true).catch(() => {});
    } catch (e) {
      error = (e as Error).message;
    }
  }
  function toggle(id: string) {
    const has = project.label_ids.includes(id);
    save(has ? project.label_ids.filter((x) => x !== id) : [...project.label_ids, id]);
  }
  async function add() {
    const name = newName.trim();
    if (!name) return;
    error = '';
    try {
      const l = await projectLabels.create(name, newColor);
      newName = '';
      await save([...project.label_ids, l.id]);
    } catch (e) {
      error = (e as Error).message;
    }
  }
  function outside(e: MouseEvent) {
    if (menu && root && !root.contains(e.target as Node)) menu = false;
  }
</script>

<svelte:window onclick={outside} onkeydown={(e) => e.key === 'Escape' && (menu = false)} />

{#if mine.length || canEdit}
  <div class="pl" bind:this={root}>
    {#each mine as l (l.id)}
      <LabelChip label={l} onremove={canEdit ? () => toggle(l.id) : undefined} />
    {/each}
    {#if canEdit}
      <button class="btn ghost sm" onclick={() => (menu = !menu)} aria-expanded={menu}><Tag size={14} strokeWidth={2} />Метка</button>
      {#if menu}
        <div class="menu" role="group" aria-label="Метки проекта">
          <div class="opts">
            {#each projectLabels.list as l (l.id)}
              <label class="opt"><input type="checkbox" checked={project.label_ids.includes(l.id)} onchange={() => toggle(l.id)} /><LabelChip label={l} /></label>
            {:else}
              <p class="faint small none">Меток пока нет — создайте первую, например «Ожидаем оплату».</p>
            {/each}
          </div>
          {#if projectLabels.canCreate}
            <div class="new">
              <input class="input sm" placeholder="Новая метка" maxlength="40" bind:value={newName}
                     onkeydown={(e) => e.key === 'Enter' && add()} aria-label="Название новой метки" />
              <div class="swatches" role="radiogroup" aria-label="Цвет метки">
                {#each LABEL_COLORS as c}
                  <button class="sw" class:on={newColor === c} style="background:{c}" onclick={() => (newColor = c)} role="radio" aria-checked={newColor === c} aria-label="Цвет {c}"></button>
                {/each}
              </div>
              <button class="btn sm" disabled={!newName.trim()} onclick={add}>Добавить</button>
            </div>
          {/if}
          {#if error}<div class="error-box">{error}</div>{/if}
        </div>
      {/if}
    {/if}
  </div>
{/if}

<style>
  .pl { position: relative; display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
  .menu {
    position: absolute; z-index: 30; top: calc(100% + 6px); left: 0; width: min(440px, calc(100vw - 48px));
    border: 1px solid var(--border); border-radius: var(--radius-lg, 10px); background: var(--surface); box-shadow: var(--shadow-md);
    padding: 10px; display: grid; gap: 8px;
  }
  .opts { display: flex; flex-wrap: wrap; gap: 4px 8px; max-height: 220px; overflow-y: auto; }
  .opt { display: flex; align-items: center; gap: 6px; padding: 2px 4px; border-radius: 6px; cursor: pointer; }
  .opt:hover { background: var(--surface-2); }
  .new { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; border-top: 1px solid var(--border); padding-top: 8px; }
  .new .input { width: 180px; }
  .swatches { display: flex; gap: 4px; }
  .sw { width: 18px; height: 18px; border-radius: 50%; border: 2px solid transparent; cursor: pointer; padding: 0; }
  .sw.on { border-color: var(--text); }
  .none { margin: 0; }
</style>
