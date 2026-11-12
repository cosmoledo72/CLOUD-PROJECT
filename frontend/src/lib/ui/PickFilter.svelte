<script lang="ts">
  // Filter button with a single-choice dropdown (the look of MultiFilter).
  // The first option (value '') means "all" and shows the button unhighlighted.
  import Check from '@lucide/svelte/icons/check';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';

  type Option = { value: string; label: string; hint?: string };
  let { label, options, value = $bindable(''), search = false }: {
    /** Accessible name of the filter. */
    label: string;
    options: Option[];
    value?: string;
    search?: boolean;
  } = $props();

  let open = $state(false);
  let q = $state('');
  let root: HTMLDivElement;
  const current = $derived(options.find((o) => o.value === value) ?? options[0]);
  const shown = $derived(q.trim() ? options.filter((o) => o.label.toLowerCase().includes(q.trim().toLowerCase())) : options);
  function pick(v: string) {
    value = v;
    open = false;
    q = '';
  }
  function onDocClick(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:document onclick={onDocClick} onkeydown={(e) => e.key === 'Escape' && (open = false)} />

<div class="pf" bind:this={root}>
  <button class="btn sm trigger" class:on={!!value} onclick={() => (open = !open)} aria-expanded={open} aria-haspopup="listbox" aria-label="{label}: {current?.label}">
    <span class="sum">{current?.label ?? label}</span><ChevronDown size={14} strokeWidth={2} />
  </button>
  {#if open}
    <div class="menu card">
      {#if search}<!-- svelte-ignore a11y_autofocus --><input class="input sm" placeholder="Найти" bind:value={q} aria-label="Поиск: {label}" autofocus />{/if}
      <div class="opts" role="listbox" aria-label={label}>
        {#each shown as o (o.value)}
          <button class="opt" role="option" aria-selected={o.value === value} onclick={() => pick(o.value)}>
            <span class="lbl">{o.label}</span>
            {#if o.hint}<span class="faint small">{o.hint}</span>{/if}
            {#if o.value === value}<Check size={15} />{/if}
          </button>
        {:else}
          <p class="faint small none">Нет вариантов</p>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .pf { position: relative; }
  .trigger { gap: 4px; max-width: 240px; }
  .trigger.on { background: var(--accent-soft); color: var(--accent); border-color: var(--accent); }
  .sum { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .menu { position: absolute; z-index: 20; top: calc(100% + 4px); left: 0; min-width: 220px; max-width: 340px; padding: 6px; display: grid; gap: 6px; box-shadow: var(--shadow-lg); }
  .opts { display: grid; gap: 1px; max-height: 320px; overflow-y: auto; }
  .opt { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border: 0; background: none; border-radius: 6px; cursor: pointer;
    font: inherit; font-size: 13.5px; color: var(--text); text-align: left; }
  .opt:hover { background: var(--surface-2); }
  .opt[aria-selected='true'] { color: var(--accent); font-weight: 500; }
  .lbl { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .none { margin: 4px 6px; }
  @media (max-width: 640px) { .menu { right: auto; max-width: calc(100vw - 32px); } }
</style>
