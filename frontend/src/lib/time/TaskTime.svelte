<script lang="ts">
  // «Учёт времени» of a task card: the timer button, a manual entry
  // (date, «1:30» / «1,5 ч», comment), entries by day with editing, and —
  // for project managers and finance — how the work is paid: by the hour
  // (hours × the employee's rate) or a fixed amount for the volume.
  import Pencil from '@lucide/svelte/icons/pencil';
  import Play from '@lucide/svelte/icons/play';
  import Square from '@lucide/svelte/icons/square';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { api, formatDate, localToday } from '$lib/api';
  import { money } from '$lib/fieldLook';
  import { clock, hm, parseDuration, timer } from './timer.svelte';

  let { taskId, onchanged }: { taskId: string; onchanged?: () => void } = $props();

  interface Entry {
    id: string;
    user_id: string;
    user_name: string;
    work_date: string;
    minutes: number;
    note: string;
    source: 'manual' | 'timer';
    hour_rate: number | null;
    can_edit: boolean;
  }
  interface Pay {
    pay_mode: 'hourly' | 'fixed';
    pay_amount: number | null;
    minutes: number;
    labor: { accrued: number; expected: number; unrated_minutes?: number } | null;
    can_edit: boolean;
    sees_rates: boolean;
  }

  let entries = $state<Entry[]>([]);
  let total = $state(0);
  let canLog = $state(false);
  let pay = $state<Pay | null>(null);
  let error = $state('');

  async function load() {
    try {
      const r = await api.get<{ entries: Entry[]; total_minutes: number; can_log: boolean }>(`/tasks/${taskId}/time`);
      entries = r.entries;
      total = r.total_minutes;
      canLog = r.can_log;
    } catch (e) {
      error = (e as Error).message;
    }
    pay = await api.get<Pay>(`/tasks/${taskId}/pay`).catch(() => null);
  }
  $effect(() => {
    void taskId;
    void timer.version;
    load();
  });

  const running = $derived(timer.isOn(taskId));
  async function toggleTimer() {
    error = '';
    try {
      if (running) await timer.stop();
      else await timer.start(taskId);
      await load();
      onchanged?.();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  // --- Manual entry ---------------------------------------------------------
  let date = $state(localToday());
  let dur = $state('');
  let note = $state('');
  const parsed = $derived(parseDuration(dur));
  async function add() {
    if (!parsed) return;
    error = '';
    try {
      await api.post(`/tasks/${taskId}/time`, { work_date: date, minutes: parsed, note });
      dur = '';
      note = '';
      await load();
      onchanged?.();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  // --- Editing ----------------------------------------------------------------
  let editId = $state<string | null>(null);
  let eDate = $state('');
  let eDur = $state('');
  let eNote = $state('');
  function edit(e: Entry) {
    editId = e.id;
    eDate = e.work_date;
    eDur = `${Math.floor(e.minutes / 60)}:${String(e.minutes % 60).padStart(2, '0')}`;
    eNote = e.note;
  }
  async function save() {
    const m = parseDuration(eDur);
    if (!editId || !m) return;
    error = '';
    try {
      await api.patch(`/time/${editId}`, { work_date: eDate, minutes: m, note: eNote });
      editId = null;
      await load();
      onchanged?.();
    } catch (e) {
      error = (e as Error).message;
    }
  }
  async function remove(e: Entry) {
    if (!confirm(`Удалить запись ${hm(e.minutes)} от ${formatDate(e.work_date)}?`)) return;
    try {
      await api.del(`/time/${e.id}`);
      await load();
      onchanged?.();
    } catch (err) {
      error = (err as Error).message;
    }
  }

  // Entries grouped by day.
  const days = $derived.by(() => {
    const map = new Map<string, Entry[]>();
    for (const e of entries) {
      if (!map.has(e.work_date)) map.set(e.work_date, []);
      map.get(e.work_date)!.push(e);
    }
    return [...map.entries()].map(([d, list]) => ({ date: d, list, sum: list.reduce((s, x) => s + x.minutes, 0) }));
  });
  let showAll = $state(false);
  const shownDays = $derived(showAll ? days : days.slice(0, 5));

  // --- Pay ------------------------------------------------------------------------
  let payMode = $state<'hourly' | 'fixed'>('hourly');
  let payAmount = $state('');
  $effect(() => {
    if (pay) {
      payMode = pay.pay_mode;
      payAmount = pay.pay_amount != null ? String(pay.pay_amount) : '';
    }
  });
  async function savePay(mode = payMode) {
    if (!pay) return;
    const amount = Number(payAmount.replace(/\s/g, '').replace(',', '.'));
    if (mode === 'fixed' && (!payAmount.trim() || !Number.isFinite(amount))) {
      payMode = 'fixed';
      return;
    }
    error = '';
    try {
      await api.put(`/tasks/${taskId}/pay`, { pay_mode: mode, pay_amount: mode === 'fixed' ? amount : null });
      await load();
    } catch (e) {
      error = (e as Error).message;
    }
  }
</script>

<section class="sec tt">
  <div class="head">
    <h3>Учёт времени</h3>
    <span class="tot">{total ? hm(total) : 'нет записей'}</span>
    <span class="spacer"></span>
    {#if canLog}
      <button class="btn tbtn" class:on={running} onclick={toggleTimer} title={running ? 'Остановить и записать время' : 'Начать отсчёт времени по задаче'}>
        {#if running}<Square size={13} fill="currentColor" /><span class="nums">{clock(timer.seconds)}</span>
        {:else}<Play size={14} fill="currentColor" />{total ? 'Продолжить' : 'Начать'}{/if}
      </button>
    {/if}
  </div>

  {#if canLog}
    <div class="add">
      <input class="input d" type="date" bind:value={date} aria-label="Дата" />
      <input class="input dur" placeholder="1:30" bind:value={dur} aria-label="Длительность" title="Часы и минуты: 1:30, 1,5 ч, 90 мин"
             onkeydown={(e) => e.key === 'Enter' && add()} />
      <input class="input nt" placeholder="Комментарий" bind:value={note} aria-label="Комментарий" onkeydown={(e) => e.key === 'Enter' && add()} />
      <button class="btn" disabled={!parsed} onclick={add}>Записать{parsed ? ` ${hm(parsed)}` : ''}</button>
    </div>
    {#if dur.trim() && !parsed}<p class="hint">Не понял длительность: введите «1:30», «1,5 ч» или «90 мин».</p>{/if}
  {/if}

  {#if days.length}
    <div class="days">
      {#each shownDays as d (d.date)}
        <div class="day">
          <div class="dh"><span>{formatDate(d.date)}</span><span class="nums">{hm(d.sum)}</span></div>
          {#each d.list as e (e.id)}
            {#if editId === e.id}
              <div class="ent edit">
                <input class="input d" type="date" bind:value={eDate} aria-label="Дата" />
                <input class="input dur" bind:value={eDur} aria-label="Длительность" />
                <input class="input nt" bind:value={eNote} placeholder="Комментарий" aria-label="Комментарий" onkeydown={(ev) => ev.key === 'Enter' && save()} />
                <button class="btn sm primary" disabled={!parseDuration(eDur)} onclick={save}>Сохранить</button>
                <button class="btn sm ghost" onclick={() => (editId = null)}>Отмена</button>
              </div>
            {:else}
              <div class="ent">
                <span class="who">{e.user_name}</span>
                <span class="nums m">{hm(e.minutes)}</span>
                {#if e.source === 'timer'}<span class="src" title="Записано таймером">таймер</span>{/if}
                <span class="note">{e.note}</span>
                {#if e.hour_rate != null && pay?.pay_mode !== 'fixed'}<span class="rate nums" title="Ставка на момент записи">{money(e.hour_rate)}/ч</span>{/if}
                {#if e.can_edit}
                  <button class="ib" onclick={() => edit(e)} title="Изменить" aria-label="Изменить запись"><Pencil size={13} /></button>
                  <button class="ib" onclick={() => remove(e)} title="Удалить" aria-label="Удалить запись"><Trash2 size={13} /></button>
                {/if}
              </div>
            {/if}
          {/each}
        </div>
      {/each}
      {#if days.length > 5}
        <button class="more" onclick={() => (showAll = !showAll)}>{showAll ? 'Свернуть' : `Ещё ${days.length - 5} дн.`}</button>
      {/if}
    </div>
  {/if}

  {#if pay}
    <div class="pay">
      <span class="pl">Оплата работы</span>
      {#if pay.can_edit}
        <div class="seg" role="radiogroup" aria-label="Оплата работы">
          <button role="radio" aria-checked={payMode === 'hourly'} class:on={payMode === 'hourly'} onclick={() => { payMode = 'hourly'; savePay('hourly'); }}>По часам</button>
          <button role="radio" aria-checked={payMode === 'fixed'} class:on={payMode === 'fixed'} onclick={() => { payMode = 'fixed'; if (payAmount) savePay('fixed'); }}>Фикс. за объём</button>
        </div>
        {#if payMode === 'fixed'}
          <input class="input amt" inputmode="decimal" placeholder="Сумма, ₽" bind:value={payAmount} onchange={() => savePay('fixed')} aria-label="Сумма за объём" />
        {/if}
      {:else}
        <span>{pay.pay_mode === 'fixed' ? `фиксированная: ${money(pay.pay_amount ?? 0)}` : 'по часам'}</span>
      {/if}
      <span class="spacer"></span>
      {#if pay.labor}
        <span class="lab" title={pay.pay_mode === 'fixed' ? 'Фиксированная сумма засчитывается, когда задача выполнена' : 'Часы × ставка сотрудника на момент записи'}>
          {#if pay.labor.expected}ожидается {money(pay.labor.expected)}{:else}трудозатраты {money(pay.labor.accrued)}{/if}
        </span>
      {/if}
    </div>
    {#if pay.pay_mode === 'hourly' && pay.labor?.unrated_minutes}
      <p class="hint">{hm(pay.labor.unrated_minutes)} без ставки — не учтены в деньгах. Ставки задаются в «Отчёты → Ставки».</p>
    {/if}
  {/if}
  {#if error}<div class="error-box">{error}</div>{/if}
</section>

<style>
  .head { display: flex; align-items: center; gap: 10px; }
  .head h3 { margin: 0; white-space: nowrap; }
  .head { flex-wrap: wrap; }
  .tot { color: var(--text-3); font-size: 13px; }
  .spacer { flex: 1; }
  .tbtn { display: inline-flex; align-items: center; gap: 6px; }
  .tbtn.on { border-color: var(--danger); color: var(--danger); background: var(--danger-soft); }
  .add { display: grid; grid-template-columns: 150px 80px 1fr auto; gap: 6px; }
  .hint { margin: 0; font-size: 12.5px; color: var(--text-3); }
  .days { display: grid; gap: 8px; margin-top: 6px; }
  .day { display: grid; gap: 2px; }
  .dh { display: flex; justify-content: space-between; font-size: 12px; color: var(--text-3); font-weight: 600; padding: 0 2px; }
  .ent { display: flex; align-items: center; gap: 8px; padding: 5px 6px; border-radius: 7px; font-size: 13.5px; }
  .ent:hover { background: var(--surface-2); }
  .ent.edit { display: grid; grid-template-columns: 140px 70px 1fr auto auto; background: var(--surface-2); }
  .who { font-weight: 500; white-space: nowrap; }
  .m { white-space: nowrap; }
  .src { font-size: 11px; color: var(--accent); background: color-mix(in srgb, var(--accent) 10%, transparent); border-radius: 999px; padding: 0 7px; }
  .note { flex: 1; min-width: 0; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rate { font-size: 12px; color: var(--text-3); }
  .ib { border: 0; background: none; color: var(--text-3); cursor: pointer; padding: 3px; border-radius: 5px; display: grid; place-items: center; opacity: 0; }
  .ent:hover .ib, .ib:focus-visible { opacity: 1; }
  .ib:hover { color: var(--text); background: var(--surface); }
  .more { justify-self: start; border: 0; background: none; color: var(--accent); cursor: pointer; font: inherit; font-size: 13px; padding: 2px; }
  .pay { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; padding: 8px 10px; border-radius: 9px; background: var(--surface-2); font-size: 13.5px; }
  .pl { color: var(--text-3); font-size: 12.5px; }
  .pay .seg button { padding: 3px 10px; font-size: 13px; }
  .amt { width: 140px; }
  .lab { font-weight: 500; font-variant-numeric: tabular-nums; }
  .nums { font-variant-numeric: tabular-nums; }
  @media (max-width: 720px) {
    .add { grid-template-columns: 1fr 90px; }
    .add .nt { grid-column: 1 / -1; }
    .add .btn { grid-column: 1 / -1; }
    .ent.edit { grid-template-columns: 1fr 80px; }
    .ent.edit .nt { grid-column: 1 / -1; }
    .ib { opacity: 1; }
    .ent { flex-wrap: wrap; row-gap: 2px; }
    .ent .note { flex-basis: 100%; white-space: normal; order: 10; }
  }
</style>
