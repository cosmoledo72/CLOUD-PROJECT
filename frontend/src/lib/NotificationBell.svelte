<script lang="ts">
  import { goto } from '$app/navigation';
  import { timeAgo, type NotificationItem } from './api';
  import { notifications } from './notifications.svelte';

  let open = $state(false);
  let onlyUnread = $state(false);
  let root = $state<HTMLElement>();
  let now = $state(Date.now());

  // Relative times stay fresh while the panel is open.
  $effect(() => {
    if (!open) return;
    now = Date.now();
    const t = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(t);
  });

  function toggle() {
    open = !open;
    if (open && !notifications.loaded) notifications.load().catch(() => {});
  }

  // Close on outside click and Escape.
  $effect(() => {
    if (!open) return;
    const click = (e: MouseEvent) => {
      if (root && !root.contains(e.target as Node)) open = false;
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') open = false;
    };
    document.addEventListener('mousedown', click);
    document.addEventListener('keydown', key);
    return () => {
      document.removeEventListener('mousedown', click);
      document.removeEventListener('keydown', key);
    };
  });

  async function openItem(n: NotificationItem) {
    if (!n.read_at) notifications.markRead([n.id]).catch(() => {});
    if (n.link) {
      open = false;
      goto(n.link);
    }
  }

  const shown = $derived(onlyUnread ? notifications.items.filter((n) => !n.read_at) : notifications.items);

  // One icon per family of kinds.
  const icon: Record<string, string> = {
    task: 'M9 11l2 2 4-4M5 4h14v16H5z',
    due: 'M12 7v5l3 2M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z',
    comment: 'M4 5h16v11H9l-5 4z',
    mention: 'M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0zm0 0v1.5a2.5 2.5 0 0 0 5 0V12a9 9 0 1 0-4 7.5',
    share: 'M4 7h6l2 2h8v10H4zM12 13h6M15 10v6',
    crm: 'M3 7h18v13H3zM8 7V4h8v3M3 12h18',
    project: 'M4 6h10M4 12h16M4 18h7'
  };
  function iconOf(kind: string): string {
    if (kind === 'task_due_soon' || kind === 'task_overdue') return icon.due;
    if (kind === 'task_comment') return icon.comment;
    if (kind === 'mention') return icon.mention;
    if (kind === 'file_shared') return icon.share;
    if (kind.startsWith('deal') || kind.startsWith('company')) return icon.crm;
    if (kind === 'project_added') return icon.project;
    return icon.task;
  }
</script>

<div class="bell" bind:this={root}>
  <button class="btn ghost trigger" class:on={open} onclick={toggle} aria-haspopup="dialog" aria-expanded={open}
          aria-label={notifications.unread ? `Уведомления: ${notifications.unread} непрочитанных` : 'Уведомления'}>
    <svg viewBox="0 0 24 24" width="19" height="19" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M6 9a6 6 0 1 1 12 0c0 5 2 6.5 2 6.5H4S6 14 6 9zM10 19a2 2 0 0 0 4 0" />
    </svg>
    {#if notifications.unread > 0}<span class="count">{notifications.unread > 99 ? '99+' : notifications.unread}</span>{/if}
  </button>

  {#if open}
    <div class="panel card" role="dialog" aria-label="Уведомления">
      <div class="head">
        <b>Уведомления</b>
        <div class="spacer"></div>
        <label class="check small"><input type="checkbox" bind:checked={onlyUnread} /> непрочитанные</label>
        <button class="btn ghost sm" disabled={!notifications.unread} onclick={() => notifications.markAll()}>Прочитать все</button>
      </div>
      <div class="list">
        {#each shown as n (n.id)}
          <button class="item" class:unread={!n.read_at} onclick={() => openItem(n)}>
            <svg class="ic k-{n.kind}" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={iconOf(n.kind)} /></svg>
            <span class="text">
              <span class="title">{n.title}</span>
              {#if n.body}<span class="body">{n.body}</span>{/if}
              <span class="when">{timeAgo(n.created_at, now)}</span>
            </span>
            {#if !n.read_at}<span class="dot" aria-label="не прочитано"></span>{/if}
          </button>
        {:else}
          <p class="empty faint">{onlyUnread ? 'Всё прочитано.' : 'Уведомлений пока нет.'}</p>
        {/each}
        {#if notifications.hasMore && !onlyUnread}
          <button class="btn ghost sm more" onclick={() => notifications.more()}>Показать ещё</button>
        {/if}
      </div>
      <a class="foot small" href="/settings#notifications" onclick={() => (open = false)}>Настроить уведомления</a>
    </div>
  {/if}
</div>

<style>
  .bell { position: relative; }
  .trigger { position: relative; padding: 4px 6px; color: inherit; }
  .trigger.on { background: var(--surface-2); }
  .count {
    position: absolute; top: -2px; right: -4px; min-width: 17px; height: 17px; padding: 0 4px; border-radius: 9px;
    background: var(--danger); color: #fff; font-size: 11px; font-weight: 600; line-height: 17px; text-align: center;
  }
  .panel {
    position: fixed; z-index: 50; top: 10px; left: 248px; width: 400px; max-height: min(640px, calc(100vh - 20px));
    display: flex; flex-direction: column; box-shadow: var(--shadow-lg); color: var(--text); padding: 0; overflow: hidden;
  }
  .head { display: flex; align-items: center; gap: 8px; padding: 10px 12px; border-bottom: 1px solid var(--border); }
  .check { display: flex; gap: 4px; align-items: center; color: var(--text-2); }
  .list { overflow-y: auto; flex: 1; }
  .item {
    display: flex; gap: 10px; width: 100%; text-align: left; border: 0; background: none; cursor: pointer;
    padding: 10px 12px; border-bottom: 1px solid var(--border); color: inherit; font: inherit; align-items: flex-start;
  }
  .item:hover { background: var(--surface-2); }
  .item.unread { background: color-mix(in srgb, var(--accent) 6%, transparent); }
  .item.unread:hover { background: color-mix(in srgb, var(--accent) 11%, transparent); }
  .ic { flex: none; margin-top: 2px; color: var(--text-3); }
  .ic.k-task_overdue { color: var(--danger); }
  .ic.k-task_due_soon { color: var(--warn); }
  .ic.k-mention { color: var(--accent); }
  .text { display: grid; gap: 2px; min-width: 0; flex: 1; }
  .title { font-weight: 500; overflow-wrap: anywhere; }
  .unread .title { font-weight: 600; }
  .body { color: var(--text-2); font-size: 13px; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .when { color: var(--text-3); font-size: 12px; }
  .dot { flex: none; width: 8px; height: 8px; border-radius: 50%; background: var(--accent); margin-top: 7px; }
  .empty { padding: 28px 12px; text-align: center; margin: 0; }
  .more { margin: 8px auto; display: block; }
  .foot { padding: 9px 12px; border-top: 1px solid var(--border); text-align: center; }
  @media (max-width: 860px) {
    .panel { left: 8px; right: 8px; width: auto; top: 52px; max-height: calc(100vh - 64px); }
  }
</style>
