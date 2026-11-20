<script lang="ts">
  // «A new version of the portal is out»: SvelteKit polls /_app/version.json
  // (kit.version.pollInterval) and sets `updated.current` after a rebuild.
  // A tab opened before the rebuild keeps running the old bundle until it is
  // reloaded, so offer the reload instead of leaving stale pages on screen.
  import { updated } from '$app/state';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import X from '@lucide/svelte/icons/x';

  let dismissed = $state(false);
</script>

{#if updated.current && !dismissed}
  <div class="upd card" role="status">
    <i class="chip-icon accent"><RefreshCw size={17} /></i>
    <div class="txt"><b>Портал обновлён</b><span class="muted">Обновите страницу, чтобы открыть новую версию.</span></div>
    <button class="btn primary sm" onclick={() => location.reload()}>Обновить</button>
    <button class="x" onclick={() => (dismissed = true)} title="Позже" aria-label="Скрыть"><X size={16} /></button>
  </div>
{/if}

<style>
  .upd {
    position: fixed; z-index: 60; left: 50%; bottom: 20px; transform: translateX(-50%);
    display: flex; align-items: center; gap: 12px; padding: 10px 10px 10px 14px;
    box-shadow: var(--shadow-lg); max-width: calc(100vw - 24px); width: max-content;
  }
  .txt { display: grid; gap: 1px; font-size: 13.5px; }
  .txt .muted { font-size: 12.5px; }
  .x { border: 0; background: none; color: var(--text-3); cursor: pointer; padding: 4px; border-radius: 6px; display: grid; }
  .x:hover { background: var(--surface-2); color: var(--text); }
  @media (max-width: 640px) {
    .upd { bottom: 12px; gap: 10px; }
    .txt .muted { display: none; }
  }
</style>
