<script lang="ts">
  import { page } from '$app/state';
  import { crmDue } from '$lib/crm/due.svelte';
  import { session } from '$lib/session.svelte';

  let { children } = $props();
  const tabs = $derived([
    { href: '/crm/deals', label: 'Сделки' },
    { href: '/crm/tasks', label: 'Дела' },
    { href: '/crm/companies', label: 'Контрагенты' },
    { href: '/crm/contacts', label: 'Контакты' },
    ...(session.crmManager ? [{ href: '/crm/settings', label: 'Этапы воронки' }] : [])
  ]);
</script>

<div class="page">
  {#if !session.crm}
    <div class="card denied"><p class="muted">У вас нет доступа к CRM. Обратитесь к администратору портала.</p></div>
  {:else}
    <nav class="tabs" aria-label="Разделы CRM">
      {#each tabs as t}
        <a href={t.href} class:on={page.url.pathname.startsWith(t.href)} aria-current={page.url.pathname.startsWith(t.href) ? 'page' : undefined}>
          {t.label}{#if t.href === '/crm/tasks' && crmDue.count}<span class="cnt" class:hot={crmDue.overdue > 0}>{crmDue.count}</span>{/if}
        </a>
      {/each}
    </nav>
    {@render children()}
  {/if}
</div>

<style>
  .tabs { display: flex; gap: 2px; border-bottom: 1px solid var(--border); margin-bottom: 16px; overflow-x: auto; }
  .tabs a { padding: 8px 12px; color: var(--text-2); border-bottom: 2px solid transparent; margin-bottom: -1px; white-space: nowrap; }
  .tabs a:hover { color: var(--text); text-decoration: none; }
  .tabs a.on { color: var(--accent); border-color: var(--accent); font-weight: 500; }
  .cnt { margin-left: 6px; padding: 0 6px; border-radius: 9px; font-size: 11px; font-weight: 600; background: var(--warn); color: #fff; }
  .cnt.hot { background: var(--danger); }
  .denied { padding: 24px; }
  .denied p { margin: 0; }
</style>
