<script lang="ts">
  import Upload from '@lucide/svelte/icons/upload';
  import { api, docKindLabel, formatDateTime, formatSize, type DocKind, type DocTemplate, type PlaceholderInfo } from '#lib/api.ts';
  import Modal from '#lib/Modal.svelte';

  let allTemplates = $state<DocTemplate[]>([]);
  let catalogue = $state<PlaceholderInfo[]>([]);
  // Project templates use project requisites; sales templates — client, deal, document, organisation.
  let scope = $state<'project' | 'sales'>('project');
  const templates = $derived(allTemplates.filter((t) => t.scope === scope));
  let error = $state('');
  let notice = $state('');
  let copied = $state('');

  async function load() {
    const [t, c] = await Promise.all([
      api.get<{ templates: DocTemplate[] }>('/templates'),
      api.get<{ placeholders: PlaceholderInfo[] }>(`/templates/placeholders?scope=${scope}`)
    ]);
    allTemplates = t.templates;
    catalogue = c.placeholders;
  }
  $effect(() => {
    scope;
    load().catch((e) => (error = e.message));
  });

  const known = $derived(new Set(catalogue.map((p) => p.key)));
  // Same rule as the server: {{key}} and {{project_key}} also mean {{project.key}}.
  function resolve(k: string): string | null {
    if (known.has(k)) return k;
    for (const c of [k.startsWith('project_') ? 'project.' + k.slice(8) : '', 'project.' + k]) if (c && known.has(c)) return c;
    return null;
  }
  const labelOf = (k: string) => {
    const r = resolve(k);
    const label = catalogue.find((p) => p.key === r)?.label;
    if (!label) return 'неизвестная метка';
    return r === k ? label : `${label} — то же, что {{${r}}}`;
  };
  // Closest catalogue key for a marker that was not found (typo help).
  function suggest(k: string): string | null {
    const tail = (x: string) => x.replace(/^project[._]/, '');
    const t = tail(k);
    let best: string | null = null;
    let bestD = 3;
    for (const p of catalogue) {
      const c = tail(p.key);
      const d = c.includes(t) || t.includes(c) ? 1 : distance(t, c);
      if (d < bestD) [best, bestD] = [p.key, d];
    }
    return best;
  }
  function distance(a: string, b: string): number {
    if (Math.abs(a.length - b.length) > 2) return 99;
    let prev = Array.from({ length: b.length + 1 }, (_, i) => i);
    for (let i = 1; i <= a.length; i++) {
      const cur = [i];
      for (let j = 1; j <= b.length; j++) cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
      prev = cur;
    }
    return prev[b.length];
  }
  const categories = $derived([...new Set(templates.map((t) => t.category))]);
  const groups = $derived.by(() => {
    const out: { name: string; items: PlaceholderInfo[] }[] = [];
    for (const p of catalogue) {
      let g = out.find((x) => x.name === p.group);
      if (!g) out.push((g = { name: p.group, items: [] }));
      g.items.push(p);
    }
    return out;
  });

  async function run(fn: () => Promise<unknown>, ok: string) {
    error = '';
    notice = '';
    try {
      await fn();
      notice = ok;
      await load();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  function copy(key: string) {
    const text = `{{${key}}}`;
    navigator.clipboard?.writeText(text).catch(() => {});
    copied = key;
    setTimeout(() => (copied = ''), 1500);
  }

  // --- Upload / edit -------------------------------------------------------------
  let editOpen = $state(false);
  type Edit = { id: string | null; name: string; category: string; description: string; file: File | null; doc_kind: DocKind };
  let edit = $state<Edit>({ id: null, name: '', category: '', description: '', file: null, doc_kind: 'invoice' });
  function openUpload() {
    const cat = categories[0] ?? (scope === 'sales' ? 'Документы продаж' : 'Общие');
    edit = { id: null, name: '', category: cat, description: '', file: null, doc_kind: 'invoice' };
    editOpen = true;
  }
  function openEdit(t: DocTemplate) {
    edit = { id: t.id, name: t.name, category: t.category, description: t.description, file: null, doc_kind: t.doc_kind ?? 'invoice' };
    editOpen = true;
  }
  function onFile(e: Event & { currentTarget: HTMLInputElement }) {
    const f = e.currentTarget.files?.[0] ?? null;
    edit.file = f;
    if (f && !edit.name.trim()) edit.name = f.name.replace(/\.(docx|xlsx)$/i, '');
  }
  async function save() {
    const id = edit.id;
    const file = edit.file;
    await run(async () => {
      if (id) {
        await api.put(`/admin/templates/${id}`, {
          name: edit.name, category: edit.category, description: edit.description, scope,
          doc_kind: scope === 'sales' ? edit.doc_kind : null
        });
        if (file) {
          const fd = new FormData();
          fd.append('file', file, file.name);
          await api.post(`/admin/templates/${id}/file`, fd);
        }
      } else {
        if (!file) throw new Error('Выберите файл .docx или .xlsx');
        const fd = new FormData();
        fd.append('name', edit.name);
        fd.append('category', edit.category);
        fd.append('description', edit.description);
        fd.append('scope', scope);
        if (scope === 'sales') fd.append('doc_kind', edit.doc_kind);
        fd.append('file', file, file.name);
        await api.post('/admin/templates', fd);
      }
    }, id ? 'Шаблон сохранён.' : 'Шаблон загружен.');
    if (!error) editOpen = false;
  }
  function remove(t: DocTemplate) {
    if (!confirm(`Удалить шаблон «${t.name}»? Уже созданные по нему документы останутся.`)) return;
    run(() => api.del(`/admin/templates/${t.id}`), 'Шаблон удалён.');
  }
  const installSample = () =>
    scope === 'sales'
      ? run(() => api.post('/admin/templates/sales-samples'), 'Добавлены образцы: КП, договор, счёт и акт. Заполните реквизиты организации в разделе «Организация».')
      : run(() => api.post('/admin/templates/sample'), 'Пример титульного листа добавлен.');
</script>

<div class="seg scope" role="tablist" aria-label="Назначение шаблонов">
  <button role="tab" aria-selected={scope === 'project'} class:on={scope === 'project'} onclick={() => (scope = 'project')}>Для проектов</button>
  <button role="tab" aria-selected={scope === 'sales'} class:on={scope === 'sales'} onclick={() => (scope = 'sales')}>Документы продаж</button>
</div>
<div class="row toolbar">
  <p class="muted intro">
    {#if scope === 'project'}
      Шаблоны — это документы Word или Excel с метками вроде <span class="mono">{'{{project.code}}'}</span>.
      В проекте кнопка «Документ из шаблона» создаёт копию с подставленными реквизитами.
    {:else}
      КП, договоры, счета и акты формируются из сделки CRM: подставляются реквизиты контрагента, вашей организации,
      сумма прописью и НДС, номер присваивается автоматически. Каждый шаблон относится к одному виду документа.
    {/if}
  </p>
  <div class="spacer"></div>
  <button class="btn" onclick={installSample}>{scope === 'sales' ? 'Добавить образцы: КП, договор, счёт, акт' : 'Добавить пример: титульный лист'}</button>
  <button class="btn primary" onclick={openUpload}><Upload size={16} />Загрузить шаблон</button>
</div>
{#if error}<div class="error-box">{error}</div>{/if}
{#if notice}<div class="ok-box">{notice}</div>{/if}

<div class="layout">
  <div class="list-col">
    {#if templates.length === 0}
      <div class="card empty">
        <h3>Шаблонов пока нет</h3>
        <p class="muted">{scope === 'sales' ? 'Добавьте образцы документов продаж или загрузите свои бланки.' : 'Добавьте пример титульного листа отчёта или загрузите свой бланк в формате .docx или .xlsx.'}</p>
      </div>
    {:else}
      {#each categories as cat (cat)}
        <h3 class="cat">{cat}</h3>
        {#each templates.filter((t) => t.category === cat) as t (t.id)}
          {@const unknown = t.placeholders.filter((k) => !resolve(k))}
          <article class="card tpl">
            <div class="row">
              <span class="ext {t.extension}">{t.extension === 'docx' ? 'W' : 'X'}</span>
              <div class="title">
                <b>{t.name}{#if t.doc_kind}{' '}<span class="badge">{docKindLabel[t.doc_kind]}</span>{/if}</b>
                <span class="faint small">{formatSize(t.size_bytes)} · изменён {formatDateTime(t.updated_at)}</span>
              </div>
              <div class="spacer"></div>
              <a class="btn ghost sm" href="/api/templates/{t.id}/download">Скачать</a>
              <button class="btn ghost sm" onclick={() => openEdit(t)}>Изменить</button>
              <button class="btn ghost sm danger" onclick={() => remove(t)}>Удалить</button>
            </div>
            {#if t.description}<p class="muted desc">{t.description}</p>{/if}
            {#if t.placeholders.length === 0}
              <p class="warn-note">В файле не найдено ни одной метки — документ будет скопирован без изменений.</p>
            {:else}
              <div class="chips">
                {#each t.placeholders as k}
                  <span class="chip" class:bad={!resolve(k)} title={labelOf(k)}>{k}</span>
                {/each}
              </div>
              {#if unknown.length}
                <div class="warn-note">
                  <p>Не найдены метки — при формировании документа они будут заменены пустым текстом:</p>
                  <ul>
                    {#each unknown as k}
                      {@const s = suggest(k)}
                      <li><span class="mono">{'{{' + k + '}}'}</span>{#if s}{' — возможно, имелось в виду'} <span class="mono">{'{{' + s + '}}'}</span> ({labelOf(s)}){/if}</li>
                    {/each}
                  </ul>
                  <p>Реквизиты проекта пишутся как <span class="mono">{'{{project.ключ}}'}</span>; ключи всех реквизитов — в справочнике справа и в разделе «Администрирование → Реквизиты проектов».</p>
                </div>
              {/if}
            {/if}
          </article>
        {/each}
      {/each}
    {/if}
  </div>

  <aside class="card ref">
    <h3>Доступные метки</h3>
    <p class="faint small">Нажмите, чтобы скопировать, и вставьте в документ Word или ячейку Excel.</p>
    {#each groups as g (g.name)}
      <div class="ref-group">
        <div class="ref-title">{g.name}</div>
        {#each g.items as p (p.key)}
          <button class="ref-item" onclick={() => copy(p.key)} title="Копировать">
            <span class="mono key">{copied === p.key ? 'скопировано' : `{{${p.key}}}`}</span>
            <span class="lbl">{p.label}</span>
          </button>
        {/each}
      </div>
    {/each}
  </aside>
</div>

<Modal bind:open={editOpen} title={edit.id ? 'Шаблон' : 'Новый шаблон'} width={520}>
  <label class="field">{edit.id ? 'Заменить файл (необязательно)' : 'Файл .docx или .xlsx'}
    <input class="input file" type="file" accept=".docx,.xlsx,application/vnd.openxmlformats-officedocument.wordprocessingml.document,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" onchange={onFile} />
  </label>
  <label class="field">Название <input class="input" bind:value={edit.name} placeholder={scope === 'sales' ? 'Например: Счёт на оплату' : 'Например: Акт обследования'} /></label>
  {#if scope === 'sales'}
    <label class="field">Вид документа
      <select class="input" bind:value={edit.doc_kind}>
        {#each Object.entries(docKindLabel) as [k, l]}<option value={k}>{l}</option>{/each}
      </select>
    </label>
  {/if}
  <label class="field">Категория
    <input class="input" bind:value={edit.category} list="tpl-cats" />
    <datalist id="tpl-cats">{#each categories as c}<option value={c}></option>{/each}</datalist>
  </label>
  <label class="field">Описание <textarea class="input" rows="2" bind:value={edit.description}></textarea></label>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (editOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!edit.name.trim() || (!edit.id && !edit.file)} onclick={save}>Сохранить</button>
  {/snippet}
</Modal>

<style>
  .seg { margin-bottom: 12px; }
  .toolbar { margin-bottom: 12px; flex-wrap: wrap; align-items: center; }
  .intro { margin: 0; max-width: 640px; }
  .ok-box, .error-box { margin-bottom: 12px; }
  .layout { display: grid; grid-template-columns: minmax(0, 1fr) 340px; gap: 16px; align-items: start; }
  .list-col { display: grid; gap: 10px; }
  .cat { margin: 6px 0 0; color: var(--text-2); font-size: 13px; text-transform: uppercase; letter-spacing: .04em; }
  .tpl { padding: 12px 14px; display: grid; gap: 8px; }
  .tpl .row { flex-wrap: wrap; }
  .ext { width: 26px; height: 26px; border-radius: 5px; display: grid; place-items: center; color: #fff; font-weight: 700; font-size: 13px; flex: none; }
  .ext.docx { background: var(--word); }
  .ext.xlsx { background: var(--cell); }
  .title { display: grid; min-width: 0; }
  .desc { margin: 0; font-size: 13px; }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .chip { font-family: var(--mono); font-size: 11.5px; padding: 1px 7px; border-radius: 999px; background: var(--ok-soft); color: var(--ok); }
  .chip.bad { background: var(--danger-soft); color: var(--danger); }
  .warn-note p, .warn-note ul { margin: 2px 0; }
  .warn-note ul { padding-left: 18px; }
  .warn-note { margin: 0; background: var(--warn-soft); color: var(--warn); padding: 6px 10px; border-radius: var(--radius); font-size: 12.5px; }
  .empty { padding: 32px; text-align: center; display: grid; gap: 6px; }
  .empty h3, .empty p { margin: 0; }
  .ref { padding: 14px; position: sticky; top: 16px; max-height: calc(100vh - 32px); overflow-y: auto; }
  .ref h3 { margin: 0 0 4px; }
  .ref > p { margin: 0 0 10px; }
  .ref-group { margin-bottom: 10px; }
  .ref-title { font-size: 12px; color: var(--text-3); text-transform: uppercase; letter-spacing: .04em; margin: 8px 0 4px; }
  .ref-item {
    display: grid; width: 100%; text-align: left; gap: 1px; padding: 5px 8px; border: 0; background: none;
    border-radius: var(--radius); cursor: pointer;
  }
  .ref-item:hover { background: var(--surface-2); }
  .key { color: var(--accent); font-size: 12px; }
  .lbl { font-size: 12.5px; color: var(--text-2); }
  .file { padding: 4px 6px; height: auto; }
  @media (max-width: 1000px) {
    .layout { grid-template-columns: 1fr; }
    .ref { position: static; max-height: none; }
  }
</style>
