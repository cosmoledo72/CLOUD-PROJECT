<script lang="ts">
  // Round avatar: the person's photo when there is one, else initials on a
  // colour derived from the id.
  import { onMount } from 'svelte';
  import { avatarHue, initials } from '#lib/api.ts';
  import { avatars } from '#lib/avatars.svelte.ts';

  let { id, name, size = 32 }: { id: string; name: string; size?: number } = $props();
  const src = $derived(avatars.url(id));
  let broken = $state(false);
  onMount(() => { avatars.ensure(); });
</script>

{#if src && !broken}
  <img class="av photo" {src} alt="" width={size} height={size} style="width: {size}px; height: {size}px" loading="lazy" onerror={() => (broken = true)} />
{:else}
  <span
    class="av"
    style="--h: {avatarHue(id)}; width: {size}px; height: {size}px; font-size: {Math.round(size * 0.38)}px"
    aria-hidden="true">{initials(name)}</span>
{/if}

<style>
  .av {
    display: inline-grid; place-items: center; flex: none; border-radius: 50%;
    background: hsl(var(--h) 55% 88%); color: hsl(var(--h) 45% 30%); font-weight: 600; letter-spacing: .02em;
  }
  .photo { object-fit: cover; background: var(--surface-3); }
  :global([data-theme='dark']) .av:not(.photo) { background: hsl(var(--h) 30% 26%); color: hsl(var(--h) 60% 82%); }
</style>
