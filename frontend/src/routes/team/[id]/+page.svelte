<script lang="ts">
  // Employee card: main data, requisites for documents, access, projects and
  // security. The person edits their own data; administrators edit anyone's
  // and manage access. Colleagues see the work card and contacts.
  import { page } from '$app/state';
  import { beforeNavigate, goto } from '$app/navigation';
  import Camera from '@lucide/svelte/icons/camera';
  import FileSignature from '@lucide/svelte/icons/file-signature';
  import FolderKanban from '@lucide/svelte/icons/folder-kanban';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import Mail from '@lucide/svelte/icons/mail';
  import MessageSquare from '@lucide/svelte/icons/message-square';
  import Phone from '@lucide/svelte/icons/phone';
  import Send from '@lucide/svelte/icons/send';
  import Settings2 from '@lucide/svelte/icons/settings-2';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import Upload from '@lucide/svelte/icons/upload';
  import UserRound from '@lucide/svelte/icons/user-round';
  import Avatar from '$lib/Avatar.svelte';
  import Modal from '$lib/Modal.svelte';
  import { api, crmRoleLabel, formatDate, formatDateTime, plainMessage, roleLabel, timeAgo, upload, type UserItem } from '$lib/api';
  import { avatars } from '$lib/avatars.svelte';
  import { session } from '$lib/session.svelte';
  import PhotoCrop from '$lib/people/PhotoCrop.svelte';
  import UserFieldsEditor from '$lib/people/UserFieldsEditor.svelte';
  import { generatePassword, validPhone } from '$lib/people/password';
  import type { Profile, UserFieldDef } from '$lib/people/types';

  type Tab = 'main' | 'docs' | 'access' | 'work' | 'security';

  const id = $derived(page.params.id === 'me' ? (session.me?.id ?? '') : (page.params.id ?? ''));
  let data = $state<Profile | null>(null);
  let error = $state('');
  let notice = $state('');
  let loadError = $state('');
  let tab = $state<Tab>('main');
  let saving = $state(false);
  let photoOpen = $state(false);
  let fieldsOpen = $state(false);

  // Editable copy of the card.
  type Form = Record<string, string>;
  const KEYS = ['last_name', 'first_name', 'middle_name', 'display_name', 'birth_date', 'position', 'department', 'manager_id', 'hired_on',
    'phone', 'phone_mobile', 'telegram', 'doc_short_name', 'doc_position', 'name_genitive', 'name_dative', 'nrs_number', 'qual_cert',
    'power_of_attorney'] as const;
  let f = $state<Form>({});
  let orig = $state<Form>({});
  let custom = $state<Record<string, string>>({});
  let origCustom = $state<Record<string, string>>({});
  let people = $state<UserItem[]>([]);
  let allGroups = $state<{ id: string; name: string }[]>([]);
  let fieldDefs = $derived<UserFieldDef[]>((data?.custom ?? []).map(({ id, label, field_type }) => ({ id, label, field_type })));

  async function load() {
    if (!id) return;
    loadError = '';
    try {
      const d = await api.get<Profile>(`/users/${id}/profile`);
      data = d;
      const u = d.user as unknown as Record<string, unknown>;
      const next: Form = {};
      for (const k of KEYS) next[k] = (u[k] as string | null | undefined) ?? '';
      f = { ...next };
      orig = next;
      custom = Object.fromEntries(d.custom.map((c) => [c.id, c.value]));
      origCustom = { ...custom };
      if (!['main', 'work'].includes(tab) && !d.can_edit) tab = 'main';
    } catch (e) {
      loadError = plainMessage((e as Error).message);
    }
  }
  $effect(() => { if (id) load(); });
  $effect(() => {
    if (!data?.can_edit) return;
    api.get<{ users: UserItem[] }>('/users').then((r) => (people = r.users.filter((u) => u.is_active))).catch(() => {});
    if (data.can_admin) api.get<{ groups: { id: string; name: string }[] }>('/groups').then((r) => (allGroups = r.groups)).catch(() => {});
  });

  const self = $derived(!!data && data.user.id === session.me?.id);
  const dirty = $derived(KEYS.some((k) => (f[k] ?? '') !== (orig[k] ?? '')) || Object.keys(custom).some((k) => custom[k] !== origCustom[k]));
  // Field checks before saving (the server checks the same).
  const errs = $derived.by(() => {
    const e: Record<string, string> = {};
    if (f.phone && !validPhone(f.phone)) e.phone = 'Цифры, пробелы, «+», «-», скобки и «доб.» — не email';
    if (f.phone_mobile && !validPhone(f.phone_mobile)) e.phone_mobile = 'Цифры, пробелы, «+», «-» и скобки';
    const tg = (f.telegram ?? '').replace(/^https:\/\/t\.me\//, '').replace(/^@/, '');
    if (tg && !/^[A-Za-z0-9_]{4,32}$/.test(tg)) e.telegram = 'Имя пользователя латиницей, например @ivanov';
    if (f.birth_date && f.birth_date > new Date().toISOString().slice(0, 10)) e.birth_date = 'Дата ещё не наступила';
    if (!(f.display_name ?? '').trim() && !(f.first_name ?? '').trim() && !(f.last_name ?? '').trim()) e.display_name = 'Укажите имя';
    return e;
  });
  const hasErrs = $derived(Object.keys(errs).length > 0);
  const shortSuggest = $derived.by(() => {
    const init = (s: string) => (s.trim() ? s.trim()[0] + '.' : '');
    const i = [init(f.first_name ?? ''), init(f.middle_name ?? '')].filter(Boolean).join(' ');
    return f.last_name?.trim() ? `${f.last_name.trim()}${i ? ' ' + i : ''}` : '';
  });

  async function save() {
    if (!data || hasErrs) return;
    saving = true;
    error = '';
    notice = '';
    try {
      const body: Record<string, unknown> = {};
      for (const k of KEYS) if ((f[k] ?? '') !== (orig[k] ?? '')) body[k] = f[k] ?? '';
      const ch = Object.fromEntries(Object.entries(custom).filter(([k, v]) => v !== origCustom[k]));
      if (Object.keys(ch).length) body.custom = ch;
      await api.put(`/users/${data.user.id}/profile`, body);
      if (self) await session.load();
      notice = 'Сохранено';
      await load();
    } catch (e) {
      error = plainMessage((e as Error).message);
    } finally {
      saving = false;
    }
  }
  beforeNavigate((nav) => {
    if (dirty && !saving && !confirm('Изменения в карточке не сохранены. Уйти со страницы?')) nav.cancel();
  });

  async function write() {
    try {
      const r = await api.post<{ id: string }>('/chats/direct', { user_id: data!.user.id });
      goto(`/chats?c=${r.id}`);
    } catch (e) {
      error = plainMessage((e as Error).message);
    }
  }
  async function savePhoto(file: File) {
    const r = await upload<{ at: string }>(`/users/${data!.user.id}/avatar`, file);
    avatars.set(data!.user.id, r.at);
    await load();
  }
  async function removePhoto() {
    await api.del(`/users/${data!.user.id}/avatar`);
    avatars.set(data!.user.id, null);
    await load();
  }
  let sigInput = $state<HTMLInputElement>();
  async function pickSignature(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    (e.currentTarget as HTMLInputElement).value = '';
    if (!file) return;
    error = '';
    if (file.type !== 'image/png') { error = 'Подпись — PNG с прозрачным фоном'; return; }
    try {
      await upload(`/users/${data!.user.id}/signature`, file);
      await load();
    } catch (err) {
      error = plainMessage((err as Error).message);
    }
  }
  async function removeSignature() {
    if (!confirm('Удалить образец подписи?')) return;
    await api.del(`/users/${data!.user.id}/signature`);
    await load();
  }

  // --- Access (administrators): saved at once -----------------------------------------
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
  const patch = (body: Record<string, unknown>, ok = 'Доступ изменён') => act(() => api.patch(`/users/${data!.user.id}`, body), ok);
  let rate = $state('');
  $effect(() => { rate = data?.user.hour_rate != null ? String(data.user.hour_rate) : ''; });
  function saveRate() {
    const v = rate.trim() === '' ? null : Number(rate.replace(',', '.').replace(/\s/g, ''));
    if (v !== null && (!Number.isFinite(v) || v < 0)) { error = 'Ставка: число от 0'; return; }
    act(() => api.put(`/time/rates/${data!.user.id}`, { hour_rate: v }), 'Ставка сохранена');
  }
  let emailEdit = $state('');
  let emailOpen = $state(false);
  function toggleGroup(gid: string, on: boolean) {
    const ids = new Set(data!.groups.map((g) => g.id));
    if (on) ids.add(gid); else ids.delete(gid);
    act(() => api.put(`/users/${data!.user.id}/groups`, { group_ids: [...ids] }), 'Группы сохранены');
  }

  // --- Security (administrators) ---------------------------------------------------------
  let pwOpen = $state(false);
  let newPw = $state('');
  function resetPw() {
    act(() => api.post(`/admin/users/${data!.user.id}/reset-password`, { password: newPw }),
      `Новый пароль: ${newPw}. Передайте его сотруднику; все его сеансы завершены.`).then(() => (pwOpen = false));
  }

  const canEditRole = $derived(!!data && data.can_admin && !self && data.user.role !== 'owner' && (session.me?.role === 'owner' || data.user.role !== 'admin'));
  const financeLabel: Record<string, string> = { none: 'Нет доступа', view: 'Просмотр', edit: 'Просмотр и изменение' };
  const projectRole: Record<string, string> = { manager: 'Руководитель', member: 'Участник', viewer: 'Наблюдатель' };
  const statusLabel: Record<string, string> = { active: 'активный', paused: 'приостановлен', closed: 'закрыт' };
  const tabs = $derived(
    ([
      ['main', 'Основное', UserRound, true],
      ['docs', 'Реквизиты для документов', FileSignature, !!data?.can_edit],
      ['access', 'Доступы', KeyRound, !!data?.can_edit],
      ['work', 'Проекты и задачи', FolderKanban, true],
      ['security', 'Безопасность', ShieldCheck, !!data?.can_edit]
    ] as const).filter((t) => t[3])
  );
  const tel = (p: string) => 'tel:' + p.split(/доб/i)[0].replace(/[^\d+]/g, '');
  const seen = $derived(data?.user.last_seen_at ? `был(а) ${timeAgo(data.user.last_seen_at)}` : '');
</script>

<svelte:head><title>{data?.user.display_name ?? 'Сотрудник'}</title></svelte:head>

<div class="page">
  <nav class="crumbs small"><a href="/team">Команда</a> › {data?.user.display_name ?? '…'}</nav>
  {#if loadError}
    <div class="error-box">{loadError}</div>
  {:else if data}
    {@const u = data.user}
    <section class="card hero">
      <div class="photo">
        <Avatar id={u.id} name={u.display_name} size={84} />
        {#if data.can_edit}
          <button class="cam" onclick={() => (photoOpen = true)} title="Изменить фото" aria-label="Изменить фото"><Camera size={15} /></button>
        {/if}
      </div>
      <div class="who">
        <h1>{u.display_name}</h1>
        <div class="sub">{[u.position, u.department].filter(Boolean).join(' · ') || (data.can_edit ? 'Должность не указана' : '')}</div>
        <div class="badges">
          <span class="badge">{roleLabel[u.role]}</span>
          {#if !u.is_active}<span class="badge grey">заблокирован</span>{/if}
          {#if u.invite_pending}<span class="badge warn">приглашён</span>{/if}
          {#if data.can_edit}
            {#if u.crm_role !== 'none'}<span class="badge grey">CRM: {crmRoleLabel[u.crm_role].toLowerCase()}</span>{/if}
            {#if u.finance_role !== 'none'}<span class="badge grey">Финансы: {financeLabel[u.finance_role].toLowerCase()}</span>{/if}
            {#if u.totp_enabled}<span class="badge ok">2FA включена</span>{/if}
          {/if}
          {#if seen}<span class="faint small">{seen}</span>{/if}
        </div>
      </div>
      <div class="spacer"></div>
      <div class="acts">
        {#if !self}
          {#if session.has('chats') && u.role !== 'guest' && session.me?.role !== 'guest'}<button class="btn" onclick={write}><MessageSquare size={16} />Написать</button>{/if}
          <a class="btn" href="mailto:{u.email}"><Mail size={16} />Почта</a>
        {/if}
        {#if data.can_edit && (tab === 'main' || tab === 'docs')}
          <button class="btn primary" disabled={!dirty || saving || hasErrs} onclick={save}>{saving ? 'Сохранение…' : 'Сохранить'}</button>
        {/if}
      </div>
    </section>

    <nav class="tabs-line" aria-label="Разделы карточки">
      {#each tabs as [k, l, Icon] (k)}
        <button class:on={tab === k} aria-current={tab === k ? 'page' : undefined} onclick={() => (tab = k)}><Icon size={17} />{l}</button>
      {/each}
    </nav>

    {#if error}<div class="error-box">{error}</div>{/if}
    {#if notice}<div class="ok-box">{notice}</div>{/if}

    {#if tab === 'main'}
      {#if data.can_edit}
        <div class="g2">
          <section class="card pad">
            <h2>Личные данные</h2>
            <div class="fg">
              <label class="field">Фамилия <input class="input" bind:value={f.last_name} maxlength="60" autocomplete="family-name" /></label>
              <label class="field">Имя <input class="input" bind:value={f.first_name} maxlength="60" autocomplete="given-name" /></label>
              <label class="field">Отчество <input class="input" bind:value={f.middle_name} maxlength="60" autocomplete="additional-name" /></label>
              <label class="field">Дата рождения <input class="input" type="date" bind:value={f.birth_date} max={new Date().toISOString().slice(0, 10)} />
                {#if errs.birth_date}<span class="ferr">{errs.birth_date}</span>{:else}<span class="hint">коллеги видят только день и месяц</span>{/if}</label>
              <label class="field wide">Как показывать в портале <input class="input" bind:value={f.display_name} maxlength="100" placeholder={[f.first_name, f.last_name].filter(Boolean).join(' ')} autocomplete="off" />
                {#if errs.display_name}<span class="ferr">{errs.display_name}</span>{:else}<span class="hint">пусто — «Имя Фамилия»</span>{/if}</label>
            </div>
          </section>
          <section class="card pad">
            <h2>Работа и контакты</h2>
            <div class="fg">
              <label class="field">Должность <input class="input" bind:value={f.position} maxlength="100" placeholder="например, ведущий инженер" autocomplete="organization-title" /></label>
              <label class="field">Отдел <input class="input" bind:value={f.department} maxlength="100" list="departments" autocomplete="off" /></label>
              <label class="field">Рабочий телефон <input class="input" type="tel" bind:value={f.phone} maxlength="40" placeholder="+7 4242 49-00-12, доб. 214" autocomplete="off" aria-invalid={!!errs.phone} />
                {#if errs.phone}<span class="ferr">{errs.phone}</span>{/if}</label>
              <label class="field">Мобильный <input class="input" type="tel" bind:value={f.phone_mobile} maxlength="40" placeholder="+7 900 000-00-00" autocomplete="off" aria-invalid={!!errs.phone_mobile} />
                {#if errs.phone_mobile}<span class="ferr">{errs.phone_mobile}</span>{/if}</label>
              <label class="field">Email (вход)
                <span class="ro-row"><input class="input" value={u.email} readonly aria-readonly="true" />
                  {#if data.can_admin && !(u.role === 'owner' && session.me?.role !== 'owner')}<button class="btn sm" onclick={() => { emailEdit = u.email; emailOpen = true; }}>Изменить</button>{/if}</span></label>
              <label class="field">Telegram <input class="input" bind:value={f.telegram} maxlength="40" placeholder="@ivanov" autocomplete="off" aria-invalid={!!errs.telegram} />
                {#if errs.telegram}<span class="ferr">{errs.telegram}</span>{/if}</label>
              <label class="field">Руководитель
                <select class="input" bind:value={f.manager_id}>
                  <option value="">—</option>
                  {#each people.filter((p) => p.id !== u.id) as p (p.id)}<option value={p.id}>{p.display_name}</option>{/each}
                </select></label>
              <label class="field">В компании с <input class="input" type="date" bind:value={f.hired_on} /></label>
            </div>
          </section>
        </div>
      {:else}
        <section class="card pad view">
          <dl>
            {#if u.position}<dt>Должность</dt><dd>{u.position}</dd>{/if}
            {#if u.department}<dt>Отдел</dt><dd>{u.department}</dd>{/if}
            {#if u.manager_name}<dt>Руководитель</dt><dd><a href="/team/{u.manager_id}">{u.manager_name}</a></dd>{/if}
            <dt>Email</dt><dd><a href="mailto:{u.email}">{u.email}</a></dd>
            {#if u.phone}<dt>Рабочий телефон</dt><dd><a href={tel(u.phone)}><Phone size={14} /> {u.phone}</a></dd>{/if}
            {#if u.phone_mobile}<dt>Мобильный</dt><dd><a href={tel(u.phone_mobile)}><Phone size={14} /> {u.phone_mobile}</a></dd>{/if}
            {#if u.telegram}<dt>Telegram</dt><dd><a href="https://t.me/{u.telegram.slice(1)}" target="_blank" rel="noopener noreferrer"><Send size={14} /> {u.telegram}</a></dd>{/if}
            {#if u.birthday}<dt>День рождения</dt><dd>{u.birthday}</dd>{/if}
            {#if u.hired_on}<dt>В компании с</dt><dd>{formatDate(u.hired_on)}</dd>{/if}
            {#if data.groups.length}<dt>Группы</dt><dd>{data.groups.map((g) => g.name).join(', ')}</dd>{/if}
          </dl>
        </section>
      {/if}
    {:else if tab === 'docs'}
      <div class="g2">
        <section class="card pad">
          <h2>Реквизиты для документов <span class="faint small">подставляются в шаблоны: «{'{user.short_name}'}», «{'{user.position}'}» и др.</span></h2>
          <div class="fg">
            <label class="field">Фамилия и инициалы <input class="input" bind:value={f.doc_short_name} maxlength="80" placeholder={shortSuggest} autocomplete="off" />
              {#if !f.doc_short_name && shortSuggest}<span class="hint">пусто — «{shortSuggest}»</span>{/if}</label>
            <label class="field">Должность в документах <input class="input" bind:value={f.doc_position} maxlength="150" placeholder={f.position || 'например, ГИП'} autocomplete="off" /></label>
            <label class="field">ФИО в родительном падеже <input class="input" bind:value={f.name_genitive} maxlength="150" placeholder="кого: Петровой Анны Сергеевны" autocomplete="off" /></label>
            <label class="field">ФИО в дательном падеже <input class="input" bind:value={f.name_dative} maxlength="150" placeholder="кому: Петровой Анне Сергеевне" autocomplete="off" /></label>
            <label class="field">Номер в НРС (НОПРИЗ) <input class="input" bind:value={f.nrs_number} maxlength="40" placeholder="П-123456" autocomplete="off" /></label>
            <label class="field">Квалификационный аттестат <input class="input" bind:value={f.qual_cert} maxlength="200" placeholder="номер и срок действия" autocomplete="off" /></label>
            <label class="field wide">Доверенность <input class="input" bind:value={f.power_of_attorney} maxlength="300" placeholder="№ 14 от 10.01.2026, действует до 31.12.2026" autocomplete="off" /></label>
          </div>
          <div class="sig">
            <span class="lbl">Образец подписи</span>
            <input bind:this={sigInput} type="file" accept="image/png" hidden onchange={pickSignature} />
            {#if u.signature_at}
              <img src="/api/users/{u.id}/signature?v={u.signature_at}" alt="Образец подписи" />
              <div class="row"><button class="btn sm" onclick={() => sigInput?.click()}><Upload size={14} />Заменить</button>
                <button class="btn sm ghost" onclick={removeSignature}><Trash2 size={14} />Удалить</button></div>
            {:else}
              <button class="drop" onclick={() => sigInput?.click()}><Upload size={18} />Загрузить PNG с прозрачным фоном</button>
            {/if}
            <span class="hint">видят только сотрудник и администраторы; пригодится для штампов</span>
          </div>
        </section>
        <section class="card pad">
          <h2>Свои реквизиты
            {#if data.can_admin}<button class="btn sm" onclick={() => (fieldsOpen = true)}><Settings2 size={14} />Настроить поля</button>{/if}</h2>
          {#if data.custom.length}
            <div class="fg">
              {#each data.custom as c (c.id)}
                <label class="field" class:wide={c.field_type === 'textarea'}>{c.label}
                  {#if c.field_type === 'textarea'}<textarea class="input" rows="3" bind:value={custom[c.id]} maxlength="2000"></textarea>
                  {:else if c.field_type === 'date'}<input class="input" type="date" bind:value={custom[c.id]} />
                  {:else if c.field_type === 'number'}<input class="input" inputmode="decimal" bind:value={custom[c.id]} />
                  {:else}<input class="input" bind:value={custom[c.id]} maxlength="300" autocomplete="off" />{/if}
                </label>
              {/each}
            </div>
          {:else}
            <p class="muted small">{data.can_admin ? 'Добавьте поля, которые нужны вашей организации: удостоверения, допуски, пропуска.' : 'Администратор ещё не добавил своих полей.'}</p>
          {/if}
        </section>
      </div>
    {:else if tab === 'access'}
      <section class="card pad narrowcard">
        <h2>Доступы {#if !data.can_admin}<span class="faint small">меняет администратор</span>{/if}</h2>
        <div class="arow"><span>Роль в портале</span>
          {#if canEditRole}
            <select class="input auto" value={u.role} onchange={(e) => patch({ role: e.currentTarget.value })}>
              {#if session.me?.role === 'owner'}<option value="admin">Администратор</option>{/if}
              <option value="member">Сотрудник</option><option value="guest">Гость</option>
            </select>
          {:else}<b>{roleLabel[u.role]}</b>{/if}</div>
        {#if session.has('crm')}
          <div class="arow"><span>CRM</span>
            {#if data.can_admin && u.role !== 'guest'}
              <select class="input auto" value={u.crm_role} onchange={(e) => patch({ crm_role: e.currentTarget.value })}>
                {#each Object.entries(crmRoleLabel) as [k, l]}<option value={k}>{l}</option>{/each}
              </select>
            {:else}<b>{crmRoleLabel[u.crm_role]}</b>{/if}</div>
        {/if}
        {#if session.has('finance')}
          <div class="arow"><span>Финансы</span>
            {#if data.can_admin && !['owner', 'admin', 'guest'].includes(u.role)}
              <select class="input auto" value={u.finance_role} onchange={(e) => patch({ finance_role: e.currentTarget.value })}>
                {#each Object.entries(financeLabel) as [k, l]}<option value={k}>{l}</option>{/each}
              </select>
            {:else}<b>{['owner', 'admin'].includes(u.role) ? 'Полный (администратор)' : financeLabel[u.finance_role]}</b>{/if}</div>
        {/if}
        {#if data.rate_visible}
          <div class="arow"><span>Ставка, ₽/ч</span>
            {#if session.isAdmin || session.financeEdit}
              <span class="ro-row"><input class="input rate" inputmode="decimal" bind:value={rate} placeholder="не задана" />
                <button class="btn sm" disabled={rate === (u.hour_rate != null ? String(u.hour_rate) : '')} onclick={saveRate}>Сохранить</button></span>
            {:else}<b>{u.hour_rate != null ? u.hour_rate.toLocaleString('ru-RU') : 'не задана'}</b>{/if}</div>
        {/if}
        <div class="arow top"><span>Группы</span>
          <div class="chips">
            {#if data.can_admin}
              {#each allGroups as g (g.id)}
                {@const on = data.groups.some((x) => x.id === g.id)}
                <button class="chip" class:on onclick={() => toggleGroup(g.id, !on)} aria-pressed={on}>{g.name}</button>
              {:else}<span class="faint small">Групп нет — <a href="/admin/groups">создать</a></span>{/each}
            {:else}
              {#each data.groups as g (g.id)}<span class="chip on static">{g.name}</span>{:else}<span class="faint small">нет</span>{/each}
            {/if}
          </div></div>
        <div class="arow"><span>Учётная запись</span>
          <b class:ok-text={u.is_active} class:danger-text={!u.is_active}>{u.is_active ? 'Активна' : 'Заблокирована'}</b></div>
        <p class="faint small foot">Роль в каждом проекте задаётся в самом проекте («Участники»); список — на вкладке «Проекты и задачи».</p>
      </section>
    {:else if tab === 'work'}
      <div class="stat-grid">
        <div class="card pad mini"><span>Открытых задач</span><b>{data.stats.open_tasks}</b></div>
        <div class="card pad mini"><span>Просрочено</span><b class:danger-text={data.stats.overdue_tasks > 0}>{data.stats.overdue_tasks}</b></div>
        {#if data.stats.hours_month != null}<div class="card pad mini"><span>Часов в этом месяце</span><b>{(data.stats.hours_month / 60).toLocaleString('ru-RU', { maximumFractionDigits: 1 })}</b></div>{/if}
        <div class="card pad mini"><span>Проектов</span><b>{data.projects.filter((p) => p.status !== 'closed').length}</b></div>
      </div>
      <section class="card">
        {#if data.projects.length}
          <table class="ptable">
            <thead><tr><th>Проект</th><th>Роль</th><th>Статус</th></tr></thead>
            <tbody>
              {#each data.projects as p (p.id)}
                <tr class:closed={p.status === 'closed'}><td><a href="/projects/{p.id}">{p.name}</a></td><td>{projectRole[p.role]}</td><td class="faint">{statusLabel[p.status]}</td></tr>
              {/each}
            </tbody>
          </table>
          <p class="faint small foot pad">Задачи сотрудника — в разделе <a href="/tasks?assignee={u.id}">«Задачи»</a>.</p>
        {:else}
          <p class="muted pad">{data.can_admin || self ? 'Сотрудник не участвует в проектах.' : 'Общих с вами проектов нет.'}</p>
        {/if}
      </section>
    {:else if tab === 'security'}
      <section class="card pad narrowcard">
        <h2>Безопасность</h2>
        <div class="arow"><span>Двухфакторная защита</span><b class:ok-text={u.totp_enabled}>{u.totp_enabled ? 'Включена' : 'Не включена'}</b></div>
        <div class="arow"><span>Последний визит</span><b>{u.last_seen_at ? formatDateTime(u.last_seen_at) : '—'}</b></div>
        <div class="arow"><span>В портале с</span><b>{formatDate(u.created_at)}</b></div>
        {#if self}
          <p class="small">Пароль и двухфакторная защита — в разделе <a href="/settings">«Профиль и безопасность»</a>.</p>
        {/if}
        {#if data.can_admin && !self && u.role !== 'owner'}
          <div class="admin-acts">
            {#if u.invite_pending}<button class="btn" onclick={() => act(() => api.post(`/admin/users/${u.id}/invite`), 'Приглашение отправлено повторно')}><Send size={15} />Отправить приглашение ещё раз</button>{/if}
            <button class="btn" onclick={() => { newPw = generatePassword(); pwOpen = true; }}><KeyRound size={15} />Задать новый пароль</button>
            {#if u.totp_enabled}<button class="btn" onclick={() => confirm(`Отключить 2FA у ${u.display_name}?`) && act(() => api.post(`/admin/users/${u.id}/reset-mfa`), '2FA отключена')}>Отключить 2FA</button>{/if}
            <button class="btn" onclick={() => confirm(`Завершить все сеансы ${u.display_name}?`) && act(() => api.post(`/admin/users/${u.id}/revoke-sessions`), 'Сеансы завершены')}>Завершить все сеансы</button>
            {#if canEditRole || session.me?.role === 'owner'}
              <button class="btn" class:danger={u.is_active} onclick={() => confirm(u.is_active ? `Заблокировать ${u.display_name}? Он не сможет войти.` : `Разблокировать ${u.display_name}?`) && patch({ is_active: !u.is_active }, u.is_active ? 'Учётная запись заблокирована' : 'Учётная запись разблокирована')}>
                {u.is_active ? 'Заблокировать' : 'Разблокировать'}</button>
            {/if}
          </div>
        {/if}
      </section>
    {/if}

    <datalist id="departments">{#each [...new Set(people.map((p) => (p as unknown as { department?: string }).department).filter(Boolean))] as d}<option value={d}></option>{/each}</datalist>
    <PhotoCrop bind:open={photoOpen} onsave={savePhoto} onremove={u.avatar_at ? removePhoto : undefined} />
    {#if data.can_admin}<UserFieldsEditor bind:open={fieldsOpen} fields={fieldDefs} onsaved={load} />{/if}
  {:else}
    <p class="muted">Загрузка…</p>
  {/if}
</div>

<Modal title="Email для входа" bind:open={emailOpen} width={420}>
  <label class="field">Новый адрес <input class="input" type="email" bind:value={emailEdit} autocomplete="off" /></label>
  <p class="faint small">Сотрудник будет входить с новым адресом; уведомления придут на него же.</p>
  {#snippet footer()}
    <button class="btn" onclick={() => (emailOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!emailEdit.includes('@') || emailEdit === data?.user.email}
      onclick={() => { emailOpen = false; patch({ email: emailEdit.trim() }, 'Email изменён'); }}>Сохранить</button>
  {/snippet}
</Modal>

<Modal title="Новый пароль" bind:open={pwOpen} width={420}>
  <p class="small">Пароль для {data?.user.display_name}. Передайте его лично; все сеансы сотрудника будут завершены.</p>
  <label class="field">Пароль <input class="input mono" bind:value={newPw} minlength="10" autocomplete="new-password" /></label>
  {#snippet footer()}
    <button class="btn" onclick={() => (pwOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={newPw.length < 10} onclick={resetPw}>Задать пароль</button>
  {/snippet}
</Modal>

<style>
  .crumbs { color: var(--text-3); margin-bottom: 10px; }
  .crumbs a { color: var(--text-2); }
  .hero { display: flex; gap: 18px; align-items: center; padding: 18px 20px; margin-bottom: 14px; flex-wrap: wrap; }
  .photo { position: relative; flex: none; }
  .cam { position: absolute; right: -2px; bottom: -2px; width: 30px; height: 30px; border-radius: 50%; border: 1px solid var(--border);
    background: var(--surface); color: var(--text-2); display: grid; place-items: center; cursor: pointer; }
  .cam:hover { color: var(--accent); border-color: var(--accent); }
  .who { min-width: 0; }
  h1 { font-size: 22px; margin: 0; }
  .sub { color: var(--text-2); margin: 2px 0 8px; }
  .badges { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
  .spacer { flex: 1; }
  .acts { display: flex; gap: 8px; flex-wrap: wrap; }
  .g2 { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; }
  .pad { padding: 16px 18px; }
  h2 { font-size: 15px; margin: 0 0 12px; display: flex; gap: 10px; justify-content: space-between; align-items: center; flex-wrap: wrap; }
  .fg { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px 12px; }
  .field { display: grid; gap: 4px; font-size: 12.5px; color: var(--text-2); min-width: 0; }
  .field.wide { grid-column: 1 / -1; }
  .hint { font-size: 11.5px; color: var(--text-3); }
  .ferr { font-size: 11.5px; color: var(--danger); }
  [aria-invalid='true'] { border-color: var(--danger); }
  .ro-row { display: flex; gap: 6px; align-items: center; }
  .ro-row .input { flex: 1; min-width: 0; }
  input[readonly] { background: var(--surface-2); color: var(--text-2); }
  .view dl { display: grid; grid-template-columns: 180px 1fr; gap: 10px 16px; margin: 0; font-size: 14px; }
  .view dt { color: var(--text-3); }
  .view dd { margin: 0; }
  .view dd a { display: inline-flex; gap: 6px; align-items: center; }
  .sig { display: grid; gap: 6px; margin-top: 14px; justify-items: start; }
  .sig .lbl { font-size: 12.5px; color: var(--text-2); }
  .sig img { max-width: 260px; max-height: 90px; background: repeating-conic-gradient(var(--surface-2) 0 25%, var(--surface) 0 50%) 0 0 / 14px 14px;
    border: 1px solid var(--border); border-radius: 8px; padding: 6px; }
  .row { display: flex; gap: 6px; }
  .drop { display: inline-flex; gap: 8px; align-items: center; padding: 16px 18px; border: 1.5px dashed var(--border-strong, var(--border)); border-radius: 10px;
    background: var(--surface-2); color: var(--text-2); font: inherit; font-size: 13px; cursor: pointer; width: 100%; justify-content: center; }
  .drop:hover { border-color: var(--accent); color: var(--accent); }
  .narrowcard { max-width: 720px; }
  .arow { display: flex; gap: 12px; justify-content: space-between; align-items: center; padding: 10px 0; border-bottom: 1px solid var(--border); font-size: 14px; }
  .arow.top { align-items: flex-start; }
  .arow > span:first-child { color: var(--text-2); flex: none; }
  .auto { width: auto; min-width: 200px; }
  .rate { width: 120px; flex: none; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; justify-content: flex-end; }
  .chip { border: 1px solid var(--border); background: var(--surface); border-radius: 14px; padding: 3px 10px; font: inherit; font-size: 12.5px; color: var(--text-2); cursor: pointer; }
  .chip.on { border-color: var(--accent); background: var(--accent-soft); color: var(--accent); }
  .chip.static { cursor: default; }
  .foot { margin: 10px 0 0; }
  .ok-text { color: var(--ok); }
  .danger-text { color: var(--danger); }
  .admin-acts { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 14px; }
  .btn.danger { color: var(--danger); }
  .mini { display: grid; gap: 2px; }
  .mini span { font-size: 12.5px; color: var(--text-3); }
  .mini b { font-size: 21px; font-weight: 650; }
  .ptable { width: 100%; border-collapse: collapse; font-size: 13.5px; }
  .ptable th { text-align: left; font-weight: 500; color: var(--text-3); padding: 9px 16px; border-bottom: 1px solid var(--border); }
  .ptable td { padding: 9px 16px; border-bottom: 1px solid var(--border); }
  .ptable tr.closed td { opacity: .6; }
  .mono { font-family: var(--font-mono, monospace); }
  @media (max-width: 900px) {
    .g2 { grid-template-columns: minmax(0, 1fr); }
  }
  @media (max-width: 640px) {
    .fg { grid-template-columns: minmax(0, 1fr); }
    .view dl { grid-template-columns: minmax(0, 1fr); gap: 2px; }
    .view dd { margin-bottom: 8px; }
    .acts { width: 100%; }
    .arow { flex-wrap: wrap; }
    .auto { min-width: 0; width: 100%; }
  }
</style>
