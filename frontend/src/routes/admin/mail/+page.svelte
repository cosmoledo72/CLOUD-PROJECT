<script lang="ts">
  import { api, formatDateTime, mailKindLabel, type MailSecurity, type MailSettings, type OutboxItem } from '#lib/api.ts';
  import { session } from '#lib/session.svelte.ts';

  type Loaded = {
    settings: MailSettings;
    system: { host: string; from_email: string } | null;
    available: boolean;
    public_url: string;
    outbox: OutboxItem[];
  };

  let data = $state<Loaded | null>(null);
  let f = $state<MailSettings & { password: string }>({
    enabled: false, host: '', port: 587, security: 'starttls', username: '', has_password: false,
    from_email: '', from_name: '', reply_to: '', password: ''
  });
  let clearPassword = $state(false);
  let saved = $state('');
  let error = $state('');
  let notice = $state('');
  let testTo = $state('');
  let testResult = $state<{ ok: boolean; error?: string } | null>(null);
  let testing = $state(false);

  async function load() {
    data = await api.get<Loaded>('/admin/mail');
    f = { ...data.settings, password: '' };
    clearPassword = false;
    saved = snapshot();
  }
  $effect(() => {
    load().catch((e) => (error = e.message));
    testTo = session.me?.email ?? '';
  });

  const snapshot = () => JSON.stringify({ ...f, clearPassword });
  const dirty = $derived(data !== null && JSON.stringify({ ...f, clearPassword }) !== saved);

  // Typical servers: port and security for one click.
  const presets: { name: string; host: string; port: number; security: MailSecurity }[] = [
    { name: 'Яндекс', host: 'smtp.yandex.ru', port: 465, security: 'tls' },
    { name: 'Mail.ru', host: 'smtp.mail.ru', port: 465, security: 'tls' },
    { name: 'Gmail', host: 'smtp.gmail.com', port: 587, security: 'starttls' },
    { name: 'Microsoft 365', host: 'smtp.office365.com', port: 587, security: 'starttls' }
  ];
  function applyPreset(p: (typeof presets)[number]) {
    f.host = p.host;
    f.port = p.port;
    f.security = p.security;
    f.enabled = true;
  }
  function securityChanged() {
    // Suggest the conventional port when the old one belonged to another mode.
    const conventional: Record<MailSecurity, number> = { tls: 465, starttls: 587, none: 25 };
    if ([465, 587, 25].includes(f.port)) f.port = conventional[f.security];
  }

  function payload() {
    return {
      enabled: f.enabled, host: f.host.trim(), port: Number(f.port), security: f.security, username: f.username.trim(),
      password: clearPassword ? '' : f.password ? f.password : null,
      from_email: f.from_email.trim(), from_name: f.from_name.trim(), reply_to: f.reply_to.trim()
    };
  }

  async function save() {
    error = '';
    notice = '';
    try {
      await api.put('/admin/mail', payload());
      await load();
      notice = 'Настройки почты сохранены.';
    } catch (e) {
      error = (e as Error).message;
    }
  }

  async function test() {
    error = '';
    testResult = null;
    testing = true;
    try {
      testResult = await api.post<{ ok: boolean; error?: string }>('/admin/mail/test', { to: testTo.trim(), settings: payload() });
      await refreshLog();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      testing = false;
    }
  }

  async function refreshLog() {
    const r = await api.get<Loaded>('/admin/mail');
    if (data) {
      data.outbox = r.outbox;
      data.available = r.available;
    }
  }
  async function retry(m: OutboxItem) {
    error = '';
    try {
      await api.post(`/admin/mail/outbox/${m.id}/retry`);
      setTimeout(() => refreshLog(), 1500);
      await refreshLog();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  const statusText = { queued: 'в очереди', sent: 'отправлено', failed: 'не доставлено' } as const;
  const statusClass = { queued: 'warn', sent: 'ok', failed: 'danger' } as const;
  const formValid = $derived(!f.enabled || (f.host.trim() !== '' && /.+@.+\..+/.test(f.from_email.trim()) && f.port >= 1 && f.port <= 65535));
</script>

{#if error}<div class="error-box">{error}</div>{/if}
{#if notice}<div class="ok-box">{notice}</div>{/if}

{#if data}
  <div class="status card" class:bad={!data.available}>
    <div>
      {#if data.settings.enabled}
        <b>Почта отправляется через сервер портала</b> {data.settings.host}:{data.settings.port}.
      {:else if data.system}
        <b>Почта отправляется через общий сервер</b> системы от имени {data.system.from_email}. Чтобы письма уходили с адреса вашей организации, подключите свой сервер.
      {:else}
        <b>Почта не настроена.</b> Приглашения и восстановление пароля по почте не работают: приглашения можно передавать ссылкой вручную.
      {/if}
    </div>
    <span class="faint small">Ссылки в письмах ведут на {data.public_url}.</span>
  </div>

  <div class="cols">
    <section class="card box">
      <div class="row head">
        <h3>Почтовый сервер портала</h3>
        <div class="spacer"></div>
        <label class="check"><input type="checkbox" bind:checked={f.enabled} /> Использовать</label>
      </div>
      <div class="presets">
        <span class="faint small">Быстрая настройка:</span>
        {#each presets as p}<button class="btn sm" onclick={() => applyPreset(p)}>{p.name}</button>{/each}
      </div>
      <fieldset disabled={!f.enabled}>
        <div class="grid">
          <label class="field wide">Сервер SMTP <input class="input" bind:value={f.host} placeholder="smtp.example.ru" autocomplete="off" /></label>
          <label class="field">Защита
            <select class="input" bind:value={f.security} onchange={securityChanged}>
              <option value="tls">SSL/TLS (обычно порт 465)</option>
              <option value="starttls">STARTTLS (обычно порт 587)</option>
              <option value="none">Без шифрования</option>
            </select>
          </label>
          <label class="field">Порт <input class="input" type="number" min="1" max="65535" bind:value={f.port} /></label>
          <label class="field">Логин <input class="input" bind:value={f.username} autocomplete="off" placeholder="обычно адрес почты" /></label>
          <label class="field">Пароль
            <input class="input" type="password" bind:value={f.password} autocomplete="new-password" disabled={clearPassword}
                   placeholder={data.settings.has_password ? 'сохранён — оставьте пустым' : 'пароль приложения'} />
            {#if data.settings.has_password}<label class="check small"><input type="checkbox" bind:checked={clearPassword} /> удалить сохранённый пароль</label>{/if}
          </label>
          <label class="field">Адрес отправителя <input class="input" type="email" bind:value={f.from_email} placeholder="noreply@example.ru" /></label>
          <label class="field">Имя отправителя <input class="input" bind:value={f.from_name} placeholder={session.me?.tenant_name} /></label>
          <label class="field wide">Адрес для ответа <input class="input" type="email" bind:value={f.reply_to} placeholder="необязательно, например office@example.ru" /></label>
        </div>
      </fieldset>
      <p class="faint small">
        Для Яндекса, Mail.ru и Gmail нужен пароль приложения, а не пароль от почтового ящика. Пароль хранится в зашифрованном виде и не показывается.
      </p>
      <div class="row">
        <div class="spacer"></div>
        <button class="btn" disabled={!dirty} onclick={load}>Отменить</button>
        <button class="btn primary" disabled={!dirty || !formValid} onclick={save}>Сохранить</button>
      </div>
    </section>

    <section class="card box">
      <h3>Проверка</h3>
      <p class="muted small">Письмо уйдёт с настройками из формы, даже несохранёнными. Ответ сервера покажем здесь.</p>
      <div class="row">
        <input class="input" type="email" bind:value={testTo} aria-label="Адрес для проверки" />
        <button class="btn" disabled={testing || !testTo.trim() || !formValid} onclick={test}>{testing ? 'Отправка…' : 'Отправить'}</button>
      </div>
      {#if testResult?.ok}<div class="ok-box">Письмо принято сервером. Проверьте ящик {testTo}, в том числе папку «Спам».</div>{/if}
      {#if testResult && !testResult.ok}<div class="error-box"><b>Не отправлено:</b> <span class="mono small">{testResult.error}</span></div>{/if}
    </section>
  </div>

  <section class="card log">
    <div class="row head">
      <h3>Журнал отправки</h3>
      <div class="spacer"></div>
      <button class="btn sm" onclick={refreshLog}>Обновить</button>
    </div>
    {#if data.outbox.length === 0}
      <p class="faint empty">Писем ещё не было.</p>
    {:else}
      <div class="scroll">
        <table class="list">
          <thead><tr><th>Создано</th><th>Тип</th><th>Кому</th><th class="hide-sm">Тема</th><th>Статус</th><th></th></tr></thead>
          <tbody>
            {#each data.outbox as m (m.id)}
              <tr>
                <td class="muted nowrap">{formatDateTime(m.created_at)}</td>
                <td>{mailKindLabel[m.kind] ?? m.kind}</td>
                <td>{m.to_email}</td>
                <td class="hide-sm muted">{m.subject}</td>
                <td>
                  <span class="badge {statusClass[m.status]}">{statusText[m.status]}</span>
                  {#if m.status === 'sent' && m.sent_at}<span class="faint small">{formatDateTime(m.sent_at)}</span>{/if}
                  {#if m.status === 'queued' && m.attempts > 0}<span class="faint small">попыток: {m.attempts}, следующая {formatDateTime(m.next_attempt_at)}</span>{/if}
                  {#if m.last_error && m.status !== 'sent'}<div class="err mono small" title={m.last_error}>{m.last_error}</div>{/if}
                </td>
                <td class="actions">{#if m.status !== 'sent'}<button class="btn ghost sm" onclick={() => retry(m)}>Повторить</button>{/if}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </section>
{/if}

<style>
  .error-box, .ok-box { margin-bottom: 12px; }
  .status { padding: 12px 16px; margin-bottom: 14px; display: grid; gap: 4px; border-left: 3px solid var(--ok); }
  .status.bad { border-left-color: var(--warn); }
  .cols { display: grid; grid-template-columns: minmax(0, 3fr) minmax(0, 2fr); gap: 12px; align-items: start; margin-bottom: 14px; }
  .box { padding: 16px; display: grid; gap: 12px; }
  h3 { margin: 0; }
  p { margin: 0; }
  .head { align-items: center; }
  .presets { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  fieldset { border: 0; padding: 0; margin: 0; min-width: 0; }
  fieldset:disabled { opacity: .55; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px 12px; }
  .wide { grid-column: 1 / -1; }
  .check { display: flex; gap: 6px; align-items: center; font-weight: normal; }
  .box .row .input { flex: 1; min-width: 0; }
  .ok-box, .error-box { margin: 0; }
  .log { padding: 14px 16px; }
  .log .head { margin-bottom: 8px; }
  .scroll { overflow-x: auto; }
  .nowrap { white-space: nowrap; }
  .actions { text-align: right; }
  .err { color: var(--danger); max-width: 420px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  td .faint.small { display: block; }
  .empty { padding: 8px 0; }
  @media (max-width: 960px) { .cols { grid-template-columns: 1fr; } }
  @media (max-width: 560px) { .grid { grid-template-columns: 1fr; } .hide-sm { display: none; } }
</style>
