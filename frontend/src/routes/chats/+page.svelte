<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { onDestroy, tick } from 'svelte';
  import { api, type ChatChannel, type UserItem } from '$lib/api';
  import Avatar from '$lib/Avatar.svelte';
  import { chats, type ChatEvent, type ChatMessage } from '$lib/chat.svelte';
  import { session } from '$lib/session.svelte';
  import Search from '@lucide/svelte/icons/search';
  import Hash from '@lucide/svelte/icons/hash';
  import UsersRound from '@lucide/svelte/icons/users-round';
  import SquarePen from '@lucide/svelte/icons/square-pen';
  import SendHorizontal from '@lucide/svelte/icons/send-horizontal';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import Settings2 from '@lucide/svelte/icons/settings-2';
  import LogOut from '@lucide/svelte/icons/log-out';
  import MessagesSquare from '@lucide/svelte/icons/messages-square';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import FolderKanban from '@lucide/svelte/icons/folder-kanban';

  type Member = { id: string; display_name: string; position: string };

  let channels = $state<ChatChannel[]>([]);
  let loaded = $state(false);
  let error = $state('');
  let filter = $state('');
  let users = $state<UserItem[]>([]);

  const cid = $derived(page.url.searchParams.get('c'));
  let channel = $state<ChatChannel | null>(null);
  let members = $state<Member[]>([]);
  let canManage = $state(false);
  let messages = $state<ChatMessage[]>([]);
  let hasMore = $state(false);
  let loadingOlder = $state(false);
  let draft = $state('');
  let sending = $state(false);
  let editingId = $state<string | null>(null);
  let editText = $state('');
  let scroller = $state<HTMLDivElement | null>(null);
  let input = $state<HTMLTextAreaElement | null>(null);

  async function loadChannels() {
    channels = (await api.get<{ channels: ChatChannel[] }>('/chats')).channels;
    loaded = true;
  }
  $effect(() => { loadChannels().catch((e) => (error = e.message)); });
  $effect(() => {
    api.get<{ users: UserItem[] }>('/users').then((r) => (users = r.users)).catch(() => {});
  });

  const sections = $derived.by(() => {
    const f = filter.trim().toLowerCase();
    const list = channels.filter((c) => !f || c.name.toLowerCase().includes(f));
    return [
      { key: 'project', title: 'Проекты', items: list.filter((c) => c.kind === 'project') },
      { key: 'group', title: 'Группы', items: list.filter((c) => c.kind === 'group') },
      { key: 'direct', title: 'Личные', items: list.filter((c) => c.kind === 'direct') }
    ].filter((s) => s.items.length);
  });

  async function scrollToBottom() {
    await tick();
    if (scroller) scroller.scrollTop = scroller.scrollHeight;
  }
  const nearBottom = () => !scroller || scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 120;

  // Open a channel.
  $effect(() => {
    const id = cid;
    chats.openChannel = id;
    editingId = null;
    if (!id) { channel = null; messages = []; return; }
    Promise.all([
      api.get<{ channel: ChatChannel; members: Member[]; can_manage: boolean }>(`/chats/${id}`),
      api.get<{ messages: ChatMessage[]; has_more: boolean }>(`/chats/${id}/messages?limit=50`)
    ])
      .then(async ([info, list]) => {
        if (cid !== id) return;
        channel = info.channel;
        members = info.members;
        canManage = info.can_manage;
        messages = list.messages;
        hasMore = list.has_more;
        await scrollToBottom();
        input?.focus();
        markRead(id);
      })
      .catch((e) => { channel = null; error = e.status === 404 ? 'Чат не найден или у вас нет к нему доступа.' : e.message; });
  });
  onDestroy(() => (chats.openChannel = null));

  async function markRead(id: string) {
    const ch = channels.find((c) => c.id === id);
    if (ch) ch.unread = 0;
    try { await api.post(`/chats/${id}/read`); } catch { /* ignore */ }
  }

  async function loadOlder() {
    if (!channel || !hasMore || loadingOlder || !messages.length) return;
    loadingOlder = true;
    const prevH = scroller?.scrollHeight ?? 0;
    try {
      const r = await api.get<{ messages: ChatMessage[]; has_more: boolean }>(
        `/chats/${channel.id}/messages?limit=50&before=${encodeURIComponent(messages[0].created_at)}`);
      messages = [...r.messages, ...messages];
      hasMore = r.has_more;
      await tick();
      if (scroller) scroller.scrollTop += scroller.scrollHeight - prevH;
    } finally {
      loadingOlder = false;
    }
  }

  // Live events.
  const unsubscribe = chats.subscribe(async (ev: ChatEvent) => {
    const ch = channels.find((c) => c.id === ev.channel_id);
    if (ev.type === 'chat_message') {
      if (!ch) { loadChannels().catch(() => {}); }
      else {
        ch.last_body = ev.message.body;
        ch.last_author = ev.message.author_name;
        ch.last_message_at = ev.message.created_at;
        if (ev.channel_id !== cid && ev.message.author_id !== session.me?.id) ch.unread += 1;
        channels = [ch, ...channels.filter((c) => c.id !== ch.id)];
      }
      if (ev.channel_id === cid && !messages.some((m) => m.id === ev.message.id)) {
        const stick = nearBottom();
        messages = [...messages, ev.message];
        if (stick || ev.message.author_id === session.me?.id) await scrollToBottom();
        if (ev.message.author_id !== session.me?.id && document.visibilityState === 'visible') markRead(ev.channel_id);
      }
    } else if (ev.type === 'chat_edit' && ev.channel_id === cid) {
      messages = messages.map((m) => (m.id === ev.message.id ? ev.message : m));
    } else if (ev.type === 'chat_delete' && ev.channel_id === cid) {
      messages = messages.filter((m) => m.id !== ev.id);
    } else if (ev.type === 'chat_read' && ch) {
      ch.unread = 0;
    }
  });
  onDestroy(() => { unsubscribe(); });

  async function send() {
    const body = draft.trim();
    if (!body || !channel || sending) return;
    sending = true;
    try {
      const r = await api.post<{ message: ChatMessage }>(`/chats/${channel.id}/messages`, { body });
      draft = '';
      if (!messages.some((m) => m.id === r.message.id)) messages = [...messages, r.message];
      await scrollToBottom();
      autosize();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      sending = false;
      input?.focus();
    }
  }
  function onComposerKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      send();
    } else if (e.key === 'ArrowUp' && !draft) {
      const mine = [...messages].reverse().find((m) => m.author_id === session.me?.id);
      if (mine) { e.preventDefault(); startEdit(mine); }
    }
  }
  function autosize() {
    if (!input) return;
    input.style.height = 'auto';
    input.style.height = Math.min(input.scrollHeight, 180) + 'px';
  }

  function startEdit(m: ChatMessage) {
    editingId = m.id;
    editText = m.body;
  }
  async function saveEdit() {
    if (!editingId) return;
    try {
      const r = await api.patch<{ message: ChatMessage }>(`/chat-messages/${editingId}`, { body: editText });
      messages = messages.map((m) => (m.id === r.message.id ? r.message : m));
      editingId = null;
    } catch (e) {
      error = (e as Error).message;
    }
  }
  async function del(m: ChatMessage) {
    if (!confirm('Удалить сообщение?')) return;
    try {
      await api.del(`/chat-messages/${m.id}`);
      messages = messages.filter((x) => x.id !== m.id);
    } catch (e) {
      error = (e as Error).message;
    }
  }

  // ------------------------------------------------------------------ formatting
  const pad = (n: number) => String(n).padStart(2, '0');
  const time = (iso: string) => { const d = new Date(iso); return `${pad(d.getHours())}:${pad(d.getMinutes())}`; };
  const dayKey = (iso: string) => new Date(iso).toDateString();
  function dayLabel(iso: string) {
    const d = new Date(iso);
    const today = new Date();
    const y = new Date(); y.setDate(y.getDate() - 1);
    if (d.toDateString() === today.toDateString()) return 'Сегодня';
    if (d.toDateString() === y.toDateString()) return 'Вчера';
    return d.toLocaleDateString('ru-RU', { day: 'numeric', month: 'long', year: d.getFullYear() === today.getFullYear() ? undefined : 'numeric' });
  }
  function listTime(iso: string | null) {
    if (!iso) return '';
    const d = new Date(iso);
    return d.toDateString() === new Date().toDateString() ? time(iso) : d.toLocaleDateString('ru-RU', { day: 'numeric', month: 'short' });
  }
  const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]!);
  const linkify = (s: string) =>
    esc(s).replace(/\bhttps?:\/\/[^\s<]+[^\s<.,;:!?)»"]/g, (u) => `<a href="${u}" target="_blank" rel="noopener noreferrer">${u}</a>`);
  // Consecutive messages of one author within 5 minutes form a group.
  const grouped = (i: number) => {
    if (i === 0) return false;
    const a = messages[i - 1];
    const b = messages[i];
    return a.author_id === b.author_id && dayKey(a.created_at) === dayKey(b.created_at) &&
      new Date(b.created_at).getTime() - new Date(a.created_at).getTime() < 5 * 60_000;
  };

  // ------------------------------------------------------------------ new chats
  let dialog = $state<HTMLDialogElement | null>(null);
  let mode = $state<'direct' | 'group' | 'edit'>('direct');
  let gName = $state('');
  let gMembers = $state<string[]>([]);
  let who = $state('');
  const colleagues = $derived(
    users.filter((u) => u.is_active && u.role !== 'guest' && u.id !== session.me?.id &&
      (!who.trim() || u.display_name.toLowerCase().includes(who.trim().toLowerCase())))
  );
  function openDialog(m: typeof mode) {
    mode = m;
    who = '';
    if (m === 'edit' && channel) {
      gName = channel.name;
      gMembers = members.map((x) => x.id).filter((id) => id !== session.me?.id);
    } else {
      gName = '';
      gMembers = [];
    }
    dialog?.showModal();
  }
  async function openDirect(userId: string) {
    try {
      const r = await api.post<{ id: string }>('/chats/direct', { user_id: userId });
      dialog?.close();
      await loadChannels();
      goto(`/chats?c=${r.id}`);
    } catch (e) {
      error = (e as Error).message;
    }
  }
  async function saveGroup() {
    try {
      if (mode === 'edit' && channel) {
        await api.put(`/chats/${channel.id}`, { name: gName, member_ids: gMembers });
        dialog?.close();
        await loadChannels();
        const info = await api.get<{ channel: ChatChannel; members: Member[] }>(`/chats/${channel.id}`);
        channel = info.channel;
        members = info.members;
      } else {
        const r = await api.post<{ id: string }>('/chats', { name: gName, member_ids: gMembers });
        dialog?.close();
        await loadChannels();
        goto(`/chats?c=${r.id}`);
      }
    } catch (e) {
      error = (e as Error).message;
    }
  }
  async function leave() {
    if (!channel || !confirm(`Покинуть чат «${channel.name}»?`)) return;
    try {
      await api.post(`/chats/${channel.id}/leave`);
      await loadChannels();
      goto('/chats');
    } catch (e) {
      error = (e as Error).message;
    }
  }
  const toggle = (id: string) => (gMembers = gMembers.includes(id) ? gMembers.filter((x) => x !== id) : [...gMembers, id]);
</script>

<svelte:head><title>{channel ? `${channel.name} — ` : ''}Чаты</title></svelte:head>

<div class="page chatpage">

<div class="chat card" class:has-open={!!cid}>
  <aside class="list">
    <div class="lhead">
      <label class="search"><Search size={15} /><input class="input" placeholder="Найти чат" bind:value={filter} /></label>
      <button class="btn ghost icon" title="Новое сообщение" aria-label="Новое сообщение" onclick={() => openDialog('direct')}><SquarePen size={18} /></button>
    </div>
    <div class="items">
      {#each sections as s (s.key)}
        <div class="sec faint">{s.title}</div>
        {#each s.items as c (c.id)}
          <a class="ch" class:on={c.id === cid} href="/chats?c={c.id}">
            {#if c.kind === 'direct' && c.other_user_id}
              <Avatar id={c.other_user_id} name={c.name} size={36} />
            {:else}
              <span class="cic k-{c.kind}">{#if c.kind === 'project'}<FolderKanban size={17} />{:else}<UsersRound size={17} />{/if}</span>
            {/if}
            <span class="cbody">
              <span class="crow"><span class="cn">{c.name}</span><span class="ct faint">{listTime(c.last_message_at)}</span></span>
              <span class="crow">
                <span class="cl faint">{#if c.last_body}{#if c.kind !== 'direct' && c.last_author}{c.last_author + ': '}{/if}{c.last_body}{:else}Нет сообщений{/if}</span>
                {#if c.unread && c.id !== cid}<span class="un">{c.unread > 99 ? '99+' : c.unread}</span>{/if}
              </span>
            </span>
          </a>
        {/each}
      {/each}
      {#if loaded && !channels.length}
        <p class="faint small none">Чатов пока нет. Чат проекта появляется автоматически у его участников; личную переписку можно начать кнопкой вверху.</p>
      {/if}
    </div>
    <button class="btn newgroup" onclick={() => openDialog('group')}><UsersRound size={15} /> Создать групповой чат</button>
  </aside>

  <section class="pane">
    {#if channel}
      <header class="phead">
        <a class="btn ghost icon back" href="/chats" aria-label="К списку чатов"><ArrowLeft size={18} /></a>
        {#if channel.kind === 'direct' && channel.other_user_id}
          <Avatar id={channel.other_user_id} name={channel.name} size={34} />
        {:else}
          <span class="cic k-{channel.kind}">{#if channel.kind === 'project'}<Hash size={17} />{:else}<UsersRound size={17} />{/if}</span>
        {/if}
        <div class="pt">
          <div class="pn">{channel.name}</div>
          <div class="faint small">
            {#if channel.kind === 'direct'}{members.find((m) => m.id === channel?.other_user_id)?.position || 'личная переписка'}
            {:else}{members.length} {members.length % 10 === 1 && members.length % 100 !== 11 ? 'участник' : members.length % 10 >= 2 && members.length % 10 <= 4 && (members.length % 100 < 10 || members.length % 100 >= 20) ? 'участника' : 'участников'}: {members.slice(0, 4).map((m) => m.display_name).join(', ')}{members.length > 4 ? '…' : ''}{/if}
          </div>
        </div>
        <div class="spacer"></div>
        {#if channel.kind === 'project'}<a class="btn sm" href="/projects/{channel.project_id}"><FolderKanban size={14} /> Проект</a>{/if}
        {#if canManage}<button class="btn ghost icon" title="Настроить чат" aria-label="Настроить чат" onclick={() => openDialog('edit')}><Settings2 size={17} /></button>{/if}
        {#if channel.kind === 'group'}<button class="btn ghost icon" title="Покинуть чат" aria-label="Покинуть чат" onclick={leave}><LogOut size={17} /></button>{/if}
      </header>

      <div class="msgs" bind:this={scroller} onscroll={() => { if (scroller && scroller.scrollTop < 80) loadOlder(); }}>
        {#if hasMore}<div class="older faint small">{loadingOlder ? 'Загрузка…' : ''}</div>{/if}
        {#if !messages.length}
          <div class="empty">
            <MessagesSquare size={38} />
            <p>Здесь пока тихо. Напишите первое сообщение.</p>
          </div>
        {/if}
        {#each messages as m, i (m.id)}
          {#if i === 0 || dayKey(messages[i - 1].created_at) !== dayKey(m.created_at)}
            <div class="day"><span>{dayLabel(m.created_at)}</span></div>
          {/if}
          {@const mine = m.author_id === session.me?.id}
          {@const cont = grouped(i)}
          <div class="msg" class:cont class:mine>
            <div class="ava">{#if !cont}<Avatar id={m.author_id ?? m.id} name={m.author_name ?? '?'} size={34} />{/if}</div>
            <div class="mb">
              {#if !cont}<div class="mh"><b>{m.author_name ?? 'Удалённый пользователь'}</b> <span class="faint small">{time(m.created_at)}</span></div>{/if}
              {#if editingId === m.id}
                <textarea class="input edit" bind:value={editText} rows="2"
                  onkeydown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); saveEdit(); } else if (e.key === 'Escape') editingId = null; }}></textarea>
                <div class="faint small">Enter — сохранить · Esc — отмена</div>
              {:else}
                <div class="text">{@html linkify(m.body)}{#if m.edited_at}<span class="faint small edited" title="Изменено">(изм.)</span>{/if}</div>
              {/if}
            </div>
            {#if cont}<span class="hover-time faint">{time(m.created_at)}</span>{/if}
            {#if editingId !== m.id && (mine || session.isAdmin)}
              <div class="tools">
                {#if mine}<button title="Изменить" aria-label="Изменить" onclick={() => startEdit(m)}><Pencil size={14} /></button>{/if}
                <button title="Удалить" aria-label="Удалить" onclick={() => del(m)}><Trash2 size={14} /></button>
              </div>
            {/if}
          </div>
        {/each}
      </div>

      <form class="composer" onsubmit={(e) => { e.preventDefault(); send(); }}>
        <textarea class="input" rows="1" bind:this={input} bind:value={draft} oninput={autosize} onkeydown={onComposerKey}
          placeholder="Сообщение в «{channel.name}»" maxlength="10000"></textarea>
        <button class="btn primary send" disabled={!draft.trim() || sending} aria-label="Отправить"><SendHorizontal size={18} /></button>
      </form>
      <div class="hint faint">Enter — отправить, Shift+Enter — новая строка, ↑ — изменить последнее</div>
    {:else}
      <div class="placeholder">
        <MessagesSquare size={52} />
        <h2>Чаты</h2>
        <p class="muted">Выберите чат слева или начните новую переписку.</p>
        {#if error}<div class="error-box">{error}</div>{/if}
      </div>
    {/if}
  </section>
</div>
</div>

<dialog bind:this={dialog} class="card dlg">
  <form method="dialog" class="dh">
    <h2>{mode === 'direct' ? 'Новое сообщение' : mode === 'group' ? 'Групповой чат' : 'Настройки чата'}</h2>
    <button class="btn ghost icon" aria-label="Закрыть">✕</button>
  </form>
  {#if mode !== 'direct'}
    <label class="field">Название <input class="input" bind:value={gName} maxlength="100" placeholder="например, Обследование Корпуса 12" /></label>
    <div class="faint small sel">Участники: {gMembers.length ? `выбрано ${gMembers.length}` : 'никого не выбрано'}</div>
  {/if}
  <label class="search"><Search size={15} /><input class="input" placeholder="Найти коллегу" bind:value={who} /></label>
  <div class="people">
    {#each colleagues as u (u.id)}
      {#if mode === 'direct'}
        <button class="person" onclick={() => openDirect(u.id)}>
          <Avatar id={u.id} name={u.display_name} size={30} /><span>{u.display_name}</span>
        </button>
      {:else}
        <label class="person">
          <input type="checkbox" checked={gMembers.includes(u.id)} onchange={() => toggle(u.id)} />
          <Avatar id={u.id} name={u.display_name} size={30} /><span>{u.display_name}</span>
        </label>
      {/if}
    {:else}
      <p class="faint small">Никого не нашлось.</p>
    {/each}
  </div>
  {#if mode !== 'direct'}
    <div class="dact">
      <button class="btn primary" disabled={!gName.trim()} onclick={saveGroup}>{mode === 'edit' ? 'Сохранить' : 'Создать'}</button>
      <button class="btn" onclick={() => dialog?.close()}>Отмена</button>
    </div>
  {/if}
</dialog>

<style>
  .chatpage { max-width: none; padding-bottom: 16px; }
  .chat { display: grid; grid-template-columns: 310px minmax(0, 1fr); height: calc(100vh - 40px); min-height: 460px; overflow: hidden; padding: 0; }
  .list { border-right: 1px solid var(--border); display: flex; flex-direction: column; min-height: 0; }
  .lhead { display: flex; gap: 6px; padding: 10px; border-bottom: 1px solid var(--border); }
  .search { position: relative; flex: 1; display: block; }
  .search > :global(svg) { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); color: var(--text-3); }
  .search input { padding-left: 32px; width: 100%; }
  .icon { width: 34px; padding: 0; display: grid; place-items: center; }
  .items { flex: 1; overflow-y: auto; padding: 4px 6px; }
  .sec { font-size: 11px; text-transform: uppercase; letter-spacing: .06em; padding: 10px 8px 4px; }
  .ch { display: flex; gap: 10px; align-items: center; padding: 7px 8px; border-radius: var(--radius); color: var(--text); text-decoration: none; }
  .ch:hover { background: var(--surface-2); }
  .ch.on { background: var(--accent-soft); }
  .cic { width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center; flex: none; }
  .phead .cic { width: 34px; height: 34px; }
  .k-project { background: var(--accent-soft); color: var(--accent); }
  .k-group { background: var(--ok-soft); color: var(--ok); }
  .cbody { flex: 1; min-width: 0; display: grid; gap: 1px; }
  .crow { display: flex; gap: 6px; align-items: center; min-width: 0; }
  .cn { font-weight: 500; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ct { font-size: 11px; flex: none; }
  .cl { font-size: 12.5px; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .un { background: var(--accent); color: var(--on-accent); border-radius: 999px; font-size: 11px; font-weight: 600; padding: 1px 7px; flex: none; }
  .none { padding: 12px 8px; }
  .newgroup { margin: 8px 10px 10px; display: inline-flex; gap: 6px; align-items: center; justify-content: center; }

  .pane { display: flex; flex-direction: column; min-height: 0; min-width: 0; }
  .phead { display: flex; gap: 10px; align-items: center; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .phead .btn.sm { display: inline-flex; gap: 6px; align-items: center; }
  .pt { min-width: 0; }
  .pt .small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pn { font-weight: 600; }
  .back { display: none; }
  .msgs { flex: 1; overflow-y: auto; padding: 10px 16px 14px; }
  .older { text-align: center; padding: 6px; min-height: 20px; }
  .day { display: flex; align-items: center; gap: 10px; margin: 14px 0 6px; color: var(--text-3); font-size: 12px; }
  .day::before, .day::after { content: ''; flex: 1; border-top: 1px solid var(--border); }
  .msg { position: relative; display: grid; grid-template-columns: 34px minmax(0, 1fr); gap: 10px; padding: 6px 8px; border-radius: var(--radius); margin-top: 4px; }
  .msg.cont { margin-top: 0; padding-top: 1px; }
  .msg:hover { background: var(--surface-2); }
  .mh { margin-bottom: 1px; }
  .text { white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.45; }
  .edited { margin-left: 6px; }
  .hover-time { position: absolute; left: 10px; top: 4px; font-size: 10.5px; opacity: 0; }
  .msg:hover .hover-time { opacity: 1; }
  .tools { position: absolute; right: 8px; top: -10px; display: none; background: var(--surface); border: 1px solid var(--border); border-radius: 8px; box-shadow: var(--shadow); }
  .msg:hover .tools { display: flex; }
  .tools button { border: 0; background: none; padding: 5px 7px; cursor: pointer; color: var(--text-2); display: grid; }
  .tools button:hover { color: var(--text); background: var(--surface-2); }
  .edit { width: 100%; resize: vertical; }
  .empty { display: grid; place-items: center; gap: 8px; padding: 60px 0; color: var(--text-3); text-align: center; }
  .composer { display: flex; gap: 8px; align-items: flex-end; padding: 10px 14px 4px; border-top: 1px solid var(--border); }
  .composer textarea { flex: 1; resize: none; min-height: 38px; max-height: 180px; line-height: 1.4; padding: 8px 10px; height: 38px; }
  .send { width: 42px; height: 38px; padding: 0; display: grid; place-items: center; }
  .hint { font-size: 11px; padding: 0 16px 8px; }
  .placeholder { margin: auto; display: grid; justify-items: center; gap: 6px; color: var(--text-3); text-align: center; padding: 20px; }
  .placeholder h2 { margin: 6px 0 0; color: var(--text); }
  .dlg { width: min(440px, calc(100vw - 32px)); padding: 16px; border: 1px solid var(--border); }
  .dlg::backdrop { background: rgb(0 0 0 / 35%); }
  .dh { display: flex; align-items: center; justify-content: space-between; margin-bottom: 10px; }
  .dh h2 { font-size: 17px; margin: 0; }
  .sel { margin: 6px 0 8px; }
  .people { max-height: 320px; overflow-y: auto; margin-top: 8px; display: grid; gap: 2px; }
  .person { display: flex; gap: 10px; align-items: center; padding: 6px 8px; border-radius: var(--radius); border: 0; background: none; font: inherit; color: var(--text); cursor: pointer; text-align: left; }
  .person:hover { background: var(--surface-2); }
  .dact { display: flex; gap: 8px; margin-top: 12px; }
  @media (max-width: 760px) {
    .chat { grid-template-columns: 1fr; height: calc(100vh - 100px); }
    .chat.has-open .list { display: none; }
    .chat:not(.has-open) .pane { display: none; }
    .back { display: grid; }
  }
</style>
