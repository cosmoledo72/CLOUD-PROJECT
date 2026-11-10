<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, authError, quoted, type Me } from '$lib/api';
  import AuthShell from '$lib/AuthShell.svelte';
  import { branding, type Branding } from '$lib/branding.svelte';
  import { session } from '$lib/session.svelte';

  type Info = { email: string; display_name: string; tenant_slug: string; tenant_name: string };
  const token = $derived(page.params.token ?? '');
  let info = $state<Info | null>(null);
  let invalid = $state(false);
  let brand = $state<Branding | null>(null);
  let name = $state('');
  let password = $state('');
  let password2 = $state('');
  let error = $state('');
  let busy = $state(false);

  $effect(() => {
    api.post<Info>('/public/invite/check', { token })
      .then(async (r) => {
        info = r;
        name = r.display_name;
        brand = await branding.loadPublic(r.tenant_slug);
      })
      .catch(() => (invalid = true));
  });

  const mismatch = $derived(password2.length > 0 && password !== password2);

  async function accept() {
    if (!info) return;
    error = '';
    busy = true;
    try {
      const r = await api.post<{ tenant_slug: string; email: string }>('/public/invite/accept', { token, password, display_name: name });
      // Sign in right away with the new password.
      const login = await api.post<{ mfa_required: boolean; user?: Me }>('/auth/login', { tenant: r.tenant_slug, email: r.email, password });
      try { localStorage.setItem('wh_tenant', r.tenant_slug); } catch { /* storage unavailable */ }
      if (login.user) {
        session.me = login.user;
        session.loaded = true;
        await branding.loadOwn();
        goto('/projects');
      } else {
        goto(`/login?portal=${encodeURIComponent(r.tenant_slug)}`);
      }
    } catch (e) {
      error = authError(e);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head><title>Приглашение{info ? ` — ${info.tenant_name}` : ''}</title></svelte:head>

<AuthShell {brand}>
  {#if invalid}
    <div class="stack">
      <h2>Ссылка недействительна</h2>
      <p class="muted">Приглашение устарело, уже использовано или было заменено новым. Попросите администратора портала отправить приглашение ещё раз.</p>
      <a class="btn full" href="/login">Ко входу</a>
    </div>
  {:else if info}
    <form onsubmit={(e) => { e.preventDefault(); accept(); }}>
      <h2>Добро пожаловать</h2>
      <p class="muted">Вас пригласили в портал {quoted(info.tenant_name)}. Задайте пароль — и можно начинать работу.</p>
      <div class="field"><span>Логин</span><div class="ro">{info.email}</div></div>
      <label class="field">Как вас называть <input class="input" bind:value={name} required autocomplete="name" /></label>
      <label class="field">Пароль
        <input class="input" type="password" bind:value={password} required minlength="10" autocomplete="new-password" />
        <span class="faint">не менее 10 символов</span>
      </label>
      <label class="field">Пароль ещё раз
        <input class="input" type="password" bind:value={password2} required autocomplete="new-password" aria-invalid={mismatch} />
        {#if mismatch}<span class="err small">пароли не совпадают</span>{/if}
      </label>
      {#if error}<div class="error-box">{error}</div>{/if}
      <button class="btn primary full" disabled={busy || password.length < 10 || password !== password2 || !name.trim()}>Присоединиться</button>
    </form>
  {:else}
    <p class="faint">Проверяем приглашение…</p>
  {/if}
</AuthShell>

<style>
  .stack { display: grid; gap: 12px; }
  .ro { padding: 6px 0; font-weight: 500; }
  .err { color: var(--danger); }
</style>
