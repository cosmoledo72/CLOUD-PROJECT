<script lang="ts">
  import '../app.css';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { branding } from '$lib/branding.svelte';
  import NotificationBell from '$lib/NotificationBell.svelte';
  import { notifications } from '$lib/notifications.svelte';
  import { crmDue } from '$lib/crm/due.svelte';
  import { session } from '$lib/session.svelte';
  import { chats } from '$lib/chat.svelte';
  import BookOpen from '@lucide/svelte/icons/book-open';
  import BriefcaseBusiness from '@lucide/svelte/icons/briefcase-business';
  import ChevronsLeft from '@lucide/svelte/icons/chevrons-left';
  import FolderKanban from '@lucide/svelte/icons/folder-kanban';
  import HardDrive from '@lucide/svelte/icons/hard-drive';
  import LogOut from '@lucide/svelte/icons/log-out';
  import MessagesSquare from '@lucide/svelte/icons/messages-square';
  import Settings from '@lucide/svelte/icons/settings';
  import UsersRound from '@lucide/svelte/icons/users-round';
  import Wallet from '@lucide/svelte/icons/wallet';
  import type { Component } from 'svelte';

  let { children } = $props();

  // Pages reachable without a session: sign-in and the one-time links from emails.
  const isPublic = (p: string) => p === '/login' || p.startsWith('/invite/') || p.startsWith('/reset/');
  // Full-screen routes render without the application shell.
  const bare = $derived(page.url.pathname.startsWith('/editor/') || isPublic(page.url.pathname));

  $effect(() => {
    if (!session.loaded) {
      session.load().then((me) => {
        if (me) branding.loadOwn();
        else if (!isPublic(page.url.pathname)) goto('/login');
      });
    }
  });

  type NavItem = { href: string; label: string; icon: Component; match: string[]; count?: () => number; hot?: () => boolean; show?: () => boolean };
  const nav: NavItem[] = [
    { href: '/projects', label: 'Проекты', icon: FolderKanban, match: ['/projects'] },
    { href: '/kb', label: 'База знаний', icon: BookOpen, match: ['/kb'] },
    { href: '/finance', label: 'Финансы', icon: Wallet, match: ['/finance'], show: () => session.crm },
    {
      href: '/crm', label: 'CRM', icon: BriefcaseBusiness, match: ['/crm'], show: () => session.crm,
      count: () => crmDue.count, hot: () => crmDue.overdue > 0
    },
    { href: '/chats', label: 'Чаты', icon: MessagesSquare, match: ['/chats'], count: () => chats.unreadTotal, hot: () => true },
    { href: '/files', label: 'Хранилище', icon: HardDrive, match: ['/files', '/shared', '/trash'] },
    { href: '/team', label: 'Команда', icon: UsersRound, match: ['/team'], show: () => session.me?.role !== 'guest' }
  ];
  const isActive = (item: NavItem) => item.match.some((m) => page.url.pathname.startsWith(m));

  // Icons-only sidebar, remembered per browser.
  let collapsed = $state(false);
  $effect(() => {
    try {
      collapsed = localStorage.getItem('wh_nav_collapsed') === '1';
    } catch {
      /* storage unavailable */
    }
  });
  function toggleCollapsed() {
    collapsed = !collapsed;
    try {
      localStorage.setItem('wh_nav_collapsed', collapsed ? '1' : '0');
    } catch {
      /* storage unavailable */
    }
  }

  // Live notifications while signed in.
  $effect(() => {
    if (session.me) {
      notifications.start();
      chats.refreshUnread();
    } else notifications.stop();
  });
  // Overdue and today's CRM to-dos next to the CRM menu item.
  $effect(() => {
    if (!session.crm) {
      crmDue.reset();
      return;
    }
    crmDue.refresh();
    const t = setInterval(() => crmDue.refresh(), 5 * 60_000);
    return () => clearInterval(t);
  });

  async function logout() {
    notifications.stop();
    await session.logout();
    goto('/login');
  }

  let menuOpen = $state(false);
  const initials = (n: string) => n.split(/\s+/).filter(Boolean).map((w) => w[0]).slice(0, 2).join('').toUpperCase();
  const b = $derived(branding.current);
  const portalName = $derived(b?.display_name ?? session.me?.tenant_name ?? '');
</script>

{#if bare}
  {@render children()}
{:else if session.me}
  <div class="shell" class:collapsed>
    <aside class:open={menuOpen} data-sidebar={b?.sidebar_style ?? 'light'}>
      <div class="top">
        <a class="brand" href="/projects" title={portalName}>
          {#if b?.logo_url && !collapsed}
            <img class="logo" class:plate={b.logo_plate} src={b.logo_url} alt={portalName} />
          {:else}
            {#if b?.icon_url}
              <img class="icon" src={b.icon_url} alt="" width="28" height="28" />
            {:else}
              <svg viewBox="0 0 32 32" width="28" height="28" aria-hidden="true"><rect width="32" height="32" rx="7" fill="var(--accent)"/><path d="M8 10h4l2 9 2-9h2l2 9 2-9h4l-4 13h-3l-2-8-2 8h-3z" fill="var(--on-accent)"/></svg>
            {/if}
            {#if !collapsed}<span class="brand-name">{portalName}</span>{/if}
          {/if}
        </a>
        {#if !collapsed}<div class="bell-desktop"><NotificationBell /></div>{/if}
      </div>
      <nav aria-label="Разделы">
        {#each nav.filter((i) => !i.show || i.show()) as item (item.href)}
          {@const Icon = item.icon}
          {@const n = item.count?.() ?? 0}
          <a href={item.href} class:active={isActive(item)} onclick={() => (menuOpen = false)} title={collapsed ? item.label : undefined}
             aria-current={isActive(item) ? 'page' : undefined}>
            <span class="ico"><Icon size={20} strokeWidth={1.75} /></span>
            <span class="label">{item.label}</span>
            {#if n}<span class="nav-count" class:hot={item.hot?.()} aria-label="{n} новых">{n > 99 ? '99+' : n}</span>{/if}
          </a>
        {/each}
      </nav>
      <div class="spacer"></div>
      <nav class="bottom" aria-label="Служебное">
        {#if collapsed}<div class="bell-collapsed"><NotificationBell /></div>{/if}
        {#if session.isAdmin}
          <a href="/admin" class:active={page.url.pathname.startsWith('/admin')} onclick={() => (menuOpen = false)} title={collapsed ? 'Администрирование' : undefined}>
            <span class="ico"><Settings size={20} strokeWidth={1.75} /></span>
            <span class="label">Администрирование</span>
          </a>
        {/if}
        <button class="collapse" onclick={toggleCollapsed} title={collapsed ? 'Развернуть меню' : 'Свернуть меню'} aria-label={collapsed ? 'Развернуть меню' : 'Свернуть меню'}>
          <span class="ico" class:flip={collapsed}><ChevronsLeft size={18} strokeWidth={1.75} /></span>
          <span class="label">Свернуть</span>
        </button>
      </nav>
      <div class="me">
        <a href="/settings" class="avatar" title="Профиль: {session.me.display_name}">{initials(session.me.display_name)}</a>
        {#if !collapsed}<a href="/settings" class="me-name">{session.me.display_name}</a>{/if}
        <button class="btn ghost sm out" onclick={logout} title="Выйти" aria-label="Выйти"><LogOut size={16} strokeWidth={1.75} /></button>
      </div>
    </aside>
    <main>
      <div class="mobile-bar">
        <button class="btn ghost burger" aria-label="Меню" onclick={() => (menuOpen = !menuOpen)}>☰</button>
        <div class="spacer"></div>
        <NotificationBell />
      </div>
      {@render children()}
    </main>
  </div>
{/if}

<style>
  .shell { display: grid; grid-template-columns: 248px 1fr; min-height: 100vh; transition: grid-template-columns .15s; }
  aside {
    background: var(--surface); color: var(--text); border-right: 1px solid var(--border);
    display: flex; flex-direction: column; padding: 14px 10px; position: sticky; top: 0; height: 100vh;
  }
  .nav-count { margin-left: auto; min-width: 20px; height: 18px; padding: 0 6px; border-radius: 9px; font-size: 11px; font-weight: 600;
    line-height: 18px; text-align: center; background: var(--warn); color: #fff; }
  .nav-count.hot { background: var(--danger); }
  .top { display: flex; align-items: flex-start; gap: 4px; }
  .top .brand { flex: 1; min-width: 0; }
  .bell-desktop { padding-top: 10px; color: var(--text-2); }
  .brand { display: flex; gap: 10px; align-items: center; padding: 4px 8px 18px; color: var(--text); min-height: 52px; }
  .brand:hover { text-decoration: none; }
  /* Explicit height: an SVG with only a viewBox has no intrinsic size and would collapse. */
  .logo { height: 40px; width: auto; max-width: 100%; object-fit: contain; object-position: left center; }
  .logo.plate { background: #fff; padding: 5px 8px; border-radius: var(--radius); height: 46px; }
  .icon { border-radius: 6px; object-fit: contain; flex: none; }
  .brand-name { font-weight: 600; line-height: 1.25; overflow-wrap: anywhere; }
  nav { display: grid; gap: 2px; }
  nav a, .collapse {
    display: flex; align-items: center; gap: 12px; padding: 8px 10px; border-radius: 10px;
    color: var(--text-2); font: inherit; font-size: 14.5px; border: 0; background: none; cursor: pointer; text-align: left; width: 100%;
    transition: background .12s, color .12s;
  }
  nav a:hover, .collapse:hover { background: var(--surface-2); text-decoration: none; color: var(--text); }
  nav a.active { background: var(--accent-soft); color: var(--accent); font-weight: 500; }
  .ico { display: grid; place-items: center; width: 22px; flex: none; color: var(--text-3); transition: color .12s; }
  nav a:hover .ico { color: var(--text-2); }
  nav a.active .ico { color: var(--accent); }
  .ico.flip { transform: rotate(180deg); }
  .label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bottom { margin-bottom: 8px; }
  .collapse { font-size: 13px; color: var(--text-3); }
  .bell-collapsed { display: grid; place-items: center; padding: 4px 0; color: var(--text-2); }
  .me { display: flex; align-items: center; gap: 8px; padding: 10px 4px 0; border-top: 1px solid var(--border); }
  .avatar {
    width: 30px; height: 30px; border-radius: 50%; flex: none; display: grid; place-items: center; font-size: 12px; font-weight: 600;
    background: var(--accent-soft); color: var(--accent);
  }
  .avatar:hover { text-decoration: none; }
  .me-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text); }
  .me :global(.btn) { color: var(--text-2); }
  .out { margin-left: auto; }
  /* Icons-only sidebar */
  .shell.collapsed { grid-template-columns: 68px 1fr; }
  .collapsed aside { padding-left: 8px; padding-right: 8px; }
  .collapsed .label, .collapsed .nav-count:not(.dot) { display: none; }
  .collapsed nav a, .collapsed .collapse { justify-content: center; padding: 9px 0; position: relative; }
  .collapsed nav a .nav-count { display: block; position: absolute; top: 3px; right: 6px; min-width: 16px; height: 16px; line-height: 16px; padding: 0 4px; font-size: 10px; }
  .collapsed .brand { justify-content: center; padding: 4px 0 18px; }
  .collapsed .me { flex-direction: column; padding: 10px 0 0; }
  .collapsed .out { margin: 0; }
  main { min-width: 0; }
  .mobile-bar { display: none; align-items: center; padding: 6px 8px 0; }
  @media (max-width: 860px) {
    .shell, .shell.collapsed { grid-template-columns: 1fr; }
    .collapse { display: none; }
    aside { position: fixed; z-index: 20; width: 250px; transform: translateX(-100%); transition: transform .2s; box-shadow: var(--shadow-lg); }
    aside.open { transform: none; }
    .mobile-bar { display: flex; }
    .bell-desktop { display: none; }
  }
</style>
