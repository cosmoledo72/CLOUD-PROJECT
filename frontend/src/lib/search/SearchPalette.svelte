<script lang="ts">
  // Ctrl+K palette: a few hits per section as you type; Enter opens the
  // highlighted hit, «Все результаты» goes to /search with filters.
  import { goto } from '$app/navigation';
  import Search from '@lucide/svelte/icons/search';
  import CornerDownLeft from '@lucide/svelte/icons/corner-down-left';
  import Clock from '@lucide/svelte/icons/clock';
  import Hl from './Hl.svelte';
  import HitIcon from './HitIcon.svelte';
  import { palette, queryWords, recent, remember, runSearch, type Hit, type HitGroup } from './search.svelte';

  let dialog: HTMLDialogElement;
  let input = $state<HTMLInputElement>();
  let groups = $state<HitGroup[]>([]);
  let busy = $state(false);
  let error = $state('');
  let active = $state(0);
  let recents = $state<string[]>([]);
  let seq = 0;

  const q = $derived(palette.q.trim());
  const flat = $derived(groups.flatMap((g) => g.items));
  const words = $derived(queryWords(q));
  // Rows the arrows walk through: hits, then «all results».
  const total = $derived(flat.length + (q.length >= 2 ? 1 : 0));

  $effect(() => {
    if (palette.open && !dialog.open) {
      recents = recent();
      dialog.showModal();
      setTimeout(() => input?.select());
    }
    if (!palette.open && dialog.open) dialog.close();
  });

  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const text = q;
    clearTimeout(timer);
    active = 0;
    if (text.length < 2) {
      groups = [];
      busy = false;
      return;
    }
    busy = true;
    const my = ++seq;
    timer = setTimeout(() => {
      runSearch({ q: text })
        .then((g) => { if (my === seq) { groups = g; error = ''; } })
        .catch((e) => { if (my === seq) error = (e as Error).message; })
        .finally(() => { if (my === seq) busy = false; });
    }, 180);
  });

  function close() {
    palette.open = false;
  }
  function open(h: Hit) {
    remember(q);
    close();
    goto(h.url);
  }
  function all(type = '') {
    remember(q);
    close();
    goto(`/search?q=${encodeURIComponent(q)}${type ? `&type=${type}` : ''}`);
  }
  function keydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (!total) return;
      active = (active + (e.key === 'ArrowDown' ? 1 : total - 1)) % total;
      document.getElementById(`sp-${active}`)?.scrollIntoView({ block: 'nearest' });
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (active < flat.length) open(flat[active]);
      else if (q.length >= 2) all();
    }
  }
  const index = (h: Hit) => flat.indexOf(h);
</script>

<dialog bind:this={dialog} class="sp" onclose={close} aria-label="Поиск по порталу"
        onclick={(e) => { if (e.target === dialog) close(); }}>
  <div class="box">
    <div class="bar">
      <Search size={18} />
      <input bind:this={input} bind:value={palette.q} onkeydown={keydown} placeholder="Поиск: проекты, задачи, документы, контрагенты…"
             aria-label="Что найти" role="combobox" aria-expanded={flat.length > 0} aria-controls="sp-list" aria-activedescendant={total ? `sp-${active}` : undefined}
             autocomplete="off" spellcheck="false" />
      {#if busy}<span class="spin" aria-hidden="true"></span>{/if}
      <kbd>Esc</kbd>
    </div>
    <div class="list" id="sp-list" role="listbox">
      {#if q.length < 2}
        {#if recents.length}
          <div class="gh">Недавние запросы</div>
          {#each recents as r (r)}
            <button class="recent" onclick={() => (palette.q = r)}><Clock size={15} />{r}</button>
          {/each}
        {/if}
        <p class="tip">Ищет в названиях и в тексте документов Word, Excel, PDF, в задачах и комментариях, в CRM и чатах — только там, куда у вас есть доступ.
          <br />Номер задачи — <b>#234</b>, контрагент — по ИНН. Окно поиска: <kbd>Ctrl</kbd>+<kbd>K</kbd> или <kbd>/</kbd>.</p>
      {:else}
        {#if error}<div class="error-box">{error}</div>{/if}
        {#each groups as g (g.type)}
          <div class="gh">{g.label}{#if g.more}<button class="more" onclick={() => all(g.type)}>все</button>{/if}</div>
          {#each g.items as h (h.type + h.id)}
            {@const i = index(h)}
            <button id="sp-{i}" class="hit" class:on={i === active} role="option" aria-selected={i === active}
                    onmousemove={() => (active = i)} onclick={() => open(h)}>
              <HitIcon hit={h} />
              <span class="tx">
                <span class="t">{#if h.number}<span class="num">#{h.number}</span>{/if}<Hl text={h.title} {words} /></span>
                {#if h.snippet}<span class="sn"><Hl text={h.snippet} /></span>{/if}
                {#if h.subtitle}<span class="sub">{h.subtitle}</span>{/if}
              </span>
              {#if i === active}<CornerDownLeft size={15} class="enter" />{/if}
            </button>
          {/each}
        {/each}
        {#if !busy && !groups.length && !error}<p class="tip">Ничего не найдено по запросу «{q}».</p>{/if}
        <button id="sp-{flat.length}" class="hit allr" class:on={active === flat.length} role="option" aria-selected={active === flat.length}
                onmousemove={() => (active = flat.length)} onclick={() => all()}>
          <span class="hi0"><Search size={16} /></span><span class="tx"><span class="t">Все результаты по «{q}»</span><span class="sub">С фильтрами по разделу, проекту и дате</span></span>
        </button>
      {/if}
    </div>
  </div>
</dialog>

<style>
  .sp { border: 0; padding: 0; background: transparent; width: min(680px, calc(100vw - 24px)); max-height: none; margin: 10vh auto auto; overflow: visible; }
  .sp::backdrop { background: rgb(15 20 30 / 40%); }
  .box { background: var(--surface); color: var(--text); border: 1px solid var(--border); border-radius: 14px; box-shadow: var(--shadow-lg); overflow: hidden; }
  .bar { display: flex; align-items: center; gap: 10px; padding: 12px 14px; border-bottom: 1px solid var(--border); color: var(--text-3); }
  .bar input { flex: 1; min-width: 0; border: 0; outline: 0; background: none; font: inherit; font-size: 16px; color: var(--text); }
  kbd { font: 11px/1 var(--font-mono, ui-monospace, monospace); padding: 3px 5px; border: 1px solid var(--border); border-bottom-width: 2px; border-radius: 5px; color: var(--text-2); background: var(--surface-2, var(--surface)); }
  .list { max-height: min(62vh, 560px); overflow-y: auto; padding: 6px; scrollbar-width: thin; }
  .gh { display: flex; align-items: center; justify-content: space-between; font-size: 11.5px; font-weight: 600; letter-spacing: .04em; text-transform: uppercase;
        color: var(--text-3); padding: 10px 10px 4px; }
  .more { border: 0; background: none; font: inherit; font-size: 12px; text-transform: none; letter-spacing: 0; color: var(--accent); cursor: pointer; font-weight: 500; }
  .hit { display: flex; align-items: flex-start; gap: 10px; width: 100%; padding: 7px 10px; border: 0; border-radius: 9px; background: none; font: inherit;
         text-align: left; color: var(--text); cursor: pointer; }
  .hit.on { background: var(--accent-soft); }
  .tx { display: grid; gap: 1px; min-width: 0; flex: 1; }
  .t { font-size: 14px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .num { color: var(--text-3); margin-right: 6px; font-variant-numeric: tabular-nums; }
  .sn { font-size: 12.5px; color: var(--text-2); display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .sub { font-size: 12px; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hit :global(.enter) { color: var(--accent); margin-top: 6px; flex: none; }
  .allr { margin-top: 4px; border-top: 1px solid var(--border); border-radius: 0 0 9px 9px; }
  .hi0 { display: grid; place-items: center; width: 28px; height: 28px; border-radius: 7px; color: var(--accent); flex: none; }
  .recent { display: flex; align-items: center; gap: 10px; width: 100%; padding: 7px 10px; border: 0; border-radius: 9px; background: none; font: inherit; font-size: 14px;
            color: var(--text-2); cursor: pointer; text-align: left; }
  .recent:hover { background: var(--surface-3); color: var(--text); }
  .tip { margin: 8px 10px 10px; font-size: 13px; color: var(--text-3); line-height: 1.6; }
  .spin { width: 14px; height: 14px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: r .7s linear infinite; }
  @keyframes r { to { transform: rotate(360deg); } }
  @media (max-width: 640px) {
    .sp { margin-top: 8px; }
    .bar kbd { display: none; }
    .sn { -webkit-line-clamp: 3; line-clamp: 3; }
  }
</style>
