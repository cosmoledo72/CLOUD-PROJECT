<script lang="ts">
  // «Экспорт графика»: PDF (A3/A4 landscape with the brand stamp), Excel or
  // MS Project XML. The server builds the file; the stamp names are suggested
  // (the exporting user, the project's ГИП) and remembered for the project.
  import Download from '@lucide/svelte/icons/download';
  import FileSpreadsheet from '@lucide/svelte/icons/file-spreadsheet';
  import FileText from '@lucide/svelte/icons/file-text';
  import FileCode from '@lucide/svelte/icons/file-code';
  import { api } from '#lib/api.ts';
  import Modal from '#lib/Modal.svelte';

  let { open = $bindable(false), projectId, hasBaseline = false }: { open?: boolean; projectId: string; hasBaseline?: boolean } = $props();

  type Format = 'pdf' | 'xlsx' | 'xml';
  function stored<T>(k: string, d: T): T {
    try {
      const v = localStorage.getItem(k);
      return v ? (JSON.parse(v) as T) : d;
    } catch {
      return d;
    }
  }
  const saved = stored('wh_gantt_export', { format: 'pdf' as Format, paper: 'a3', scale: 'auto', hideDone: false, critical: true, baseline: false });
  let format = $state<Format>(saved.format);
  let paper = $state(saved.paper);
  let scale = $state(saved.scale);
  let hideDone = $state(saved.hideDone);
  let critical = $state(saved.critical);
  let baseline = $state(saved.baseline);
  let stamp = $state({ developer: '', checker: '', gip: '' });
  let busy = $state(false);
  let error = $state('');

  $effect(() => {
    if (!open) return;
    error = '';
    api.get<{ stamp: typeof stamp }>(`/projects/${projectId}/gantt/export`).then((r) => (stamp = r.stamp)).catch(() => {});
  });

  async function download() {
    busy = true;
    error = '';
    try {
      localStorage.setItem('wh_gantt_export', JSON.stringify({ format, paper, scale, hideDone, critical, baseline }));
    } catch { /* storage unavailable */ }
    try {
      const res = await fetch(`/api/projects/${projectId}/gantt/export`, {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ format, paper, scale, hide_done: hideDone, critical, baseline: baseline && hasBaseline, stamp: format === 'pdf' ? stamp : undefined })
      });
      if (!res.ok) {
        const t = await res.text();
        let msg = res.statusText;
        try { msg = JSON.parse(t).message ?? msg; } catch { /* not JSON */ }
        throw new Error(msg);
      }
      const blob = await res.blob();
      const cd = res.headers.get('Content-Disposition') ?? '';
      const m = /filename\*=UTF-8''([^;]+)/i.exec(cd);
      const name = m ? decodeURIComponent(m[1]) : `График.${format}`;
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = name;
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(url), 10_000);
      open = false;
    } catch (e) {
      error = (e as Error).message === 'Failed to fetch' ? 'Нет связи с сервером' : (e as Error).message;
    } finally {
      busy = false;
    }
  }

  const FORMATS = [
    { k: 'pdf' as Format, title: 'PDF', sub: 'для печати и заказчика', icon: FileText },
    { k: 'xlsx' as Format, title: 'Excel', sub: 'график ячейками и список задач', icon: FileSpreadsheet },
    { k: 'xml' as Format, title: 'MS Project', sub: 'XML: MS Project, ProjectLibre', icon: FileCode }
  ];
</script>

<Modal bind:open title="Экспорт графика" width={560}>
  <div class="fmts" role="radiogroup" aria-label="Формат">
    {#each FORMATS as f (f.k)}
      {@const Icon = f.icon}
      <button role="radio" aria-checked={format === f.k} class="fmt" class:on={format === f.k} onclick={() => (format = f.k)}>
        <Icon size={20} /><b>{f.title}</b><span>{f.sub}</span>
      </button>
    {/each}
  </div>

  {#if format === 'pdf'}
    <div class="grid2">
      <div class="field">Лист
        <div class="seg" role="radiogroup" aria-label="Лист">
          <button role="radio" aria-checked={paper === 'a3'} class:on={paper === 'a3'} onclick={() => (paper = 'a3')}>А3 альбомный</button>
          <button role="radio" aria-checked={paper === 'a4'} class:on={paper === 'a4'} onclick={() => (paper = 'a4')}>А4 альбомный</button>
        </div>
      </div>
    </div>
  {/if}
  {#if format !== 'xml'}
    <div class="field">Шкала времени
      <div class="seg" role="radiogroup" aria-label="Шкала">
        {#each [['auto', 'Авто'], ['day', 'Дни'], ['week', 'Недели'], ['month', 'Месяцы']] as [k, l] (k)}
          <button role="radio" aria-checked={scale === k} class:on={scale === k} onclick={() => (scale = k)}>{l}</button>
        {/each}
      </div>
      <span class="hint">{format === 'pdf' ? (scale === 'auto' ? 'Весь срок проекта — по ширине листа.' : 'Если срок не помещается по ширине, шкала продолжается на соседних листах.') : 'Авто: дни — до 3 месяцев, недели — до 3 лет.'}</span>
    </div>
  {/if}

  <div class="checks">
    <label class="check"><input type="checkbox" bind:checked={hideDone} />Без выполненных задач</label>
    {#if format !== 'xml'}<label class="check"><input type="checkbox" bind:checked={critical} />Выделить критический путь</label>{/if}
    {#if hasBaseline}<label class="check"><input type="checkbox" bind:checked={baseline} />Базовый план</label>{/if}
  </div>

  {#if format === 'pdf'}
    <fieldset class="stamp">
      <legend>Штамп</legend>
      <label class="field">Разработал <input class="input" bind:value={stamp.developer} maxlength="60" /></label>
      <label class="field">Проверил <input class="input" bind:value={stamp.checker} maxlength="60" placeholder="—" /></label>
      <label class="field">ГИП <input class="input" bind:value={stamp.gip} maxlength="60" /></label>
      <p class="hint">В штампе также логотип портала, шифр и название проекта, дата и номер листа. Фамилии запоминаются для проекта.</p>
    </fieldset>
  {/if}

  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (open = false)}>Отмена</button>
    <button class="btn primary" disabled={busy} onclick={download}><Download size={16} />{busy ? 'Готовлю файл…' : 'Скачать'}</button>
  {/snippet}
</Modal>

<style>
  .fmts { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; margin-bottom: 14px; }
  .fmt { display: grid; justify-items: start; align-content: start; gap: 2px; padding: 10px 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface);
         font: inherit; text-align: left; cursor: pointer; color: var(--text-2); }
  .fmt b { color: var(--text); font-size: 14px; margin-top: 4px; }
  .fmt span { font-size: 12px; color: var(--text-3); line-height: 1.3; }
  .fmt:hover { border-color: var(--accent); }
  .fmt.on { border-color: var(--accent); box-shadow: inset 0 0 0 1px var(--accent); color: var(--accent); background: color-mix(in srgb, var(--accent) 5%, var(--surface)); }
  .field { display: grid; gap: 6px; margin-bottom: 12px; font-size: 13px; font-weight: 500; }
  .field .seg { justify-self: start; }
  .hint { font-size: 12px; color: var(--text-3); font-weight: 400; margin: 0; }
  .checks { display: flex; flex-wrap: wrap; gap: 8px 18px; margin-bottom: 12px; }
  .stamp { border: 1px solid var(--border); border-radius: 10px; padding: 8px 12px 10px; margin: 0 0 8px; display: grid; grid-template-columns: repeat(3, 1fr); gap: 0 10px; }
  .stamp legend { font-size: 13px; font-weight: 600; padding: 0 4px; }
  .stamp .field { margin-bottom: 6px; }
  .stamp .hint { grid-column: 1 / -1; }
  .btn.primary { display: inline-flex; align-items: center; gap: 6px; }
  @media (max-width: 720px) {
    .fmts, .stamp { grid-template-columns: 1fr; }
  }
</style>
