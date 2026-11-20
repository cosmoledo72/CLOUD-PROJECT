<script lang="ts">
  // Start page: go to the first section whose module is on.
  import { goto } from '$app/navigation';
  import { session } from '#lib/session.svelte.ts';
  $effect(() => {
    if (session.loaded) goto(session.me ? session.home : '/login', { replace: true });
  });
</script>
