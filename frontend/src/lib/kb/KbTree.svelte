<script lang="ts">
  import type { KbNode } from '$lib/api';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import FileText from '@lucide/svelte/icons/file-text';
  import Plus from '@lucide/svelte/icons/plus';
  import Self from './KbTree.svelte';

  let {
    nodes,
    parent = null,
    current,
    expanded = $bindable(),
    depth = 0,
    onadd
  }: {
    nodes: KbNode[];
    parent?: string | null;
    current: string | null;
    expanded: Record<string, boolean>;
    depth?: number;
    onadd: (parent: string) => void;
  } = $props();

  const items = $derived(nodes.filter((n) => n.parent_id === parent));
  const hasKids = (id: string) => nodes.some((n) => n.parent_id === id);
</script>

{#each items as n (n.id)}
  {@const kids = hasKids(n.id)}
  <div class="item" class:on={n.id === current} style="padding-left: {6 + depth * 14}px">
    {#if kids}
      <button class="tw" class:open={expanded[n.id]} aria-label={expanded[n.id] ? 'Свернуть' : 'Развернуть'}
        onclick={() => (expanded[n.id] = !expanded[n.id])}><ChevronRight size={14} /></button>
    {:else}
      <span class="tw"></span>
    {/if}
    <a href="/kb?p={n.id}" title={n.title}>
      {#if n.icon}<span class="emoji">{n.icon}</span>{:else}<FileText size={15} />{/if}
      <span class="t">{n.title}</span>
    </a>
    <button class="add" title="Добавить вложенную страницу" aria-label="Добавить вложенную страницу" onclick={() => onadd(n.id)}><Plus size={14} /></button>
  </div>
  {#if kids && expanded[n.id]}
    <Self {nodes} parent={n.id} {current} bind:expanded depth={depth + 1} {onadd} />
  {/if}
{/each}

<style>
  .item { display: flex; align-items: center; gap: 2px; border-radius: var(--radius); padding-right: 4px; }
  .item:hover { background: var(--surface-2); }
  .item.on { background: var(--accent-soft); }
  .item.on a { color: var(--accent); font-weight: 500; }
  .tw { width: 20px; height: 22px; flex: none; display: grid; place-items: center; border: 0; background: none; color: var(--text-3); cursor: pointer; padding: 0; }
  .tw :global(svg) { transition: transform .12s; }
  .tw.open :global(svg) { transform: rotate(90deg); }
  a { flex: 1; min-width: 0; display: flex; align-items: center; gap: 7px; padding: 5px 2px; color: var(--text); text-decoration: none; font-size: 13.5px; }
  a :global(svg) { color: var(--text-3); flex: none; }
  .emoji { width: 15px; text-align: center; font-size: 14px; line-height: 1; flex: none; }
  .t { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .add { opacity: 0; border: 0; background: none; color: var(--text-3); cursor: pointer; width: 22px; height: 22px; border-radius: 5px; display: grid; place-items: center; padding: 0; }
  .item:hover .add, .add:focus-visible { opacity: 1; }
  .add:hover { background: var(--border); color: var(--text); }
</style>
