<script lang="ts">
  // Requisite field «company»: search the CRM directory by name or INN, or
  // keep a typed-in name for a customer that is not in the directory.
  // Users without CRM access get a plain text field.
  import { api, type CompanyListItem, type CompanyRef } from '../api';
  import { session } from '../session.svelte';
  import { emptyCompany, toPayload } from './company';

  let {
    value = $bindable<CompanyRef | null>(null),
    onpick,
    invalid = false
  }: {
    value?: CompanyRef | null;
    /** Called after a directory entry is picked (e.g. to fill the INN field). */
    onpick?: (c: CompanyRef) => void;
    invalid?: boolean;
  } = $props();

  let text = $state(value?.name ?? '');
  let results = $state<CompanyListItem[]>([]);
  let open = $state(false);
  let active = $state(0);
  let busy = $state(false);
  let error = $state('');
  let timer: ReturnType<typeof setTimeout> | undefined;

  // Follow outside changes of the value (e.g. a reset of the form).
  $effect(() => {
    const n = value?.name ?? '';
    if (!open) text = n;
  });

  function input(e: Event) {
    text = (e.currentTarget as HTMLInputElement).value;
    // Typing detaches the directory link until something is picked.
    value = text.trim() ? { id: null, name: text, inn: null } : null;
    error = '';
    if (!session.crm) return;
    open = true;
    clearTimeout(timer);
    timer = setTimeout(search, 200);
  }

  async function search() {
    const q = text.trim();
    if (q.length < 2) {
      results = [];
      return;
    }
    try {
      const r = await api.get<{ companies: CompanyListItem[] }>(`/crm/companies?q=${encodeURIComponent(q)}`);
      results = r.companies.slice(0, 8);
      active = 0;
    } catch {
      results = [];
    }
  }

  function pick(c: CompanyListItem) {
    const ref: CompanyRef = { id: c.id, name: c.name, inn: c.inn };
    value = ref;
    text = c.name;
    open = false;
    onpick?.(ref);
  }

  // Quick add: a card with just the name; requisites can be filled in later.
  async function createAndPick() {
    const name = text.trim();
    if (!name) return;
    busy = true;
    error = '';
    try {
      const r = await api.post<{ id: string }>('/crm/companies', toPayload({ ...emptyCompany(session.me?.id ?? null), name }));
      pick({ id: r.id, name, inn: null } as CompanyListItem);
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  function keydown(e: KeyboardEvent) {
    if (!open || !results.length) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      active = (active + 1) % results.length;
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      active = (active - 1 + results.length) % results.length;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      pick(results[active]);
    } else if (e.key === 'Escape') {
      open = false;
    }
  }

  function clear() {
    value = null;
    text = '';
    results = [];
  }

  const exact = $derived(results.some((r) => r.name.trim().toLowerCase() === text.trim().toLowerCase()));
</script>

<div class="picker">
  <div class="box" class:linked={!!value?.id}>
    {#if value?.id}
      <svg class="mark" viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M3 8.5l3 3 7-7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
    {/if}
    <input
      class="input"
      value={text}
      oninput={input}
      onkeydown={keydown}
      onfocus={() => { if (session.crm && text.trim().length >= 2 && !value?.id) { open = true; search(); } }}
      onblur={() => setTimeout(() => (open = false), 150)}
      placeholder={session.crm ? 'Начните вводить название или ИНН' : 'Наименование организации'}
      aria-invalid={invalid}
      role="combobox"
      aria-expanded={open && results.length > 0}
      aria-controls="company-picker-list"
      aria-autocomplete="list"
    />
    {#if value}<button type="button" class="x" onclick={clear} aria-label="Очистить">×</button>{/if}
  </div>
  {#if value?.id}
    <span class="hint small">
      Из справочника{#if value.inn}, ИНН {value.inn}{/if} · <a href="/crm/companies/{value.id}" target="_blank" rel="noopener">карточка</a>
    </span>
  {:else if value && session.crm}
    <span class="hint small warn">Не из справочника — проект не будет связан с контрагентом</span>
  {/if}
  {#if error}<span class="hint small err">{error}</span>{/if}

  {#if open && session.crm && text.trim().length >= 2}
    <ul class="pop card" id="company-picker-list" role="listbox" aria-label="Контрагенты">
      {#each results as c, i (c.id)}
        <li role="option" aria-selected={i === active}>
          <button type="button" class:on={i === active} onmousedown={(e) => { e.preventDefault(); pick(c); }}>
            <span class="name">{c.name}</span>
            <span class="meta">{c.inn ? `ИНН ${c.inn}` : 'ИНН не указан'}{c.kpp ? ` · КПП ${c.kpp}` : ''}</span>
          </button>
        </li>
      {/each}
      {#if !exact}
        <li>
          <button type="button" class="add" disabled={busy} onmousedown={(e) => { e.preventDefault(); createAndPick(); }}>
            + Добавить «{text.trim()}» в справочник
          </button>
        </li>
      {/if}
    </ul>
  {/if}
</div>

<style>
  .picker { position: relative; display: grid; gap: 2px; min-width: 0; }
  .box { position: relative; display: flex; align-items: center; }
  .box .input { width: 100%; padding-right: 26px; }
  .box.linked .input { padding-left: 26px; border-color: color-mix(in srgb, var(--ok) 50%, var(--border-strong)); }
  .mark { position: absolute; left: 8px; color: var(--ok); pointer-events: none; }
  .x { position: absolute; right: 4px; border: 0; background: none; color: var(--text-3); cursor: pointer; font-size: 16px; padding: 0 4px; }
  .hint { color: var(--text-3); }
  .hint.warn { color: var(--warn); }
  .hint.err { color: var(--danger); }
  .pop {
    position: absolute; z-index: 40; top: 34px; left: 0; right: 0; margin: 2px 0 0; padding: 4px; list-style: none;
    box-shadow: var(--shadow-lg); max-height: 300px; overflow-y: auto;
  }
  .pop button {
    display: grid; width: 100%; border: 0; background: none; padding: 6px 8px; border-radius: var(--radius);
    cursor: pointer; text-align: left; font: inherit; color: var(--text);
  }
  .pop button.on, .pop button:hover { background: var(--accent-soft); }
  .name { font-weight: 500; }
  .meta { font-size: 12px; color: var(--text-3); }
  .add { color: var(--accent) !important; }
</style>
