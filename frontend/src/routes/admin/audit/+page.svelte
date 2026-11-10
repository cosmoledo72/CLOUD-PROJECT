<script lang="ts">
  import { actionLabel, api, formatDateTime, type AuditEntry, type UserItem } from '$lib/api';

  const categories = [
    { v: '', l: 'Все действия' },
    { v: 'auth.', l: 'Вход и безопасность' },
    { v: 'user.', l: 'Пользователи' },
    { v: 'file.', l: 'Файлы' },
    { v: 'folder.', l: 'Папки' },
    { v: 'share.', l: 'Доступ' },
    { v: 'project.', l: 'Проекты' },
    { v: 'project_fields.', l: 'Реквизиты' },
    { v: 'branding.', l: 'Оформление' },
    { v: 'portal.', l: 'Портал' }
  ];

  let entries = $state<AuditEntry[]>([]);
  let next = $state<number | null>(null);
  let users = $state<UserItem[]>([]);
  let action = $state('');
  let userId = $state('');
  let from = $state('');
  let to = $state('');
  let loading = $state(false);
  let error = $state('');

  function query(before?: number) {
    const p = new URLSearchParams({ limit: '50' });
    if (action) p.set('action', action);
    if (userId) p.set('user_id', userId);
    if (from) p.set('from', from);
    if (to) p.set('to', to);
    if (before) p.set('before', String(before));
    return `/admin/audit?${p}`;
  }

  async function load(append = false) {
    loading = true;
    error = '';
    try {
      const r = await api.get<{ entries: AuditEntry[]; next_before: number | null }>(query(append ? next ?? undefined : undefined));
      entries = append ? [...entries, ...r.entries] : r.entries;
      next = r.next_before;
    } catch (e) {
      error = (e as Error).message;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    api.get<{ users: UserItem[] }>('/users').then((r) => (users = r.users));
  });
  // Reload whenever a filter changes.
  $effect(() => {
    [action, userId, from, to];
    load();
  });

  const isAlert = (a: string) => a === 'auth.login_failed' || a === 'auth.mfa_failed';

  /** Short human summary of the details payload. */
  function details(e: AuditEntry): string {
    const d = e.details ?? {};
    const parts: string[] = [];
    if (typeof d.name === 'string') parts.push(d.name);
    if (typeof d.version === 'number') parts.push(`версия ${d.version}`);
    if (typeof d.role === 'string') parts.push(`роль: ${d.role}`);
    if (typeof d.key === 'string') parts.push(d.key);
    if (typeof d.created === 'number') parts.push(`добавлено: ${d.created}`);
    if (typeof d.count === 'number') parts.push(`сеансов: ${d.count}`);
    if (typeof d.kind === 'string') parts.push(d.kind === 'logo' ? 'логотип' : 'значок');
    if (d.mfa === true) parts.push('с 2FA');
    if (Array.isArray(d.keys)) parts.push((d.keys as string[]).join(', '));
    return parts.join(' · ');
  }

  function exportCsv() {
    const esc = (s: string) => `"${s.replaceAll('"', '""')}"`;
    const lines = [['Время', 'Пользователь', 'Действие', 'Объект', 'Подробности'].map(esc).join(';')];
    for (const e of entries) {
      lines.push([formatDateTime(e.created_at), e.user_name ?? 'Система', actionLabel[e.action] ?? e.action, e.target_name ?? '', details(e)].map(esc).join(';'));
    }
    const blob = new Blob(['﻿' + lines.join('\r\n')], { type: 'text/csv;charset=utf-8' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `audit-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    URL.revokeObjectURL(a.href);
  }
</script>

<div class="filters">
  <select class="input" bind:value={action} aria-label="Тип действия">
    {#each categories as c}<option value={c.v}>{c.l}</option>{/each}
  </select>
  <select class="input" bind:value={userId} aria-label="Пользователь">
    <option value="">Все пользователи</option>
    {#each users as u}<option value={u.id}>{u.display_name}</option>{/each}
  </select>
  <label class="row date"><span class="faint small">с</span><input class="input" type="date" bind:value={from} /></label>
  <label class="row date"><span class="faint small">по</span><input class="input" type="date" bind:value={to} /></label>
  <div class="spacer"></div>
  <button class="btn" onclick={exportCsv} disabled={entries.length === 0}>Выгрузить CSV</button>
</div>
{#if error}<div class="error-box">{error}</div>{/if}

<div class="card scroll">
  <table class="list">
    <thead><tr><th>Время</th><th>Пользователь</th><th>Действие</th><th>Объект</th><th>Подробности</th></tr></thead>
    <tbody>
      {#each entries as e (e.id)}
        <tr class:alert={isAlert(e.action)}>
          <td class="muted nowrap">{formatDateTime(e.created_at)}</td>
          <td>{e.user_name ?? 'Система'}</td>
          <td>{actionLabel[e.action] ?? e.action}</td>
          <td class="muted">{e.target_name ?? ''}</td>
          <td class="faint">{details(e)}</td>
        </tr>
      {:else}
        <tr><td colspan="5" class="faint">{loading ? 'Загрузка…' : 'Записей нет'}</td></tr>
      {/each}
    </tbody>
  </table>
</div>
{#if next}
  <div class="more"><button class="btn" disabled={loading} onclick={() => load(true)}>Показать ещё</button></div>
{/if}

<style>
  .filters { display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 12px; align-items: center; }
  .filters > select { width: auto; min-width: 180px; }
  .date .input { width: 150px; }
  .scroll { overflow-x: auto; }
  .nowrap { white-space: nowrap; }
  tr.alert td { background: var(--danger-soft); }
  tr.alert td:nth-child(3) { color: var(--danger); font-weight: 500; }
  .more { display: flex; justify-content: center; margin-top: 12px; }
  .error-box { margin-bottom: 12px; }
</style>
