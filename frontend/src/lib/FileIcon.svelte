<script lang="ts">
  import { docKind } from './api';

  let { ext = '', folder = false }: { ext?: string; folder?: boolean } = $props();
  const kind = $derived(folder ? 'folder' : docKind(ext));
  const label = $derived({ word: 'W', cell: 'X', slide: 'P', pdf: 'PDF', other: ext.slice(0, 3).toUpperCase() || '?', folder: '' }[kind]);
</script>

{#if kind === 'folder'}
  <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
    <path d="M3 6.5A1.5 1.5 0 0 1 4.5 5h4.6l2 2h8.4A1.5 1.5 0 0 1 21 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z" fill="#e8b84a" />
  </svg>
{:else}
  <span class="ic {kind}" aria-hidden="true">{label}</span>
{/if}

<style>
  .ic {
    display: inline-grid; place-items: center; width: 22px; height: 22px; border-radius: 4px;
    font-size: 9px; font-weight: 700; color: #fff; background: var(--text-3); letter-spacing: -.02em;
  }
  .word { background: var(--word); font-size: 12px; }
  .cell { background: var(--cell); font-size: 12px; }
  .slide { background: var(--slide); font-size: 12px; }
  .pdf { background: var(--pdf); }
</style>
