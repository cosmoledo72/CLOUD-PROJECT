<script lang="ts">
  // Project tab «Финансы»: budget (contract amount or deal), paid, due, spent,
  // profit; the payment schedule; invoices of the project's deal; payments.
  // Finance editors manage every payment; the project manager adds expenses.
  import { api, formatDate, formatMoney, invoiceStateLabel, plainMessage, type Payment, type ProjectFinance } from '$lib/api';
  import { session } from '$lib/session.svelte';
  import Plus from '@lucide/svelte/icons/plus';
  import PaymentDialog from './PaymentDialog.svelte';
  import { signed } from './period';

  let { projectId }: { projectId: string } = $props();

  let f = $state<ProjectFinance | null>(null);
  let error = $state('');
  let tick = $state(0);
  $effect(() => {
    void tick;
    api.get<ProjectFinance>(`/projects/${projectId}/finance`).then((r) => { f = r; error = ''; }).catch((e) => (error = plainMessage(e.message)));
  });

  const today = new Date().toISOString().slice(0, 10);
  const schedule = $derived((f?.payments ?? []).filter((p) => p.status === 'plan').sort((a, b) => a.pay_date.localeCompare(b.pay_date)));
  const facts = $derived((f?.payments ?? []).filter((p) => p.status === 'fact'));
  const paidPct = $derived(f?.budget ? Math.min(100, Math.round((f.paid / f.budget) * 100)) : 0);
  const spentPct = $derived(f?.budget ? Math.min(100, Math.round((f.spent / f.budget) * 100)) : 0);

  let open = $state(false);
  let editing = $state<Payment | null>(null);
  let preset = $state<{ kind?: 'income' | 'expense'; status?: 'plan' | 'fact'; project_id?: string; invoice_id?: string | null }>({});
  let managerOnly = $state(false);
  function add(kind: 'income' | 'expense', status: 'plan' | 'fact' = 'fact', invoice_id: string | null = null) {
    editing = null;
    managerOnly = !f?.can_edit;
    preset = { kind, status, project_id: projectId, invoice_id };
    open = true;
  }
  function edit(p: Payment) {
    const mine = !f?.can_edit && p.kind === 'expense';
    if (!f?.can_edit && !mine) return;
    editing = p;
    managerOnly = !f?.can_edit;
    preset = { project_id: projectId };
    open = true;
  }
  const editable = (p: Payment) => !!f && (f.can_edit || (f.can_add_expense && p.kind === 'expense'));
</script>

{#if error}<div class="error-box">{error}</div>{/if}

{#if f}
  <div class="tiles">
    <div class="card t">
      <span>Бюджет</span>
      <b>{f.budget != null ? formatMoney(f.budget) : '—'}</b>
      <small class="faint">{f.budget_label ? `из «${f.budget_label}»` : 'заполните «Стоимость работ» в реквизитах'}</small>
    </div>
    <div class="card t">
      <span>Оплачено заказчиком</span>
      <b class="in">{formatMoney(f.paid)}</b>
      {#if f.budget}<div class="bar"><i class="in" style="width:{paidPct}%"></i></div><small class="faint">{paidPct}% бюджета</small>{/if}
    </div>
    <div class="card t">
      <span>Остаток к оплате</span>
      <b>{f.due != null ? formatMoney(f.due) : '—'}</b>
      {#if f.plan_overdue}<small class="late">просрочено по графику {formatMoney(f.plan_overdue)}</small>
      {:else if f.income_plan}<small class="faint">в графике оплат {formatMoney(f.income_plan)}</small>{/if}
    </div>
    <div class="card t">
      <span>Расходы</span>
      <b class="out">{formatMoney(f.spent)}</b>
      {#if f.budget}<div class="bar"><i class="out" style="width:{spentPct}%"></i></div><small class="faint">{spentPct}% бюджета</small>{/if}
    </div>
    {#if f.labor || f.labor_expected}
      <div class="card t">
        <span>Трудозатраты</span>
        <b class="out">{formatMoney(f.labor)}</b>
        <small class="faint">{#if f.labor_expected}ещё ожидается {formatMoney(f.labor_expected)} — фикс. оплата невыполненных задач{:else}часы × ставки и фикс. оплата выполненных задач{/if}</small>
      </div>
    {/if}
    <div class="card t">
      <span>Прибыль по договору</span>
      <b class:in={(f.profit ?? 0) > 0} class:out={(f.profit ?? 0) < 0}>{f.profit != null ? formatMoney(f.profit) : '—'}</b>
      <small class="faint">бюджет − расходы{f.labor || f.labor_expected ? ' − трудозатраты' : ''}; по деньгам сейчас {formatMoney(f.cash)}</small>
    </div>
  </div>

  <div class="actions">
    {#if f.can_edit}
      <button class="btn" onclick={() => add('income', 'plan')}><Plus size={15} /> Этап оплаты</button>
      <button class="btn add-in" onclick={() => add('income')}><Plus size={15} /> Поступление</button>
    {/if}
    {#if f.can_add_expense}<button class="btn add-out" onclick={() => add('expense')}><Plus size={15} /> Расход</button>{/if}
  </div>

  <div class="grid">
    <section class="card sec">
      <h3>График оплат</h3>
      {#if !schedule.length}
        <p class="muted small">{f.can_edit ? 'Добавьте этапы оплаты по договору: аванс, промежуточные платежи, окончательный расчёт.' : 'Этапы оплаты не заданы.'}</p>
      {:else}
        <ul class="sched">
          {#each schedule as p (p.id)}
            <li class:late={p.pay_date < today}>
              <button class="row-btn" disabled={!editable(p)} onclick={() => edit(p)}>
                <span class="d">{formatDate(p.pay_date)}{#if p.pay_date < today}<small> · просрочен</small>{/if}</span>
                <span class="desc">{p.description || p.category_name || 'Оплата'}</span>
                <span class="v">{formatMoney(p.amount)}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    {#if session.has('crm')}
      <section class="card sec">
        <h3>Счета по сделке</h3>
        {#if !f.invoices.length}
          <p class="muted small">Счетов нет. Счета выставляются в карточке сделки, из которой создан проект.</p>
        {:else}
          <ul class="sched">
            {#each f.invoices as i (i.id)}
              <li>
                <div class="row-btn static">
                  <span class="d">№ {i.number} от {formatDate(i.doc_date)}</span>
                  <span class="desc"><span class="st st-{i.state}">{i.state === 'overdue' ? `просрочен на ${i.days_overdue} дн.` : invoiceStateLabel[i.state]}</span>
                    {#if i.paid && i.state !== 'paid'}<small class="faint"> оплачено {formatMoney(i.paid)}</small>{/if}</span>
                  <span class="v">{formatMoney(i.amount)}</span>
                  {#if f.can_edit && i.state !== 'paid'}<button class="btn sm" onclick={() => add('income', 'fact', i.id)}>Оплата</button>{/if}
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}
  </div>

  <section class="card tbl">
    <h3>Платежи</h3>
    {#if !facts.length}
      <p class="muted small">Проведённых платежей пока нет.</p>
    {:else}
      <table>
        <thead><tr><th>Дата</th><th class="num">Сумма</th><th>Статья</th><th>Контрагент</th><th>Комментарий</th></tr></thead>
        <tbody>
          {#each facts as p (p.id)}
            <tr class:click={editable(p)} onclick={() => editable(p) && edit(p)}>
              <td class="nowrap">{formatDate(p.pay_date)}</td>
              <td class="num"><span class={p.kind === 'income' ? 'in' : 'out'}>{signed(p.amount, p.kind)}</span></td>
              <td>{p.category_name ?? '—'}</td>
              <td>{p.company_name || '—'}{#if p.invoice_number}<div class="faint small">счёт № {p.invoice_number}</div>{/if}</td>
              <td class="desc">{p.description}{#if p.created_by_name}<div class="faint small">{p.created_by_name}</div>{/if}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>
{/if}

<PaymentDialog bind:open payment={editing} {preset} {managerOnly} onsaved={() => tick++} />

<style>
  .tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 10px; margin-bottom: 12px; }
  .t { padding: 12px 14px; display: grid; gap: 3px; align-content: start; }
  .t span { font-size: 12px; color: var(--text-3); text-transform: uppercase; letter-spacing: .04em; }
  .t b { font-size: 19px; font-weight: 650; font-variant-numeric: tabular-nums; }
  .t small { font-size: 12px; }
  .bar { height: 5px; border-radius: 3px; background: var(--surface-2); overflow: hidden; margin-top: 3px; }
  .bar i { display: block; height: 100%; }
  .bar i.in { background: var(--ok); }
  .bar i.out { background: var(--danger); opacity: .8; }
  .in { color: var(--ok); }
  .out { color: var(--danger); }
  .late { color: var(--danger); }
  .actions { display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 12px; }
  .actions .btn { display: inline-flex; gap: 6px; align-items: center; }
  .add-in { color: var(--ok); }
  .add-out { color: var(--danger); }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(340px, 1fr)); gap: 12px; margin-bottom: 12px; align-items: start; }
  .sec, .tbl { padding: 14px 16px; }
  h3 { margin: 0 0 8px; font-size: 12.5px; text-transform: uppercase; letter-spacing: .05em; color: var(--text-2); }
  p { margin: 0; }
  .sched { list-style: none; margin: 0; padding: 0; }
  .sched li { border-top: 1px solid var(--border); }
  .sched li:first-child { border-top: 0; }
  .row-btn { display: flex; gap: 10px; align-items: center; width: 100%; border: 0; background: none; padding: 8px 2px; font: inherit; font-size: 13.5px; color: var(--text); text-align: left; }
  button.row-btn:not(:disabled) { cursor: pointer; }
  button.row-btn:not(:disabled):hover { background: var(--surface-2); }
  .row-btn .d { white-space: nowrap; color: var(--text-2); min-width: 92px; }
  li.late .d { color: var(--danger); }
  .row-btn .desc { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  .row-btn .v { font-variant-numeric: tabular-nums; font-weight: 500; white-space: nowrap; }
  .st { font-size: 11.5px; padding: 0 7px; border-radius: 999px; font-weight: 500; }
  .st-overdue { background: var(--danger-soft); color: var(--danger); }
  .st-partial { background: var(--warn-soft); color: var(--warn); }
  .st-issued { background: var(--surface-2); color: var(--text-2); }
  .st-paid { background: var(--ok-soft); color: var(--ok); }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  th { text-align: left; font-weight: 500; color: var(--text-3); padding: 6px 8px; border-bottom: 1px solid var(--border); white-space: nowrap; }
  td { padding: 8px; border-bottom: 1px solid var(--border); vertical-align: top; }
  tbody tr:last-child td { border-bottom: 0; }
  tr.click { cursor: pointer; }
  tr.click:hover { background: var(--surface-2); }
  .num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; font-weight: 500; }
  .nowrap { white-space: nowrap; }
  .desc { color: var(--text-2); }
</style>
