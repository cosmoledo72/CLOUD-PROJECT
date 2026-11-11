<script lang="ts">
  // Invite several people at once: role, groups, projects and a personal message.
  import { api, plainMessage, type InviteResult } from '$lib/api';
  import Modal from '$lib/Modal.svelte';
  import { session } from '$lib/session.svelte';
  import X from '@lucide/svelte/icons/x';

  let { open = $bindable(false), ondone }: { open?: boolean; ondone?: () => void } = $props();

  type Group = { id: string; name: string };
  type Project = { id: string; name: string; status?: string; fields?: Record<string, unknown> };
  const codeOf = (p: Project) => (typeof p.fields?.code === 'string' ? p.fields.code : '');

  let emails = $state('');
  let role = $state('member');
  let groupIds = $state<string[]>([]);
  let picked = $state<{ id: string; name: string; role: 'manager' | 'member' | 'viewer' }[]>([]);
  let message = $state('');
  let groups = $state<Group[]>([]);
  let projects = $state<Project[]>([]);
  let projectQuery = $state('');
  let mailReady = $state(true);
  let error = $state('');
  let busy = $state(false);
  let results = $state<InviteResult[] | null>(null);

  // Reset and load the lists each time the dialog opens.
  let wasOpen = false;
  $effect(() => {
    if (open && !wasOpen) {
      emails = '';
      role = 'member';
      groupIds = [];
      picked = [];
      message = '';
      projectQuery = '';
      error = '';
      results = null;
      api.get<{ groups: Group[] }>('/groups').then((r) => (groups = r.groups)).catch(() => {});
      if (session.has('projects')) {
        api.get<{ projects: Project[] }>('/projects').then((r) => (projects = r.projects)).catch(() => {});
      }
      api.get<{ available: boolean }>('/admin/mail').then((r) => (mailReady = r.available)).catch(() => {});
    }
    wasOpen = open;
  });

  // Rough count of the addresses typed in, for the button label.
  const count = $derived((emails.match(/[^\s<>,;]+@[^\s<>,;]+/g) ?? []).length);
  const matches = $derived.by(() => {
    const q = projectQuery.trim().toLowerCase();
    if (!q) return [];
    return projects
      .filter((p) => !picked.some((x) => x.id === p.id) && (p.name.toLowerCase().includes(q) || codeOf(p).toLowerCase().includes(q)))
      .slice(0, 8);
  });

  function addProject(p: Project) {
    picked = [...picked, { id: p.id, name: p.name, role: role === 'guest' ? 'viewer' : 'member' }];
    projectQuery = '';
  }
  function toggleGroup(id: string) {
    groupIds = groupIds.includes(id) ? groupIds.filter((g) => g !== id) : [...groupIds, id];
  }

  async function send() {
    error = '';
    busy = true;
    try {
      const r = await api.post<{ results: InviteResult[] }>('/admin/invitations', {
        emails, role, group_ids: groupIds, message,
        projects: picked.map((p) => ({ project_id: p.id, role: p.role }))
      });
      results = r.results;
      ondone?.();
    } catch (e) {
      error = plainMessage((e as Error).message);
    } finally {
      busy = false;
    }
  }

  const invited = $derived(results?.filter((r) => r.status === 'invited') ?? []);
  const skipped = $derived(results?.filter((r) => r.status !== 'invited') ?? []);
  const skipLabel: Record<string, string> = {
    exists: 'уже в портале',
    already_invited: 'уже приглашён — отправьте приглашение снова из списка',
    invalid: 'не похоже на адрес почты'
  };
  const copy = (t: string) => navigator.clipboard?.writeText(t).catch(() => {});
</script>

<Modal bind:open title={results ? 'Приглашения отправлены' : 'Пригласить в портал'} width={620}>
  {#if results}
    <div class="res">
      {#if invited.length}
        <p>{invited.length === 1 ? 'Приглашён' : 'Приглашены'} {invited.length} {invited.length === 1 ? 'человек' : invited.length < 5 ? 'человека' : 'человек'}.
          {#if invited.some((r) => !r.emailed)}Почта портала не настроена — передайте ссылки сами (действуют 7 дней).{:else}Письма уже в пути.{/if}</p>
        <ul class="list">
          {#each invited as r (r.id)}
            <li>
              <span class="who"><b>{r.display_name}</b> <span class="faint">{r.email}</span></span>
              {#if !r.emailed && r.invite_url}
                <button class="btn sm" onclick={() => copy(r.invite_url!)}>Копировать ссылку</button>
              {:else}<span class="badge ok">письмо отправлено</span>{/if}
            </li>
          {/each}
        </ul>
      {/if}
      {#if skipped.length}
        <p class="muted">Пропущены:</p>
        <ul class="list">
          {#each skipped as r}
            <li><span class="who">{r.email ?? r.input}</span><span class="faint small">{skipLabel[r.status]}</span></li>
          {/each}
        </ul>
      {/if}
    </div>
  {:else}
    <label class="field">Адреса почты
      <textarea class="input" rows="4" bind:value={emails}
        placeholder={'ivan.petrov@partner-garant.ru\nАнна Смирнова <anna@example.ru>'}></textarea>
      <span class="faint small">Каждый адрес с новой строки или через запятую. Имя можно указать рядом — иначе приглашённый введёт его сам.</span>
    </label>
    <div class="two">
      <label class="field">Роль в портале
        <select class="input" bind:value={role}>
          <option value="member">Сотрудник</option>
          {#if session.me?.role === 'owner'}<option value="admin">Администратор</option>{/if}
          <option value="guest">Гость (заказчик, подрядчик)</option>
        </select>
      </label>
      {#if groups.length}
        <div class="field">
          <span>Группы</span>
          <div class="chips">
            {#each groups as g (g.id)}
              <button type="button" class="chip" class:on={groupIds.includes(g.id)} aria-pressed={groupIds.includes(g.id)} onclick={() => toggleGroup(g.id)}>{g.name}</button>
            {/each}
          </div>
        </div>
      {/if}
    </div>
    {#if session.has('projects')}
      <div class="field">
        <span>Сразу добавить в проекты</span>
        {#if picked.length}
          <ul class="picked">
            {#each picked as p, i (p.id)}
              <li>
                <span class="pname">{p.name}</span>
                <select class="input sm" bind:value={picked[i].role} aria-label="Роль в проекте {p.name}">
                  <option value="manager">Руководитель</option>
                  <option value="member">Участник</option>
                  <option value="viewer">Наблюдатель</option>
                </select>
                <button type="button" class="icon-btn" aria-label="Убрать {p.name}" onclick={() => (picked = picked.filter((x) => x.id !== p.id))}><X size={14} /></button>
              </li>
            {/each}
          </ul>
        {/if}
        <div class="pick">
          <input class="input" bind:value={projectQuery} placeholder="Найти проект по названию или шифру" aria-label="Найти проект" />
          {#if matches.length}
            <div class="menu" role="listbox">
              {#each matches as p (p.id)}
                <button type="button" role="option" aria-selected="false" onclick={() => addProject(p)}>
                  {#if codeOf(p)}<span class="mono faint">{codeOf(p)}</span>{/if} {p.name}
                </button>
              {/each}
            </div>
          {/if}
        </div>
      </div>
    {/if}
    <label class="field"><span>Сообщение в письме <span class="faint">(необязательно)</span></span>
      <textarea class="input" rows="2" bind:value={message} maxlength="1000" placeholder="Например: подключайтесь к проектам по Холмску"></textarea>
    </label>
    {#if !mailReady}
      <p class="faint small">Почта портала не настроена: после отправки покажем ссылки-приглашения, которые нужно передать самим.</p>
    {/if}
    {#if error}<div class="error-box">{error}</div>{/if}
  {/if}
  {#snippet footer()}
    {#if results}
      <button class="btn" onclick={() => (results = null)}>Пригласить ещё</button>
      <button class="btn primary" onclick={() => (open = false)}>Готово</button>
    {:else}
      <button class="btn" onclick={() => (open = false)}>Отмена</button>
      <button class="btn primary" disabled={busy || count === 0} onclick={send}>
        {count > 1 ? `Пригласить (${count})` : 'Пригласить'}
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  textarea.input { height: auto; padding-top: 6px; resize: vertical; }
  .two { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr); gap: 12px; align-items: start; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { border: 1px solid var(--border); background: var(--surface); color: var(--text-2); border-radius: 999px; padding: 3px 10px; font: inherit; font-size: 13px; cursor: pointer; }
  .chip.on { background: var(--accent-soft); border-color: var(--accent); color: var(--accent); font-weight: 500; }
  .picked { list-style: none; margin: 0 0 6px; padding: 0; display: grid; gap: 4px; }
  .picked li { display: flex; align-items: center; gap: 8px; padding: 4px 6px 4px 10px; border: 1px solid var(--border); border-radius: 6px; }
  .pname { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .input.sm { height: 26px; width: auto; padding: 0 6px; }
  .icon-btn { border: 0; background: none; color: var(--text-3); cursor: pointer; display: grid; place-items: center; padding: 2px; }
  .icon-btn:hover { color: var(--danger); }
  .pick { position: relative; }
  .menu { position: absolute; z-index: 5; left: 0; right: 0; top: calc(100% + 2px); background: var(--surface); border: 1px solid var(--border);
          border-radius: 6px; box-shadow: var(--shadow, 0 6px 20px rgba(0,0,0,.12)); max-height: 240px; overflow: auto; }
  .menu button { display: block; width: 100%; text-align: left; border: 0; background: none; padding: 7px 10px; font: inherit; cursor: pointer; color: var(--text); }
  .menu button:hover { background: var(--surface-2); }
  .res { display: grid; gap: 10px; }
  .res p { margin: 0; }
  .list { list-style: none; margin: 0; padding: 0; display: grid; gap: 4px; }
  .list li { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 6px 0; border-top: 1px solid var(--border); }
  .list li:first-child { border-top: 0; }
  .who { min-width: 0; overflow-wrap: anywhere; }
  @media (max-width: 560px) {
    .two { grid-template-columns: 1fr; }
  }
</style>
