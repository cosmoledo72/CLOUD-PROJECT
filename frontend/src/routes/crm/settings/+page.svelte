<script lang="ts">
  import { api, type Stage } from '$lib/api';
  import { session } from '$lib/session.svelte';

  type Row = { id: string | null; name: string; kind: Stage['kind']; probability: number; color: string };
  let rows = $state<Row[]>([]);
  let saved = $state('');
  let error = $state('');
  let notice = $state('');

  async function load() {
    const r = await api.get<{ stages: Stage[] }>('/crm/stages');
    rows = r.stages.map((s) => ({ id: s.id, name: s.name, kind: s.kind, probability: s.probability, color: s.color }));
    saved = JSON.stringify(rows);
  }
  $effect(() => {
    load().catch((e) => (error = e.message));
  });

  const dirty = $derived(JSON.stringify(rows) !== saved);
  const kindText = { open: 'В работе', won: 'Выиграна', lost: 'Проиграна' } as const;
  const problems = $derived.by(() => {
    const p: string[] = [];
    for (const k of ['open', 'won', 'lost'] as const) if (!rows.some((r) => r.kind === k)) p.push(`нужен хотя бы один этап «${kindText[k]}»`);
    if (rows.some((r) => !r.name.trim())) p.push('у каждого этапа должно быть название');
    return p;
  });

  function move(i: number, d: number) {
    const j = i + d;
    if (j < 0 || j >= rows.length) return;
    [rows[i], rows[j]] = [rows[j], rows[i]];
  }
  function add() {
    rows.splice(Math.max(0, rows.findLastIndex((r) => r.kind === 'open') + 1), 0, { id: null, name: '', kind: 'open', probability: 50, color: '#3b82f6' });
  }
  function kindChanged(r: Row) {
    if (r.kind === 'won') r.probability = 100;
    if (r.kind === 'lost') r.probability = 0;
  }

  async function save() {
    error = '';
    notice = '';
    try {
      const r = await api.put<{ stages: Stage[] }>('/crm/stages', {
        stages: rows.map((x) => ({ ...x, name: x.name.trim(), probability: Math.max(0, Math.min(100, Math.round(x.probability))) }))
      });
      rows = r.stages.map((s) => ({ id: s.id, name: s.name, kind: s.kind, probability: s.probability, color: s.color }));
      saved = JSON.stringify(rows);
      notice = 'Воронка сохранена.';
    } catch (e) {
      error = (e as Error).message;
    }
  }
</script>

<div class="page-head">
  <h1>Этапы воронки</h1>
  <div class="spacer"></div>
  <button class="btn" disabled={!dirty} onclick={load}>Отменить</button>
  <button class="btn primary" disabled={!dirty || problems.length > 0 || !session.crmManager} onclick={save}>Сохранить</button>
</div>
<p class="muted lead">
  Порядок этапов задаёт колонки канбана. Вероятность используется во взвешенной сумме воронки.
  Этап, на котором есть сделки, удалить нельзя — сначала перенесите сделки.
</p>
{#if error}<div class="error-box">{error}</div>{/if}
{#if notice}<div class="ok-box">{notice}</div>{/if}
{#if problems.length}<div class="warn-box">Чтобы сохранить: {problems.join('; ')}.</div>{/if}

<div class="card">
  <table class="list">
    <thead><tr><th class="w-ord"></th><th>Название</th><th>Тип</th><th>Вероятность, %</th><th>Цвет</th><th></th></tr></thead>
    <tbody>
      {#each rows as r, i (r)}
        <tr>
          <td class="w-ord">
            <button class="btn ghost sm" aria-label="Выше" disabled={i === 0} onclick={() => move(i, -1)}>↑</button>
            <button class="btn ghost sm" aria-label="Ниже" disabled={i === rows.length - 1} onclick={() => move(i, 1)}>↓</button>
          </td>
          <td><input class="input" bind:value={r.name} placeholder="Название этапа" aria-label="Название" /></td>
          <td>
            <select class="input" bind:value={r.kind} onchange={() => kindChanged(r)} aria-label="Тип">
              <option value="open">В работе</option><option value="won">Выиграна</option><option value="lost">Проиграна</option>
            </select>
          </td>
          <td><input class="input num" type="number" min="0" max="100" bind:value={r.probability} disabled={r.kind !== 'open'} aria-label="Вероятность" /></td>
          <td><input class="color" type="color" bind:value={r.color} aria-label="Цвет" /></td>
          <td><button class="btn ghost sm danger" onclick={() => rows.splice(i, 1)}>Удалить</button></td>
        </tr>
      {/each}
    </tbody>
  </table>
  <div class="foot"><button class="btn sm" onclick={add}>Добавить этап</button></div>
</div>

<style>
  .lead { margin: 0 0 12px; max-width: 760px; }
  .error-box, .ok-box, .warn-box { margin-bottom: 10px; }
  .warn-box { background: color-mix(in srgb, #f59e0b 14%, var(--surface)); border: 1px solid color-mix(in srgb, #f59e0b 40%, transparent); padding: 8px 12px; border-radius: var(--radius); }
  .w-ord { width: 80px; white-space: nowrap; }
  .num { width: 90px; }
  .color { width: 40px; height: 30px; border: 1px solid var(--border); border-radius: var(--radius); background: none; padding: 2px; cursor: pointer; }
  .foot { padding: 10px; }
</style>
