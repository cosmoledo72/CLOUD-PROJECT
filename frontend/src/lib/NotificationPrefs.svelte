<script lang="ts">
  // Per-kind channel settings: in the app and by email.
  import { api, type NotificationPref } from './api';

  let prefs = $state<NotificationPref[]>([]);
  let emailAvailable = $state(true);
  let email = $state('');
  let saved = $state('');
  let error = $state('');
  let notice = $state('');

  async function load() {
    const r = await api.get<{ prefs: NotificationPref[]; email_available: boolean; email: string }>('/notifications/prefs');
    prefs = r.prefs;
    emailAvailable = r.email_available;
    email = r.email;
    saved = JSON.stringify(prefs);
  }
  $effect(() => {
    load().catch((e) => (error = e.message));
  });

  const dirty = $derived(JSON.stringify(prefs) !== saved);
  const groups = $derived([...new Set(prefs.map((p) => p.group))]);

  async function save() {
    error = '';
    notice = '';
    try {
      await api.put('/notifications/prefs', prefs.map((p) => ({ kind: p.kind, in_app: p.in_app, email: p.email })));
      saved = JSON.stringify(prefs);
      notice = 'Настройки уведомлений сохранены.';
    } catch (e) {
      error = (e as Error).message;
    }
  }
  function defaults() {
    for (const p of prefs) {
      p.in_app = p.default_in_app;
      p.email = p.default_email;
    }
  }
</script>

<section class="card" id="notifications">
  <h2>Уведомления</h2>
  <p class="muted">
    Уведомления приходят в колокольчик в приложении. По почте ({email}) — только непрочитанные:
    если вы увидели уведомление в приложении в течение нескольких минут, письма не будет. Несколько уведомлений собираются в одно письмо.
  </p>
  {#if !emailAvailable}<div class="warn-box">Почта портала не настроена — письма не отправляются. Обратитесь к администратору.</div>{/if}
  {#if error}<div class="error-box">{error}</div>{/if}
  {#if notice}<div class="ok-box">{notice}</div>{/if}
  <table class="list">
    <thead><tr><th>Событие</th><th class="c">В приложении</th><th class="c">По почте</th></tr></thead>
    <tbody>
      {#each groups as g}
        <tr class="group"><td colspan="3">{g}</td></tr>
        {#each prefs.filter((p) => p.group === g) as p (p.kind)}
          <tr>
            <td>{p.label}</td>
            <td class="c"><input type="checkbox" bind:checked={p.in_app} aria-label="{p.label}: в приложении" /></td>
            <td class="c"><input type="checkbox" bind:checked={p.email} aria-label="{p.label}: по почте" /></td>
          </tr>
        {/each}
      {/each}
    </tbody>
  </table>
  <div class="row actions">
    <button class="btn ghost sm" onclick={defaults}>По умолчанию</button>
    <div class="spacer"></div>
    <button class="btn" disabled={!dirty} onclick={load}>Отменить</button>
    <button class="btn primary" disabled={!dirty} onclick={save}>Сохранить</button>
  </div>
</section>

<style>
  section { padding: 18px; margin-bottom: 14px; }
  section p { margin: 0 0 12px; }
  .c { text-align: center; width: 120px; }
  .group td { font-weight: 600; color: var(--text-2); background: var(--surface-2); font-size: 12px; text-transform: uppercase; letter-spacing: .04em; }
  .actions { margin-top: 12px; }
  .warn-box, .error-box, .ok-box { margin-bottom: 12px; }
  .warn-box { background: var(--warn-soft); color: var(--warn); padding: 8px 12px; border-radius: var(--radius); }
  input[type='checkbox'] { width: 16px; height: 16px; cursor: pointer; }
</style>
