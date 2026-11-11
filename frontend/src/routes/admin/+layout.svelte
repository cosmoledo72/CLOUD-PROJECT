<script lang="ts">
  import { page } from '$app/state';
  import { session } from '$lib/session.svelte';

  let { children } = $props();

  const tabs: { href: string; label: string; exact?: boolean; show?: () => boolean }[] = [
    { href: '/admin', label: 'Обзор', exact: true },
    { href: '/admin/users', label: 'Пользователи' },
    { href: '/admin/groups', label: 'Группы' },
    { href: '/admin/modules', label: 'Модули' },
    { href: '/admin/organization', label: 'Организация', show: () => session.has('crm') },
    { href: '/admin/requisites', label: 'Реквизиты проектов', show: () => session.has('projects') },
    { href: '/admin/templates', label: 'Шаблоны документов' },
    { href: '/admin/branding', label: 'Оформление' },
    { href: '/admin/mail', label: 'Почта' },
    { href: '/admin/audit', label: 'Журнал действий' },
    { href: '/admin/portal', label: 'Портал' }
  ];
  const isActive = (t: (typeof tabs)[number]) =>
    t.exact ? page.url.pathname === t.href : page.url.pathname.startsWith(t.href);
</script>

<div class="page">
  <div class="page-head"><h1>Администрирование</h1></div>
  {#if !session.isAdmin}
    <div class="card denied"><p class="muted">Раздел доступен только администраторам портала.</p></div>
  {:else}
    <nav class="tabs" aria-label="Разделы администрирования">
      {#each tabs.filter((t) => !t.show || t.show()) as t}
        <a href={t.href} class:on={isActive(t)} aria-current={isActive(t) ? 'page' : undefined}>{t.label}</a>
      {/each}
    </nav>
    {@render children()}
  {/if}
</div>

<style>
  .tabs {
    display: flex; gap: 2px; border-bottom: 1px solid var(--border); margin-bottom: 18px;
    overflow-x: auto; scrollbar-width: thin;
  }
  .tabs a {
    padding: 8px 12px; color: var(--text-2); border-bottom: 2px solid transparent; margin-bottom: -1px;
    white-space: nowrap;
  }
  .tabs a:hover { color: var(--text); text-decoration: none; }
  .tabs a.on { color: var(--accent); border-color: var(--accent); font-weight: 500; }
  .denied { padding: 24px; }
  .denied p { margin: 0; }
</style>
