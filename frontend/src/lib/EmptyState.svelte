<script lang="ts">
  // Empty state: an icon in a soft circle, a title, a short explanation and actions.
  import type { Component, Snippet } from 'svelte';

  let { icon, title, text = '', compact = false, children }: { icon: Component; title: string; text?: string; compact?: boolean; children?: Snippet } = $props();
  const Icon = $derived(icon);
</script>

<div class="empty-state" class:compact>
  <div class="es-icon"><Icon size={compact ? 20 : 24} /></div>
  <h3>{title}</h3>
  {#if text}<p>{text}</p>{/if}
  {#if children}<div class="es-actions">{@render children()}</div>{/if}
</div>

<style>
  /* Inside a small card: less air, smaller icon. */
  .compact { padding: 18px 16px 20px; gap: 6px; }
  .compact .es-icon { width: 42px; height: 42px; margin-bottom: 2px; box-shadow: 0 0 0 6px color-mix(in srgb, var(--surface-3) 45%, transparent); }
  .compact h3 { font-size: 14px; }
  .compact p { font-size: 13px; }
</style>
