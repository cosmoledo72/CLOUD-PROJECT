<script lang="ts">
  import Plus from '@lucide/svelte/icons/plus';
  import { api, fieldTypeLabel, type FieldDef, type FieldType, type FieldValues, type UserItem } from '$lib/api';
  import Modal from '$lib/Modal.svelte';
  import RequisitesForm from '$lib/RequisitesForm.svelte';

  let defs = $state<FieldDef[]>([]);
  let users = $state<UserItem[]>([]);
  let error = $state('');
  let notice = $state('');
  let previewValues = $state<FieldValues>({});

  async function load() {
    defs = (await api.get<{ fields: FieldDef[] }>('/project-fields')).fields;
  }
  $effect(() => {
    load().catch((e) => (error = e.message));
    api.get<{ users: UserItem[] }>('/users').then((r) => (users = r.users));
  });

  async function run(fn: () => Promise<unknown>, ok = '') {
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

  const sections = $derived.by(() => {
    const out: { name: string; fields: FieldDef[] }[] = [];
    for (const d of defs) {
      let s = out.find((x) => x.name === d.section);
      if (!s) out.push((s = { name: d.section, fields: [] }));
      s.fields.push(d);
    }
    return out;
  });
  const sectionNames = $derived([...new Set(defs.map((d) => d.section))]);

  // --- Reordering (within a section) --------------------------------------------
  async function move(d: FieldDef, dir: -1 | 1) {
    const ordered = sections.flatMap((s) => s.fields);
    const sameSection = ordered.filter((x) => x.section === d.section);
    const i = sameSection.findIndex((x) => x.id === d.id);
    const other = sameSection[i + dir];
    if (!other) return;
    const ids = ordered.map((x) => x.id);
    const a = ids.indexOf(d.id);
    const b = ids.indexOf(other.id);
    [ids[a], ids[b]] = [ids[b], ids[a]];
    await run(() => api.post('/admin/project-fields/reorder', { ids }));
  }

  // --- Editor ------------------------------------------------------------------------
  const RU: Record<string, string> = {
    а: 'a', б: 'b', в: 'v', г: 'g', д: 'd', е: 'e', ё: 'e', ж: 'zh', з: 'z', и: 'i', й: 'y', к: 'k', л: 'l', м: 'm',
    н: 'n', о: 'o', п: 'p', р: 'r', с: 's', т: 't', у: 'u', ф: 'f', х: 'h', ц: 'ts', ч: 'ch', ш: 'sh', щ: 'sch',
    ъ: '', ы: 'y', ь: '', э: 'e', ю: 'yu', я: 'ya'
  };
  /** Machine key from a Russian label: "Номер договора" → "nomer_dogovora". */
  function toKey(label: string): string {
    const latin = label.toLowerCase().split('').map((c) => RU[c] ?? c).join('');
    let key = latin.replace(/[^a-z0-9]+/g, '_').replace(/^_+|_+$/g, '').slice(0, 40);
    if (!/^[a-z]/.test(key)) key = 'f_' + key;
    return key.replace(/_+$/, '') || 'field';
  }

  interface Draft {
    id: string | null;
    key: string;
    keyTouched: boolean;
    label: string;
    section: string;
    field_type: FieldType;
    optionsText: string;
    help_text: string;
    is_required: boolean;
    show_in_list: boolean;
    is_archived: boolean;
  }
  let editOpen = $state(false);
  let draft = $state<Draft>(blankDraft());
  function blankDraft(): Draft {
    return { id: null, key: '', keyTouched: false, label: '', section: sectionNames[0] ?? 'Общие сведения', field_type: 'text',
             optionsText: '', help_text: '', is_required: false, show_in_list: false, is_archived: false };
  }
  function openNew() {
    draft = blankDraft();
    editOpen = true;
  }
  function openEdit(d: FieldDef) {
    draft = { id: d.id, key: d.key, keyTouched: true, label: d.label, section: d.section, field_type: d.field_type,
              optionsText: d.options.join('\n'), help_text: d.help_text, is_required: d.is_required,
              show_in_list: d.show_in_list, is_archived: d.is_archived };
    editOpen = true;
  }
  function onLabel(v: string) {
    draft.label = v;
    if (!draft.keyTouched) draft.key = toKey(v);
  }
  async function save() {
    const body = {
      key: draft.key, label: draft.label, section: draft.section, field_type: draft.field_type,
      options: draft.optionsText.split('\n').map((s) => s.trim()).filter(Boolean),
      help_text: draft.help_text, is_required: draft.is_required, show_in_list: draft.show_in_list,
      is_archived: draft.is_archived
    };
    const id = draft.id;
    await run(() => (id ? api.put(`/admin/project-fields/${id}`, body) : api.post('/admin/project-fields', body)));
    if (!error) editOpen = false;
  }
  function remove(d: FieldDef) {
    if (!confirm(`Удалить реквизит «${d.label}»? Значения во всех проектах будут удалены безвозвратно. Чтобы сохранить значения, используйте «В архив».`)) return;
    run(() => api.del(`/admin/project-fields/${d.id}`), 'Реквизит удалён.');
  }
  function toggleArchive(d: FieldDef) {
    run(() => api.put(`/admin/project-fields/${d.id}`, { ...d, is_archived: !d.is_archived }));
  }
  async function applyPreset() {
    let created = 0;
    await run(async () => {
      created = (await api.post<{ created: number }>('/admin/project-fields/preset')).created;
    });
    if (!error) notice = created ? `Добавлено реквизитов: ${created}.` : 'Все реквизиты типового набора уже есть.';
  }

  let tab = $state<'fields' | 'preview'>('fields');
</script>

<div class="row toolbar">
  <p class="muted intro">
    Реквизиты — это поля, которые заполняются в каждом проекте: шифр, заказчик, договор, стадия, ГИП.
    Обязательные реквизиты запрашиваются при создании проекта.
  </p>
  <div class="spacer"></div>
  <button class="btn" onclick={applyPreset}>Типовой набор: обследование и проектирование</button>
  <button class="btn primary" onclick={openNew}><Plus size={16} />Добавить реквизит</button>
</div>
{#if error}<div class="error-box">{error}</div>{/if}
{#if notice}<div class="ok-box">{notice}</div>{/if}

<div class="seg" role="tablist">
  <button role="tab" aria-selected={tab === 'fields'} class:on={tab === 'fields'} onclick={() => (tab = 'fields')}>Поля ({defs.length})</button>
  <button role="tab" aria-selected={tab === 'preview'} class:on={tab === 'preview'} onclick={() => (tab = 'preview')}>Предпросмотр формы</button>
</div>

{#if tab === 'fields'}
  {#if defs.length === 0}
    <div class="card empty">
      <h3>Реквизиты ещё не настроены</h3>
      <p class="muted">Начните с типового набора для обследования и проектирования (15 полей) или добавьте поля вручную.</p>
      <div class="row center">
        <button class="btn primary" onclick={applyPreset}>Загрузить типовой набор</button>
        <button class="btn" onclick={openNew}>Добавить вручную</button>
      </div>
    </div>
  {:else}
    {#each sections as s (s.name)}
      <section class="card sec">
        <h3 class="sec-title">{s.name}</h3>
        <table class="list fields">
          <colgroup><col style="width:76px" /><col /><col style="width:28%" /><col style="width:150px" /><col class="c-act" /></colgroup>
          <tbody>
            {#each s.fields as d, i (d.id)}
              <tr class:archived={d.is_archived}>
                <td class="order">
                  <button class="btn ghost sm" disabled={i === 0} onclick={() => move(d, -1)} aria-label="Выше">↑</button>
                  <button class="btn ghost sm" disabled={i === s.fields.length - 1} onclick={() => move(d, 1)} aria-label="Ниже">↓</button>
                </td>
                <td>
                  <div class="lbl">{d.label}</div>
                  <div class="mono faint">{d.key}</div>
                </td>
                <td class="muted type">
                  {fieldTypeLabel[d.field_type]}
                  {#if d.field_type === 'select'}<div class="faint small opts">{d.options.join(' · ')}</div>{/if}
                </td>
                <td class="flags">
                  {#if d.is_required}<span class="badge danger">обязательный</span>{/if}
                  {#if d.show_in_list}<span class="badge">в карточке</span>{/if}
                  {#if d.is_archived}<span class="badge grey">архив</span>{/if}
                </td>
                <td class="actions">
                  <button class="btn ghost sm" onclick={() => openEdit(d)}>Изменить</button>
                  <button class="btn ghost sm" onclick={() => toggleArchive(d)}>{d.is_archived ? 'Из архива' : 'В архив'}</button>
                  <button class="btn ghost sm danger" onclick={() => remove(d)}>Удалить</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </section>
    {/each}
  {/if}
{:else}
  <div class="card preview">
    <p class="faint small">Так форма реквизитов выглядит в проекте. Значения здесь никуда не сохраняются.</p>
    <RequisitesForm defs={defs} bind:values={previewValues} {users} />
  </div>
{/if}

<Modal bind:open={editOpen} title={draft.id ? 'Реквизит' : 'Новый реквизит'} width={560}>
  <label class="field">Название <input class="input" value={draft.label} oninput={(e) => onLabel(e.currentTarget.value)} placeholder="Например: Номер договора" /></label>
  <div class="two">
    <label class="field">Тип поля
      <select class="input" bind:value={draft.field_type}>
        {#each Object.entries(fieldTypeLabel) as [v, l]}<option value={v}>{l}</option>{/each}
      </select>
    </label>
    <label class="field">Раздел
      <input class="input" bind:value={draft.section} list="sections" />
      <datalist id="sections">{#each sectionNames as n}<option value={n}></option>{/each}</datalist>
    </label>
  </div>
  {#if draft.field_type === 'select'}
    <label class="field">Значения списка (по одному в строке)
      <textarea class="input" rows="5" bind:value={draft.optionsText} placeholder={'П\nРД\nП+РД'}></textarea>
    </label>
  {/if}
  <label class="field">Подсказка <input class="input" bind:value={draft.help_text} placeholder="Необязательно" /></label>
  <label class="field">Системный ключ
    <input class="input mono" value={draft.key} oninput={(e) => { draft.key = e.currentTarget.value; draft.keyTouched = true; }} />
    <span class="faint small">Латиница, цифры и «_». Используется в API и будущих шаблонах документов: <span class="mono">{'{{project.' + (draft.key || 'key') + '}}'}</span></span>
  </label>
  <div class="checks">
    <label class="check"><input type="checkbox" bind:checked={draft.is_required} />Обязательный</label>
    <label class="check"><input type="checkbox" bind:checked={draft.show_in_list} />Показывать в карточке проекта</label>
    {#if draft.id}<label class="check"><input type="checkbox" bind:checked={draft.is_archived} />В архиве</label>{/if}
  </div>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (editOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!draft.label.trim() || !draft.key.trim()} onclick={save}>Сохранить</button>
  {/snippet}
</Modal>

<style>
  .seg { margin-bottom: 12px; }
  .toolbar { margin-bottom: 12px; flex-wrap: wrap; align-items: center; }
  .intro { margin: 0; max-width: 620px; }
  .ok-box, .error-box { margin-bottom: 12px; }
  .sec { margin-bottom: 12px; overflow-x: auto; container-type: inline-size; }
  .sec-title { padding: 12px 14px 4px; margin: 0; }
  .order { width: 72px; white-space: nowrap; padding-right: 0; }
  .lbl { font-weight: 500; }
  .fields { table-layout: fixed; min-width: 560px; }
  .fields td { overflow: hidden; }
  .opts { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .flags { white-space: normal; line-height: 1.9; }
  .flags .badge { margin-right: 4px; }
  .actions { text-align: right; white-space: nowrap; }
  .fields td.actions { overflow: visible; }
  /* The action buttons keep their full width; the label and type columns shrink. */
  .c-act { width: 300px; }
  /* Narrow cards: buttons stack in a column so nothing is cut off. */
  @container (max-width: 900px) {
    .c-act { width: 120px; }
    .fields col:nth-child(4) { width: 120px; }
    .fields td.actions { display: grid; justify-items: end; gap: 2px; }
  }
  tr.archived td:not(.actions) { opacity: .55; }
  .empty { padding: 32px; text-align: center; display: grid; gap: 8px; justify-items: center; }
  .empty h3, .empty p { margin: 0; }
  .center { justify-content: center; margin-top: 6px; }
  .preview { padding: 18px; display: grid; gap: 12px; }
  .preview p { margin: 0; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .checks { display: flex; flex-wrap: wrap; gap: 8px 18px; }
  @media (max-width: 560px) { .two { grid-template-columns: 1fr; } }
</style>
