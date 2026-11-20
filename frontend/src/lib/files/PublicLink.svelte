<script lang="ts">
  // «Ссылка для внешних»: open a file, folder or knowledge base page to people
  // without an account — copy, expiry (30 days by default), password, opens
  // and downloads counters, revoke.
  import Link2 from '@lucide/svelte/icons/link-2';
  import Copy from '@lucide/svelte/icons/copy';
  import Check from '@lucide/svelte/icons/check';
  import { api, localToday } from '#lib/api.ts';
  import { plural, relTime } from './fmt';

  type RType = 'file' | 'folder' | 'kb_page';
  interface Link {
    id: string;
    token: string;
    has_password: boolean;
    expires_on: string | null;
    created_by_name: string | null;
    created_at: string;
    opens: number;
    downloads: number;
    last_opened_at: string | null;
  }

  let { resourceType, resourceId, active = true, highlight = false }: { resourceType: RType; resourceId: string; active?: boolean; highlight?: boolean } = $props();

  let link = $state<Link | null>(null);
  let exists = $state(false);
  let canManage = $state(false);
  let enabled = $state(true);
  let defaultDays = $state(30);
  let loaded = $state(false);
  let busy = $state(false);
  let error = $state('');
  let copied = $state(false);
  let expires = $state('');
  let noExpiry = $state(false);
  let password = $state('');
  let editingPassword = $state(false);
  let box = $state<HTMLElement>();

  async function load() {
    error = '';
    try {
      const r = await api.get<{ link: Link | null; exists: boolean; can_manage: boolean; enabled: boolean; default_days: number }>(
        `/links?resource_type=${resourceType}&resource_id=${resourceId}`
      );
      link = r.link;
      exists = r.exists;
      canManage = r.can_manage;
      enabled = r.enabled;
      defaultDays = r.default_days;
      sync();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      loaded = true;
    }
  }
  function sync() {
    noExpiry = !!link && !link.expires_on;
    expires = link?.expires_on ?? addDays(defaultDays);
    password = '';
    editingPassword = false;
  }
  function addDays(n: number) {
    const d = new Date(localToday());
    d.setDate(d.getDate() + n);
    return d.toISOString().slice(0, 10);
  }
  $effect(() => {
    if (active && resourceId) load();
  });
  $effect(() => {
    if (highlight && loaded && box) setTimeout(() => box?.scrollIntoView({ block: 'nearest', behavior: 'smooth' }), 50);
  });

  const url = $derived(link ? `${location.origin}/s/${link.token}` : '');

  async function save(extra: Record<string, unknown> = {}) {
    busy = true;
    error = '';
    try {
      const r = await api.put<{ link: Link }>('/links', {
        resource_type: resourceType,
        resource_id: resourceId,
        expires_on: noExpiry ? null : expires || null,
        no_expiry: noExpiry,
        password: password.trim() || undefined,
        ...extra
      });
      const fresh = !link;
      link = r.link;
      exists = true;
      sync();
      if (fresh) await copy();
    } catch (e) {
      const st = (e as { status?: number }).status;
      error = st === 403 ? 'Ссылки для внешних отключены администратором портала' : (e as Error).message;
    } finally {
      busy = false;
    }
  }
  async function revoke() {
    if (!link || !confirm('Отключить ссылку? Тот, у кого она есть, больше не сможет открыть. Новая ссылка будет другой.')) return;
    busy = true;
    try {
      await api.del(`/links/${link.id}`);
      link = null;
      exists = false;
      sync();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }
  async function copy() {
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      prompt('Ссылка для внешних:', url);
    }
  }
  const fmtDate = (d: string) => new Date(d + 'T00:00').toLocaleDateString('ru-RU', { day: '2-digit', month: '2-digit', year: 'numeric' });
  const what = $derived(resourceType === 'folder' ? 'папку' : resourceType === 'file' ? 'файл' : 'страницу');
</script>

{#if loaded && (canManage || exists)}
  <section class="pl" class:hl={highlight} bind:this={box} aria-label="Ссылка для внешних">
    <h4><Link2 size={15} />Ссылка для внешних — без входа на портал</h4>
    {#if !enabled}
      <p class="hint">Ссылки для внешних отключены администратором портала.</p>
    {:else if !canManage}
      <p class="hint">На {what} есть ссылка для внешних. Управлять ею может владелец или участник с полным доступом.</p>
    {:else if !link}
      <p class="hint">Заказчик или подрядчик сможет открыть {what} по ссылке{resourceType === 'kb_page' ? '' : ' — смотреть и скачивать'}, без учётной записи. По умолчанию ссылка действует {defaultDays} {plural(defaultDays, 'день', 'дня', 'дней')}.</p>
      <div class="opts">
        <label class="field">Действует до
          <input class="input" type="date" bind:value={expires} disabled={noExpiry} min={localToday()} />
        </label>
        <label class="field">Пароль
          <input class="input" type="text" bind:value={password} maxlength="128" placeholder="необязательно" autocomplete="off" />
        </label>
      </div>
      <label class="check"><input type="checkbox" bind:checked={noExpiry} />Бессрочно</label>
      <div><button class="btn primary" disabled={busy} onclick={() => save()}><Link2 size={15} />{busy ? 'Создаю…' : 'Создать ссылку и скопировать'}</button></div>
    {:else}
      <div class="url">
        <input class="input mono" readonly value={url} aria-label="Ссылка" onfocus={(e) => e.currentTarget.select()} />
        <button class="btn primary" onclick={copy}>{#if copied}<Check size={15} />Скопирована{:else}<Copy size={15} />Копировать{/if}</button>
      </div>
      <p class="hint">{resourceType === 'kb_page' ? 'Только просмотр страницы' : `Смотреть и скачивать${resourceType === 'folder' ? ' (и всю папку архивом)' : ''}`}. {link.expires_on ? `Действует по ${fmtDate(link.expires_on)} включительно.` : 'Бессрочно.'} {link.has_password ? 'Защищена паролем.' : ''}</p>
      <div class="opts">
        <label class="field">Действует до
          <input class="input" type="date" bind:value={expires} disabled={noExpiry} min={localToday()} onchange={() => save()} />
        </label>
        <div class="field">Пароль
          {#if editingPassword}
            <div class="pw">
              <input class="input" type="text" bind:value={password} maxlength="128" placeholder="новый пароль" autocomplete="off" />
              <button class="btn" disabled={busy || password.trim().length < 4} onclick={() => save()}>Сохранить</button>
            </div>
          {:else}
            <div class="pw">
              <span class="pwstate">{link.has_password ? '••••••••' : 'без пароля'}</span>
              <button class="btn ghost sm" onclick={() => (editingPassword = true)}>{link.has_password ? 'Сменить' : 'Задать'}</button>
              {#if link.has_password}<button class="btn ghost sm" disabled={busy} onclick={() => save({ remove_password: true })}>Убрать</button>{/if}
            </div>
          {/if}
        </div>
      </div>
      <label class="check"><input type="checkbox" bind:checked={noExpiry} onchange={() => save()} />Бессрочно</label>
      <div class="stat">
        {#if link.opens}Открывали {link.opens} {plural(link.opens, 'раз', 'раза', 'раз')}{#if link.last_opened_at}, последний — {relTime(link.last_opened_at)}{/if}{:else}Ещё не открывали{/if}
        · скачиваний: {link.downloads}
        {#if link.created_by_name}<span class="faint"> · создал(а) {link.created_by_name}</span>{/if}
      </div>
      <div class="foot">
        <span class="hint">Пароль меняет доступ сразу: кто открыл со старым паролем, введёт новый.</span>
        <button class="btn ghost sm danger" disabled={busy} onclick={revoke}>Отключить ссылку</button>
      </div>
    {/if}
    {#if error}<div class="error-box">{error}</div>{/if}
  </section>
{/if}

<style>
  .pl { border: 1px solid var(--border); border-radius: 12px; padding: 12px 14px; display: grid; gap: 10px; transition: box-shadow .3s; }
  .pl.hl { box-shadow: 0 0 0 3px var(--accent-soft); border-color: var(--accent); }
  h4 { display: flex; align-items: center; gap: 7px; margin: 0; font-size: 13.5px; }
  .hint { margin: 0; font-size: 12.5px; color: var(--text-3); }
  .url { display: flex; gap: 8px; }
  .url input { flex: 1; min-width: 0; font-size: 13px; background: var(--surface-2); }
  .mono { font-family: ui-monospace, monospace; }
  .btn { display: inline-flex; align-items: center; gap: 6px; }
  .opts { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .field { display: grid; gap: 5px; font-size: 12.5px; color: var(--text-2); }
  .pw { display: flex; align-items: center; gap: 6px; min-height: 36px; }
  .pw input { flex: 1; min-width: 0; }
  .pwstate { font-size: 13.5px; color: var(--text); }
  .stat { font-size: 12.5px; color: var(--text-2); background: var(--surface-2); border-radius: 8px; padding: 8px 10px; }
  .foot { display: flex; align-items: center; gap: 10px; justify-content: space-between; flex-wrap: wrap; }
  .foot .hint { flex: 1; min-width: 200px; }
  @media (max-width: 560px) { .opts { grid-template-columns: 1fr; } }
</style>
