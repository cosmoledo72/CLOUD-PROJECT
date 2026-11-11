<script lang="ts">
  // Empty state: an icon in a soft circle, a title, a short explanation and actions.
  import type { Component, Snippet } from 'svelte';

  let { icon, title, text = '', children }: { icon: Component; title: string; text?: string; children?: Snippet } = $props();
  const Icon = $derived(icon);
</script>

<div class="empty-state">
  <div class="es-icon"><Icon size={24} /></div>
  <h3>{title}</h3>
  {#if text}<p>{text}</p>{/if}
  {#if children}<div class="es-actions">{@render children()}</div>{/if}
</div>
