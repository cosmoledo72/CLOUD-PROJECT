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
  import ListTodo from '@lucide/svelte/icons/list-todo';
  import LogOut from '@lucide/svelte/icons/log-out';
  import Menu from '@lucide/svelte/icons/menu';
  import MessagesSquare from '@lucide/svelte/icons/messages-square';
  import Settings from '@lucide/svelte/icons/settings';
  import UsersRound from '@lucide/svelte/icons/users-round';
  import Wallet from '@lucide/svelte/icons/wallet';
  import Wrench from '@lucide/svelte/icons/wrench';
  import ChartColumn from '@lucide/svelte/icons/chart-column';
  import TimerChip from '$lib/time/TimerChip.svelte';
  import { timer } from '$lib/time/timer.svelte';
  import Search from '@lucide/svelte/icons/search';
  import Plus from '@lucide/svelte/icons/plus';
  import SearchPalette from '$lib/search/SearchPalette.svelte';
  import QuickCreate from '$lib/search/QuickCreate.svelte';
  import { palette } from '$lib/search/search.svelte';
  import { openQuick } from '$lib/search/quick.svelte';
  import type { ModuleKey } from '$lib/api';
  import type { Component } from 'svelte';

  let { children } = $props();

  // Pages reachable without a session: sign-in and the one-time links from emails.
  const isPublic = (p: string) => p === '/login' || p.startsWith('/invite/') || p.startsWith('/reset/') || p.startsWith('/s/') || p === '/register' || p.startsWith('/register/');
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
    { href: '/projects', label: 'Проекты', icon: FolderKanban, match: ['/projects'], show: () => session.has('projects') },
    { href: '/tasks', label: 'Задачи', icon: ListTodo, match: ['/tasks'], show: () => session.has('projects') },
    { href: '/reports', label: 'Отчёты', icon: ChartColumn, match: ['/reports'], show: () => session.me?.role !== 'guest' && session.has('time') },
    { href: '/kb', label: 'База знаний', icon: BookOpen, match: ['/kb'], show: () => session.has('kb') },
    { href: '/finance', label: 'Финансы', icon: Wallet, match: ['/finance'], show: () => session.has('finance') && (session.finance || session.crm) },
    { href: '/tools', label: 'Инструменты', icon: Wrench, match: ['/tools'], show: () => session.me?.role !== 'guest' && session.has('tools') },
    {
      href: '/crm', label: 'CRM', icon: BriefcaseBusiness, match: ['/crm'], show: () => session.crm,
      count: () => crmDue.count, hot: () => crmDue.overdue > 0
    },
    { href: '/chats', label: 'Чаты', icon: MessagesSquare, match: ['/chats'], count: () => chats.unreadTotal, hot: () => true, show: () => session.has('chats') },
    { href: '/files', label: 'Хранилище', icon: HardDrive, match: ['/files', '/shared', '/trash'], show: () => session.has('storage') },
    { href: '/team', label: 'Команда', icon: UsersRound, match: ['/team'], show: () => session.me?.role !== 'guest' && session.has('team') }
  ];

  // Sections of switched-off modules (Администрирование → Модули) show a notice instead.
  const SECTION_MODULES: [string, ModuleKey, string][] = [
    ['/projects', 'projects', 'Проекты'], ['/tasks', 'projects', 'Проекты'], ['/kb', 'kb', 'База знаний'],
    ['/chats', 'chats', 'Чаты'], ['/crm', 'crm', 'CRM'], ['/finance', 'finance', 'Финансы'],
    ['/files', 'storage', 'Хранилище'], ['/shared', 'storage', 'Хранилище'], ['/trash', 'storage', 'Хранилище'],
    ['/team', 'team', 'Команда'], ['/tools', 'tools', 'Инструменты'],
    ['/reports', 'time', 'Учёт времени']
  ];
  const blocked = $derived.by(() => {
    if (!session.me) return null;
    const p = page.url.pathname;
    const hit = SECTION_MODULES.find(([prefix]) => p === prefix || p.startsWith(prefix + '/'));
    return hit && !session.has(hit[1]) ? hit[2] : null;
  });
  const home = $derived(session.home);
  // The running timer (time tracking module).
  const timeOn = $derived(!!session.me && session.me.role !== 'guest' && session.has('time'));
  $effect(() => { if (timeOn) timer.init(); });
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

  // Search (Ctrl+K, «/») and quick create (C) from anywhere but text fields.
  const staff = $derived(!!session.me && session.me.role !== 'guest');
  const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);
  let createBtn = $state<HTMLButtonElement>();
  function openSearch() {
    menuOpen = false;
    palette.q = '';
    palette.open = true;
  }
  function hotkeys(e: KeyboardEvent) {
    if (!session.me || bare) return;
    if ((e.ctrlKey || e.metaKey) && !e.altKey && (e.key === 'k' || e.key === 'K' || e.key === 'л' || e.key === 'Л' || e.code === 'KeyK')) {
      e.preventDefault();
      if (palette.open) palette.open = false;
      else openSearch();
      return;
    }
    if (e.ctrlKey || e.metaKey || e.altKey || e.repeat || palette.open) return;
    const el = e.target as HTMLElement | null;
    if (el && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName))) return;
    if (document.querySelector('dialog[open]')) return;
    if (e.key === '/' || e.code === 'Slash' && !e.shiftKey) {
      e.preventDefault();
      openSearch();
    } else if (staff && e.code === 'KeyC' && !e.shiftKey) {
      e.preventDefault();
      openQuick(createBtn?.offsetParent ? createBtn : null);
    }
  }
  const initials = (n: string) => n.split(/\s+/).filter(Boolean).map((w) => w[0]).slice(0, 2).join('').toUpperCase();
  const b = $derived(branding.current);
  const portalName = $derived(b?.display_name ?? session.me?.tenant_name ?? '');
</script>

<svelte:window onkeydown={hotkeys} />

{#if bare}
  {@render children()}
{:else if session.me}
  <div class="shell" class:collapsed>
    <aside class:open={menuOpen} data-sidebar={b?.sidebar_style ?? 'light'}>
      <div class="top">
        <a class="brand" href={home} title={portalName}>
          {#if b?.logo_url && !collapsed}
            <img class="logo" class:plate={b.logo_plate} src={b.logo_url} alt={portalName} />
          {:else}
            {#if b?.icon_url}
              <img class="icon" src={b.icon_url} alt="" width="28" height="28" />
            {:else}
              <svg viewBox="0 0 32 32" width="28" height="28" aria-hidden="true"><rect width="32" height="32" rx="7" fill="var(--accent)"/><path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z" transform="translate(4 4.5)" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" stroke="var(--on-accent)"/></svg>
            {/if}
            {#if !collapsed}<span class="brand-name">{portalName}</span>{/if}
          {/if}
        </a>
        {#if !collapsed}<div class="bell-desktop"><NotificationBell /></div>{/if}
      </div>
      <div class="find">
        <button class="sbox" onclick={openSearch} title="Поиск по порталу ({isMac ? '⌘' : 'Ctrl'}+K)" aria-label="Поиск по порталу">
          <Search size={16} strokeWidth={1.9} /><span class="label">Поиск</span><kbd class="label">{isMac ? '⌘' : 'Ctrl'} K</kbd>
        </button>
        {#if staff}
          <button class="plus" bind:this={createBtn} onclick={(e) => openQuick(e.currentTarget)} title="Создать (C)" aria-label="Создать" aria-haspopup="menu">
            <Plus size={18} strokeWidth={2} />
          </button>
        {/if}
      </div>
      <nav aria-label="Разделы">
        {#each nav.filter((i) => !i.show || i.show()) as item (item.href)}
          {@const Icon = item.icon}
          {@const n = item.count?.() ?? 0}
          <a href={item.href} class:active={isActive(item)} onclick={() => (menuOpen = false)} title={collapsed ? item.label : undefined}
             aria-current={isActive(item) ? 'page' : undefined}>
            <span class="ico"><Icon size={19} /></span>
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
            <span class="ico"><Settings size={19} /></span>
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
        <button class="icon-btn burger" aria-label="Меню" onclick={() => (menuOpen = !menuOpen)}><Menu size={20} /></button>
        <span class="mb-name">{portalName}</span>
        <div class="spacer"></div>
        <button class="icon-btn" aria-label="Поиск" onclick={openSearch}><Search size={19} /></button>
        {#if staff}<button class="icon-btn" aria-label="Создать" aria-haspopup="menu" onclick={(e) => openQuick(e.currentTarget)}><Plus size={20} /></button>{/if}
        <NotificationBell />
      </div>
      {#if blocked}
        <div class="page"><div class="card off">
          <h2>Модуль «{blocked}» отключён</h2>
          <p class="muted">Администратор портала отключил этот раздел. Данные сохранены и вернутся, когда модуль включат снова.</p>
          <div class="row">
            <a class="btn" href={home}>На главную</a>
            {#if session.isAdmin}<a class="btn ghost" href="/admin/modules">Модули портала</a>{/if}
          </div>
        </div></div>
      {:else}
        {@render children()}
      {/if}
      {#if timeOn}<TimerChip />{/if}
    </main>
  </div>
  <SearchPalette />
  {#if staff}<QuickCreate />{/if}
{/if}

<style>
  .shell { display: grid; grid-template-columns: 252px 1fr; min-height: 100vh; transition: grid-template-columns .15s; }
  aside {
    background: var(--surface); color: var(--text); border-right: 1px solid var(--border);
    display: flex; flex-direction: column; padding: 14px 12px 12px; position: sticky; top: 0; height: 100vh; overflow-y: auto;
    scrollbar-width: thin;
  }
  .nav-count { margin-left: auto; min-width: 20px; height: 18px; padding: 0 6px; border-radius: 9px; font-size: 11px; font-weight: 600;
    line-height: 18px; text-align: center; background: var(--surface-3); color: var(--text-2); font-variant-numeric: tabular-nums; }
  .nav-count.hot { background: var(--danger); color: #fff; }
  .top { display: flex; align-items: flex-start; gap: 4px; padding-bottom: 12px; margin-bottom: 10px; border-bottom: 1px solid var(--border); }
  .top .brand { flex: 1; min-width: 0; }
  .bell-desktop { padding-top: 8px; color: var(--text-2); }
  .brand { display: flex; gap: 10px; align-items: center; padding: 4px 6px; color: var(--text); min-height: 44px; }
  .brand:hover { text-decoration: none; }
  /* Explicit height: an SVG with only a viewBox has no intrinsic size and would collapse. */
  .logo { height: 38px; width: auto; max-width: 100%; object-fit: contain; object-position: left center; }
  .logo.plate { background: #fff; padding: 5px 8px; border-radius: var(--radius); height: 44px; }
  .icon { border-radius: 7px; object-fit: contain; flex: none; }
  .brand-name { font-weight: 650; font-size: 14px; letter-spacing: .01em; line-height: 1.25; overflow-wrap: anywhere; }
  nav { display: grid; gap: 1px; }
  .find { display: flex; gap: 6px; margin-bottom: 10px; }
  .sbox { flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; height: 36px; padding: 0 10px; border: 1px solid var(--border); border-radius: var(--radius);
          background: var(--surface-2, var(--bg)); color: var(--text-3); font: inherit; font-size: 13.5px; cursor: text; text-align: left; transition: border-color .12s, color .12s; }
  .sbox:hover { border-color: var(--border-strong, var(--text-3)); color: var(--text-2); }
  .sbox kbd { flex: none; margin-left: auto; font: 11px/1 ui-monospace, monospace; padding: 3px 5px; border: 1px solid var(--border); border-radius: 5px; color: var(--text-3); background: var(--surface); }
  .sbox .label:not(kbd) { flex: 1; }
  .plus { flex: none; width: 36px; height: 36px; display: grid; place-items: center; border: 0; border-radius: var(--radius); background: var(--accent); color: var(--on-accent, #fff);
          cursor: pointer; transition: filter .12s; }
  .plus:hover { filter: brightness(1.08); }
  .collapsed .find { flex-direction: column; align-items: center; }
  .collapsed .sbox { flex: none; width: 36px; padding: 0; justify-content: center; }
  nav a, .collapse {
    position: relative; display: flex; align-items: center; gap: 11px; padding: 8px 10px; border-radius: var(--radius);
    color: var(--text-2); font: inherit; font-size: 14px; font-weight: 500; border: 0; background: none; cursor: pointer; text-align: left; width: 100%;
    transition: background .12s, color .12s;
  }
  nav a:hover, .collapse:hover { background: var(--surface-3); text-decoration: none; color: var(--text); }
  nav a.active { background: var(--accent-soft); color: var(--accent); }
  nav a.active::before {
    content: ''; position: absolute; left: -12px; top: 7px; bottom: 7px; width: 3px; border-radius: 0 3px 3px 0; background: var(--accent);
  }
  .ico { display: grid; place-items: center; width: 20px; flex: none; color: var(--text-3); transition: color .12s; }
  nav a:hover .ico { color: var(--text-2); }
  nav a.active .ico { color: var(--accent); }
  .ico.flip { transform: rotate(180deg); }
  .label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bottom { margin-bottom: 8px; }
  .collapse { font-size: 13px; color: var(--text-3); }
  .off { padding: 28px; max-width: 560px; display: grid; gap: 10px; justify-items: start; }
  .off h2, .off p { margin: 0; }
  .bell-collapsed { display: grid; place-items: center; padding: 4px 0; color: var(--text-2); }
  .me { display: flex; align-items: center; gap: 10px; padding: 10px 4px 0; border-top: 1px solid var(--border); }
  .avatar {
    width: 32px; height: 32px; border-radius: 50%; flex: none; display: grid; place-items: center; font-size: 12px; font-weight: 600;
    background: var(--accent-soft); color: var(--accent);
  }
  .avatar:hover { text-decoration: none; }
  .me-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text); font-weight: 500; font-size: 13.5px; }
  .me :global(.btn) { color: var(--text-2); }
  .out { margin-left: auto; }
  /* Icons-only sidebar */
  .shell.collapsed { grid-template-columns: 68px 1fr; }
  .collapsed aside { padding-left: 10px; padding-right: 10px; }
  .collapsed .label, .collapsed .nav-count:not(.dot) { display: none; }
  .collapsed nav a, .collapsed .collapse { justify-content: center; padding: 9px 0; }
  .collapsed nav a.active::before { left: -10px; }
  .collapsed nav a .nav-count { display: block; position: absolute; top: 3px; right: 4px; min-width: 16px; height: 16px; line-height: 16px; padding: 0 4px; font-size: 10px; }
  .collapsed .brand { justify-content: center; padding: 4px 0; }
  .collapsed .me { flex-direction: column; padding: 10px 0 0; }
  .collapsed .out { margin: 0; }
  main { min-width: 0; }
  .mb-name { font-weight: 600; margin-left: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mobile-bar { display: none; align-items: center; padding: 8px 10px; border-bottom: 1px solid var(--border); background: var(--surface); position: sticky; top: 0; z-index: 10; }
  @media (max-width: 860px) {
    .shell, .shell.collapsed { grid-template-columns: 1fr; }
    .collapse { display: none; }
    aside { position: fixed; z-index: 20; width: 260px; transform: translateX(-100%); transition: transform .2s; box-shadow: var(--shadow-lg); }
    aside.open { transform: none; }
    .mobile-bar { display: flex; }
    .bell-desktop { display: none; }
  }
</style>
