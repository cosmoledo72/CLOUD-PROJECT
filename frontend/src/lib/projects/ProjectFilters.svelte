<script lang="ts" module>
  export interface ProjectFilter {
    managers: string[];
    members: string[];
    companies: string[];
  }
  export const EMPTY_FILTER: ProjectFilter = { managers: [], members: [], companies: [] };
  export const filterCount = (f: ProjectFilter) => f.managers.length + f.members.length + f.companies.length;
</script>

<script lang="ts">
  // «Фильтры» popover of the project list: managers (ГИП), members and
  // customers. Within a section any choice matches; sections combine.
  import type { ProjectPerson } from '$lib/api';
  import Filter from '@lucide/svelte/icons/list-filter';

  let {
    value = $bindable(),
    managers,
    members,
    companies
  }: {
    value: ProjectFilter;
    managers: ProjectPerson[];
    members: ProjectPerson[];
    /** Customer names present in the list. */
    companies: string[];
  } = $props();

  let open = $state(false);
  let q = $state('');
  let root: HTMLElement | undefined = $state();
  $effect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => { if (root && !root.contains(e.target as Node)) open = false; };
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') open = false; };
    document.addEventListener('mousedown', onDoc);
    document.addEventListener('keydown', onKey);
    return () => { document.removeEventListener('mousedown', onDoc); document.removeEventListener('keydown', onKey); };
  });

  const n = $derived(filterCount(value));
  const match = (s: string) => !q.trim() || s.toLowerCase().includes(q.trim().toLowerCase());
  function toggle(key: keyof ProjectFilter, id: string) {
    const list = value[key];
    value = { ...value, [key]: list.includes(id) ? list.filter((x) => x !== id) : [...list, id] };
  }
  const sections = $derived([
    { key: 'managers' as const, title: 'Руководитель проекта', items: managers.map((p) => ({ id: p.id, name: p.display_name })) },
    { key: 'members' as const, title: 'Участник', items: members.map((p) => ({ id: p.id, name: p.display_name })) },
    { key: 'companies' as const, title: 'Заказчик', items: companies.map((c) => ({ id: c, name: c })) }
  ].filter((s) => s.items.length));
  const total = $derived(sections.reduce((s, x) => s + x.items.length, 0));
</script>

<div class="wrap" bind:this={root}>
  <button class="btn" class:on={n > 0} onclick={() => (open = !open)} aria-expanded={open} aria-haspopup="dialog">
    <Filter size={16} />Фильтры{#if n}<span class="cnt">{n}</span>{/if}
  </button>
  {#if open}
    <div class="pop" role="dialog" aria-label="Фильтры проектов">
      {#if total > 10}<input class="input" placeholder="Найти человека или заказчика" bind:value={q} />{/if}
      {#each sections as s (s.key)}
        {@const shown = s.items.filter((i) => match(i.name) || value[s.key].includes(i.id))}
        {#if shown.length}
          <div class="sec">
            <div class="st">{s.title}</div>
            {#each shown as i (i.id)}
              <label class="opt"><input type="checkbox" checked={value[s.key].includes(i.id)} onchange={() => toggle(s.key, i.id)} /><span>{i.name}</span></label>
            {/each}
          </div>
        {/if}
      {:else}
        <p class="faint small">Фильтровать пока не по чему: в проектах нет участников и заказчиков.</p>
      {/each}
      {#if n}
        <div class="foot"><button class="btn small" onclick={() => (value = { managers: [], members: [], companies: [] })}>Сбросить</button></div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .wrap { position: relative; }
  .btn { display: inline-flex; align-items: center; gap: 6px; }
  .btn.on { border-color: var(--accent); color: var(--accent); }
  .cnt { background: var(--accent); color: var(--on-accent); border-radius: 999px; font-size: 11px; padding: 0 6px; line-height: 17px; font-weight: 600; }
  .pop { position: absolute; right: 0; top: calc(100% + 6px); z-index: 30; width: 300px; max-height: 70vh; overflow-y: auto; background: var(--surface);
         border: 1px solid var(--border); border-radius: 10px; box-shadow: 0 12px 32px rgba(0, 0, 0, .16); padding: 10px; display: grid; gap: 10px; }
  .sec { display: grid; gap: 1px; }
  .st { font-size: 11px; text-transform: uppercase; letter-spacing: .05em; color: var(--text-3); font-weight: 600; padding: 2px 6px 4px; }
  .opt { display: flex; align-items: center; gap: 8px; padding: 5px 6px; border-radius: 6px; font-size: 13.5px; cursor: pointer; }
  .opt:hover { background: var(--surface-2); }
  .opt span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .foot { display: flex; justify-content: flex-end; border-top: 1px solid var(--border); padding-top: 8px; }
  @media (max-width: 720px) {
    .pop { position: fixed; left: 12px; right: 12px; top: 120px; width: auto; }
  }
</style>
