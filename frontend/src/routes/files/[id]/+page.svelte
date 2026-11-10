<script lang="ts">
  import { page } from '$app/state';
  import FolderBrowser from '$lib/FolderBrowser.svelte';
  import StorageTabs from '$lib/StorageTabs.svelte';

  const id = $derived(page.params.id ?? '');
  let activeRoot = $state<string | null>(null);
</script>

<div class="page">
  <StorageTabs {activeRoot} />
  {#key id}
    <FolderBrowser folderId={id} embedded={false} onview={(v) => (activeRoot = v.breadcrumbs[0]?.id ?? null)} />
  {/key}
</div>
