<script lang="ts">
  import Avatar from '#lib/Avatar.svelte';
  import UserRound from '@lucide/svelte/icons/user-round';
  import { goto } from '$app/navigation';
  import QRCode from 'qrcode';
  import { api, plainMessage, roleLabel } from '#lib/api.ts';
  import NotificationPrefs from '#lib/NotificationPrefs.svelte';
  import Modal from '#lib/Modal.svelte';
  import RecoveryCodes from '#lib/RecoveryCodes.svelte';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import { session } from '#lib/session.svelte.ts';

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
      const r = await api.post<{ recovery_codes: string[] }>('/auth/totp/enable', { code });
      setup = null;
      code = '';
      notice = 'Двухфакторная аутентификация включена.';
      showCodes(r.recovery_codes, false);
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

  // --- Recovery codes ---------------------------------------------------------
  let codes = $state<string[]>([]);
  let codesOpen = $state(false);
  let codesReissued = $state(false);
  function showCodes(list: string[], reissued: boolean) {
    codes = list;
    codesReissued = reissued;
    codesOpen = true;
  }
  const codesLeft = $derived(session.me?.recovery_codes_left ?? 0);
  let reissueOpen = $state(false);
  let reissuePassword = $state('');
  let reissueError = $state('');
  async function reissue() {
    reissueError = '';
    try {
      const r = await api.post<{ recovery_codes: string[] }>('/auth/totp/recovery-codes', { password: reissuePassword });
      reissueOpen = false;
      reissuePassword = '';
      showCodes(r.recovery_codes, true);
      await session.load();
    } catch {
      reissueError = 'Неверный пароль';
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
        <div class="rc" class:low={codesLeft <= 2}>
          <KeyRound size={18} />
          <span class="rc-text">
            {#if codesLeft === 0}Резервных кодов нет — выпустите их, чтобы войти без телефона
            {:else}Резервные коды: <b>осталось {codesLeft} из 10</b>{#if codesLeft <= 2} — пора выпустить новые{/if}{/if}
          </span>
          <button class="btn sm" onclick={() => { reissueOpen = true; reissueError = ''; }}><RefreshCw size={15} />Выпустить новые</button>
        </div>
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

<Modal bind:open={reissueOpen} title="Новые резервные коды" width={420}>
  <p class="muted small">Прежние коды перестанут действовать. Для подтверждения введите пароль.</p>
  <input class="pm-user" type="email" name="username" autocomplete="username" value={session.me?.email ?? ''} readonly tabindex="-1" aria-hidden="true" />
  <!-- svelte-ignore a11y_autofocus -->
  <input class="input" type="password" placeholder="Пароль" bind:value={reissuePassword} autocomplete="current-password" autofocus
         onkeydown={(e) => e.key === 'Enter' && reissuePassword && reissue()} />
  {#if reissueError}<div class="error-box">{reissueError}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (reissueOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!reissuePassword} onclick={reissue}>Выпустить</button>
  {/snippet}
</Modal>
<RecoveryCodes bind:open={codesOpen} {codes} reissued={codesReissued} portal={session.me?.tenant_slug ?? ''} email={session.me?.email ?? ''} />

<style>
  .rc { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; padding: 10px 12px; margin-bottom: 12px;
        border: 1px solid var(--border); border-radius: 8px; color: var(--text-2); font-size: 13.5px; }
  .rc.low { border-color: var(--warn); background: var(--warn-soft); color: var(--warn); }
  .rc-text { flex: 1; min-width: 200px; color: var(--text); }
  .rc.low .rc-text { color: var(--warn); }
  .error-box { margin-top: 10px; }
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
