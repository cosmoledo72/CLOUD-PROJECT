<script lang="ts">
  import { page } from '$app/state';
  import { crmDue } from '$lib/crm/due.svelte';
  import { session } from '$lib/session.svelte';
  import Building2 from '@lucide/svelte/icons/building-2';
  import ChartColumn from '@lucide/svelte/icons/chart-column';
  import Contact from '@lucide/svelte/icons/contact';
  import Handshake from '@lucide/svelte/icons/handshake';
  import ListTodo from '@lucide/svelte/icons/list-todo';
  import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal';

  let { children } = $props();
  // Section tabs with icons, as in «Хранилище» and «Финансы».
  const tabs = $derived([
    { href: '/crm/deals', label: 'Сделки', icon: Handshake },
    { href: '/crm/tasks', label: 'Дела', icon: ListTodo },
    { href: '/crm/companies', label: 'Контрагенты', icon: Building2 },
    { href: '/crm/contacts', label: 'Контакты', icon: Contact },
    { href: '/crm/reports', label: 'Отчёты', icon: ChartColumn },
    ...(session.crmManager ? [{ href: '/crm/settings', label: 'Этапы воронки', icon: SlidersHorizontal }] : [])
  ]);
</script>

<div class="page">
  {#if !session.crm}
    <div class="card denied"><p class="muted">У вас нет доступа к CRM. Обратитесь к администратору портала.</p></div>
  {:else}
    <nav class="tabs-line" aria-label="Разделы CRM">
      {#each tabs as t (t.href)}
        {@const Icon = t.icon}
        <a href={t.href} class:on={page.url.pathname.startsWith(t.href)} aria-current={page.url.pathname.startsWith(t.href) ? 'page' : undefined}>
          <Icon size={17} />{t.label}{#if t.href === '/crm/tasks' && crmDue.count}<span class="cnt" class:hot={crmDue.overdue > 0}>{crmDue.count}</span>{/if}
        </a>
      {/each}
    </nav>
    {@render children()}
  {/if}
</div>

<style>
  .tabs-line .cnt { margin-left: 2px; padding: 0 6px; border-radius: 9px; font-size: 11px; font-weight: 600; background: var(--warn); color: #fff; }
  .tabs-line .cnt.hot { background: var(--danger); }
  .denied { padding: 24px; }
  .denied p { margin: 0; }
</style>
