<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, ApiError, type Me } from '$lib/api';
  import { branding, type Branding } from '$lib/branding.svelte';
  import AuthShell from '$lib/AuthShell.svelte';
  import { session } from '$lib/session.svelte';

  type Mode = 'login' | 'mfa' | 'signup' | 'forgot' | 'sent';
  let mode = $state<Mode>('login');
  let error = $state('');
  let busy = $state(false);

  function storedTenant(): string {
    try { return localStorage.getItem('wh_tenant') ?? ''; } catch { return ''; }
  }

  // ?portal=<slug> comes from the login link shown in the admin panel.
  const linkPortal = page.url.searchParams.get('portal') ?? '';
  let tenant = $state(linkPortal || storedTenant());
  let email = $state('');
  let password = $state('');
  let code = $state('');
  let tenantName = $state('');
  let displayName = $state('');

  // --- Portal branding (debounced lookup by slug) -----------------------------
  let brand = $state<Branding | null>(null);
  let lookupTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const slug = tenant.trim().toLowerCase();
    clearTimeout(lookupTimer);
    if (mode === 'signup' || slug.length < 2) {
      if (brand) { brand = null; branding.reset(); }
      return;
    }
    lookupTimer = setTimeout(async () => {
      const b = await branding.loadPublic(slug);
      brand = b;
      if (!b) branding.reset();
    }, 350);
  });

  async function run(fn: () => Promise<void>) {
    error = '';
    busy = true;
    try {
      await fn();
    } catch (e) {
      error = !(e instanceof ApiError)
        ? 'Сервер недоступен'
        : e.status === 401
          ? 'Неверные данные для входа'
          : e.status === 429
            ? mode === 'forgot'
              ? 'Слишком много запросов. Повторите через час.'
              : 'Слишком много неудачных попыток. Вход временно заблокирован — повторите через 15 минут.'
            : e.message;
    } finally {
      busy = false;
    }
  }

  async function finish(me: Me) {
    try { localStorage.setItem('wh_tenant', me.tenant_slug); } catch { /* storage unavailable */ }
    session.me = me;
    session.loaded = true;
    await branding.loadOwn();
    goto('/projects');
  }

  const login = () => run(async () => {
    const r = await api.post<{ mfa_required: boolean; user?: Me }>('/auth/login', { tenant, email, password });
    if (r.mfa_required) {
      mode = 'mfa';
      code = '';
    } else if (r.user) {
      await finish(r.user);
    }
  });

  const forgot = () => run(async () => {
    await api.post('/public/password/forgot', { tenant: tenant.trim().toLowerCase(), email: email.trim() });
    mode = 'sent';
  });

  const verify = () => run(async () => finish(await api.post<Me>('/auth/totp/verify', { code })));

  const signup = () => run(async () => finish(await api.post<Me>('/auth/signup', {
    tenant_slug: tenant, tenant_name: tenantName, email, display_name: displayName, password
  })));

</script>

<AuthShell {brand}>
  {#if mode === 'login'}
    <form onsubmit={(e) => { e.preventDefault(); login(); }}>
      <h2>Вход</h2>
      <label class="field">Портал <input class="input" bind:value={tenant} required autocomplete="organization" placeholder="company" /></label>
      <label class="field">Email <input class="input" type="email" bind:value={email} required autocomplete="username" /></label>
      <label class="field">Пароль <input class="input" type="password" bind:value={password} required autocomplete="current-password" /></label>
      <button type="button" class="linkish forgot" onclick={() => { mode = 'forgot'; error = ''; }}>Забыли пароль?</button>
      {#if error}<div class="error-box">{error}</div>{/if}
      <button class="btn primary full" disabled={busy}>Войти</button>
      {#if brand?.registration_open}
        <p class="join">Нет учётной записи? <a href="/register?portal={encodeURIComponent(brand.tenant_slug)}">Зарегистрироваться в портале</a></p>
      {/if}
      <button type="button" class="btn ghost full" onclick={() => { mode = 'signup'; error = ''; }}>Создать новый портал</button>
    </form>
  {:else if mode === 'forgot'}
    <form onsubmit={(e) => { e.preventDefault(); forgot(); }}>
      <h2>Восстановление пароля</h2>
      <p class="muted">Укажите портал и адрес почты, с которым вы входите. Пришлём ссылку для смены пароля.</p>
      <label class="field">Портал <input class="input" bind:value={tenant} required autocomplete="organization" placeholder="company" /></label>
      <label class="field">Email <input class="input" type="email" bind:value={email} required autocomplete="username" /></label>
      {#if error}<div class="error-box">{error}</div>{/if}
      <button class="btn primary full" disabled={busy}>Отправить ссылку</button>
      <button type="button" class="btn ghost full" onclick={() => { mode = 'login'; error = ''; }}>Назад ко входу</button>
    </form>
  {:else if mode === 'sent'}
    <div class="stack">
      <h2>Проверьте почту</h2>
      <p class="muted">
        Если в портале «{tenant}» есть учётная запись {email}, на этот адрес придёт письмо со ссылкой. Ссылка действует 60 минут.
      </p>
      <p class="faint small">Письма нет несколько минут — проверьте папку «Спам» или обратитесь к администратору портала: он может задать пароль сам.</p>
      <button class="btn primary full" onclick={() => { mode = 'login'; password = ''; }}>Ко входу</button>
    </div>
  {:else if mode === 'mfa'}
    <form onsubmit={(e) => { e.preventDefault(); verify(); }}>
      <h2>Подтверждение входа</h2>
      <p class="muted">Введите 6-значный код из приложения-аутентификатора.</p>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="input code" bind:value={code} inputmode="numeric" maxlength="6" autocomplete="one-time-code" required autofocus aria-label="Код" />
      {#if error}<div class="error-box">{error}</div>{/if}
      <button class="btn primary full" disabled={busy || code.length !== 6}>Подтвердить</button>
      <button type="button" class="btn ghost full" onclick={() => { mode = 'login'; error = ''; }}>Назад</button>
    </form>
  {:else}
    <form onsubmit={(e) => { e.preventDefault(); signup(); }}>
      <h2>Новый портал</h2>
      <label class="field">Название организации <input class="input" bind:value={tenantName} required /></label>
      <label class="field">Адрес портала
        <input class="input" bind:value={tenant} required pattern="[a-z0-9][a-z0-9\-]{'{'}1,62{'}'}" placeholder="company" />
        <span class="faint">латиница в нижнем регистре, цифры и «-»</span>
      </label>
      <label class="field">Ваше имя <input class="input" bind:value={displayName} required autocomplete="name" /></label>
      <label class="field">Email <input class="input" type="email" bind:value={email} required autocomplete="username" /></label>
      <label class="field">Пароль <input class="input" type="password" bind:value={password} required minlength="10" autocomplete="new-password" />
        <span class="faint">не менее 10 символов</span>
      </label>
      {#if error}<div class="error-box">{error}</div>{/if}
      <button class="btn primary full" disabled={busy}>Создать портал</button>
      <button type="button" class="btn ghost full" onclick={() => { mode = 'login'; error = ''; }}>У меня уже есть портал</button>
    </form>
  {/if}
</AuthShell>

<style>
  .code { font-family: var(--mono); font-size: 22px; letter-spacing: .4em; text-align: center; height: 44px; }
  .stack { display: grid; gap: 12px; }
  .linkish { border: 0; background: none; padding: 0; color: var(--accent); cursor: pointer; font: inherit; }
  .linkish:hover { text-decoration: underline; }
  .forgot { justify-self: end; font-size: 13px; margin-top: -4px; }
  .join { margin: 0; text-align: center; font-size: 13.5px; color: var(--text-2); }
</style>
