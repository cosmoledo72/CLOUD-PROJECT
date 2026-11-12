<script lang="ts">
  import Avatar from '$lib/Avatar.svelte';
  import UserRound from '@lucide/svelte/icons/user-round';
  import { goto } from '$app/navigation';
  import QRCode from 'qrcode';
  import { api, plainMessage, roleLabel } from '$lib/api';
  import NotificationPrefs from '$lib/NotificationPrefs.svelte';
  import { session } from '$lib/session.svelte';

  let error = $state('');
  let notice = $state('');

  // --- TOTP enrolment -------------------------------------------------------
  let setup = $state<{ secret: string; otpauth_url: string; qr: string } | null>(null);
  let code = $state('');
  let disablePassword = $state('');

  async function startSetup() {
    error = '';
    try {
      const r = await api.post<{ secret: string; otpauth_url: string }>('/auth/totp/setup');
      setup = { ...r, qr: await QRCode.toDataURL(r.otpauth_url, { margin: 1, width: 200 }) };
    } catch (e) {
      error = plainMessage((e as Error).message);
    }
  }
  async function enable() {
    error = '';
    try {
      await api.post('/auth/totp/enable', { code });
      setup = null;
      code = '';
      notice = 'Двухфакторная аутентификация включена.';
      await session.load();
    } catch (e) {
      error = plainMessage((e as Error).message);
    }
  }
  async function disable() {
    error = '';
    try {
      await api.post('/auth/totp/disable', { password: disablePassword });
      disablePassword = '';
      notice = 'Двухфакторная аутентификация отключена.';
      await session.load();
    } catch {
      error = 'Неверный пароль';
    }
  }

  // --- Profile card ----------------------------------------------------------
  // --- Password ---------------------------------------------------------------
  let current = $state('');
  let next = $state('');
  async function changePassword() {
    error = '';
    try {
      await api.post('/auth/password', { current_password: current, new_password: next });
      session.me = null;
      goto('/login');
    } catch (e) {
      error = plainMessage((e as Error).message);
    }
  }
</script>

<div class="page narrow">
  <div class="page-head"><h1>Профиль и безопасность</h1></div>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#if notice}<div class="badge note">{notice}</div>{/if}

  {#if session.me}
    <section class="card me-card">
      <Avatar id={session.me.id} name={session.me.display_name} size={48} />
      <div class="me-text">
        <h2>{session.me.display_name}</h2>
        <p class="muted">{session.me.email} · {roleLabel[session.me.role]} · портал «{session.me.tenant_name}»</p>
      </div>
      <a class="btn" href="/team/me"><UserRound size={16} />Моя карточка</a>
    </section>
    <p class="faint small hint">Имя, должность, телефоны, фото и реквизиты для документов — в карточке сотрудника.</p>

    <NotificationPrefs />

    <section class="card">
      <h2>Двухфакторная аутентификация</h2>
      {#if session.me.totp_enabled}
        <p>Включена. При входе запрашивается код из приложения-аутентификатора.</p>
        <div class="row">
          <input class="input" type="password" placeholder="Пароль для подтверждения" bind:value={disablePassword} autocomplete="current-password" />
          <button class="btn danger" disabled={!disablePassword} onclick={disable}>Отключить</button>
        </div>
      {:else if setup}
        <ol>
          <li>Отсканируйте QR-код в приложении (Яндекс Ключ, Google Authenticator, Aegis и т.п.).</li>
          <li>Или введите ключ вручную: <code>{setup.secret}</code></li>
          <li>Введите 6-значный код из приложения.</li>
        </ol>
        <img src={setup.qr} alt="QR-код для приложения-аутентификатора" width="200" height="200" />
        <div class="row">
          <input class="input code" inputmode="numeric" maxlength="6" bind:value={code} placeholder="000000" />
          <button class="btn primary" disabled={code.length !== 6} onclick={enable}>Подтвердить</button>
        </div>
      {:else}
        <p class="muted">Защитите вход одноразовыми кодами (TOTP).</p>
        <button class="btn primary" onclick={startSetup}>Настроить</button>
      {/if}
    </section>

    <section class="card">
      <h2>Смена пароля</h2>
      <input class="pm-user" type="email" name="username" autocomplete="username" value={session.me.email} readonly tabindex="-1" aria-hidden="true" />
      <div class="grid">
        <label class="field">Текущий пароль <input class="input" type="password" bind:value={current} autocomplete="current-password" /></label>
        <label class="field">Новый пароль (не менее 10 символов) <input class="input" type="password" bind:value={next} minlength="10" autocomplete="new-password" /></label>
      </div>
      <p class="faint">После смены пароля все сеансы будут завершены.</p>
      <button class="btn" disabled={!current || next.length < 10} onclick={changePassword}>Сменить пароль</button>
    </section>
  {/if}
</div>

<style>
  .me-card { display: flex; gap: 14px; align-items: center; flex-wrap: wrap; }
  .me-card h2 { margin: 0; }
  .me-card p { margin: 2px 0 0; }
  .me-text { flex: 1; min-width: 200px; }
  .hint { margin: -6px 0 14px; }
  /* Login for password managers (kept out of sight). */
  .pm-user { position: absolute; width: 1px; height: 1px; opacity: 0; pointer-events: none; }
  .narrow { max-width: 760px; }
  section { padding: 18px; margin-bottom: 14px; }
  section p { margin: 0 0 12px; }
  .grid { display: grid; gap: 12px; margin-bottom: 8px; }
  .code { width: 140px; font-family: var(--mono); letter-spacing: .3em; }
  code { font-family: var(--mono); background: var(--surface-2); padding: 2px 6px; border-radius: 4px; word-break: break-all; }
  ol { padding-left: 20px; margin: 0 0 12px; }
  img { display: block; margin-bottom: 12px; border-radius: 6px; background: #fff; }
  .note { margin-bottom: 12px; }
</style>
