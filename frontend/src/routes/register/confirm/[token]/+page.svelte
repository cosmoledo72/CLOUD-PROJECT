<script lang="ts">
  // Address confirmation link from the registration email.
  import { page } from '$app/state';
  import { api, authError, quoted } from '$lib/api';
  import AuthShell from '$lib/AuthShell.svelte';
  import { branding, type Branding } from '$lib/branding.svelte';

  type Result = { status: 'active' | 'pending'; tenant_slug: string; tenant_name: string; email?: string };
  const token = $derived(page.params.token ?? '');
  let result = $state<Result | null>(null);
  let error = $state('');
  let brand = $state<Branding | null>(null);
  let started = false;

  $effect(() => {
    if (started) return;
    started = true;
    api.post<Result>('/public/register/confirm', { token })
      .then(async (r) => {
        result = r;
        try { localStorage.setItem('wh_tenant', r.tenant_slug); } catch { /* storage unavailable */ }
        brand = await branding.loadPublic(r.tenant_slug);
      })
      .catch((e) => (error = authError(e)));
  });

  const loginHref = $derived(result ? `/login?portal=${encodeURIComponent(result.tenant_slug)}` : '/login');
</script>

<svelte:head><title>Подтверждение адреса</title></svelte:head>

<AuthShell {brand}>
  <div class="stack">
    {#if error}
      <h2>Ссылка недействительна</h2>
      <p class="muted">Ссылка устарела, уже использована или заявка подана заново (тогда действует ссылка из последнего письма). Подайте заявку ещё раз, если нужно.</p>
      <a class="btn full" href="/register">К регистрации</a>
    {:else if result?.status === 'active'}
      <h2>Добро пожаловать!</h2>
      <p class="muted">Адрес подтверждён, учётная запись в портале {quoted(result.tenant_name)} создана. Войдите с адресом <b>{result.email}</b> и паролем, который указали при регистрации.</p>
      <a class="btn primary full" href={loginHref}>Войти</a>
    {:else if result}
      <h2>Адрес подтверждён</h2>
      <p class="muted">Заявка передана администратору портала {quoted(result.tenant_name)}. Когда её одобрят, придёт письмо — после этого можно будет войти.</p>
      <a class="btn full" href={loginHref}>Ко входу</a>
    {:else}
      <p class="faint">Проверяем ссылку…</p>
    {/if}
  </div>
</AuthShell>

<style>
  .stack { display: grid; gap: 12px; }
  .stack p, .stack h2 { margin: 0; }
</style>
