<script lang="ts">
  // Coloured task label; with `onremove` shows a small × button.
  import type { TaskLabel } from '$lib/api';

  let { label, onremove, small = false }: { label: Pick<TaskLabel, 'name' | 'color'>; onremove?: () => void; small?: boolean } = $props();
</script>

<span class="chip" class:small style="--c: {label.color}" title={label.name}>
  <span class="name">{label.name}</span>
  {#if onremove}<button onclick={onremove} aria-label="Снять метку {label.name}">×</button>{/if}
</span>

<style>
  .chip {
    display: inline-flex; align-items: center; gap: 2px; max-width: 180px; padding: 2px 8px; border-radius: 999px;
    font-size: 12px; font-weight: 500; line-height: 1.35;
    background: color-mix(in srgb, var(--c) 16%, transparent); color: color-mix(in srgb, var(--c) 78%, var(--text));
  }
  .chip.small { font-size: 11px; padding: 1px 6px; max-width: 120px; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button { border: 0; background: none; color: inherit; cursor: pointer; padding: 0 0 0 3px; font-size: 13px; line-height: 1; opacity: .7; }
  button:hover { opacity: 1; }
</style>
