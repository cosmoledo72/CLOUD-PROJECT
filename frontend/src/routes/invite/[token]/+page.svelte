<script lang="ts">
  import UserRound from '@lucide/svelte/icons/user-round';
  import PasswordInput from '#lib/PasswordInput.svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, authError, quoted, type Me } from '#lib/api.ts';
  import AuthShell from '#lib/AuthShell.svelte';
  import { branding, type Branding } from '#lib/branding.svelte.ts';
  import { session } from '#lib/session.svelte.ts';

  type Info = {
    email: string; display_name: string; tenant_slug: string; tenant_name: string;
    invited_by_name: string | null; message: string; projects: string[]; groups: string[];
  };
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
      <p class="muted">
        {info.invited_by_name ? `${info.invited_by_name} приглашает вас` : 'Вас пригласили'} в портал {quoted(info.tenant_name)}. Задайте пароль — и можно начинать работу.
      </p>
      {#if info.message}<blockquote class="msg">{info.message}{#if info.invited_by_name}<span class="from">— {info.invited_by_name}</span>{/if}</blockquote>{/if}
      {#if info.projects.length}
        <div class="where">
          <span class="faint small">{info.projects.length === 1 ? 'Проект' : 'Проекты'}</span>
          <div class="chips">{#each info.projects as p}<span class="chip">{p}</span>{/each}</div>
        </div>
      {/if}
      <div class="field"><span>Логин</span><div class="ro">{info.email}</div></div>
      <label class="field">Как вас называть <span class="with-icon"><UserRound size={17} /><input class="input" bind:value={name} required autocomplete="name" /></span></label>
      <label class="field">Пароль
        <PasswordInput bind:value={password} autocomplete="new-password" minlength={10} strength />
        {#if !password}<span class="faint">не менее 10 символов</span>{/if}
      </label>
      <label class="field">Пароль ещё раз
        <PasswordInput bind:value={password2} autocomplete="new-password" invalid={mismatch} />
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
  .msg { margin: 0; padding: 10px 12px; border-left: 3px solid var(--accent); background: var(--surface-2); border-radius: 0 6px 6px 0;
         white-space: pre-wrap; font-size: 14px; }
  .from { display: block; margin-top: 4px; font-size: 12.5px; color: var(--text-3); }
  .where { display: grid; gap: 4px; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { padding: 2px 10px; border-radius: 999px; background: var(--accent-soft); color: var(--accent); font-size: 13px; font-weight: 500; }
</style>
