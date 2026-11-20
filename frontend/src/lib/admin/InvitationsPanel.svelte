<script lang="ts">
  // Pending invitations: who, by whom, where to, until when; resend, copy, revoke.
  import { api, formatDate, plainMessage, roleLabel, type Invitation } from '#lib/api.ts';

  let { items, onchange }: { items: Invitation[]; onchange: () => void } = $props();

  let error = $state('');
  let notice = $state('');
  // Fresh links after "send again" (shown for copying when mail is off).
  let links = $state<Record<string, string>>({});

  async function resend(i: Invitation) {
    error = notice = '';
    try {
      const r = await api.post<{ invite_url: string; emailed: boolean }>(`/admin/users/${i.id}/invite`);
      links[i.id] = r.invite_url;
      notice = r.emailed ? `Приглашение для ${i.display_name} отправлено снова. Прежняя ссылка больше не действует.`
        : `Новая ссылка для ${i.display_name} готова — почта портала не настроена, передайте её сами.`;
      onchange();
    } catch (e) {
      error = plainMessage((e as Error).message);
    }
  }

  async function revoke(i: Invitation) {
    if (!confirm(`Отозвать приглашение ${i.email}? Ссылка перестанет работать.`)) return;
    error = notice = '';
    try {
      await api.del(`/admin/invitations/${i.id}`);
      notice = `Приглашение ${i.email} отозвано.`;
      onchange();
    } catch (e) {
      error = plainMessage((e as Error).message);
    }
  }

  const projectRole: Record<string, string> = { manager: 'руководитель', member: 'участник', viewer: 'наблюдатель' };
  const copy = (t: string) => navigator.clipboard?.writeText(t).catch(() => {});
  function left(i: Invitation): string {
    if (!i.expires_at) return 'ссылки нет';
    const d = Math.ceil((Date.parse(i.expires_at) - Date.now()) / 86_400_000);
    return d <= 0 ? 'истекло' : d === 1 ? 'истекает завтра' : `ещё ${d} дн.`;
  }
</script>

{#if error}<div class="error-box">{error}</div>{/if}
{#if notice}<div class="ok-box">{notice}</div>{/if}

{#if items.length === 0}
  <div class="card empty"><p class="muted">Неотвеченных приглашений нет.</p></div>
{:else}
  <div class="card scroll">
    <table class="list">
      <thead><tr><th>Кого</th><th>Роль</th><th>Куда</th><th>Пригласил</th><th>Срок ссылки</th><th></th></tr></thead>
      <tbody>
        {#each items as i (i.id)}
          <tr class:off={!i.is_active}>
            <td>
              <div class="name">{i.display_name}</div>
              <div class="faint small">{i.email}</div>
              {#if i.message}<div class="msg small" title={i.message}>«{i.message}»</div>{/if}
            </td>
            <td>{roleLabel[i.role] ?? i.role}</td>
            <td class="where">
              {#each i.projects as p (p.id)}<span class="tag" title="Роль в проекте: {projectRole[p.role] ?? p.role}">{p.name}</span>{/each}
              {#each i.groups as g}<span class="tag grp">{g}</span>{/each}
              {#if !i.projects.length && !i.groups.length}<span class="faint">—</span>{/if}
            </td>
            <td><div>{i.invited_by_name ?? '—'}</div><div class="faint small">{formatDate(i.created_at)}</div></td>
            <td>
              {#if i.expired}<span class="badge warn">{i.expires_at ? 'истекло' : 'нет ссылки'}</span>
              {:else}<span class="muted">{left(i)}</span>{/if}
            </td>
            <td class="actions">
              {#if links[i.id]}<button class="btn ghost sm" onclick={() => copy(links[i.id])}>Копировать ссылку</button>{/if}
              <button class="btn ghost sm" onclick={() => resend(i)}>{i.expired ? 'Отправить заново' : 'Отправить снова'}</button>
              <button class="btn ghost sm danger" onclick={() => revoke(i)}>Отозвать</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<style>
  .ok-box, .error-box { margin-bottom: 12px; }
  .empty { padding: 20px; }
  .empty p { margin: 0; }
  .scroll { overflow-x: auto; }
  th { white-space: nowrap; }
  .name { font-weight: 500; }
  .msg { color: var(--text-2); max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-style: italic; }
  .where { max-width: 280px; }
  .tag { display: inline-block; margin: 0 4px 4px 0; padding: 1px 8px; border-radius: 999px; background: var(--accent-soft); color: var(--accent); font-size: 12.5px; }
  .tag.grp { background: var(--surface-2); color: var(--text-2); }
  .actions { text-align: right; white-space: nowrap; }
  tr.off td { opacity: .55; }
</style>
