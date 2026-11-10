<script lang="ts">
  import { api, crmRoleLabel, formatDate, roleLabel, type UserItem } from '$lib/api';
  import Modal from '$lib/Modal.svelte';
  import { session } from '$lib/session.svelte';

  let users = $state<UserItem[]>([]);
  let error = $state('');
  let notice = $state('');
  let filter = $state('');

  async function load() {
    users = (await api.get<{ users: UserItem[] }>('/users')).users;
  }
  $effect(() => {
    load();
  });

  const shown = $derived(
    users.filter((u) => {
      const q = filter.trim().toLowerCase();
      return !q || u.display_name.toLowerCase().includes(q) || u.email.toLowerCase().includes(q);
    })
  );

  /** Random readable password (no ambiguous characters). */
  function generatePassword(len = 14): string {
    const alphabet = 'abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789';
    const bytes = crypto.getRandomValues(new Uint32Array(len));
    return Array.from(bytes, (b) => alphabet[b % alphabet.length]).join('');
  }

  async function act(fn: () => Promise<unknown>, ok: string) {
    error = '';
    notice = '';
    try {
      await fn();
      notice = ok;
      await load();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  // Owners are untouchable; admins can be managed by the owner only; nobody manages themselves here.
  const manageable = (u: UserItem) =>
    u.id !== session.me?.id && u.role !== 'owner' && (u.role !== 'admin' || session.me?.role === 'owner');

  // --- Create ------------------------------------------------------------------
  let createOpen = $state(false);
  let form = $state({ email: '', display_name: '', password: '', role: 'member' });
  let invite = $state(true);
  let mailReady = $state(false);
  function openCreate() {
    form = { email: '', display_name: '', password: generatePassword(), role: 'member' };
    invite = true;
    error = '';
    createOpen = true;
    api.get<{ available: boolean }>('/admin/mail').then((r) => (mailReady = r.available)).catch(() => {});
  }
  // Link shown when the invitation could not be emailed (or for copying).
  let inviteLink = $state<{ name: string; url: string; emailed: boolean } | null>(null);
  async function create() {
    const name = form.display_name;
    if (invite) {
      error = '';
      notice = '';
      try {
        const r = await api.post<{ invite_url: string; emailed: boolean }>('/users', { ...form, password: null });
        createOpen = false;
        inviteLink = { name, url: r.invite_url, emailed: r.emailed };
        await load();
      } catch (e) {
        error = (e as Error).message;
      }
      return;
    }
    await act(() => api.post('/users', form), `Пользователь ${name} создан. Передайте ему пароль: ${form.password}`);
    if (!error) createOpen = false;
  }
  async function resendInvite(u: UserItem) {
    error = '';
    notice = '';
    try {
      const r = await api.post<{ invite_url: string; emailed: boolean }>(`/admin/users/${u.id}/invite`);
      inviteLink = { name: u.display_name, url: r.invite_url, emailed: r.emailed };
    } catch (e) {
      error = (e as Error).message;
    }
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

  function update(u: UserItem, patch: Partial<Pick<UserItem, 'role' | 'is_active' | 'crm_role'>>) {
    act(() => api.patch(`/users/${u.id}`, patch), 'Изменения сохранены.');
  }

  function copy(text: string) {
    navigator.clipboard?.writeText(text).catch(() => {});
  }
</script>

<div class="row toolbar">
  <input class="input search" placeholder="Поиск по имени или email" bind:value={filter} aria-label="Поиск" />
  <div class="spacer"></div>
  <button class="btn primary" onclick={openCreate}>Добавить пользователя</button>
</div>
{#if error && !createOpen}<div class="error-box">{error}</div>{/if}
{#if notice}<div class="ok-box">{notice}</div>{/if}
{#if inviteLink}
  <div class="ok-box invite">
    <div>
      {#if inviteLink.emailed}
        Приглашение для {inviteLink.name} отправлено на почту. Ссылку можно передать и вручную — она действует 7 дней:
      {:else}
        Пользователь {inviteLink.name} создан, но почта портала не настроена — письмо не отправлено.
        Передайте ссылку-приглашение сами (действует 7 дней) или <a href="/admin/mail">настройте почту</a>:
      {/if}
    </div>
    <div class="row">
      <input class="input mono" readonly value={inviteLink.url} aria-label="Ссылка-приглашение" onfocus={(e) => e.currentTarget.select()} />
      <button class="btn sm" onclick={() => copy(inviteLink!.url)}>Копировать</button>
      <button class="btn ghost sm" onclick={() => (inviteLink = null)} aria-label="Скрыть">×</button>
    </div>
  </div>
{/if}

<div class="card scroll">
  <table class="list">
    <thead><tr><th>Имя</th><th>Email</th><th>Роль</th><th>CRM</th><th>2FA</th><th>Создан</th><th></th></tr></thead>
    <tbody>
      {#each shown as u (u.id)}
        <tr class:inactive={!u.is_active}>
          <td>{u.display_name}{#if u.invite_pending}{' '}<span class="badge warn" title="Пользователь ещё не принял приглашение">приглашён</span>{/if}{#if !u.is_active}{' '}<span class="badge grey">заблокирован</span>{/if}</td>
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
          <td>{#if u.totp_enabled}<span class="badge ok">вкл.</span>{:else}<span class="badge grey">нет</span>{/if}</td>
          <td class="muted">{formatDate(u.created_at)}</td>
          <td class="actions">
            {#if manageable(u) && u.invite_pending && u.is_active}
              <button class="btn ghost sm" onclick={() => resendInvite(u)}>Пригласить снова</button>
            {/if}
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

<Modal bind:open={createOpen} title="Новый пользователь">
  <label class="field">Имя <input class="input" bind:value={form.display_name} /></label>
  <label class="field">Email <input class="input" type="email" bind:value={form.email} /></label>
  <div class="field" role="radiogroup" aria-label="Способ входа">
    <label class="check"><input type="radio" bind:group={invite} value={true} /> Пригласить по ссылке — пользователь сам задаст пароль</label>
    <label class="check"><input type="radio" bind:group={invite} value={false} /> Задать начальный пароль и передать его самому</label>
  </div>
  {#if invite}
    <p class="faint small">
      {mailReady ? 'Письмо с приглашением уйдёт на указанный адрес.' : 'Почта портала не настроена: после создания покажем ссылку, которую нужно передать пользователю.'}
    </p>
  {:else}
    <label class="field">Начальный пароль
      <div class="row">
        <input class="input mono" bind:value={form.password} />
        <button class="btn" type="button" onclick={() => (form.password = generatePassword())}>Новый</button>
        <button class="btn" type="button" onclick={() => copy(form.password)}>Копировать</button>
      </div>
    </label>
  {/if}
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
    <button class="btn primary" disabled={!form.email || !form.display_name || (!invite && form.password.length < 10)} onclick={create}>{invite ? 'Пригласить' : 'Создать'}</button>
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
  .toolbar { margin-bottom: 12px; flex-wrap: wrap; }
  .search { max-width: 320px; }
  .ok-box, .error-box { margin-bottom: 12px; }
  .scroll { overflow-x: auto; }
  .input.sm { height: 26px; width: auto; padding: 0 6px; }
  .actions { text-align: right; white-space: nowrap; }
  tr.inactive td:not(.actions) { opacity: .55; }
  p { margin: 0; }
  .invite { display: grid; gap: 8px; }
  .invite .row { gap: 6px; }
  .invite .mono { flex: 1; min-width: 0; }
  .check { display: flex; gap: 8px; align-items: center; }
</style>
