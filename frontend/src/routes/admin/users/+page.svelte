<script lang="ts">
  import { generatePassword } from '#lib/people/password.ts';
  import UserPlus from '@lucide/svelte/icons/user-plus';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, crmRoleLabel, formatDate, plainMessage, roleLabel, type Invitation, type RegistrationSettings, type UserItem } from '#lib/api.ts';
  import InvitationsPanel from '#lib/admin/InvitationsPanel.svelte';
  import InviteDialog from '#lib/admin/InviteDialog.svelte';
  import RequestsPanel from '#lib/admin/RequestsPanel.svelte';
  import Modal from '#lib/Modal.svelte';
  import { session } from '#lib/session.svelte.ts';

  let users = $state<UserItem[]>([]);
  let invitations = $state<Invitation[]>([]);
  let registration = $state<RegistrationSettings | null>(null);
  let error = $state('');
  let notice = $state('');
  let filter = $state('');

  // Sub-tabs: people, invitations, registration requests (?tab=… — used by notification links).
  type Tab = 'people' | 'invites' | 'requests';
  const tab = $derived<Tab>((['invites', 'requests'] as const).find((t) => t === page.url.searchParams.get('tab')) ?? 'people');
  const setTab = (t: Tab) => goto(t === 'people' ? '/admin/users' : `/admin/users?tab=${t}`, { replace: true, reset: false });

  async function load() {
    const [u, i, r] = await Promise.all([
      api.get<{ users: UserItem[] }>('/users'),
      api.get<{ invitations: Invitation[] }>('/admin/invitations'),
      api.get<RegistrationSettings>('/admin/registration')
    ]);
    users = u.users;
    invitations = i.invitations;
    registration = r;
  }
  $effect(() => {
    load().catch((e) => (error = plainMessage(e.message)));
  });
  const people = $derived(users.filter((u) => !u.invite_pending));
  let inviteOpen = $state(false);

  const shown = $derived(
    users.filter((u) => !u.invite_pending).filter((u) => {
      const q = filter.trim().toLowerCase();
      return !q || u.display_name.toLowerCase().includes(q) || u.email.toLowerCase().includes(q);
    })
  );

  async function act(fn: () => Promise<unknown>, ok: string) {
    error = '';
    notice = '';
    try {
      await fn();
      notice = ok;
      await load();
    } catch (e) {
      error = plainMessage((e as Error).message);
    }
  }

  // Owners are untouchable; admins can be managed by the owner only; nobody manages themselves here.
  const manageable = (u: UserItem) =>
    u.id !== session.me?.id && u.role !== 'owner' && (u.role !== 'admin' || session.me?.role === 'owner');

  // --- Create with a password (handed over in person) ---------------------------
  let createOpen = $state(false);
  let form = $state({ email: '', display_name: '', password: '', role: 'member' });
  function openCreate() {
    form = { email: '', display_name: '', password: generatePassword(), role: 'member' };
    error = '';
    createOpen = true;
  }
  async function create() {
    const name = form.display_name;
    await act(() => api.post('/users', form), `Пользователь ${name} создан. Передайте ему пароль: ${form.password}`);
    if (!error) createOpen = false;
  }

  // --- Password reset ---------------------------------------------------------
  let resetOpen = $state(false);
  let resetUser = $state<UserItem | null>(null);
  let resetPassword = $state('');
  function openReset(u: UserItem) {
    resetUser = u;
    resetPassword = generatePassword();
    resetOpen = true;
  }
  async function doReset() {
    if (!resetUser) return;
    const u = resetUser;
    await act(() => api.post(`/admin/users/${u.id}/reset-password`, { password: resetPassword }),
      `Пароль ${u.display_name} изменён: ${resetPassword}. Все сеансы пользователя завершены.`);
    if (!error) resetOpen = false;
  }

  function resetMfa(u: UserItem) {
    if (!confirm(`Отключить двухфакторную защиту у ${u.display_name}? Пользователь сможет войти только по паролю и настроить 2FA заново.`)) return;
    act(() => api.post(`/admin/users/${u.id}/reset-mfa`), `2FA пользователя ${u.display_name} отключена.`);
  }

  function revoke(u: UserItem) {
    if (!confirm(`Завершить все сеансы ${u.display_name}? Пользователю придётся войти заново.`)) return;
    act(() => api.post(`/admin/users/${u.id}/revoke-sessions`), `Сеансы ${u.display_name} завершены.`);
  }

  function update(u: UserItem, patch: Partial<Pick<UserItem, 'role' | 'is_active' | 'crm_role' | 'finance_role'>>) {
    act(() => api.patch(`/users/${u.id}`, patch), 'Изменения сохранены.');
  }

  function copy(text: string) {
    navigator.clipboard?.writeText(text).catch(() => {});
  }
</script>

<div class="row toolbar">
  <div class="seg" role="tablist" aria-label="Пользователи портала">
    <button role="tab" aria-selected={tab === 'people'} class:on={tab === 'people'} onclick={() => setTab('people')}>Сотрудники <span class="n">{people.length}</span></button>
    <button role="tab" aria-selected={tab === 'invites'} class:on={tab === 'invites'} onclick={() => setTab('invites')}>Приглашения{#if invitations.length}<span class="n">{invitations.length}</span>{/if}</button>
    <button role="tab" aria-selected={tab === 'requests'} class:on={tab === 'requests'} onclick={() => setTab('requests')}>Заявки{#if registration?.pending}<span class="n hot">{registration.pending}</span>{/if}</button>
  </div>
  <div class="spacer"></div>
  <button class="btn" onclick={openCreate}>Создать с паролем</button>
  <button class="btn primary" onclick={() => (inviteOpen = true)}><UserPlus size={16} />Пригласить</button>
</div>
{#if error && !createOpen}<div class="error-box">{error}</div>{/if}

{#if tab === 'invites'}
  <InvitationsPanel items={invitations} onchange={() => load()} />
{:else if tab === 'requests'}
  <RequestsPanel settings={registration} onchange={() => load()} />
{:else}
{#if notice}<div class="ok-box">{notice}</div>{/if}
<input class="input search" placeholder="Поиск по имени или email" bind:value={filter} aria-label="Поиск" />
<div class="card scroll">
  <table class="list">
    <thead><tr><th>Имя</th><th>Email</th><th>Роль</th><th>CRM</th>{#if session.has('finance')}<th>Финансы</th>{/if}<th>2FA</th><th>Создан</th><th></th></tr></thead>
    <tbody>
      {#each shown as u (u.id)}
        <tr class:inactive={!u.is_active}>
          <td class="who"><a href="/team/{u.id}">{u.display_name}</a>{#if u.invite_pending}{' '}<span class="badge warn" title="Пользователь ещё не принял приглашение">приглашён</span>{/if}{#if !u.is_active}{' '}<span class="badge grey">заблокирован</span>{/if}</td>
          <td class="muted">{u.email}</td>
          <td>
            {#if manageable(u)}
              <select class="input sm" value={u.role} onchange={(e) => update(u, { role: e.currentTarget.value })} aria-label="Роль">
                {#if session.me?.role === 'owner'}<option value="admin">Администратор</option>{/if}
                <option value="member">Сотрудник</option><option value="guest">Гость</option>
              </select>
            {:else}{roleLabel[u.role]}{/if}
          </td>
          <td>
            {#if u.role === 'guest'}<span class="faint">—</span>
            {:else if session.me?.role === 'owner' || session.me?.role === 'admin'}
              <select class="input sm" value={u.crm_role} onchange={(e) => update(u, { crm_role: e.currentTarget.value as UserItem['crm_role'] })} aria-label="Доступ к CRM">
                {#if u.role === 'owner' || u.role === 'admin'}
                  <option value="none">{crmRoleLabel.none}</option><option value="user">Полный (как администратор)</option>
                  {#if u.crm_role === 'manager'}<option value="manager">Полный (как администратор)</option>{/if}
                {:else}
                  {#each Object.entries(crmRoleLabel) as [v, l]}<option value={v}>{l}</option>{/each}
                {/if}
              </select>
            {:else}{crmRoleLabel[u.crm_role]}{/if}
          </td>
          {#if session.has('finance')}
            <td>
              {#if u.role === 'guest'}<span class="faint">—</span>
              {:else if u.role === 'owner' || u.role === 'admin'}<span class="muted small">полный</span>
              {:else}
                <select class="input sm" value={u.finance_role} onchange={(e) => update(u, { finance_role: e.currentTarget.value as UserItem['finance_role'] })} aria-label="Доступ к финансам">
                  <option value="none">Нет</option><option value="view">Просмотр</option><option value="edit">Внесение платежей</option>
                </select>
              {/if}
            </td>
          {/if}
          <td>{#if u.totp_enabled}<span class="badge ok">вкл.</span>{:else}<span class="badge grey">нет</span>{/if}</td>
          <td class="muted">{formatDate(u.created_at)}</td>
          <td class="actions">
            {#if manageable(u)}
              <button class="btn ghost sm" onclick={() => openReset(u)}>{u.invite_pending ? 'Задать пароль' : 'Сбросить пароль'}</button>
              {#if u.totp_enabled}<button class="btn ghost sm" onclick={() => resetMfa(u)}>Сбросить 2FA</button>{/if}
              <button class="btn ghost sm" onclick={() => revoke(u)}>Завершить сеансы</button>
              <button class="btn ghost sm" class:danger={u.is_active} onclick={() => update(u, { is_active: !u.is_active })}>
                {u.is_active ? 'Заблокировать' : 'Разблокировать'}
              </button>
            {/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
{/if}

<InviteDialog bind:open={inviteOpen} ondone={() => load()} />

<Modal bind:open={createOpen} title="Новый пользователь с паролем">
  <label class="field">Имя <input class="input" bind:value={form.display_name} /></label>
  <label class="field">Email <input class="input" type="email" bind:value={form.email} /></label>
  <p class="faint small">Учётная запись сразу активна. Передайте пароль сотруднику лично — при первом входе его лучше сменить. Чтобы человек задал пароль сам, используйте «Пригласить».</p>
  <label class="field">Начальный пароль
    <div class="row">
      <input class="input mono" bind:value={form.password} />
      <button class="btn" type="button" onclick={() => (form.password = generatePassword())}>Новый</button>
      <button class="btn" type="button" onclick={() => copy(form.password)}>Копировать</button>
    </div>
  </label>
  <label class="field">Роль
    <select class="input" bind:value={form.role}>
      <option value="member">Сотрудник</option>
      {#if session.me?.role === 'owner'}<option value="admin">Администратор</option>{/if}
      <option value="guest">Гость</option>
    </select>
  </label>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (createOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!form.email || !form.display_name || form.password.length < 10} onclick={create}>Создать</button>
  {/snippet}
</Modal>

<Modal bind:open={resetOpen} title={`Сброс пароля: ${resetUser?.display_name ?? ''}`}>
  <p class="muted">Пользователь получит новый пароль, все его текущие сеансы будут завершены.</p>
  <label class="field">Новый пароль
    <div class="row">
      <input class="input mono" bind:value={resetPassword} />
      <button class="btn" type="button" onclick={() => (resetPassword = generatePassword())}>Новый</button>
      <button class="btn" type="button" onclick={() => copy(resetPassword)}>Копировать</button>
    </div>
  </label>
  {#snippet footer()}
    <button class="btn" onclick={() => (resetOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={resetPassword.length < 10} onclick={doReset}>Сменить пароль</button>
  {/snippet}
</Modal>

<style>
  .who a { white-space: nowrap; }
  .toolbar { margin-bottom: 12px; flex-wrap: wrap; gap: 8px; }
  .search { max-width: 320px; margin-bottom: 12px; }
  .n { font-size: 11.5px; padding: 0 6px; border-radius: 999px; background: var(--surface-2); color: var(--text-3); line-height: 18px; }
  .n.hot { background: var(--danger); color: #fff; }
  .ok-box, .error-box { margin-bottom: 12px; }
  .scroll { overflow-x: auto; }
  .input.sm { height: 26px; width: auto; padding: 0 6px; }
  .actions { text-align: right; white-space: nowrap; }
  tr.inactive td:not(.actions) { opacity: .55; }
  p { margin: 0; }
</style>
