<script lang="ts">
  // «Финансы»: payments, analytics, receivables (finance role) and the sales
  // documents registry (CRM users). The tab is kept in the address (?tab=).
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import AnalyticsTab from '$lib/finance/AnalyticsTab.svelte';
  import DocumentsTab from '$lib/finance/DocumentsTab.svelte';
  import FinanceSettings from '$lib/finance/FinanceSettings.svelte';
  import PaymentsTab from '$lib/finance/PaymentsTab.svelte';
  import ReceivablesTab from '$lib/finance/ReceivablesTab.svelte';
  import { session } from '$lib/session.svelte';
  import Settings from '@lucide/svelte/icons/settings';

  type Tab = 'payments' | 'analytics' | 'receivables' | 'documents';
  const tabs = $derived(
    ([
      ['payments', 'Платежи', session.finance],
      ['analytics', 'Аналитика', session.finance],
      ['receivables', 'Дебиторка', session.finance],
      ['documents', 'Документы', session.crm]
    ] as [Tab, string, boolean][]).filter(([, , ok]) => ok)
  );
  const tab = $derived<Tab | undefined>(
    tabs.find(([k]) => k === page.url.searchParams.get('tab'))?.[0] ?? tabs[0]?.[0]
  );
  const setTab = (t: Tab) => goto(`/finance?tab=${t}`, { replaceState: true, noScroll: true, keepFocus: true });
  let settingsOpen = $state(false);
</script>

<svelte:head><title>Финансы</title></svelte:head>

<div class="page">
  <div class="page-head">
    <h1>Финансы</h1>
    <div class="spacer"></div>
    {#if session.financeEdit}
      <button class="btn" onclick={() => (settingsOpen = true)}><Settings size={15} /> Статьи и сроки</button>
    {/if}
  </div>

  {#if !tabs.length}
    <div class="card empty"><p class="muted">Нет доступа к финансам. Доступ выдаёт администратор портала («Администрирование → Пользователи», столбец «Финансы»).</p></div>
  {:else}
    <nav class="tabs" aria-label="Разделы финансов">
      {#each tabs as [k, l] (k)}
        <button class:on={tab === k} aria-current={tab === k ? 'page' : undefined} onclick={() => setTab(k)}>{l}</button>
      {/each}
    </nav>
    {#if tab === 'payments'}<PaymentsTab />
    {:else if tab === 'analytics'}<AnalyticsTab />
    {:else if tab === 'receivables'}<ReceivablesTab />
    {:else if tab === 'documents'}<DocumentsTab />{/if}
  {/if}
</div>

<FinanceSettings bind:open={settingsOpen} />

<style>
  .page-head .btn { display: inline-flex; gap: 6px; align-items: center; }
  .tabs { display: flex; gap: 2px; border-bottom: 1px solid var(--border); margin-bottom: 16px; overflow-x: auto; }
  .tabs button { border: 0; background: none; font: inherit; padding: 8px 14px; color: var(--text-2); border-bottom: 2px solid transparent;
                 margin-bottom: -1px; cursor: pointer; white-space: nowrap; }
  .tabs button:hover { color: var(--text); }
  .tabs button.on { color: var(--accent); border-color: var(--accent); font-weight: 500; }
  .empty { padding: 30px; text-align: center; }
  .empty p { margin: 0; }
</style>
