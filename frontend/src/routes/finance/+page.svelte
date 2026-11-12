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
  import ChartColumn from '@lucide/svelte/icons/chart-column';
  import HandCoins from '@lucide/svelte/icons/hand-coins';
  import FileText from '@lucide/svelte/icons/file-text';
  import Wallet from '@lucide/svelte/icons/wallet';
  import type { Component } from 'svelte';

  type Tab = 'payments' | 'analytics' | 'receivables' | 'documents';
  // Section tabs with icons on top (as in «Хранилище» and CRM); each tab shows
  // its own title and actions.
  const tabs = $derived(
    ([
      ['payments', 'Платежи', Wallet, session.finance],
      ['analytics', 'Аналитика', ChartColumn, session.finance],
      ['receivables', 'Дебиторка', HandCoins, session.finance],
      ['documents', 'Документы', FileText, session.crm]
    ] as [Tab, string, Component<{ size?: number }>, boolean][]).filter(([, , , ok]) => ok)
  );
  const tab = $derived<Tab | undefined>(
    tabs.find(([k]) => k === page.url.searchParams.get('tab'))?.[0] ?? tabs[0]?.[0]
  );
  const setTab = (t: Tab) => goto(`/finance?tab=${t}`, { replaceState: true, noScroll: true, keepFocus: true });
  let settingsOpen = $state(false);
</script>

<svelte:head><title>Финансы</title></svelte:head>

<div class="page">
  {#if !tabs.length}
    <div class="page-head"><h1>Финансы</h1></div>
    <div class="card empty"><p class="muted">Нет доступа к финансам. Доступ выдаёт администратор портала («Администрирование → Пользователи», столбец «Финансы»).</p></div>
  {:else}
    <nav class="tabs-line" aria-label="Разделы финансов">
      {#each tabs as [k, l, Icon] (k)}
        <button class:on={tab === k} aria-current={tab === k ? 'page' : undefined} onclick={() => setTab(k)}><Icon size={17} />{l}</button>
      {/each}
    </nav>
    {#if tab === 'payments'}<PaymentsTab onsettings={session.financeEdit ? () => (settingsOpen = true) : undefined} />
    {:else if tab === 'analytics'}<AnalyticsTab />
    {:else if tab === 'receivables'}<ReceivablesTab onsettings={session.financeEdit ? () => (settingsOpen = true) : undefined} />
    {:else if tab === 'documents'}<DocumentsTab />{/if}
  {/if}
</div>

<FinanceSettings bind:open={settingsOpen} />

<style>
  .empty { padding: 30px; text-align: center; }
  .empty p { margin: 0; }
</style>
