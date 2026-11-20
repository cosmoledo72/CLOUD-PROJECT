<script lang="ts">
  import { page } from '$app/state';
  import { api, authError, quoted } from '#lib/api.ts';
  import AuthShell from '#lib/AuthShell.svelte';
  import { branding, type Branding } from '#lib/branding.svelte.ts';

  type Info = { email: string; tenant_slug: string; tenant_name: string };
  const token = $derived(page.params.token ?? '');
  let info = $state<Info | null>(null);
  let invalid = $state(false);
  let done = $state(false);
  let brand = $state<Branding | null>(null);
  let password = $state('');
  let password2 = $state('');
  let error = $state('');
  let busy = $state(false);

  $effect(() => {
    api.post<Info>('/public/password/check', { token })
      .then(async (r) => {
        info = r;
        brand = await branding.loadPublic(r.tenant_slug);
      })
      .catch(() => (invalid = true));
  });

  const mismatch = $derived(password2.length > 0 && password !== password2);

  async function reset() {
    error = '';
    busy = true;
    try {
      await api.post('/public/password/reset', { token, password });
      done = true;
    } catch (e) {
      error = authError(e);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head><title>Новый пароль</title></svelte:head>

<AuthShell {brand}>
  {#if invalid}
    <div class="stack">
      <h2>Ссылка недействительна</h2>
      <p class="muted">Ссылка для смены пароля действует 60 минут и только один раз. Запросите новую на странице входа.</p>
      <a class="btn primary full" href="/login">Ко входу</a>
    </div>
  {:else if done && info}
    <div class="stack">
      <h2>Пароль изменён</h2>
      <p class="muted">Все прежние сеансы завершены. Войдите с новым паролем.</p>
      <a class="btn primary full" href="/login?portal={encodeURIComponent(info.tenant_slug)}">Войти</a>
    </div>
  {:else if info}
    <form onsubmit={(e) => { e.preventDefault(); reset(); }}>
      <h2>Новый пароль</h2>
      <p class="muted">Портал {quoted(info.tenant_name)}, учётная запись {info.email}.</p>
      <label class="field">Новый пароль
        <input class="input" type="password" bind:value={password} required minlength="10" autocomplete="new-password" />
        <span class="faint">не менее 10 символов</span>
      </label>
      <label class="field">Ещё раз
        <input class="input" type="password" bind:value={password2} required autocomplete="new-password" aria-invalid={mismatch} />
        {#if mismatch}<span class="err small">пароли не совпадают</span>{/if}
      </label>
      {#if error}<div class="error-box">{error}</div>{/if}
      <button class="btn primary full" disabled={busy || password.length < 10 || password !== password2}>Сохранить пароль</button>
    </form>
  {:else}
    <p class="faint">Проверяем ссылку…</p>
  {/if}
</AuthShell>

<style>
  .stack { display: grid; gap: 12px; }
  .err { color: var(--danger); }
</style>
