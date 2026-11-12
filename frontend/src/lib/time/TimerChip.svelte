<script lang="ts">
  // Floating chip of the running timer, on every page: task, elapsed time,
  // «Стоп» (with an optional comment) and «Сбросить» (drop without recording).
  import Square from '@lucide/svelte/icons/square';
  import Timer from '@lucide/svelte/icons/timer';
  import X from '@lucide/svelte/icons/x';
  import { hm, clock, timer } from './timer.svelte';

  let open = $state(false);
  let note = $state('');
  let busy = $state(false);
  let done = $state('');

  async function stop() {
    busy = true;
    try {
      const e = await timer.stop(note);
      note = '';
      open = false;
      done = e ? `Записано: ${hm(e.minutes)}` : 'Меньше минуты — не записано';
      setTimeout(() => (done = ''), 3500);
    } finally {
      busy = false;
    }
  }
  async function discard() {
    if (!confirm('Сбросить таймер без записи времени?')) return;
    await timer.discard();
    open = false;
  }
</script>

{#if timer.timer}
  {@const t = timer.timer}
  <div class="chip" class:open role="region" aria-label="Таймер">
    <div class="row">
      <span class="dot" aria-hidden="true"></span>
      <a class="task" href="/projects/{t.project_id}?task={t.task_id}" title="{t.project_name} · #{t.task_number} {t.task_title}">
        <span class="t">{t.task_title}</span>
        <span class="p">{t.project_name}</span>
      </a>
      <span class="clock nums" aria-live="off">{clock(timer.seconds)}</span>
      <button class="stop" onclick={() => (open = !open)} title="Остановить" aria-expanded={open}><Square size={13} fill="currentColor" /></button>
    </div>
    {#if open}
      <div class="more">
        <input class="input" placeholder="Комментарий (необязательно)" bind:value={note} onkeydown={(e) => e.key === 'Enter' && stop()} />
        <div class="acts">
          <button class="btn ghost sm" onclick={discard} title="Сбросить без записи"><X size={14} />Сбросить</button>
          <button class="btn primary sm" disabled={busy} onclick={stop}><Timer size={14} />Записать {hm(Math.max(1, Math.round(timer.seconds / 60)))}</button>
        </div>
      </div>
    {/if}
  </div>
{:else if done}
  <div class="chip toast" role="status">{done}</div>
{/if}

<style>
  .chip { position: fixed; right: 20px; bottom: 20px; z-index: 60; width: min(360px, calc(100vw - 32px)); background: var(--surface);
          border: 1px solid var(--accent); border-radius: 14px; box-shadow: 0 12px 32px rgba(0, 0, 0, .18); padding: 8px 8px 8px 12px; }
  .row { display: flex; align-items: center; gap: 10px; }
  .dot { width: 9px; height: 9px; border-radius: 50%; background: var(--danger); flex: none; animation: pulse 1.6s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: .35; } }
  .task { flex: 1; min-width: 0; display: grid; color: var(--text); line-height: 1.25; }
  .task:hover { text-decoration: none; }
  .task:hover .t { color: var(--accent); }
  .t { font-weight: 600; font-size: 13.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .p { font-size: 11.5px; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .clock { font-size: 16px; font-weight: 600; font-variant-numeric: tabular-nums; letter-spacing: .02em; }
  .stop { width: 32px; height: 32px; border-radius: 9px; border: 0; background: var(--danger-soft); color: var(--danger); display: grid; place-items: center; cursor: pointer; flex: none; }
  .stop:hover { background: var(--danger); color: #fff; }
  .more { display: grid; gap: 8px; margin-top: 8px; padding-top: 8px; border-top: 1px solid var(--border); }
  .acts { display: flex; justify-content: space-between; gap: 8px; }
  .acts .btn { display: inline-flex; align-items: center; gap: 5px; }
  .toast { padding: 10px 14px; font-size: 13.5px; border-color: var(--ok); width: auto; }
  @media (max-width: 720px) {
    .chip { right: 12px; left: 12px; bottom: 12px; width: auto; }
  }
  @media print { .chip { display: none; } }
</style>
