<script lang="ts">
  // Every working link for people without an account: who shared what, until
  // when, how often it was opened; revoke any of them; switch links off for
  // the whole portal.
  import Link2 from '@lucide/svelte/icons/link-2';
  import Lock from '@lucide/svelte/icons/lock';
  import { api, formatDateTime } from '$lib/api';
  import EmptyState from '$lib/EmptyState.svelte';
  import { relTime } from '$lib/files/fmt';

  interface Row {
    id: string;
    resource_type: 'file' | 'folder' | 'kb_page';
    resource_id: string;
    token: string;
    name: string | null;
    missing: boolean;
    expired: boolean;
    has_password: boolean;
    expires_on: string | null;
    created_by_name: string | null;
    created_at: string;
    opens: number;
    downloads: number;
    last_opened_at: string | null;
  }

  let links = $state<Row[]>([]);
  let enabled = $state(true);
  let loaded = $state(false);
  let error = $state('');

  async function load() {
    try {
      const r = await api.get<{ links: Row[]; enabled: boolean }>('/admin/links');
      links = r.links;
      enabled = r.enabled;
    } catch (e) {
      error = (e as Error).message;
    } finally {
      loaded = true;
    }
  }
  $effect(() => {
    load();
  });

  async function toggle() {
    const next = !enabled;
    if (!next && !confirm('Отключить ссылки для внешних на всём портале? Все выданные ссылки сразу перестанут открываться (их можно будет включить обратно).')) return;
    try {
      enabled = (await api.put<{ enabled: boolean }>('/admin/links/settings', { enabled: next })).enabled;
    } catch (e) {
      error = (e as Error).message;
    }
  }
  async function revoke(l: Row) {
    if (!confirm(`Отключить ссылку на «${l.name ?? 'удалённый объект'}»?`)) return;
    try {
      await api.del(`/links/${l.id}`);
      await load();
    } catch (e) {
      error = (e as Error).message;
    }
  }
  const kind = { file: 'Файл', folder: 'Папка', kb_page: 'База знаний' };
  const place = (l: Row) => (l.resource_type === 'file' ? `/editor/${l.resource_id}` : l.resource_type === 'folder' ? `/files/${l.resource_id}` : `/kb?p=${l.resource_id}`);
  const fmtDate = (d: string) => new Date(d + 'T00:00').toLocaleDateString('ru-RU');
</script>

<p class="muted intro">Файлы, папки и страницы базы знаний, открытые заказчикам и подрядчикам по ссылке без учётной записи. Ссылку создают в окне «Доступ» (у страницы базы знаний — «Поделиться»).</p>

<div class="card toggle">
  <label class="check"><input type="checkbox" checked={enabled} onchange={toggle} /><b>Разрешить ссылки для внешних</b></label>
  <span class="muted small">{enabled ? 'Сотрудники с полным доступом к файлу или папке могут выдавать ссылки.' : 'Ссылки не выдаются, выданные не открываются.'}</span>
</div>

{#if error}<div class="error-box">{error}</div>{/if}

{#if loaded && !links.length}
  <div class="card"><EmptyState icon={Link2} title="Ссылок нет" text="Здесь появятся все действующие ссылки портала." /></div>
{:else if links.length}
  <div class="card tbl">
    <table class="list">
      <thead><tr><th>Что открыто</th><th>Кто выдал</th><th>Действует</th><th>Открытия</th><th></th></tr></thead>
      <tbody>
        {#each links as l (l.id)}
          <tr class:dim={l.expired || l.missing}>
            <td>
              <span class="faint small">{kind[l.resource_type]}</span><br />
              {#if l.missing}<span class="muted">удалено</span>{:else}<a href={place(l)}>{l.name}</a>{/if}
              {#if l.has_password}<span class="pw" title="С паролем"><Lock size={13} /></span>{/if}
            </td>
            <td>{l.created_by_name ?? '—'}<br /><span class="faint small">{formatDateTime(l.created_at)}</span></td>
            <td>{#if l.expired}<span class="badge">истекла</span>{:else if l.expires_on}по {fmtDate(l.expires_on)}{:else}бессрочно{/if}</td>
            <td>{l.opens} / скач. {l.downloads}{#if l.last_opened_at}<br /><span class="faint small">{relTime(l.last_opened_at)}</span>{/if}</td>
            <td class="act"><button class="btn ghost sm danger" onclick={() => revoke(l)}>Отключить</button></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<style>
  .intro { max-width: 760px; margin: 0 0 14px; }
  .toggle { padding: 14px 16px; display: grid; gap: 4px; margin-bottom: 14px; }
  .tbl { overflow-x: auto; }
  .act { text-align: right; white-space: nowrap; }
  tr.dim td { opacity: .6; }
  .pw { margin-left: 6px; color: var(--text-3); vertical-align: middle; }
</style>
