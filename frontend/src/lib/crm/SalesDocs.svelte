<script lang="ts">
  // Sales documents of a deal (with the «create» dialog) or of a company.
  import {
    api, docKindLabel, docStatusLabel, formatDate, formatMoney, localToday, vatLabel,
    type DocKind, type DocStatus, type DocTemplate, type SalesDoc
  } from '../api';
  import Modal from '../Modal.svelte';
  import { session } from '../session.svelte';

  let {
    dealId = null,
    companyId = null,
    canEdit = false,
    onchange
  }: {
    dealId?: string | null;
    companyId?: string | null;
    canEdit?: boolean;
    /** A new document adds a history entry. */
    onchange?: () => void;
  } = $props();

  let docs = $state<SalesDoc[]>([]);
  let showCancelled = $state(false);
  let error = $state('');

  const filter = $derived(dealId ? `deal_id=${dealId}` : `company_id=${companyId}`);
  async function load() {
    docs = (await api.get<{ documents: SalesDoc[] }>(`/crm/documents?${filter}`)).documents;
  }
  $effect(() => {
    filter;
    load().catch(() => {});
  });
  const visible = $derived(docs.filter((d) => showCancelled || d.status !== 'cancelled'));
  const cancelledCount = $derived(docs.filter((d) => d.status === 'cancelled').length);

  async function setStatus(d: SalesDoc, status: DocStatus) {
    if (status === 'cancelled' && !confirm(`Аннулировать ${docKindLabel[d.kind].toLowerCase()} № ${d.number}? Номер освободится, файл останется в папке.`)) return;
    error = '';
    try {
      await api.patch(`/crm/documents/${d.id}`, { status });
      await load();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  async function openFolder() {
    const cid = companyId ?? docs.find((d) => d.company_id)?.company_id;
    if (!cid) return;
    const r = await api.post<{ folder_id: string }>(`/crm/companies/${cid}/folder`);
    location.href = `/files/${r.folder_id}`;
  }

  // --- New document -----------------------------------------------------------------
  type Defaults = {
    subject: string; amount: number; vat_rate: string; has_company: boolean; org_filled: boolean;
    contracts: { id: string; number: string; date: string }[];
  };
  let open = $state(false);
  let defaults = $state<Defaults | null>(null);
  let templates = $state<DocTemplate[]>([]);
  let f = $state({ kind: 'invoice' as DocKind, template_id: '', date: localToday(), number: '', subject: '', amount: '', vat_rate: 'none', basis_id: '' });
  let autoNumber = $state('');
  let busy = $state(false);
  let created = $state<{ doc: SalesDoc; missing: string[] } | null>(null);
  // Human names of placeholders for the «left empty» warning.
  let labels = $state<Record<string, string>>({});

  async function start(kind: DocKind = 'invoice') {
    if (!dealId) return;
    error = '';
    created = null;
    const [d, t, ph] = await Promise.all([
      api.get<Defaults>(`/crm/deals/${dealId}/documents`),
      api.get<{ templates: DocTemplate[] }>('/templates'),
      api.get<{ placeholders: { key: string; label: string; group: string }[] }>('/templates/placeholders?scope=sales')
    ]);
    labels = Object.fromEntries(ph.placeholders.map((p) => [p.key, `${p.group.replace(/ \(.*\)/, '')}: ${p.label.toLowerCase()}`]));
    defaults = d;
    templates = t.templates.filter((x) => x.scope === 'sales');
    f = {
      kind, template_id: '', date: localToday(), number: '', subject: d.subject, amount: String(d.amount), vat_rate: d.vat_rate,
      basis_id: d.contracts[0]?.id ?? ''
    };
    pickTemplate();
    open = true;
  }
  const kindTemplates = $derived(templates.filter((t) => t.doc_kind === f.kind));
  function pickTemplate() {
    const list = templates.filter((t) => t.doc_kind === f.kind);
    if (!list.some((t) => t.id === f.template_id)) f.template_id = list[0]?.id ?? '';
  }
  $effect(() => {
    const kind = f.kind;
    const date = f.date;
    if (!open) return;
    api.get<{ number: string }>(`/crm/documents/next-number?kind=${kind}&date=${date}`).then((r) => (autoNumber = r.number)).catch(() => {});
  });
  const needsBasis = $derived(f.kind === 'invoice' || f.kind === 'act');
  const amountNum = $derived(Number(f.amount.replace(/\s/g, '').replace(',', '.')));
  const vatPreview = $derived(f.vat_rate === 'none' || !Number.isFinite(amountNum) ? 0 : Math.round((amountNum * +f.vat_rate) / (100 + +f.vat_rate) * 100) / 100);

  async function create() {
    if (!dealId) return;
    busy = true;
    error = '';
    try {
      const r = await api.post<{ document: SalesDoc; missing: string[] }>(`/crm/deals/${dealId}/documents`, {
        kind: f.kind, template_id: f.template_id, date: f.date, number: f.number.trim() || null, subject: f.subject,
        amount: amountNum, vat_rate: f.vat_rate, basis_id: needsBasis && f.basis_id ? f.basis_id : null
      });
      created = { doc: r.document, missing: r.missing };
      await load();
      onchange?.();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }
  const vatRates = ['none', '0', '5', '7', '10', '20', '22'];
</script>

<div class="block">
  <div class="head">
    <h3>Документы {#if visible.length}<span class="faint">— {visible.length}</span>{/if}</h3>
    <div class="spacer"></div>
    {#if docs.length || companyId}<button class="btn ghost sm" onclick={openFolder}>Папка</button>{/if}
    {#if dealId && canEdit}<button class="btn sm" onclick={() => start()}>Создать документ</button>{/if}
  </div>
  {#if error && !open}<div class="error-box">{error}</div>{/if}
  {#each visible as d (d.id)}
    <div class="doc" class:cancelled={d.status === 'cancelled'}>
      <span class="kind k-{d.kind}">{docKindLabel[d.kind]}</span>
      <div class="main">
        <div class="line1">
          {#if d.file_id}<a href="/editor/{d.file_id}" target="_blank" rel="noopener">№ {d.number} от {formatDate(d.doc_date)}</a>
          {:else}<span>№ {d.number} от {formatDate(d.doc_date)}</span>{/if}
          <b class="amt">{formatMoney(d.amount)}</b>
        </div>
        <div class="line2">
          <span>{d.subject}</span>
          {#if d.basis_number}<span class="faint">по договору № {d.basis_number}</span>{/if}
          {#if !dealId && d.deal_title}<a href="/crm/deals/{d.deal_id}">{d.deal_title}</a>{/if}
          <span class="faint">{vatLabel(d.vat_rate)}</span>
        </div>
      </div>
      {#if canEdit || session.crmManager}
        <select class="input status s-{d.status}" value={d.status} onchange={(e) => setStatus(d, e.currentTarget.value as DocStatus)} aria-label="Статус документа № {d.number}">
          {#each Object.entries(docStatusLabel) as [k, l]}<option value={k}>{l}</option>{/each}
        </select>
      {:else}
        <span class="badge s-{d.status}">{docStatusLabel[d.status]}</span>
      {/if}
    </div>
  {:else}
    <p class="faint none">{dealId ? 'Документов нет. Сформируйте КП, договор, счёт или акт из шаблона — реквизиты подставятся сами.' : 'Документов по контрагенту нет.'}</p>
  {/each}
  {#if cancelledCount}
    <button class="btn ghost sm more" onclick={() => (showCancelled = !showCancelled)}>{showCancelled ? 'Скрыть аннулированные' : `Аннулированные (${cancelledCount})`}</button>
  {/if}
</div>

<Modal bind:open title={created ? 'Документ сформирован' : 'Новый документ'} width={600}>
  {#if created}
    <div class="done">
      <p><b>{docKindLabel[created.doc.kind]} № {created.doc.number}</b> от {formatDate(created.doc.doc_date)} на {formatMoney(created.doc.amount)} сохранён в папку контрагента.</p>
      {#if created.missing.length}
        <div class="warn-box">Осталось пустым: {created.missing.map((k) => labels[k] ?? k).join('; ')}. Дополните карточку контрагента или реквизиты организации и сформируйте документ заново.</div>
      {/if}
    </div>
  {:else if defaults}
    {#if !defaults.has_company}
      <div class="warn-box">Укажите в сделке контрагента — документ оформляется на него.</div>
    {:else}
      {#if !defaults.org_filled}
        <div class="warn-box">Реквизиты вашей организации не заполнены (ИНН, расчётный счёт).{#if session.isAdmin} <a href="/admin/organization">Заполнить</a>{:else} Обратитесь к администратору.{/if}</div>
      {/if}
      <div class="kinds" role="radiogroup" aria-label="Вид документа">
        {#each Object.entries(docKindLabel) as [k, l]}
          <button type="button" role="radio" aria-checked={f.kind === k} class="chip" class:on={f.kind === k} onclick={() => { f.kind = k as DocKind; pickTemplate(); }}>{l}</button>
        {/each}
      </div>
      {#if kindTemplates.length === 0}
        <div class="warn-box">Нет шаблона для вида «{docKindLabel[f.kind]}».{#if session.isAdmin} Добавьте его в <a href="/admin/templates">Администрирование → Шаблоны документов → Документы продаж</a>.{:else} Попросите администратора добавить шаблон.{/if}</div>
      {:else}
        <div class="grid">
          <label class="field wide">Шаблон
            <select class="input" bind:value={f.template_id}>{#each kindTemplates as t}<option value={t.id}>{t.name}</option>{/each}</select>
          </label>
          <label class="field">Дата <input class="input" type="date" bind:value={f.date} /></label>
          <label class="field">Номер <input class="input mono" bind:value={f.number} placeholder={autoNumber ? `${autoNumber} (автоматически)` : 'автоматически'} /></label>
          <label class="field wide">Предмет — наименование работ, услуг
            <textarea class="input" rows="2" bind:value={f.subject}></textarea>
          </label>
          <label class="field">Сумма, ₽ <input class="input" inputmode="decimal" bind:value={f.amount} /></label>
          <label class="field">НДС
            <select class="input" bind:value={f.vat_rate}>{#each vatRates as v}<option value={v}>{vatLabel(v)}</option>{/each}</select>
            {#if vatPreview}<span class="faint small">в т. ч. НДС {formatMoney(vatPreview)}</span>{/if}
          </label>
          {#if needsBasis}
            <label class="field wide">Основание
              <select class="input" bind:value={f.basis_id}>
                <option value="">Основной договор (без номера)</option>
                {#each defaults.contracts as c}<option value={c.id}>Договор № {c.number} от {formatDate(c.date)}</option>{/each}
              </select>
            </label>
          {/if}
        </div>
      {/if}
    {/if}
  {/if}
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    {#if created}
      {#if created.doc.file_id}<a class="btn" href="/editor/{created.doc.file_id}" target="_blank" rel="noopener">Открыть документ</a>{/if}
      <div class="spacer"></div>
      <button class="btn primary" onclick={() => (open = false)}>Готово</button>
    {:else}
      <button class="btn" onclick={() => (open = false)}>Отмена</button>
      <button class="btn primary" disabled={busy || !f.template_id || !defaults?.has_company || !(amountNum >= 0)} onclick={create}>
        {busy ? 'Формирование…' : 'Сформировать'}
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  .block { display: grid; gap: 2px; grid-template-columns: minmax(0, 1fr); }
  .head { display: flex; align-items: center; gap: 6px; margin-bottom: 4px; }
  h3 { margin: 0; }
  .none { margin: 4px 0; font-size: 13px; }
  .doc { display: flex; gap: 10px; align-items: center; padding: 8px 2px; border-bottom: 1px solid var(--border); }
  .doc:last-of-type { border-bottom: 0; }
  .doc.cancelled { opacity: .55; }
  .doc.cancelled a { text-decoration: line-through; }
  .kind { flex: none; font-size: 11px; font-weight: 600; padding: 2px 6px; border-radius: 4px; width: 92px; text-align: center; background: var(--surface-2); color: var(--text-2); }
  .k-invoice { background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--accent); }
  .k-act { background: var(--ok-soft); color: var(--ok); }
  .k-contract { background: var(--warn-soft); color: var(--warn); }
  .main { flex: 1; min-width: 0; display: grid; gap: 2px; }
  .line1 { display: flex; gap: 10px; align-items: baseline; }
  .amt { margin-left: auto; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .line2 { display: flex; flex-wrap: wrap; gap: 2px 10px; font-size: 12.5px; color: var(--text-2); }
  .line2 > span:first-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
  .status { width: auto; height: 28px; font-size: 12.5px; padding: 0 6px; }
  .s-signed { color: var(--ok); }
  .s-sent { color: var(--accent); }
  .more { justify-self: start; margin-top: 4px; }
  .kinds { display: flex; gap: 6px; flex-wrap: wrap; }
  .chip { border: 1px solid var(--border-strong); background: var(--surface); border-radius: 999px; padding: 4px 12px; cursor: pointer; color: var(--text-2); font: inherit; font-size: 13px; }
  .chip.on { background: var(--accent); border-color: var(--accent); color: #fff; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px 12px; }
  .wide { grid-column: 1 / -1; }
  .warn-box { background: var(--warn-soft); color: var(--warn); padding: 8px 12px; border-radius: var(--radius); }
  .done { display: grid; gap: 10px; }
  .done p { margin: 0; }
  @media (max-width: 560px) { .grid { grid-template-columns: 1fr; } .kind { width: auto; } }
</style>
