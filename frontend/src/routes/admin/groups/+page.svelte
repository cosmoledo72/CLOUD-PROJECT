<script lang="ts">
  import { api, type UserItem } from '$lib/api';
  import Modal from '$lib/Modal.svelte';

  interface Group { id: string; name: string; member_ids: string[] }

  let groups = $state<Group[]>([]);
  let users = $state<UserItem[]>([]);
  let error = $state('');

  async function load() {
    const [g, u] = await Promise.all([api.get<{ groups: Group[] }>('/groups'), api.get<{ users: UserItem[] }>('/users')]);
    groups = g.groups;
    users = u.users;
  }
  $effect(() => {
    load().catch((e) => (error = e.message));
  });

  const nameOf = (id: string) => users.find((u) => u.id === id)?.display_name ?? '?';

  async function run(fn: () => Promise<unknown>) {
    error = '';
    try {
      await fn();
      await load();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  let editOpen = $state(false);
  let edit = $state<{ id: string | null; name: string; members: string[] }>({ id: null, name: '', members: [] });
  function openEdit(g?: Group) {
    edit = g ? { id: g.id, name: g.name, members: [...g.member_ids] } : { id: null, name: '', members: [] };
    editOpen = true;
  }
  function toggle(id: string) {
    edit.members = edit.members.includes(id) ? edit.members.filter((m) => m !== id) : [...edit.members, id];
  }
  async function save() {
    await run(async () => {
      let id = edit.id;
      if (id) await api.patch(`/groups/${id}`, { name: edit.name });
      else id = (await api.post<{ id: string }>('/groups', { name: edit.name })).id;
      await api.put(`/groups/${id}/members`, { user_ids: edit.members });
    });
    if (!error) editOpen = false;
  }
  function remove(g: Group) {
    if (!confirm(`Удалить группу «${g.name}»? Права доступа, выданные группе, будут отозваны.`)) return;
    run(() => api.del(`/groups/${g.id}`));
  }
</script>

<div class="row toolbar">
  <p class="muted">Группы упрощают выдачу доступа к папкам: права, выданные группе, получают все её участники.</p>
  <div class="spacer"></div>
  <button class="btn primary" onclick={() => openEdit()}>Новая группа</button>
</div>
{#if error}<div class="error-box">{error}</div>{/if}

{#if groups.length === 0}
  <div class="card empty"><p class="muted">Групп пока нет. Например: «Отдел обследований», «Проектный отдел», «Бухгалтерия».</p></div>
{:else}
  <div class="grid">
    {#each groups as g (g.id)}
      <div class="card group">
        <div class="row"><h3>{g.name}</h3><div class="spacer"></div><span class="badge grey">{g.member_ids.length}</span></div>
        <p class="members muted">{g.member_ids.length ? g.member_ids.map(nameOf).join(', ') : 'Нет участников'}</p>
        <div class="row">
          <button class="btn sm" onclick={() => openEdit(g)}>Изменить</button>
          <button class="btn ghost sm danger" onclick={() => remove(g)}>Удалить</button>
        </div>
      </div>
    {/each}
  </div>
{/if}

<Modal bind:open={editOpen} title={edit.id ? 'Группа' : 'Новая группа'}>
  <label class="field">Название <input class="input" bind:value={edit.name} /></label>
  <div class="field-label muted">Участники ({edit.members.length})</div>
  <div class="picker">
    {#each users.filter((u) => u.is_active) as u (u.id)}
      <label class="check"><input type="checkbox" checked={edit.members.includes(u.id)} onchange={() => toggle(u.id)} />{u.display_name} <span class="faint small">{u.email}</span></label>
    {/each}
  </div>
  {#snippet footer()}
    <button class="btn" onclick={() => (editOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!edit.name.trim()} onclick={save}>Сохранить</button>
  {/snippet}
</Modal>

<style>
  .toolbar { margin-bottom: 12px; flex-wrap: wrap; align-items: center; }
  .toolbar p { margin: 0; max-width: 640px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 12px; }
  .group { padding: 14px; display: grid; gap: 8px; }
  .group h3 { margin: 0; }
  .members { margin: 0; font-size: 13px; min-height: 2.8em; }
  .empty { padding: 32px; text-align: center; }
  .empty p { margin: 0; }
  .field-label { font-size: 13px; }
  .picker { display: grid; gap: 6px; max-height: 280px; overflow-y: auto; border: 1px solid var(--border); border-radius: var(--radius); padding: 10px; }
  .error-box { margin-bottom: 12px; }
</style>
