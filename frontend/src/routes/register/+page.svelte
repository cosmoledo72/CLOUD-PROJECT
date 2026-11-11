<script lang="ts">
  // Self-registration in an existing portal: an application that the applicant
  // confirms by mail and an administrator approves (or that joins at once for
  // the portal's own mail domains).
  import { page } from '$app/state';
  import { api, authError, quoted } from '$lib/api';
  import AuthShell from '$lib/AuthShell.svelte';
  import { branding, type Branding } from '$lib/branding.svelte';

  function storedTenant(): string {
    try { return localStorage.getItem('wh_tenant') ?? ''; } catch { return ''; }
  }

  let tenant = $state(page.url.searchParams.get('portal') ?? storedTenant());
  let name = $state('');
  let email = $state('');
  let password = $state('');
  let password2 = $state('');
  let comment = $state('');
  // Honeypot: hidden from people, filled in by bots.
  let website = $state('');
  let error = $state('');
  let busy = $state(false);
  let done = $state<'confirm_email' | 'pending' | null>(null);

  // Portal branding and whether registration is open (debounced lookup).
  let brand = $state<Branding | null>(null);
  let checked = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const slug = tenant.trim().toLowerCase();
    clearTimeout(timer);
    checked = false;
    if (slug.length < 2) {
      if (brand) { brand = null; branding.reset(); }
      return;
    }
    timer = setTimeout(async () => {
      const b = await branding.loadPublic(slug);
      brand = b;
      if (!b) branding.reset();
      checked = true;
    }, 350);
  });

  const closed = $derived(checked && (!brand || !brand.registration_open));
  const mismatch = $derived(password2.length > 0 && password !== password2);
  const ready = $derived(!!name.trim() && !!email.trim() && password.length >= 10 && password === password2 && !closed);

  async function submit() {
    error = '';
    busy = true;
    try {
      const r = await api.post<{ ok: boolean; next: 'confirm_email' | 'pending' }>('/public/register', {
        tenant: tenant.trim().toLowerCase(), display_name: name, email: email.trim(), password, comment, website
      });
      try { localStorage.setItem('wh_tenant', tenant.trim().toLowerCase()); } catch { /* storage unavailable */ }
      done = r.next;
    } catch (e) {
      error = authError(e);
    } finally {
      busy = false;
    }
  }

  const portalName = $derived(brand ? quoted(brand.display_name) : `«${tenant}»`);
  const loginHref = $derived(`/login?portal=${encodeURIComponent(tenant.trim().toLowerCase())}`);
</script>

<svelte:head><title>Регистрация{brand ? ` — ${brand.display_name}` : ''}</title></svelte:head>

<AuthShell {brand}>
  {#if done === 'confirm_email'}
    <div class="stack">
      <h2>Проверьте почту</h2>
      <p class="muted">На адрес <b>{email.trim()}</b> отправлено письмо со ссылкой. Откройте её, чтобы подтвердить адрес, — после этого заявка попадёт к администратору портала {portalName}.</p>
      <p class="faint small">Ссылка действует 48 часов. Письма нет несколько минут — проверьте папку «Спам».</p>
      <a class="btn full" href={loginHref}>Ко входу</a>
    </div>
  {:else if done === 'pending'}
    <div class="stack">
      <h2>Заявка отправлена</h2>
      <p class="muted">Администратор портала {portalName} рассмотрит заявку. Когда её одобрят, войдите с адресом <b>{email.trim()}</b> и паролем, который вы указали.</p>
      <a class="btn full" href={loginHref}>Ко входу</a>
    </div>
  {:else}
    <form onsubmit={(e) => { e.preventDefault(); submit(); }}>
      <h2>Регистрация в портале</h2>
      <label class="field">Портал <input class="input" bind:value={tenant} required autocomplete="organization" placeholder="company" /></label>
      {#if closed}
        <div class="info-box">
          {brand ? `Регистрация в портале ${quoted(brand.display_name)} закрыта. Попросите администратора прислать приглашение.` : 'Портал не найден. Проверьте адрес портала.'}
        </div>
      {/if}
      <label class="field">Имя и фамилия <input class="input" bind:value={name} required maxlength="200" autocomplete="name" /></label>
      <label class="field">Email <input class="input" type="email" bind:value={email} required autocomplete="email" /></label>
      <label class="field">Пароль
        <input class="input" type="password" bind:value={password} required minlength="10" autocomplete="new-password" />
        <span class="faint">не менее 10 символов</span>
      </label>
      <label class="field">Пароль ещё раз
        <input class="input" type="password" bind:value={password2} required autocomplete="new-password" aria-invalid={mismatch} />
        {#if mismatch}<span class="err small">пароли не совпадают</span>{/if}
      </label>
      <label class="field"><span>Комментарий для администратора <span class="faint">(необязательно)</span></span>
        <textarea class="input" rows="2" bind:value={comment} maxlength="1000" placeholder="Например: инженер отдела обследований"></textarea>
      </label>
      <label class="hp" aria-hidden="true">Сайт <input tabindex="-1" autocomplete="off" bind:value={website} /></label>
      {#if error}<div class="error-box">{error}</div>{/if}
      <button class="btn primary full" disabled={busy || !ready}>Отправить заявку</button>
      <a class="btn ghost full" href={loginHref}>У меня уже есть учётная запись</a>
    </form>
  {/if}
</AuthShell>

<style>
  .stack { display: grid; gap: 12px; }
  .stack p { margin: 0; }
  .err { color: var(--danger); }
  textarea.input { height: auto; padding-top: 6px; resize: vertical; }
  .info-box { padding: 10px 12px; border-radius: var(--radius, 6px); background: var(--warn-soft); color: var(--text); font-size: 13.5px; }
  /* Honeypot: off-screen, not display:none (some bots skip hidden fields). */
  .hp { position: absolute; left: -10000px; width: 1px; height: 1px; overflow: hidden; }
</style>
