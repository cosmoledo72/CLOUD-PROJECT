<script lang="ts">
  import { avatarHue, initials } from '$lib/api';

  let { id, name, size = 32 }: { id: string; name: string; size?: number } = $props();
</script>

<span
  class="av"
  style="--h: {avatarHue(id)}; width: {size}px; height: {size}px; font-size: {Math.round(size * 0.38)}px"
  aria-hidden="true">{initials(name)}</span>

<style>
  .av {
    display: inline-grid; place-items: center; flex: none; border-radius: 50%;
    background: hsl(var(--h) 55% 88%); color: hsl(var(--h) 45% 30%); font-weight: 600; letter-spacing: .02em;
  }
  :global([data-theme='dark']) .av { background: hsl(var(--h) 30% 26%); color: hsl(var(--h) 60% 82%); }
</style>
