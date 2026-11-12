<script lang="ts">
  // «Финансы → Дебиторка»: unpaid invoices (debt, payment term, overdue days)
  // by counterparty, and planned income that is overdue or due in 30 days.
  import { api, formatDate, formatMoney, invoiceStateLabel, plainMessage, type InvoiceDebt, type Payment } from '$lib/api';
  import { session } from '$lib/session.svelte';
  import EmptyState from '$lib/EmptyState.svelte';
  import { plural } from '$lib/fieldLook';
  import StatTile from '$lib/ui/StatTile.svelte';
  import AlarmClock from '@lucide/svelte/icons/alarm-clock';
  import CalendarClock from '@lucide/svelte/icons/calendar-clock';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import Receipt from '@lucide/svelte/icons/receipt';
  import Settings from '@lucide/svelte/icons/settings';
  import PaymentDialog from './PaymentDialog.svelte';

  type Data = {
    invoice_due_days: number;
    totals: { debt: number; overdue: number; plan_overdue: number };
    invoices: InvoiceDebt[];
    companies: { company_id: string | null; company_name: string; debt: number; overdue: number; invoices: number }[];
    plan: Payment[];
    crm: boolean;
  };
  let { onsettings }: { onsettings?: () => void } = $props();
  let data = $state<Data | null>(null);
  let error = $state('');
  let tick = $state(0);
  let company = $state('');

  $effect(() => {
    void tick;
    api.get<Data>('/finance/receivables').then((r) => { data = r; error = ''; }).catch((e) => (error = plainMessage(e.message)));
  });

  const today = new Date().toISOString().slice(0, 10);
  const shown = $derived((data?.invoices ?? []).filter((i) => !company || i.company_name === company));

  // Record a payment of an invoice, or mark a planned payment as received.
  let open = $state(false);
  let editing = $state<Payment | null>(null);
  let preset = $state<{ kind?: 'income'; invoice_id?: string | null }>({});
  function pay(i: InvoiceDebt) {
    editing = null;
    preset = { kind: 'income', invoice_id: i.id };
    open = true;
  }
  async function received(p: Payment) {
    try {
      await api.put(`/finance/payments/${p.id}`, {
        kind: p.kind, status: 'fact', pay_date: today, amount: p.amount, vat_rate: p.vat_rate, category_id: p.category_id,
        project_id: p.project_id, company_id: p.company_id, company_name: p.company_name, invoice_id: p.invoice_id, description: p.description
      });
      tick++;
    } catch (e) {
      error = plainMessage((e as Error).message);
    }
  }
</script>

<div class="page-head">
  <h1>Дебиторка</h1>
  <div class="spacer"></div>
  {#if onsettings}<button class="btn" onclick={onsettings}><Settings size={16} />Статьи и сроки</button>{/if}
</div>

{#if error}<div class="error-box">{error}</div>{/if}

{#if data}
  <div class="stat-grid">
    <StatTile label="Должны нам по счетам" value={formatMoney(data.totals.debt)} icon={Receipt} tone="accent"
      hint={data.invoices.length ? `${data.invoices.length} ${plural(data.invoices.length, 'счёт', 'счёта', 'счетов')} с долгом` : 'неоплаченных счетов нет'} />
    <StatTile label="Из них просрочено" value={formatMoney(data.totals.overdue)} icon={AlarmClock} tone={data.totals.overdue > 0 ? 'danger' : 'neutral'}
      valueTone={data.totals.overdue > 0 ? 'danger' : undefined} dim={!data.totals.overdue} hint="срок оплаты счёта — {data.invoice_due_days} дн." />
    <StatTile label="Просрочено по графику оплат" value={formatMoney(data.totals.plan_overdue)} icon={CalendarClock} tone={data.totals.plan_overdue > 0 ? 'danger' : 'neutral'}
      valueTone={data.totals.plan_overdue > 0 ? 'danger' : undefined} dim={!data.totals.plan_overdue} hint="плановые поступления, дата которых прошла" />
  </div>

  {#if data.crm}
    <div class="grid">
      <section class="card">
        <h2>По контрагентам</h2>
        {#if !data.companies.length}
          <EmptyState compact icon={CircleCheck} title="Долгов нет" text="Все выставленные счета оплачены." />
        {:else}
          <ul class="cos">
            <li><button class:on={!company} onclick={() => (company = '')}><span>Все</span><span class="v">{formatMoney(data.totals.debt)}</span></button></li>
            {#each data.companies as c (c.company_name)}
              <li>
                <button class:on={company === c.company_name} onclick={() => (company = company === c.company_name ? '' : c.company_name)}>
                  <span class="nm">{c.company_name}<small class="faint"> · {c.invoices} {c.invoices === 1 ? 'счёт' : c.invoices < 5 ? 'счёта' : 'счетов'}</small></span>
                  <span class="v">{formatMoney(c.debt)}{#if c.overdue}<small class="late">просрочено {formatMoney(c.overdue)}</small>{/if}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </section>

      <section class="card tbl">
        <h2>Неоплаченные счета{company ? ` — ${company}` : ''}</h2>
        {#if !shown.length}
          <EmptyState compact icon={Receipt} title="Неоплаченных счетов нет" text={company ? 'У этого контрагента всё оплачено.' : 'Счета выставляются в карточке сделки.'} />
        {:else}
          <div class="scroll">
            <table>
              <thead><tr><th>Счёт</th><th>Проект</th><th class="num">Сумма</th><th class="num">Долг</th><th>Оплатить до</th><th></th></tr></thead>
              <tbody>
                {#each shown as i (i.id)}
                  <tr>
                    <td>№ {i.number} от {formatDate(i.doc_date)}<div class="faint small">{i.company_name}</div></td>
                    <td class="clip">{#if i.project_id}<a href="/projects/{i.project_id}?tab=finance">{i.project_name}</a>{:else}<span class="faint">—</span>{/if}</td>
                    <td class="num">{formatMoney(i.amount)}{#if i.paid}<div class="faint small">оплачено {formatMoney(i.paid)}</div>{/if}</td>
                    <td class="num"><b>{formatMoney(i.debt)}</b></td>
                    <td class="nowrap">{formatDate(i.due_date)}
                      <div><span class="st st-{i.state}">{i.state === 'overdue' ? `просрочен на ${i.days_overdue} дн.` : invoiceStateLabel[i.state]}</span></div></td>
                    <td class="act">{#if session.financeEdit}<button class="btn sm" onclick={() => pay(i)}>Оплата</button>{/if}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </section>
    </div>
  {/if}

  <section class="card tbl">
    <h2>График оплат: просроченные и ближайшие 30 дней</h2>
    {#if !data.plan.length}
      <EmptyState compact icon={CalendarClock} title="Плановых поступлений нет" text="Этапы оплаты добавляются кнопкой «План оплаты» на вкладке «Платежи» или в проекте." />
    {:else}
      <div class="scroll">
        <table>
          <thead><tr><th>Ожидается</th><th class="num">Сумма</th><th>Проект</th><th>Плательщик</th><th>Комментарий</th><th></th></tr></thead>
          <tbody>
            {#each data.plan as p (p.id)}
              <tr>
                <td class="nowrap" class:late={p.pay_date < today}>{formatDate(p.pay_date)}{#if p.pay_date < today}<div class="small">просрочено</div>{/if}</td>
                <td class="num"><b>{formatMoney(p.amount)}</b></td>
                <td class="clip">{#if p.project_id}<a href="/projects/{p.project_id}?tab=finance">{p.project_name}</a>{:else}<span class="faint">—</span>{/if}</td>
                <td class="clip">{p.company_name || '—'}{#if p.invoice_number}<div class="faint small">счёт № {p.invoice_number}</div>{/if}</td>
                <td class="clip desc">{p.description}</td>
                <td class="act">{#if session.financeEdit}<button class="btn sm" onclick={() => received(p)} title="Отметить, что деньги пришли сегодня">Поступило</button>{/if}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </section>
{/if}

<PaymentDialog bind:open payment={editing} {preset} onsaved={() => tick++} />

<style>
  .late { color: var(--danger); }
  h2 { font-size: 14px; margin: 0 0 8px; }
  .grid { display: grid; grid-template-columns: minmax(260px, 1fr) minmax(0, 2.2fr); gap: 14px; align-items: start; margin-bottom: 14px; }
  .grid > section, .tbl { padding: 14px 16px; }
  .tbl { margin-bottom: 14px; }
  .grid .tbl { margin-bottom: 0; }
  .cos { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .cos button { width: 100%; display: flex; justify-content: space-between; gap: 10px; align-items: flex-start; border: 0; background: none;
                padding: 7px 8px; border-radius: 6px; font: inherit; font-size: 13.5px; text-align: left; cursor: pointer; color: var(--text); }
  .cos button:hover { background: var(--surface-2); }
  .cos button.on { background: var(--accent-soft); }
  .cos .nm { min-width: 0; }
  .cos .v { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; display: grid; font-weight: 500; }
  .cos small { font-size: 11.5px; font-weight: 400; }
  .scroll { overflow-x: auto; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  th { text-align: left; font-weight: 500; color: var(--text-3); padding: 6px 8px; border-bottom: 1px solid var(--border); white-space: nowrap; }
  td { padding: 8px; border-bottom: 1px solid var(--border); vertical-align: top; }
  tbody tr:last-child td { border-bottom: 0; }
  .num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .nowrap { white-space: nowrap; }
  .clip { max-width: 220px; overflow-wrap: anywhere; }
  .clip a { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .desc { color: var(--text-2); }
  .act { text-align: right; white-space: nowrap; }
  .st { font-size: 11.5px; padding: 0 7px; border-radius: 999px; font-weight: 500; }
  .st-overdue { background: var(--danger-soft); color: var(--danger); }
  .st-partial { background: var(--warn-soft); color: var(--warn); }
  .st-issued { background: var(--surface-2); color: var(--text-2); }
  .st-paid { background: var(--ok-soft); color: var(--ok); }
  .empty { margin: 6px 0; }
  @media (max-width: 900px) { .grid { grid-template-columns: 1fr; } }
</style>
