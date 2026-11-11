<script lang="ts">
  // Create or edit a payment: income or expense, actual or planned (a stage of
  // the payment schedule), category, project, counterparty, the invoice it pays.
  // In "project manager" mode only an expense of the given project can be entered.
  import {
    api, formatDate, formatMoney, localToday, plainMessage, vatLabel,
    type CompanyRef, type FinanceCategory, type InvoiceDebt, type Payment, type PayKind, type PayStatus
  } from '$lib/api';
  import CompanyPicker from '$lib/crm/CompanyPicker.svelte';
  import Modal from '$lib/Modal.svelte';
  import { session } from '$lib/session.svelte';

  type Preset = { kind?: PayKind; status?: PayStatus; project_id?: string | null; invoice_id?: string | null };
  let {
    open = $bindable(false),
    payment = null,
    preset = {},
    managerOnly = false,
    onsaved
  }: {
    open?: boolean;
    payment?: Payment | null;
    preset?: Preset;
    /** A project manager without finance rights: expense of `preset.project_id` only. */
    managerOnly?: boolean;
    onsaved?: () => void;
  } = $props();

  let kind = $state<PayKind>('income');
  let status = $state<PayStatus>('fact');
  let date = $state(localToday());
  let amount = $state('');
  let vat = $state('none');
  let categoryId = $state('');
  let projectId = $state('');
  let company = $state<CompanyRef | null>(null);
  let invoiceId = $state('');
  let description = $state('');
  let error = $state('');
  let busy = $state(false);

  let categories = $state<FinanceCategory[]>([]);
  let projects = $state<{ id: string; name: string; status: string; fields?: Record<string, unknown> }[]>([]);
  let invoices = $state<InvoiceDebt[]>([]);
  let vatRates = $state<string[]>(['none']);

  let wasOpen = false;
  $effect(() => {
    if (open && !wasOpen) init();
    wasOpen = open;
  });

  function init() {
    error = '';
    const p = payment;
    kind = p?.kind ?? (managerOnly ? 'expense' : preset.kind ?? 'income');
    status = p?.status ?? preset.status ?? 'fact';
    date = p?.pay_date ?? localToday();
    amount = p ? String(p.amount).replace('.', ',') : '';
    vat = p?.vat_rate ?? 'none';
    categoryId = p?.category_id ?? '';
    projectId = p?.project_id ?? preset.project_id ?? '';
    company = p && (p.company_id || p.company_name) ? { id: p.company_id, name: p.company_name, inn: null } : null;
    invoiceId = p?.invoice_id ?? preset.invoice_id ?? '';
    description = p?.description ?? '';
    api.get<{ categories: FinanceCategory[] }>('/finance/categories').then((r) => {
      categories = r.categories;
      // A sensible default category for a new payment.
      if (!p && !categoryId) categoryId = r.categories.find((c) => c.kind === kind && !c.is_archived)?.id ?? '';
    }).catch(() => {});
    api.get<{ vat_rates: string[] }>('/finance/settings').then((r) => (vatRates = r.vat_rates)).catch(() => {});
    if (!managerOnly) {
      api.get<{ projects: typeof projects }>('/projects').then((r) => (projects = r.projects)).catch(() => {});
      if (session.financeEdit && session.has('crm')) {
        api.get<{ invoices: InvoiceDebt[] }>('/finance/invoices').then((r) => {
          invoices = r.invoices;
          if (!p && invoiceId) pickInvoice(invoiceId);
        }).catch(() => {});
      }
    }
  }

  const cats = $derived(categories.filter((c) => c.kind === kind && (!c.is_archived || c.id === categoryId)));
  // Switching the kind resets a category of the other kind.
  $effect(() => {
    if (categoryId && categories.length && !cats.some((c) => c.id === categoryId)) {
      categoryId = cats.find((c) => !c.is_archived)?.id ?? '';
    }
  });
  const openInvoices = $derived(invoices.filter((i) => i.state !== 'paid' || i.id === invoiceId));
  const chosenInvoice = $derived(invoices.find((i) => i.id === invoiceId) ?? null);

  function pickInvoice(id: string) {
    invoiceId = id;
    const i = invoices.find((x) => x.id === id);
    if (!i) return;
    if (!amount && i.debt > 0) amount = String(i.debt).replace('.', ',');
    if (!company) company = { id: i.company_id, name: i.company_name, inn: null };
    if (!projectId && i.project_id) projectId = i.project_id;
    if (!description) description = `Оплата по счёту № ${i.number} от ${formatDate(i.doc_date)}`;
  }

  const num = $derived(Number(amount.replace(/\s/g, '').replace(',', '.')));
  const valid = $derived(num > 0 && !!date && (!managerOnly || !!projectId));

  async function save() {
    error = '';
    busy = true;
    const body = {
      kind, status, pay_date: date, amount: num, vat_rate: vat,
      category_id: categoryId || null, project_id: projectId || null,
      company_id: company?.id ?? null, company_name: company?.name ?? '',
      invoice_id: kind === 'income' && invoiceId ? invoiceId : null, description
    };
    try {
      if (payment) await api.put(`/finance/payments/${payment.id}`, body);
      else await api.post('/finance/payments', body);
      open = false;
      onsaved?.();
    } catch (e) {
      error = plainMessage((e as Error).message);
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!payment || !confirm('Удалить платёж?')) return;
    try {
      await api.del(`/finance/payments/${payment.id}`);
      open = false;
      onsaved?.();
    } catch (e) {
      error = plainMessage((e as Error).message);
    }
  }

  const title = $derived(
    payment ? 'Платёж' : managerOnly ? 'Расход проекта' : status === 'plan' ? 'Плановый платёж' : kind === 'income' ? 'Поступление' : 'Расход'
  );
</script>

<Modal bind:open {title} width={560}>
  {#if !managerOnly}
    <div class="row2">
      <div class="seg" role="radiogroup" aria-label="Тип">
        <button role="radio" aria-checked={kind === 'income'} class:on={kind === 'income'} class="in" onclick={() => (kind = 'income')}>Поступление</button>
        <button role="radio" aria-checked={kind === 'expense'} class:on={kind === 'expense'} class="out" onclick={() => (kind = 'expense')}>Расход</button>
      </div>
      <div class="seg" role="radiogroup" aria-label="Статус">
        <button role="radio" aria-checked={status === 'fact'} class:on={status === 'fact'} onclick={() => (status = 'fact')}>Проведён</button>
        <button role="radio" aria-checked={status === 'plan'} class:on={status === 'plan'} onclick={() => (status = 'plan')}>План</button>
      </div>
    </div>
    {#if status === 'plan'}<p class="faint small hint">Плановый платёж — этап графика оплат. Когда деньги придут, откройте его и отметьте «Проведён».</p>{/if}
  {/if}
  <div class="grid3">
    <label class="field">{status === 'plan' ? 'Ожидается' : 'Дата'} <input class="input" type="date" bind:value={date} /></label>
    <label class="field">Сумма, ₽ <input class="input num" inputmode="decimal" bind:value={amount} placeholder="0" /></label>
    <label class="field">НДС
      <select class="input" bind:value={vat}>{#each vatRates as r (r)}<option value={r}>{vatLabel(r)}</option>{/each}</select>
    </label>
  </div>
  {#if kind === 'income' && openInvoices.length && !managerOnly}
    <label class="field">Оплата счёта
      <select class="input" value={invoiceId} onchange={(e) => (e.currentTarget.value ? pickInvoice(e.currentTarget.value) : (invoiceId = ''))}>
        <option value="">— без счёта —</option>
        {#each openInvoices as i (i.id)}
          <option value={i.id}>№ {i.number} от {formatDate(i.doc_date)} · {i.company_name} · долг {formatMoney(i.debt)}</option>
        {/each}
      </select>
      {#if chosenInvoice}<span class="faint small">Счёт на {formatMoney(chosenInvoice.amount)}, оплачено {formatMoney(chosenInvoice.paid)}</span>{/if}
    </label>
  {/if}
  <div class="grid2">
    <label class="field">Статья
      <select class="input" bind:value={categoryId}>
        <option value="">— без статьи —</option>
        {#each cats as c (c.id)}<option value={c.id}>{c.name}{c.is_archived ? ' (архив)' : ''}</option>{/each}
      </select>
    </label>
    {#if !managerOnly}
      <label class="field">Проект
        <select class="input" bind:value={projectId}>
          <option value="">— без проекта —</option>
          {#each projects.filter((p) => p.status !== 'closed' || p.id === projectId) as p (p.id)}
            <option value={p.id}>{typeof p.fields?.code === 'string' ? `${p.fields.code} · ` : ''}{p.name}</option>
          {/each}
        </select>
      </label>
    {/if}
  </div>
  <div class="field">
    <span>{kind === 'income' ? 'Плательщик' : 'Получатель'}</span>
    <CompanyPicker bind:value={company} />
  </div>
  <label class="field">Комментарий <input class="input" bind:value={description} maxlength="2000" placeholder="Например: п/п № 15, аванс 30%" /></label>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    {#if payment}<button class="btn ghost danger" onclick={remove}>Удалить</button><span class="spacer"></span>{/if}
    <button class="btn" onclick={() => (open = false)}>Отмена</button>
    <button class="btn primary" disabled={busy || !valid} onclick={save}>{payment ? 'Сохранить' : 'Добавить'}</button>
  {/snippet}
</Modal>

<style>
  .row2 { display: flex; gap: 10px; flex-wrap: wrap; }
  .seg { display: inline-flex; border: 1px solid var(--border-strong); border-radius: var(--radius); overflow: hidden; }
  .seg button { border: 0; background: var(--surface); padding: 6px 14px; cursor: pointer; font: inherit; color: var(--text-2); }
  .seg button.on { background: var(--accent); color: var(--on-accent); }
  .seg button.in.on { background: var(--ok); }
  .seg button.out.on { background: var(--danger); }
  .hint { margin: -4px 0 0; }
  .grid3 { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 10px; }
  .grid2 { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .num { font-variant-numeric: tabular-nums; }
  .spacer { flex: 1; }
  @media (max-width: 560px) { .grid3, .grid2 { grid-template-columns: 1fr; } }
</style>
