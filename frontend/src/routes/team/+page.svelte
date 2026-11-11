<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, roleLabel, type TeamGroup, type TeamMember } from '$lib/api';
  import Avatar from '$lib/Avatar.svelte';
  import { session } from '$lib/session.svelte';
  import Search from '@lucide/svelte/icons/search';
  import Mail from '@lucide/svelte/icons/mail';
  import Phone from '@lucide/svelte/icons/phone';
  import MessageCircle from '@lucide/svelte/icons/message-circle';
  import UsersRound from '@lucide/svelte/icons/users-round';
  import LayoutGrid from '@lucide/svelte/icons/layout-grid';
  import List from '@lucide/svelte/icons/list';
  import Pencil from '@lucide/svelte/icons/pencil';

  let members = $state<TeamMember[]>([]);
  let groups = $state<TeamGroup[]>([]);
  let loaded = $state(false);
  let error = $state('');
  let q = $state('');
  let group = $state('');
  let view = $state<'cards' | 'list'>(load('wh_team_view', 'cards') as 'cards' | 'list');

  function load(key: string, dflt: string) {
    try { return localStorage.getItem(key) ?? dflt; } catch { return dflt; }
  }
  function setView(v: 'cards' | 'list') {
    view = v;
    try { localStorage.setItem('wh_team_view', v); } catch { /* storage unavailable */ }
  }

  $effect(() => {
    api.get<{ members: TeamMember[]; groups: TeamGroup[] }>('/team')
      .then((r) => { members = r.members; groups = r.groups; loaded = true; })
      .catch((e) => (error = e.message));
  });

  const groupName = $derived(new Map(groups.map((g) => [g.id, g.name])));
  const shown = $derived.by(() => {
    const s = q.trim().toLowerCase();
    return members.filter((m) =>
      (!group || (group === '-' ? !m.group_ids.length : m.group_ids.includes(group))) &&
      (!s || [m.display_name, m.position, m.email, m.phone].some((x) => x.toLowerCase().includes(s))));
  });
  const active = $derived(members.filter((m) => m.is_active).length);

  async function write(m: TeamMember) {
    try {
      const r = await api.post<{ id: string }>('/chats/direct', { user_id: m.id });
      goto(`/chats?c=${r.id}`);
    } catch (e) {
      error = (e as Error).message;
    }
  }
  const tel = (p: string) => 'tel:' + p.replace(/[^\d+]/g, '');
</script>

<svelte:head><title>Команда</title></svelte:head>

<div class="page">

<div class="page-head">
  <h1>Команда</h1>
  <span class="faint">{active} {active % 10 === 1 && active % 100 !== 11 ? 'сотрудник' : active % 10 >= 2 && active % 10 <= 4 && (active % 100 < 10 || active % 100 >= 20) ? 'сотрудника' : 'сотрудников'}</span>
  <div class="spacer"></div>
  <a class="btn" href="/settings"><Pencil size={15} /> Моя карточка</a>
  {#if session.isAdmin}<a class="btn" href="/admin/users">Управление</a>{/if}
</div>
{#if error}<div class="error-box">{error}</div>{/if}

<div class="layout">
  <aside class="groups card">
    <button class:on={!group} onclick={() => (group = '')}><UsersRound size={16} /> Все <span>{active}</span></button>
    {#each groups as g (g.id)}
      <button class:on={group === g.id} onclick={() => (group = g.id)}><span class="dot"></span>{g.name} <span>{g.members}</span></button>
    {/each}
    {#if groups.length}
      <button class:on={group === '-'} onclick={() => (group = '-')}><span class="dot none"></span>Без группы</button>
    {/if}
  </aside>

  <div class="main">
    <div class="bar">
      <label class="search">
        <Search size={16} />
        <input class="input" placeholder="Имя, должность, почта, телефон" bind:value={q} />
      </label>
      <div class="seg" role="radiogroup" aria-label="Вид">
        <button role="radio" aria-checked={view === 'cards'} class:on={view === 'cards'} onclick={() => setView('cards')} title="Карточки"><LayoutGrid size={16} /></button>
        <button role="radio" aria-checked={view === 'list'} class:on={view === 'list'} onclick={() => setView('list')} title="Список"><List size={16} /></button>
      </div>
    </div>

    {#if loaded && !shown.length}
      <div class="card empty">Никого не нашлось.</div>
    {:else if view === 'cards'}
      <div class="grid">
        {#each shown as m (m.id)}
          <article class="card person" class:off={!m.is_active}>
            <div class="top">
              <Avatar id={m.id} name={m.display_name} size={48} />
              <div class="who">
                <div class="name">{m.display_name}{#if m.id === session.me?.id} <span class="faint small">(вы)</span>{/if}</div>
                <div class="muted small">{m.position || roleLabel[m.role] || m.role}</div>
                {#if !m.is_active}<span class="badge grey">отключён</span>{/if}
              </div>
            </div>
            {#if m.group_ids.length}
              <div class="tags">{#each m.group_ids as g}<span class="tag">{groupName.get(g)}</span>{/each}</div>
            {/if}
            <div class="contacts">
              <a href="mailto:{m.email}" title={m.email}><Mail size={14} /><span>{m.email}</span></a>
              {#if m.phone}<a href={tel(m.phone)}><Phone size={14} /><span>{m.phone}</span></a>{/if}
            </div>
            {#if m.role !== 'guest'}
              <div class="stats">
                <span title="Открытых задач"><b>{m.open_tasks}</b> задач</span>
                {#if m.overdue_tasks}<span class="late" title="Просроченных задач"><b>{m.overdue_tasks}</b> просрочено</span>{/if}
                <span title="Активных проектов"><b>{m.projects}</b> проектов</span>
              </div>
            {/if}
            {#if m.id !== session.me?.id && m.is_active && m.role !== 'guest'}
              <button class="btn sm write" onclick={() => write(m)}><MessageCircle size={14} /> Написать</button>
            {/if}
          </article>
        {/each}
      </div>
    {:else}
      <div class="card rows">
        {#each shown as m (m.id)}
          <div class="row" class:off={!m.is_active}>
            <Avatar id={m.id} name={m.display_name} size={32} />
            <div class="who">
              <div class="name">{m.display_name}</div>
              <div class="faint small">{m.position || roleLabel[m.role] || m.role}</div>
            </div>
            <div class="small muted grp">{m.group_ids.map((g) => groupName.get(g)).join(', ')}</div>
            <a class="small" href="mailto:{m.email}">{m.email}</a>
            <span class="small">{#if m.phone}<a href={tel(m.phone)}>{m.phone}</a>{/if}</span>
            <span class="small muted">{m.open_tasks} / <span class:late={m.overdue_tasks}>{m.overdue_tasks}</span></span>
            <span>
              {#if m.id !== session.me?.id && m.is_active && m.role !== 'guest'}
                <button class="btn ghost sm" title="Написать" onclick={() => write(m)}><MessageCircle size={16} /></button>
              {/if}
            </span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
</div>

<style>
  .layout { display: grid; grid-template-columns: 220px minmax(0, 1fr); gap: 16px; align-items: start; }
  .groups { padding: 6px; display: grid; gap: 2px; position: sticky; top: 12px; }
  .groups button {
    display: flex; align-items: center; gap: 8px; border: 0; background: none; font: inherit; color: var(--text);
    padding: 7px 10px; border-radius: var(--radius); cursor: pointer; text-align: left;
  }
  .groups button span:last-child:not(.dot) { margin-left: auto; color: var(--text-3); font-size: 12px; }
  .groups button:hover { background: var(--surface-2); }
  .groups button.on { background: var(--accent-soft); color: var(--accent); font-weight: 500; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--accent); opacity: .6; flex: none; }
  .dot.none { background: var(--text-3); }
  .bar { display: flex; gap: 10px; margin-bottom: 12px; }
  .search { position: relative; flex: 1; }
  .search :global(svg) { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); color: var(--text-3); }
  .search input { padding-left: 34px; width: 100%; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 12px; }
  .person { padding: 16px; display: flex; flex-direction: column; gap: 10px; }
  .person.off, .row.off { opacity: .55; }
  .top { display: flex; gap: 12px; align-items: center; }
  .who { min-width: 0; }
  .name { font-weight: 600; }
  .tags { display: flex; flex-wrap: wrap; gap: 4px; }
  .tag { font-size: 11px; padding: 2px 8px; border-radius: 999px; background: var(--accent-soft); color: var(--accent); }
  .contacts { display: grid; gap: 4px; font-size: 13px; }
  .contacts a { display: flex; gap: 6px; align-items: center; color: var(--text-2); min-width: 0; }
  .contacts a span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .stats { display: flex; gap: 12px; font-size: 12px; color: var(--text-3); border-top: 1px solid var(--border); padding-top: 10px; flex-wrap: wrap; }
  .stats b { color: var(--text); font-weight: 600; }
  .late, .late b { color: var(--danger) !important; }
  .write { align-self: flex-start; margin-top: auto; display: inline-flex; gap: 6px; align-items: center; }
  .rows { padding: 4px 0; }
  .row {
    display: grid; grid-template-columns: 32px minmax(140px, 1.3fr) minmax(0, 1fr) minmax(0, 1.2fr) 140px 70px 36px;
    gap: 12px; align-items: center; padding: 8px 14px; border-bottom: 1px solid var(--border);
  }
  .row:last-child { border-bottom: 0; }
  .row a { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .grp { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { padding: 30px; text-align: center; color: var(--text-2); }
  .page-head .btn { display: inline-flex; gap: 6px; align-items: center; }
  @media (max-width: 860px) {
    .layout { grid-template-columns: 1fr; }
    .groups { position: static; display: flex; overflow-x: auto; }
    .groups button { white-space: nowrap; }
    .row { grid-template-columns: 32px 1fr 36px; }
    .row > :nth-child(3), .row > :nth-child(4), .row > :nth-child(5), .row > :nth-child(6) { display: none; }
  }
</style>
