<script lang="ts">
  // «Резервные копии»: state of the backup service, list of backups,
  // «make a backup now», downloads and how to restore. The backups are made
  // by the backup service (scripts/backup.sh); restoring stays a server command
  // because the portal is stopped while it runs.
  import { api, formatSize, type BackupItem, type BackupStatus } from '$lib/api';
  import { session } from '$lib/session.svelte';
  import EmptyState from '$lib/EmptyState.svelte';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import Copy from '@lucide/svelte/icons/copy';
  import DatabaseBackup from '@lucide/svelte/icons/database-backup';
  import Download from '@lucide/svelte/icons/download';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Terminal from '@lucide/svelte/icons/terminal';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

  let data = $state<BackupStatus | null>(null);
  let error = $state('');
  let busy = $state(false);
  let copied = $state('');

  async function load() {
    try {
      data = await api.get<BackupStatus>('/admin/reserve');
      error = '';
    } catch (e) {
      error = (e as Error).message;
    }
  }
  $effect(() => {
    if (!session.me?.backups) return;
    load();
  });
  // While a backup is requested or being written, refresh every 10 seconds.
  const waiting = $derived(!!data && (data.requested_at !== null || data.running));
  $effect(() => {
    if (!waiting) return;
    const t = setInterval(load, 10_000);
    return () => clearInterval(t);
  });

  async function makeNow() {
    busy = true;
    try {
      data = await api.post<BackupStatus>('/admin/reserve/now');
      error = '';
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  const ready = $derived((data?.backups ?? []).filter((b) => b.state === 'ready'));
  const latest = $derived(ready[0] ?? null);
  const total = (b: BackupItem) => (b.db_bytes ?? 0) + (b.files_bytes ?? 0);

  function ago(sec: number): string {
    const d = Math.max(0, (data?.now ?? Date.now() / 1000) - sec);
    if (d < 90) return 'только что';
    if (d < 3600) return `${Math.round(d / 60)} мин назад`;
    if (d < 86400 * 2) return `${Math.round(d / 3600)} ч назад`;
    return `${Math.round(d / 86400)} дн. назад`;
  }
  /** «12.11.2026 03:00» from the folder name cloud-project-2026-11-12_0300. */
  function when(b: BackupItem): string {
    const m = b.name.match(/(\d{4})-(\d{2})-(\d{2})_(\d{2})(\d{2})/);
    return m ? `${m[3]}.${m[2]}.${m[1]} ${m[4]}:${m[5]}` : b.name;
  }
  const stateText = { ready: 'Готова', running: 'Создаётся…', unfinished: 'Не завершена' };

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      copied = text;
      setTimeout(() => (copied = ''), 2000);
    } catch {
      /* clipboard unavailable */
    }
  }
  const hh = $derived(String(data?.hour ?? 3).padStart(2, '0'));
</script>

{#if !session.me?.backups}
  <div class="card pad"><p class="muted none">Раздел доступен администраторам системного портала.</p></div>
{:else}
  {#if error}<div class="error-box">{error}</div>{/if}
  {#if data}
    <div class="top">
      <section class="card state" class:bad={!data.alive || data.warnings.length > 0}>
        <div class="st-head">
          {#if data.alive && !data.warnings.length}
            <i class="chip-icon ok"><CircleCheck size={20} /></i>
            <div><b>Резервное копирование работает</b><span class="muted">Копия создаётся каждую ночь в {hh}:00, хранится {data.keep_days} дн.</span></div>
          {:else}
            <i class="chip-icon danger"><TriangleAlert size={20} /></i>
            <div><b>{data.alive ? 'Требует внимания' : 'Сервис копирования не отвечает'}</b><span class="muted">Ночная копия — в {hh}:00, хранение {data.keep_days} дн.</span></div>
          {/if}
        </div>
        {#if data.warnings.length}
          <ul class="warn">{#each data.warnings as w}<li>{w}</li>{/each}</ul>
        {/if}
        <dl class="facts">
          <div><dt>Последняя готовая копия</dt><dd>{#if latest}{when(latest)} <span class="faint">· {ago(latest.time)}</span>{:else}—{/if}</dd></div>
          <div><dt>Размер последней</dt><dd>{latest ? formatSize(total(latest)) : '—'}</dd></div>
          <div><dt>Сервис</dt><dd>{#if data.heartbeat}{data.alive ? 'на связи' : 'молчит'} <span class="faint">· {ago(data.heartbeat)}</span>{:else}нет данных{/if}</dd></div>
          <div><dt>Копий хранится</dt><dd>{ready.length}</dd></div>
        </dl>
      </section>

      <section class="card now">
        <h2>Копия вне расписания</h2>
        <p class="muted small">Например, перед обновлением портала или массовыми изменениями. Сервис возьмёт заявку в течение минуты; копия занимает от нескольких секунд до нескольких минут.</p>
        {#if data.requested_at !== null}
          <div class="pending"><RefreshCw size={15} class="spin" />Заявка принята {ago(data.requested_at)}, ждём сервис…</div>
        {:else if data.running}
          <div class="pending"><RefreshCw size={15} class="spin" />Копия создаётся…</div>
        {:else}
          <button class="btn primary" onclick={makeNow} disabled={busy || !data.available}><DatabaseBackup size={16} />Сделать копию сейчас</button>
        {/if}
        {#if data.last_result.result === 'ok' && data.last_result.time}
          <p class="faint small last">Последняя попытка: {data.last_result.time} — успешно</p>
        {/if}
      </section>
    </div>

    <section class="card list">
      <div class="list-head">
        <h2>Копии</h2>
        <button class="btn ghost sm" onclick={load} title="Обновить"><RefreshCw size={14} />Обновить</button>
      </div>
      {#if !data.backups.length}
        <EmptyState icon={DatabaseBackup} title="Копий пока нет" text="Первая появится ночью в {hh}:00 или после нажатия «Сделать копию сейчас»." />
      {:else}
        <div class="scroll">
          <table>
            <thead><tr><th>Дата и время</th><th>Состояние</th><th class="num">База</th><th class="num">Файлы</th><th class="num">Всего</th><th>Скачать</th></tr></thead>
            <tbody>
              {#each data.backups as b (b.name)}
                <tr class:dim={b.state !== 'ready'}>
                  <td>
                    <b class="nums">{when(b)}</b>
                    <div class="faint small mono" title="Имя копии для восстановления">{b.name}
                      {#if b.state === 'ready'}<button class="cp" onclick={() => copy(b.name)} title="Скопировать имя" aria-label="Скопировать имя {b.name}"><Copy size={12} /></button>{#if copied === b.name}<span class="ok-t">скопировано</span>{/if}{/if}
                    </div>
                    <div class="m-only small">{b.state === 'ready' ? formatSize(total(b)) : stateText[b.state]}</div>
                  </td>
                  <td><span class="tag t-{b.state}">{stateText[b.state]}</span></td>
                  <td class="num">{b.db_bytes !== null ? formatSize(b.db_bytes) : '—'}</td>
                  <td class="num">{b.files_bytes !== null ? formatSize(b.files_bytes) : '—'}</td>
                  <td class="num"><b>{formatSize(total(b))}</b></td>
                  <td class="dl">
                    {#if b.state === 'ready'}
                      <a class="btn ghost sm" href="/api/admin/reserve/{b.name}/db.dump" download title="Дамп базы данных PostgreSQL"><Download size={14} />База</a>
                      {#if b.files_bytes !== null}<a class="btn ghost sm" href="/api/admin/reserve/{b.name}/files.tar.gz" download title="Архив документов и файлов портала"><Download size={14} />Файлы</a>{/if}
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>

    <section class="card restore">
      <h2><Terminal size={17} />Восстановление</h2>
      <p class="muted small">Восстановление заменяет <b>все</b> данные портала содержимым копии: базу, документы и их версии. На время восстановления портал останавливается, поэтому оно запускается на сервере, а не отсюда.</p>
      <ol class="small">
        <li>Откройте командную строку в папке портала на сервере.</li>
        <li>Посмотреть список копий: <code>scripts\restore.cmd</code></li>
        <li>Восстановить: <code>scripts\restore.cmd {latest?.name ?? 'cloud-project-ГГГГ-ММ-ДД_ЧЧММ'}</code>{#if latest}<button class="cp" onclick={() => copy(`scripts\\restore.cmd ${latest.name}`)} title="Скопировать команду" aria-label="Скопировать команду"><Copy size={12} /></button>{#if copied === `scripts\\restore.cmd ${latest.name}`}<span class="ok-t">скопировано</span>{/if}{/if} и подтвердите словом <code>YES</code>.</li>
        <li>Портал запустится сам; войдите и проверьте документы.</li>
      </ol>
      <p class="faint small">Скачанные копии храните вне сервера (внешний диск, облако): это защищает от потери самого сервера. Для отдельного файла или папки копия не нужна — есть корзина и версии документов.</p>
    </section>
  {:else if !error}
    <p class="faint">Загрузка…</p>
  {/if}
{/if}

<style>
  .pad { padding: 20px; }
  .none { margin: 0; }
  h2 { font-size: 15px; margin: 0; display: flex; align-items: center; gap: 8px; }
  .top { display: grid; grid-template-columns: minmax(0, 1.6fr) minmax(260px, 1fr); gap: 14px; margin-bottom: 14px; }
  .state, .now, .list, .restore { padding: 16px 18px; }
  .st-head { display: flex; gap: 12px; align-items: center; }
  .st-head div { display: grid; gap: 2px; }
  .st-head b { font-size: 15.5px; }
  .st-head .muted { font-size: 13px; }
  .warn { margin: 12px 0 0; padding: 10px 12px 10px 28px; background: var(--danger-soft); color: var(--danger); border-radius: var(--radius); font-size: 13px; display: grid; gap: 4px; }
  .facts { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px 16px; margin: 16px 0 0; }
  .facts div { display: grid; gap: 2px; }
  .facts dt { font-size: 12px; color: var(--text-3); }
  .facts dd { margin: 0; font-weight: 500; font-variant-numeric: tabular-nums; }
  .now { display: grid; gap: 10px; align-content: start; }
  .now p { margin: 0; }
  .now .btn { justify-self: start; }
  .pending { display: inline-flex; gap: 8px; align-items: center; color: var(--accent); font-weight: 500; font-size: 13.5px; }
  .pending :global(.spin) { animation: spin 1.2s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .last { margin: 0; }
  .list { margin-bottom: 14px; padding-bottom: 6px; }
  .list-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px; }
  .scroll { overflow-x: auto; margin: 0 -18px; }
  table { width: 100%; border-collapse: collapse; font-size: 13.5px; }
  th { text-align: left; font-weight: 500; color: var(--text-3); font-size: 12.5px; padding: 8px 18px; border-bottom: 1px solid var(--border); white-space: nowrap; }
  td { padding: 10px 18px; border-bottom: 1px solid var(--border); vertical-align: middle; }
  tbody tr:last-child td { border-bottom: 0; }
  tr.dim td { color: var(--text-3); }
  .num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .nums { font-variant-numeric: tabular-nums; }
  .mono { font-family: ui-monospace, 'JetBrains Mono', Consolas, monospace; font-size: 11.5px; display: flex; align-items: center; gap: 4px; white-space: nowrap; }
  .dl { white-space: nowrap; text-align: right; }
  .tag { font-size: 12px; padding: 2px 9px; border-radius: 999px; font-weight: 500; white-space: nowrap; }
  .t-ready { background: var(--ok-soft); color: var(--ok); }
  .t-running { background: var(--accent-soft); color: var(--accent); }
  .t-unfinished { background: var(--warn-soft); color: var(--warn); }
  .m-only { display: none; }
  .cp { border: 0; background: none; color: var(--text-3); cursor: pointer; padding: 2px; display: inline-grid; vertical-align: middle; border-radius: 4px; }
  .cp:hover { color: var(--accent); background: var(--surface-2); }
  .ok-t { color: var(--ok); font-size: 11.5px; margin-left: 2px; }
  .restore p { margin: 8px 0; }
  .restore ol { margin: 8px 0; padding-left: 20px; display: grid; gap: 6px; }
  code { font-family: ui-monospace, 'JetBrains Mono', Consolas, monospace; font-size: 12.5px; background: var(--surface-2); border: 1px solid var(--border); padding: 1px 6px; border-radius: 5px; }
  @media (max-width: 860px) { .top { grid-template-columns: 1fr; } }
  @media (max-width: 640px) {
    th:nth-child(n+2):nth-child(-n+5), td:nth-child(n+2):nth-child(-n+5) { display: none; }
    .m-only { display: block; margin-top: 2px; font-weight: 500; }
    td, th { padding: 10px 12px; }
    .scroll { margin: 0 -18px; }
    .dl { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; }
  }
</style>
