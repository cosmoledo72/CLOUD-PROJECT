<script lang="ts">
  // Simple accessible modal dialog built on <dialog>.
  import type { Snippet } from 'svelte';

  let { title, open = $bindable(false), children, footer, width = 480 }: {
    title: string;
    open?: boolean;
    children: Snippet;
    footer?: Snippet;
    width?: number;
  } = $props();

  let dialog: HTMLDialogElement;

  $effect(() => {
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
</script>

<dialog bind:this={dialog} onclose={() => (open = false)} style="width: min({width}px, calc(100vw - 32px))">
  <header>
    <h2>{title}</h2>
    <button class="btn ghost sm" aria-label="Закрыть" onclick={() => (open = false)}>✕</button>
  </header>
  <div class="body">{@render children()}</div>
  {#if footer}<footer>{@render footer()}</footer>{/if}
</dialog>

<style>
  dialog {
    border: 1px solid var(--border); border-radius: 10px; padding: 0;
    background: var(--surface); color: var(--text); box-shadow: var(--shadow-lg);
  }
  dialog::backdrop { background: rgb(15 20 30 / 45%); }
  header { display: flex; align-items: center; justify-content: space-between; padding: 14px 16px 0; }
  header h2 { margin: 0; }
  .body { padding: 14px 16px; display: grid; gap: 12px; }
  footer { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 16px; border-top: 1px solid var(--border); }
</style>
