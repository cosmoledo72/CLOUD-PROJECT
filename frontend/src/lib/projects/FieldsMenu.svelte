<script lang="ts">
  // "Поля" popover of the project list: which fields the table and the cards
  // show and in what order (drag to reorder), the line under the name, and
  // resetting to the portal default / saving it as the default (admins).
  import type { FieldDef } from '#lib/api.ts';
  import Columns from '@lucide/svelte/icons/columns-3';
  import GripVertical from '@lucide/svelte/icons/grip-vertical';
  import { BUILTIN, usableDefs, type ListConfig } from './listConfig';

  let {
    defs,
    config = $bindable(),
    view,
    own,
    isAdmin = false,
    money = false,
    onreset,
    onmakedefault
  }: {
    defs: FieldDef[];
    config: ListConfig;
    view: 'rows' | 'cards';
    /** The user has an own setting (otherwise the portal default is in use). */
    own: boolean;
    isAdmin?: boolean;
    /** Project money columns are available (finance module and access). */
    money?: boolean;
    onreset: () => void;
    onmakedefault: () => void;
  } = $props();

  let open = $state(false);
  let tab = $state<'rows' | 'cards'>('rows');
  let root: HTMLElement | undefined = $state();

  function toggleOpen() {
    open = !open;
    if (open) tab = view;
  }
  $effect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => { if (root && !root.contains(e.target as Node)) open = false; };
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') open = false; };
    document.addEventListener('mousedown', onDoc);
    document.addEventListener('keydown', onKey);
    return () => { document.removeEventListener('mousedown', onDoc); document.removeEventListener('keydown', onKey); };
  });

  const usable = $derived(usableDefs(defs));
  const labelOf = (k: string) => (k in BUILTIN ? BUILTIN[k as keyof typeof BUILTIN] : usable.find((d) => d.key === k)?.label ?? k);
  // Everything that can be listed in the current tab.
  const candidates = $derived(
    [...(tab === 'rows' ? ['$name'] : []), ...usable.map((d) => d.key), '$status', '$labels', '$progress', '$dates',
     ...(money ? ['$budget', '$paid', '$due'] : []), ...(tab === 'cards' ? ['$description'] : [])]
  );
  const chosen = $derived(tab === 'rows' ? config.columns : config.card);
  const items = $derived([...chosen.filter((k) => candidates.includes(k)), ...candidates.filter((k) => !chosen.includes(k))]);

  function setChosen(list: string[]) {
    if (tab === 'rows') config = { ...config, columns: list };
    else config = { ...config, card: list };
  }
  function toggle(k: string) {
    if (k === '$name') return;
    setChosen(chosen.includes(k) ? chosen.filter((x) => x !== k) : [...chosen, k]);
  }
  function move(k: string, to: number) {
    const list = chosen.filter((x) => x !== k);
    list.splice(Math.max(0, Math.min(to, list.length)), 0, k);
    setChosen(list);
  }

  // Drag and drop among the chosen items.
  let dragKey = $state<string | null>(null);
  let overIdx = $state<number | null>(null);
  function onDrop(i: number) {
    if (dragKey && chosen.includes(dragKey)) move(dragKey, i > chosen.indexOf(dragKey) ? i - 1 : i);
    dragKey = null;
    overIdx = null;
  }
  function onKey(e: KeyboardEvent, k: string) {
    const i = chosen.indexOf(k);
    if (i < 0 || !e.altKey) return;
    if (e.key === 'ArrowUp') { e.preventDefault(); move(k, i - 1); }
    if (e.key === 'ArrowDown') { e.preventDefault(); move(k, i + 1); }
  }
  const subtitleOptions = $derived(usable.filter((d) => d.field_type !== 'checkbox'));
</script>

<div class="wrap" bind:this={root}>
  <button class="btn" class:on={open} onclick={toggleOpen} aria-expanded={open} aria-haspopup="dialog">
    <Columns size={15} /> Поля
  </button>
  {#if open}
    <div class="pop" role="dialog" aria-label="Поля списка проектов">
      <div class="tabs" role="tablist">
        <button role="tab" aria-selected={tab === 'rows'} class:on={tab === 'rows'} onclick={() => (tab = 'rows')}>Таблица</button>
        <button role="tab" aria-selected={tab === 'cards'} class:on={tab === 'cards'} onclick={() => (tab = 'cards')}>Карточки</button>
      </div>
      <ul class="list" ondragleave={() => (overIdx = null)}>
        {#each items as k, i (k)}
          {@const on = chosen.includes(k)}
          <li class:on class:over={overIdx === i && dragKey !== k} class:fixed={k === '$name'}
              draggable={on}
              ondragstart={(e) => { dragKey = k; e.dataTransfer?.setData('text/plain', k); }}
              ondragend={() => { dragKey = null; overIdx = null; }}
              ondragover={(e) => { if (dragKey && i <= chosen.length) { e.preventDefault(); overIdx = i; } }}
              ondrop={(e) => { e.preventDefault(); onDrop(i); }}>
            <span class="grip" aria-hidden="true">{#if on}<GripVertical size={14} />{/if}</span>
            <label>
              <input type="checkbox" checked={on} disabled={k === '$name'} onchange={() => toggle(k)} onkeydown={(e) => onKey(e, k)}
                     title={on ? 'Alt+↑/↓ — переместить' : ''} />
              <span class:builtin={k.startsWith('$')}>{labelOf(k)}</span>
            </label>
          </li>
        {/each}
      </ul>
      {#if tab === 'rows'}
        <label class="sub">Под названием
          <select class="input sm" value={config.subtitle ?? ''} onchange={(e) => (config = { ...config, subtitle: e.currentTarget.value || null })}>
            <option value="">ничего</option>
            <option value="$description">Описание</option>
            {#each subtitleOptions as d (d.id)}<option value={d.key}>{d.label}</option>{/each}
          </select>
        </label>
      {:else}
        <p class="hint">Шифр и срок — в шапке карточки, первый сотрудник — внизу, остальные поля — сеткой.</p>
      {/if}
      <div class="ft">
        <button class="link" disabled={!own} onclick={onreset} title="Вернуть набор полей по умолчанию для портала">Сбросить</button>
        <div class="spacer"></div>
        {#if isAdmin}<button class="link" onclick={onmakedefault} title="Этот набор полей увидят все, кто не настраивал свой">Сделать по умолчанию для всех</button>{/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .wrap { position: relative; }
  .btn { display: inline-flex; align-items: center; gap: 6px; }
  .btn.on { border-color: var(--accent); color: var(--accent); }
  .pop { position: absolute; right: 0; top: calc(100% + 6px); z-index: 30; width: 300px; background: var(--surface); border: 1px solid var(--border);
         border-radius: 10px; box-shadow: 0 12px 32px rgba(0, 0, 0, .16); padding: 10px; display: grid; gap: 8px; }
  .tabs { display: flex; gap: 2px; background: var(--surface-2); border-radius: 7px; padding: 2px; }
  .tabs button { flex: 1; border: 0; background: none; padding: 5px; border-radius: 5px; font: inherit; font-size: 13px; color: var(--text-2); cursor: pointer; }
  .tabs button.on { background: var(--surface); color: var(--text); font-weight: 500; box-shadow: 0 1px 2px rgba(0, 0, 0, .08); }
  .list { list-style: none; margin: 0; padding: 0; max-height: min(52vh, 420px); overflow: auto; }
  .list li { display: flex; align-items: center; gap: 2px; border-radius: 6px; border-top: 2px solid transparent; }
  .list li:hover { background: var(--surface-2); }
  .list li.over { border-top-color: var(--accent); }
  .list li.on { cursor: grab; }
  .grip { width: 16px; color: var(--text-3); display: grid; place-items: center; }
  label { display: flex; align-items: center; gap: 8px; padding: 5px 6px 5px 2px; flex: 1; cursor: pointer; font-size: 13.5px; min-width: 0; }
  li.fixed label { cursor: default; }
  .builtin { color: var(--text-2); }
  .sub { border-top: 1px solid var(--border); padding-top: 8px; font-size: 13px; color: var(--text-2); display: flex; align-items: center; gap: 8px; }
  .input.sm { height: 28px; flex: 1; padding: 0 6px; min-width: 0; }
  .hint { margin: 0; border-top: 1px solid var(--border); padding-top: 8px; font-size: 12px; color: var(--text-3); }
  .ft { display: flex; align-items: center; gap: 8px; border-top: 1px solid var(--border); padding-top: 8px; }
  .link { border: 0; background: none; padding: 0; font: inherit; font-size: 12.5px; color: var(--accent); cursor: pointer; }
  .link:disabled { color: var(--text-3); cursor: default; }
  .link:not(:disabled):hover { text-decoration: underline; }
</style>
