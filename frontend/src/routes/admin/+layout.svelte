<script lang="ts">
  // Administration: a grouped side menu of settings sections and the section itself.
  import { page } from '$app/state';
  import { session } from '$lib/session.svelte';
  import Blocks from '@lucide/svelte/icons/blocks';
  import Building from '@lucide/svelte/icons/building';
  import CalendarDays from '@lucide/svelte/icons/calendar-days';
  import DatabaseBackup from '@lucide/svelte/icons/database-backup';
  import ClipboardList from '@lucide/svelte/icons/clipboard-list';
  import FileStack from '@lucide/svelte/icons/file-stack';
  import Gauge from '@lucide/svelte/icons/gauge';
  import Globe from '@lucide/svelte/icons/globe';
  import History from '@lucide/svelte/icons/history';
  import Link2 from '@lucide/svelte/icons/link-2';
  import Mail from '@lucide/svelte/icons/mail';
  import Palette from '@lucide/svelte/icons/palette';
  import UserRound from '@lucide/svelte/icons/user-round';
  import UsersRound from '@lucide/svelte/icons/users-round';
  import type { Component } from 'svelte';

  let { children } = $props();

  type Item = { href: string; label: string; icon: Component; exact?: boolean; show?: () => boolean };
  const groups: { title: string; items: Item[] }[] = [
    { title: 'Портал', items: [
      { href: '/admin', label: 'Обзор', icon: Gauge, exact: true },
      { href: '/admin/portal', label: 'Портал и регистрация', icon: Globe },
      { href: '/admin/modules', label: 'Модули', icon: Blocks },
      { href: '/admin/branding', label: 'Оформление', icon: Palette },
      { href: '/admin/calendar', label: 'Рабочий календарь', icon: CalendarDays },
      { href: '/admin/mail', label: 'Почта', icon: Mail }
    ] },
    { title: 'Люди', items: [
      { href: '/admin/users', label: 'Пользователи', icon: UserRound },
      { href: '/admin/groups', label: 'Группы', icon: UsersRound }
    ] },
    { title: 'Данные и документы', items: [
      { href: '/admin/requisites', label: 'Реквизиты проектов', icon: ClipboardList, show: () => session.has('projects') },
      { href: '/admin/templates', label: 'Шаблоны документов', icon: FileStack },
      { href: '/admin/organization', label: 'Организация', icon: Building, show: () => session.has('crm') }
    ] },
    { title: 'Безопасность', items: [
      { href: '/admin/backups', label: 'Резервные копии', icon: DatabaseBackup, show: () => !!session.me?.backups },
      { href: '/admin/links', label: 'Ссылки для внешних', icon: Link2 },
      { href: '/admin/audit', label: 'Журнал действий', icon: History }
    ] }
  ];
  const isActive = (t: Item) => (t.exact ? page.url.pathname === t.href : page.url.pathname.startsWith(t.href));
  const current = $derived(groups.flatMap((g) => g.items).find(isActive));
</script>

<div class="page admin">
  {#if !session.isAdmin}
    <div class="page-head"><h1>Администрирование</h1></div>
    <div class="card denied"><p class="muted">Раздел доступен только администраторам портала.</p></div>
  {:else}
    <aside class="anav" aria-label="Разделы администрирования">
      <div class="atitle">Администрирование</div>
      {#each groups as g (g.title)}
        {@const items = g.items.filter((t) => !t.show || t.show())}
        {#if items.length}
          <div class="grp">{g.title}</div>
          {#each items as t (t.href)}
            {@const Icon = t.icon}
            <a href={t.href} class:on={isActive(t)} aria-current={isActive(t) ? 'page' : undefined}><Icon size={17} />{t.label}</a>
          {/each}
        {/if}
      {/each}
    </aside>
    <section class="acontent">
      <div class="page-head"><h1>{current?.label ?? 'Администрирование'}</h1></div>
      {@render children()}
    </section>
  {/if}
</div>

<style>
  .admin { display: grid; grid-template-columns: 210px minmax(0, 1fr); gap: 28px; align-items: start; max-width: 1560px; }
  .anav { position: sticky; top: 24px; display: grid; gap: 1px; }
  .atitle { font-size: 13px; font-weight: 600; color: var(--text); padding: 6px 10px 2px; }
  .grp { font-size: 11.5px; font-weight: 600; text-transform: uppercase; letter-spacing: .06em; color: var(--text-3); padding: 16px 10px 6px; }
  .anav a {
    display: flex; align-items: center; gap: 10px; padding: 7px 10px; border-radius: var(--radius);
    color: var(--text-2); font-weight: 500; font-size: 13.5px;
  }
  .anav a :global(svg) { color: var(--text-3); }
  .anav a:hover { background: var(--surface-3); color: var(--text); text-decoration: none; }
  .anav a.on { background: var(--surface); color: var(--accent); box-shadow: var(--shadow), 0 0 0 1px var(--border); }
  .anav a.on :global(svg) { color: var(--accent); }
  .acontent { min-width: 0; }
  .denied { padding: 24px; }
  .denied p { margin: 0; }
  @media (max-width: 960px) {
    .admin { grid-template-columns: 1fr; gap: 0; }
    .anav { position: static; display: flex; overflow-x: auto; gap: 4px; padding-bottom: 10px; margin-bottom: 8px; border-bottom: 1px solid var(--border); scrollbar-width: none; }
    .atitle, .grp { display: none; }
    .anav a { white-space: nowrap; flex: none; }
  }
</style>
