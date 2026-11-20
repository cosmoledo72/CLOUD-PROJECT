<script lang="ts">
  import { goto } from '$app/navigation';
  import { api } from '#lib/api.ts';

  // Land on the personal root (guests have none, so fall back to common).
  $effect(() => {
    api.get<{ my: string | null; common: string | null }>('/folders/roots').then((r) => {
      const target = r.my ?? r.common;
      if (target) goto(`/files/${target}`, { replace: true });
    });
  });
</script>
