<script lang="ts">
  import StorageTabs from '$lib/StorageTabs.svelte';
  import { api, docKind, formatDateTime, type FileItem, type Folder } from '$lib/api';
  import FileIcon from '$lib/FileIcon.svelte';

  let data = $state<{ folders: Folder[]; files: FileItem[] } | null>(null);
  $effect(() => {
    api.get<{ folders: Folder[]; files: FileItem[] }>('/shared').then((r) => (data = r));
  });
</script>

<div class="page">
  <StorageTabs />
  <div class="page-head"><h1>Доступные мне</h1></div>
  {#if data}
    <div class="card">
      {#if data.folders.length === 0 && data.files.length === 0}
        <p class="empty muted">С вами пока ничем не поделились.</p>
      {:else}
        <table class="list">
          <thead><tr><th>Имя</th><th>Владелец</th><th>Изменён</th></tr></thead>
          <tbody>
            {#each data.folders as f}
              <tr><td><a class="name" href="/files/{f.id}"><FileIcon folder />{f.name}</a></td><td></td><td class="muted">{formatDateTime(f.created_at)}</td></tr>
            {/each}
            {#each data.files as f}
              <tr>
                <td><a class="name" href={docKind(f.extension) !== 'other' ? `/editor/${f.id}` : `/api/files/${f.id}/download`} target="_blank"><FileIcon ext={f.extension} />{f.name}</a></td>
                <td class="muted">{f.owner_name}</td>
                <td class="muted">{formatDateTime(f.updated_at)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  {/if}
</div>

<style>
  .name { display: inline-flex; align-items: center; gap: 10px; color: var(--text); }
  .empty { padding: 40px; text-align: center; margin: 0; }
</style>
