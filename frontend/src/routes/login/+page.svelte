<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, ApiError, type Me } from '#lib/api.ts';
  import { branding, type Branding } from '#lib/branding.svelte.ts';
  import AuthShell from '#lib/AuthShell.svelte';
  import { session } from '#lib/session.svelte.ts';
  import Building2 from '@lucide/svelte/icons/building-2';
  import Mail from '@lucide/svelte/icons/mail';
  import UserRound from '@lucide/svelte/icons/user-round';
  import Link2 from '@lucide/svelte/icons/link-2';
  import Rocket from '@lucide/svelte/icons/rocket';
  import PasswordInput from '#lib/PasswordInput.svelte';
  import { onMount } from 'svelte';

  type Mode = 'login' | 'mfa' | 'recovery' | 'signup' | 'forgot' | 'sent';
  let mode = $state<Mode>('login');
  let error = $state('');
  let busy = $state(false);

  function storedTenant(): string {
    try { return localStorage.getItem('wh_tenant') ?? ''; } catch { return ''; }
  }

  // ?portal=<slug> comes from the login link shown in the admin panel.
  const linkPortal = page.url.searchParams.get('portal') ?? '';
  let tenant = $state(linkPortal || storedTenant());
  // «Запомнить меня»: a 30-day session; the choice itself is remembered too.
  let remember = $state((() => { try { return localStorage.getItem('wh_remember') === '1'; } catch { return false; } })());
  $effect(() => {
    try { localStorage.setItem('wh_remember', remember ? '1' : '0'); } catch { /* storage unavailable */ }
  });
  function storedEmail(): string {
    try { return localStorage.getItem('wh_email') ?? ''; } catch { return ''; }
  }
  let email = $state(storedEmail());
  let password = $state('');
  let code = $state('');
  let tenantName = $state('');
  let displayName = $state('');

  // --- First start of the system, portal creation allowed -------------------
  let firstRun = $state(false);
  let signupEnabled = $state(false);
  onMount(async () => {
    try {
      const s = await api.get<{ first_run: boolean; signup_enabled: boolean }>('/public/system');
      firstRun = s.first_run;
      signupEnabled = s.signup_enabled;
      if (firstRun) mode = 'signup';
    } catch {
      /* older server: keep the login form */
    }
  });

  // Portal address suggested from the organisation's name until edited by hand.
  let slugTouched = $state(false);
  const TR: Record<string, string> = { а:'a',б:'b',в:'v',г:'g',д:'d',е:'e',ё:'e',ж:'zh',з:'z',и:'i',й:'y',к:'k',л:'l',м:'m',н:'n',о:'o',п:'p',р:'r',с:'s',т:'t',у:'u',ф:'f',х:'h',ц:'ts',ч:'ch',ш:'sh',щ:'sch',ъ:'',ы:'y',ь:'',э:'e',ю:'yu',я:'ya' };
  function slugFrom(name: string): string {
    const words = name.toLowerCase().replace(/[«»"']/g, ' ').replace(/(^|\s)(ооо|оао|зао|пао|ао|ип|llc|ltd)(?=\s|$)/g, ' ');
    const t = [...words].map((c) => TR[c] ?? c).join('');
    return t.replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 40);
  }
  $effect(() => {
    if (mode === 'signup' && !slugTouched) tenant = slugFrom(tenantName);
  });

  // --- Portal branding (debounced lookup by slug) -----------------------------
  let brand = $state<Branding | null>(null);
  // A portal known in advance (login link or last sign-in) is shown as a chip
  // once its branding loads; «Сменить» turns it back into an input.
  let portalKnown = $state(!!(linkPortal || storedTenant()));
  let portalInput = $state<HTMLInputElement | null>(null);
  const portalChip = $derived(portalKnown && !!brand && brand.tenant_slug === tenant.trim().toLowerCase());
  function changePortal() {
    portalKnown = false;
    tenant = '';
    queueMicrotask(() => portalInput?.focus());
  }
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
          ? mode === 'mfa'
            ? 'Неверный код'
            : mode === 'recovery'
              ? 'Код неверный или уже использован'
              : 'Неверные данные для входа'
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
    try {
      localStorage.setItem('wh_tenant', me.tenant_slug);
      localStorage.setItem('wh_email', me.email);
    } catch { /* storage unavailable */ }
    session.me = me;
    session.loaded = true;
    await branding.loadOwn();
    goto('/projects');
  }

  const login = () => run(async () => {
    const r = await api.post<{ mfa_required: boolean; user?: Me }>('/auth/login', { tenant, email, password, remember });
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
      <div class="intro">
        {#if portalChip && brand && !brand.logo_url}
          <span class="chip"><span class="dot"></span>{brand.display_name}</span>
        {/if}
        <h2>Вход на портал</h2>
        <p class="muted">Рады видеть вас снова</p>
      </div>
      {#if !portalChip}
        <label class="field">Портал <span class="with-icon"><Building2 size={17} /><input class="input" bind:this={portalInput} bind:value={tenant} required autocomplete="organization" placeholder="company" /></span></label>
      {/if}
      <label class="field">Email <span class="with-icon"><Mail size={17} /><input class="input" type="email" bind:value={email} required autocomplete="username" placeholder="name@company.ru" /></span></label>
      <label class="field">Пароль <PasswordInput bind:value={password} /></label>
      <div class="row-between">
        <label class="check remember"><input type="checkbox" bind:checked={remember} />Запомнить меня</label>
        <button type="button" class="linkish forgot" onclick={() => { mode = 'forgot'; error = ''; }}>Забыли пароль?</button>
      </div>
      {#if error}<div class="error-box">{error}</div>{/if}
      <button class="btn primary full" disabled={busy}>Войти</button>
      {#if brand?.registration_open}
        <p class="join">Нет учётной записи? <a href="/register?portal={encodeURIComponent(brand.tenant_slug)}">Зарегистрироваться на портале</a></p>
      {/if}
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
      <button type="button" class="linkish alt" onclick={() => { mode = 'recovery'; code = ''; error = ''; }}>Нет доступа к телефону? Войти по резервному коду</button>
      <button type="button" class="btn ghost full" onclick={() => { mode = 'login'; error = ''; }}>Назад</button>
    </form>
  {:else if mode === 'recovery'}
    <form onsubmit={(e) => { e.preventDefault(); verify(); }}>
      <h2>Вход по резервному коду</h2>
      <p class="muted">Введите один из сохранённых резервных кодов. После входа он перестанет действовать.</p>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="input code rcode" bind:value={code} maxlength="12" autocomplete="off" autocapitalize="characters" spellcheck="false"
             placeholder="XXXX-XXXX" required autofocus aria-label="Резервный код" />
      {#if error}<div class="error-box">{error}</div>{/if}
      <button class="btn primary full" disabled={busy || code.replace(/[^0-9a-z]/gi, '').length !== 8}>Войти</button>
      <button type="button" class="linkish alt" onclick={() => { mode = 'mfa'; code = ''; error = ''; }}>Ввести код из приложения</button>
      <button type="button" class="btn ghost full" onclick={() => { mode = 'login'; error = ''; }}>Назад</button>
    </form>
  {:else}
    <form onsubmit={(e) => { e.preventDefault(); signup(); }}>
      <div class="intro">
        {#if firstRun}
          <span class="chip first"><Rocket size={14} />Первый запуск</span>
          <h2>Создайте портал организации</h2>
          <p class="muted">Вы станете его владельцем и сможете пригласить сотрудников</p>
        {:else}
          <h2>Новый портал</h2>
          <p class="muted">Отдельное пространство для другой организации</p>
        {/if}
      </div>
      {#if !signupEnabled && firstRun}
        <div class="note-box">Создание порталов на этом сервере выключено. Для первого портала запустите сервер один раз с <code>PROD_SIGNUP_ENABLED=true</code> в <code>.env</code> (SPEC, раздел 12).</div>
      {/if}
      <label class="field">Название организации <span class="with-icon"><Building2 size={17} /><input class="input" bind:value={tenantName} required autocomplete="organization" placeholder="ООО «Партнер-Гарант»" /></span></label>
      <label class="field">Адрес портала
        <span class="with-icon"><Link2 size={17} /><input class="input" bind:value={tenant} oninput={() => (slugTouched = true)} required pattern="[a-z0-9][a-z0-9\-]{'{'}1,62{'}'}" placeholder="partner-garant" /></span>
        <span class="faint">{tenant ? `Вход сотрудников: ${location.host}/login?portal=${tenant}` : 'латиница в нижнем регистре, цифры и «-»'}</span>
      </label>
      <label class="field">Ваше имя <span class="with-icon"><UserRound size={17} /><input class="input" bind:value={displayName} required autocomplete="name" placeholder="Имя Фамилия" /></span></label>
      <label class="field">Email <span class="with-icon"><Mail size={17} /><input class="input" type="email" bind:value={email} required autocomplete="username" placeholder="name@company.ru" /></span></label>
      <label class="field">Пароль <PasswordInput bind:value={password} autocomplete="new-password" minlength={10} strength />
        {#if !password}<span class="faint">не менее 10 символов</span>{/if}
      </label>
      {#if error}<div class="error-box">{error}</div>{/if}
      <button class="btn primary full" disabled={busy || (firstRun && !signupEnabled)}>Создать портал</button>
      {#if !firstRun}<button type="button" class="btn ghost full" onclick={() => { mode = 'login'; error = ''; }}>У меня уже есть портал</button>{/if}
    </form>
  {/if}
  {#snippet below()}
    {#if mode === 'login'}
      {#if portalChip}
        <span>Другой портал? <button type="button" class="linkish" onclick={changePortal}>Сменить</button></span>
      {:else}<span></span>{/if}
      {#if signupEnabled}<button type="button" class="linkish" onclick={() => { mode = 'signup'; error = ''; tenantName = ''; slugTouched = false; }}>Создать портал</button>{/if}
    {/if}
  {/snippet}
</AuthShell>

<style>
  .code { font-family: var(--mono); font-size: 22px; letter-spacing: .4em; text-align: center; height: 44px; }
  .stack { display: grid; gap: 12px; }
  .linkish { border: 0; background: none; padding: 0; color: var(--accent); cursor: pointer; font: inherit; }
  .linkish:hover { text-decoration: underline; }
  .linkish.alt { display: block; margin: 10px auto 2px; font-size: 13.5px; text-align: center; }
  .rcode { letter-spacing: .14em; text-transform: uppercase; }
  .rcode::placeholder { text-transform: none; letter-spacing: .14em; }
  .forgot { font-size: 13px; }
  .row-between { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-top: -4px; }
  .remember { display: inline-flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text-2); cursor: pointer; }
  .join { margin: 0; text-align: center; font-size: 13.5px; color: var(--text-2); }
  .intro { display: grid; gap: 2px; margin-bottom: 6px; }
  .chip { justify-self: start; display: inline-flex; align-items: center; gap: 7px; margin-bottom: 14px;
          padding: 4px 11px; border-radius: 99px; font-size: 12.5px; font-weight: 500;
          background: color-mix(in srgb, var(--brand) 9%, transparent); color: var(--accent); }
  .chip.first { gap: 6px; background: var(--accent-soft); }
  .note-box { font-size: 13px; line-height: 1.5; padding: 10px 12px; border-radius: 8px; background: var(--warn-soft); color: var(--warn); }
  .note-box code { font-size: 12px; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); }
</style>
