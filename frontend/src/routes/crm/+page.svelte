<script lang="ts">
  import { goto } from '$app/navigation';
  $effect(() => {
    goto('/crm/deals', { replace: true });
  });
</script>
