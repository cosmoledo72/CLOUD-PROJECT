<script lang="ts">
  // A dropdown with checkboxes: «Все сотрудники» / «2 из 7». Empty choice = all.
  import ChevronDown from '@lucide/svelte/icons/chevron-down';

  let {
    label,
    items,
    value = $bindable()
  }: { label: string; items: { id: string; name: string }[]; value: string[] } = $props();

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
  const shown = $derived(items.filter((i) => !q.trim() || i.name.toLowerCase().includes(q.trim().toLowerCase())));
  const text = $derived(
    !value.length ? label : value.length === 1 ? (items.find((i) => i.id === value[0])?.name ?? label) : `${label}: ${value.length}`
  );
  function toggle(id: string) {
    value = value.includes(id) ? value.filter((x) => x !== id) : [...value, id];
  }
</script>

<div class="mp" bind:this={root}>
  <button class="btn" class:on={value.length > 0} onclick={() => (open = !open)} aria-expanded={open} aria-haspopup="listbox">
    <span class="tx">{text}</span><ChevronDown size={14} />
  </button>
  {#if open}
    <div class="pop" role="listbox" aria-multiselectable="true" aria-label={label}>
      {#if items.length > 8}<input class="input" placeholder="Найти" bind:value={q} />{/if}
      <div class="list">
        {#each shown as i (i.id)}
          <label class="opt"><input type="checkbox" checked={value.includes(i.id)} onchange={() => toggle(i.id)} /><span>{i.name}</span></label>
        {:else}
          <p class="faint small">Ничего не найдено</p>
        {/each}
      </div>
      {#if value.length}<button class="clr" onclick={() => (value = [])}>Сбросить</button>{/if}
    </div>
  {/if}
</div>

<style>
  .mp { position: relative; }
  .btn { display: inline-flex; align-items: center; gap: 6px; max-width: 240px; }
  .btn.on { border-color: var(--accent); color: var(--accent); }
  .tx { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pop { position: absolute; left: 0; top: calc(100% + 6px); z-index: 30; width: 280px; background: var(--surface); border: 1px solid var(--border);
         border-radius: 10px; box-shadow: 0 12px 32px rgba(0, 0, 0, .16); padding: 8px; display: grid; gap: 6px; }
  .list { max-height: 300px; overflow-y: auto; display: grid; gap: 1px; }
  .opt { display: flex; align-items: center; gap: 8px; padding: 5px 6px; border-radius: 6px; font-size: 13.5px; cursor: pointer; }
  .opt:hover { background: var(--surface-2); }
  .opt span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .clr { justify-self: end; border: 0; background: none; color: var(--accent); cursor: pointer; font: inherit; font-size: 13px; }
  @media (max-width: 720px) { .pop { position: fixed; left: 12px; right: 12px; top: 120px; width: auto; } }
</style>
