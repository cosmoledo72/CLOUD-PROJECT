<script lang="ts">
  // Manage explicit shares of a folder or file, and its link for people
  // without an account.
  import Modal from './Modal.svelte';
  import PublicLink from './files/PublicLink.svelte';
  import { api, type Share, type UserItem } from './api';

  let { open = $bindable(false), resourceType, resourceId, title, focus = null }: {
    open?: boolean;
    resourceType: 'folder' | 'file';
    resourceId: string;
    title: string;
    /** «link»: scroll to and highlight the public link block. */
    focus?: 'link' | null;
  } = $props();

  const levels = [
    { v: 1, label: 'Чтение' },
    { v: 2, label: 'Комментирование' },
    { v: 3, label: 'Редактирование' },
    { v: 4, label: 'Полный доступ' }
  ];

  let shares = $state<Share[]>([]);
  let users = $state<UserItem[]>([]);
  let principal = $state('everyone');
  let level = $state(1);
  let error = $state('');

  async function load() {
    const q = `resource_type=${resourceType}&resource_id=${resourceId}`;
    shares = (await api.get<{ shares: Share[] }>(`/shares?${q}`)).shares;
    users = (await api.get<{ users: UserItem[] }>('/users')).users.filter((u) => u.is_active);
  }

  $effect(() => {
    if (open) load().catch((e) => (error = e.message));
  });

  async function add() {
    error = '';
    try {
      await api.post('/shares', {
        resource_type: resourceType,
        resource_id: resourceId,
        user_id: principal === 'everyone' ? null : principal,
        access: level
      });
      await load();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  async function remove(id: string) {
    await api.del(`/shares/${id}`);
    await load();
  }
</script>

<Modal bind:open title={`Доступ: ${title}`} width={600}>
  <h4 class="sec">Сотрудники портала</h4>
  <div class="row add">
    <select class="input" bind:value={principal}>
      <option value="everyone">Все сотрудники портала</option>
      {#each users as u}<option value={u.id}>{u.display_name} ({u.email})</option>{/each}
    </select>
    <select class="input lvl" bind:value={level}>
      {#each levels as l}<option value={l.v}>{l.label}</option>{/each}
    </select>
    <button class="btn primary" onclick={add}>Добавить</button>
  </div>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#if shares.length === 0}
    <p class="muted">Явных прав нет. Доступ определяется владельцем, ролью и родительскими папками.</p>
  {:else}
    <table class="list">
      <tbody>
        {#each shares as s}
          <tr>
            <td>{s.principal_name}</td>
            <td class="muted">{levels.find((l) => l.v === s.access)?.label}</td>
            <td style="text-align:right"><button class="btn ghost sm danger" onclick={() => remove(s.id)}>Убрать</button></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  <div class="pub"><PublicLink {resourceType} {resourceId} active={open} highlight={focus === 'link'} /></div>
</Modal>

<style>
  .add { flex-wrap: wrap; }
  .add select:first-child { flex: 2; min-width: 200px; }
  .lvl { flex: 1; min-width: 150px; }
  p { margin: 0; }
  .sec { margin: 0 0 8px; font-size: 11.5px; font-weight: 600; letter-spacing: .04em; text-transform: uppercase; color: var(--text-3); }
  .pub { margin-top: 16px; }
</style>
