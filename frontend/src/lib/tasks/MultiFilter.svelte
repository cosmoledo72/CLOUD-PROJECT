<script lang="ts">
  // Filter button with a dropdown of checkboxes. An empty selection means "all".
  import ChevronDown from '@lucide/svelte/icons/chevron-down';

  type Option = { id: string; label: string; color?: string; hint?: string };
  let { label, options, selected = $bindable([]), search = false }: {
    label: string;
    options: Option[];
    selected?: string[];
    /** Show a search box (long lists). */
    search?: boolean;
  } = $props();

  let open = $state(false);
  let q = $state('');
  let root: HTMLDivElement;
  const shown = $derived(q.trim() ? options.filter((o) => o.label.toLowerCase().includes(q.trim().toLowerCase())) : options);
  const summary = $derived.by(() => {
    if (!selected.length) return label;
    if (selected.length === 1) return options.find((o) => o.id === selected[0])?.label ?? label;
    return `${label}: ${selected.length}`;
  });
  function toggle(id: string) {
    selected = selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id];
  }
  function onDocClick(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:document onclick={onDocClick} onkeydown={(e) => e.key === 'Escape' && (open = false)} />

<div class="mf" bind:this={root}>
  <button class="btn sm trigger" class:on={selected.length > 0} onclick={() => (open = !open)} aria-expanded={open} aria-haspopup="true">
    <span class="sum">{summary}</span><ChevronDown size={14} strokeWidth={2} />
  </button>
  {#if open}
    <div class="menu card" role="group" aria-label={label}>
      {#if search}<input class="input sm" placeholder="Найти" bind:value={q} aria-label="Поиск: {label}" />{/if}
      <div class="opts">
        {#each shown as o (o.id)}
          <label class="opt">
            <input type="checkbox" checked={selected.includes(o.id)} onchange={() => toggle(o.id)} />
            {#if o.color}<span class="dot" style="background:{o.color}"></span>{/if}
            <span class="lbl">{o.label}</span>
            {#if o.hint}<span class="faint small">{o.hint}</span>{/if}
          </label>
        {:else}
          <p class="faint small none">Нет вариантов</p>
        {/each}
      </div>
      {#if selected.length}<button class="btn ghost sm reset" onclick={() => (selected = [])}>Сбросить</button>{/if}
    </div>
  {/if}
</div>

<style>
  .mf { position: relative; }
  .trigger { gap: 4px; max-width: 220px; }
  .trigger.on { background: var(--accent-soft); color: var(--accent); border-color: var(--accent); }
  .sum { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .menu { position: absolute; z-index: 20; top: calc(100% + 4px); left: 0; min-width: 220px; max-width: 320px; padding: 8px; display: grid; gap: 6px; box-shadow: var(--shadow-lg); }
  .opts { display: grid; gap: 1px; max-height: 300px; overflow-y: auto; }
  .opt { display: flex; align-items: center; gap: 8px; padding: 4px 6px; border-radius: 6px; cursor: pointer; font-size: 13.5px; }
  .opt:hover { background: var(--surface-2); }
  .dot { width: 10px; height: 10px; border-radius: 50%; flex: none; }
  .lbl { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .none { margin: 4px 6px; }
  .reset { justify-self: start; }
</style>
