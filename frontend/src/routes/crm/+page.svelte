<script lang="ts">
  import { goto } from '$app/navigation';
  $effect(() => {
    goto('/crm/deals', { replaceState: true });
  });
</script>
