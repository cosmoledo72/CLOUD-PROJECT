<script lang="ts">
  // Coloured square with the project's letter or icon.
  import { autoBadgeColor, BADGE_ICONS, badgeLetter } from './badge';

  let { id, name, color = null, icon = null, size = 28 }: {
    id: string;
    name: string;
    color?: string | null;
    icon?: string | null;
    size?: number;
  } = $props();

  const bg = $derived(color ?? autoBadgeColor(id));
  const Icon = $derived(icon ? BADGE_ICONS[icon]?.icon : undefined);
</script>

<span class="pb" style="--bg: {bg}; width: {size}px; height: {size}px; font-size: {Math.round(size * 0.46)}px; border-radius: {Math.max(4, Math.round(size * 0.25))}px" aria-hidden="true">
  {#if Icon}<Icon size={Math.round(size * 0.58)} strokeWidth={1.8} />{:else}{badgeLetter(name)}{/if}
</span>

<style>
  .pb {
    display: inline-grid; place-items: center; flex: none; background: var(--bg); color: #fff;
    font-weight: 600; line-height: 1; letter-spacing: 0; user-select: none;
  }
  .pb :global(svg) { stroke-width: 1.8px; }
</style>
