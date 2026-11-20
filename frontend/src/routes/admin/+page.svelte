<script lang="ts">
  import { actionLabel, api, formatDateTime, formatSize, type AuditEntry, type FieldDef, type Overview } from '#lib/api.ts';
  import { branding } from '#lib/branding.svelte.ts';

  let stats = $state<Overview | null>(null);
  let recent = $state<AuditEntry[]>([]);
  let fieldCount = $state(0);
  let templateCount = $state(0);
  let error = $state('');

  $effect(() => {
    Promise.all([
      api.get<{ stats: Overview; recent: AuditEntry[] }>('/admin/overview'),
      api.get<{ fields: FieldDef[] }>('/project-fields'),
      api.get<{ templates: unknown[] }>('/templates')
    ])
      .then(([o, f, t]) => {
        templateCount = t.templates.length;
        stats = o.stats;
        recent = o.recent;
        fieldCount = f.fields.filter((d) => !d.is_archived).length;
      })
      .catch((e) => (error = e.message));
  });

  const pct = (a: number, b: number) => (b > 0 ? Math.min(100, Math.round((a / b) * 100)) : 0);

  // Setup checklist: what a fresh portal still lacks.
  const checklist = $derived(
    stats
      ? [
          { done: stats.users_total > 1, label: 'Добавить сотрудников', href: '/admin/users' },
          { done: fieldCount > 0, label: 'Настроить реквизиты проектов', href: '/admin/requisites' },
          { done: templateCount > 0, label: 'Загрузить шаблоны документов', href: '/admin/templates' },
          { done: !!(branding.current?.has_logo || branding.current?.has_icon), label: 'Загрузить логотип портала', href: '/admin/branding' },
          {
            done: stats.users_active > 0 && stats.users_mfa === stats.users_active,
            label: `Включить 2FA всем сотрудникам (${stats.users_mfa} из ${stats.users_active})`,
            href: '/admin/users'
          },
          { done: stats.projects_total > 0, label: 'Создать первый проект', href: '/projects' }
        ]
      : []
  );
</script>

{#if error}<div class="error-box">{error}</div>{/if}

{#if stats}
  <div class="tiles">
    <a class="card tile" href="/admin/users">
      <span class="t-label">Пользователи</span>
      <span class="t-value">{stats.users_active}<span class="t-of">&nbsp;/ {stats.users_total}</span></span>
      <span class="t-note">активных · {stats.users_guests} гостей · онлайн {stats.sessions_active}</span>
    </a>
    <a class="card tile" href="/admin/users">
      <span class="t-label">Двухфакторная защита</span>
      <span class="t-value">{pct(stats.users_mfa, stats.users_active)}%</span>
      <div class="bar" aria-hidden="true"><div style="width:{pct(stats.users_mfa, stats.users_active)}%" class:good={stats.users_mfa === stats.users_active}></div></div>
      <span class="t-note">{stats.users_mfa} из {stats.users_active} с 2FA</span>
    </a>
    <a class="card tile" href="/admin/portal">
      <span class="t-label">Хранилище</span>
      <span class="t-value">{formatSize(stats.storage_used)}</span>
      <div class="bar" aria-hidden="true"><div style="width:{pct(stats.storage_used, stats.storage_quota)}%" class:warn={pct(stats.storage_used, stats.storage_quota) > 85}></div></div>
      <span class="t-note">из {formatSize(stats.storage_quota)} · {stats.files_total} файлов</span>
    </a>
    <a class="card tile" href="/projects">
      <span class="t-label">Проекты</span>
      <span class="t-value">{stats.projects_active}<span class="t-of">&nbsp;/ {stats.projects_total}</span></span>
      <span class="t-note">активных</span>
    </a>
    <a class="card tile" href="/projects">
      <span class="t-label">Задачи в работе</span>
      <span class="t-value">{stats.tasks_open}</span>
      <span class="t-note" class:danger={stats.tasks_overdue > 0}>{stats.tasks_overdue} просрочено</span>
    </a>
    <div class="card tile">
      <span class="t-label">Открыто в редакторе</span>
      <span class="t-value">{stats.docs_editing}</span>
      <span class="t-note">документов сейчас</span>
    </div>
  </div>

  <div class="cols">
    <section class="card block">
      <div class="row head"><h2>Последние действия</h2><div class="spacer"></div><a href="/admin/audit" class="small">Весь журнал</a></div>
      {#if recent.length === 0}
        <p class="faint">Пока пусто.</p>
      {:else}
        <ul class="feed">
          {#each recent as e (e.id)}
            <li>
              <span class="who">{e.user_name ?? 'Система'}</span>
              <span class="what">{actionLabel[e.action] ?? e.action}{#if e.target_name}{' · '}<span class="muted">{e.target_name}</span>{/if}</span>
              <time class="faint small">{formatDateTime(e.created_at)}</time>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="card block">
      <h2>Настройка портала</h2>
      <ul class="checklist">
        {#each checklist as c}
          <li class:done={c.done}>
            <span class="mark" aria-hidden="true">{c.done ? '✓' : ''}</span>
            {#if c.done}<span>{c.label}</span>{:else}<a href={c.href}>{c.label}</a>{/if}
          </li>
        {/each}
      </ul>
    </section>
  </div>
{/if}

<style>
  .tiles { display: grid; grid-template-columns: repeat(6, minmax(0, 1fr)); gap: 12px; margin-bottom: 16px; }
  @media (max-width: 1250px) { .tiles { grid-template-columns: repeat(3, minmax(0, 1fr)); } }
  @media (max-width: 640px) { .tiles { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  .tile { display: grid; gap: 4px; padding: 14px 16px; color: var(--text); align-content: start; }
  a.tile:hover { text-decoration: none; border-color: var(--accent); }
  .t-label { font-size: 12px; color: var(--text-3); text-transform: uppercase; letter-spacing: .04em; }
  .t-value { font-size: 26px; font-weight: 600; font-variant-numeric: tabular-nums; line-height: 1.2; }
  .t-of { font-size: 16px; color: var(--text-3); font-weight: 500; }
  .t-note { font-size: 12.5px; color: var(--text-2); }
  .t-note.danger { color: var(--danger); }
  .bar { height: 6px; background: var(--surface-2); border-radius: 3px; overflow: hidden; margin: 2px 0; }
  .bar div { height: 100%; background: var(--accent); }
  .bar div.good { background: var(--ok); }
  .bar div.warn { background: var(--warn); }
  .cols { display: grid; grid-template-columns: 3fr 2fr; gap: 12px; align-items: start; }
  .block { padding: 16px; }
  .head { margin-bottom: 8px; }
  .head h2 { margin: 0; }
  .feed, .checklist { list-style: none; margin: 0; padding: 0; }
  .feed li { display: grid; grid-template-columns: 140px 1fr auto; gap: 10px; padding: 7px 0; border-bottom: 1px solid var(--border); align-items: baseline; }
  .feed li:last-child { border-bottom: 0; }
  .who { font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .what { min-width: 0; overflow-wrap: anywhere; }
  .checklist li { display: flex; gap: 10px; align-items: center; padding: 7px 0; }
  .mark {
    width: 20px; height: 20px; border-radius: 50%; border: 1.5px solid var(--border-strong);
    display: grid; place-items: center; font-size: 12px; flex: none;
  }
  .done .mark { background: var(--ok); border-color: var(--ok); color: #fff; }
  .done span:last-child { color: var(--text-3); }
  p { margin: 0; }
  @media (max-width: 900px) {
    .cols { grid-template-columns: 1fr; }
    .feed li { grid-template-columns: 1fr; gap: 0; }
  }
</style>
