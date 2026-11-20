<script lang="ts">
  import { ruZones, otherZones as otherZones_ } from '#lib/timezones.ts';
  import { api, formatDate, formatSize, type Overview } from '#lib/api.ts';
  import RegistrationSettings from '#lib/admin/RegistrationSettings.svelte';
  import { session } from '#lib/session.svelte.ts';

  interface Portal {
    slug: string;
    name: string;
    quota_bytes: number;
    created_at: string;
    signup_enabled: boolean;
    session_ttl_hours: number;
    max_upload_mb: number;
    timezone: string;
  }

  let portal = $state<Portal | null>(null);
  let used = $state(0);
  let name = $state('');
  let timezone = $state('Europe/Moscow');
  // Russian zones first, then every zone the browser knows.
  const otherZones = otherZones_();
  let error = $state('');
  let notice = $state('');

  $effect(() => {
    Promise.all([api.get<Portal>('/admin/portal'), api.get<{ stats: Overview }>('/admin/overview')])
      .then(([p, o]) => {
        portal = p;
        name = p.name;
        timezone = p.timezone;
        used = o.stats.storage_used;
      })
      .catch((e) => (error = e.message));
  });

  async function save() {
    error = '';
    notice = '';
    try {
      await api.patch('/admin/portal', { name, timezone });
      if (portal) {
        portal.name = name.trim();
        portal.timezone = timezone;
      }
      if (session.me) session.me.tenant_name = name.trim();
      notice = 'Настройки портала сохранены.';
    } catch (e) {
      error = (e as Error).message;
    }
  }

  const loginLink = $derived(portal ? `${location.origin}/login?portal=${portal.slug}` : '');
  const pct = $derived(portal && portal.quota_bytes ? Math.min(100, (used / portal.quota_bytes) * 100) : 0);
</script>

{#if error}<div class="error-box">{error}</div>{/if}
{#if notice}<div class="ok-box">{notice}</div>{/if}

{#if portal}
  <div class="grid">
    <section class="card sec">
      <h2>Организация</h2>
      <label class="field">Название организации
        <div class="row">
          <input class="input" bind:value={name} maxlength="200" />
        </div>
        <span class="faint small">Используется в письмах и по умолчанию в интерфейсе. Отдельное название для интерфейса задаётся в «Оформлении».</span>
      </label>
      <label class="field">Часовой пояс
        <select class="input" bind:value={timezone}>
          <optgroup label="Россия">{#each ruZones as [z, l]}<option value={z}>{l}</option>{/each}</optgroup>
          {#if otherZones.length}<optgroup label="Другие">{#each otherZones as z}<option value={z}>{z}</option>{/each}</optgroup>{/if}
          {#if !ruZones.some(([z]) => z === timezone) && !otherZones.includes(timezone)}<option value={timezone}>{timezone}</option>{/if}
        </select>
        <span class="faint small">По нему определяется «сегодня» и «завтра» в напоминаниях о сроках задач. Напоминания приходят с 8:00 по местному времени.{#if session.me?.backups}{" "}По этому же поясу делаются ночные резервные копии сервера.{/if}</span>
      </label>
      <div class="row">
        <div class="spacer"></div>
        <button class="btn primary" disabled={!name.trim() || (name.trim() === portal.name && timezone === portal.timezone)} onclick={save}>Сохранить</button>
      </div>
      <dl>
        <dt>Адрес портала</dt><dd class="mono">{portal.slug}</dd>
        <dt>Создан</dt><dd>{formatDate(portal.created_at)}</dd>
      </dl>
      <div class="field">
        <span class="muted small">Ссылка на вход</span>
        <div class="row">
          <input class="input mono" readonly value={loginLink} onfocus={(e) => e.currentTarget.select()} />
          <button class="btn" onclick={() => navigator.clipboard?.writeText(loginLink)}>Копировать</button>
        </div>
      </div>
    </section>

    <section class="card sec">
      <h2>Хранилище</h2>
      <div class="big">{formatSize(used)} <span class="faint">из {formatSize(portal.quota_bytes)}</span></div>
      <div class="bar" aria-label="Занято {pct.toFixed(0)}%"><div style="width:{pct}%" class:warn={pct > 85}></div></div>
      <p class="faint small">Учитываются все версии файлов. Квота задаётся тарифом портала.</p>
    </section>

    <section class="card sec">
      <h2>Параметры сервера</h2>
      <p class="faint small">Задаются администратором установки в файле <span class="mono">.env</span>.</p>
      <dl>
        <dt>Длительность сеанса</dt><dd>{portal.session_ttl_hours / 24} дн.</dd>
        <dt>Максимальный размер загрузки</dt><dd>{portal.max_upload_mb} МБ</dd>
        <dt>Создание новых порталов</dt><dd>{portal.signup_enabled ? 'открыта' : 'закрыта'}</dd>
      </dl>
    </section>
  </div>
  <div class="reg"><RegistrationSettings /></div>
{/if}

<style>
  .ok-box, .error-box { margin-bottom: 12px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); gap: 12px; align-items: start; }
  .sec { padding: 16px; display: grid; gap: 12px; }
  .reg { margin-top: 12px; max-width: 760px; }
  .sec h2, .sec p { margin: 0; }
  dl { display: grid; grid-template-columns: auto 1fr; gap: 6px 16px; margin: 0; }
  dt { color: var(--text-2); }
  dd { margin: 0; }
  .big { font-size: 22px; font-weight: 600; }
  .big span { font-size: 14px; font-weight: 400; }
  .bar { height: 8px; background: var(--surface-2); border-radius: 4px; overflow: hidden; }
  .bar div { height: 100%; background: var(--accent); }
  .bar div.warn { background: var(--warn); }
</style>
