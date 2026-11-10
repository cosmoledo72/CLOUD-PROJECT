<script lang="ts">
  // Textarea with @mentions: typing «@» and a few letters suggests people;
  // picked people whose «@Name» is still in the text are reported in `mentions`.

  type Person = { id: string; name: string };
  let {
    value = $bindable(''),
    mentions = $bindable<string[]>([]),
    people,
    rows = 2,
    placeholder = '',
    disabled = false,
    onsubmit
  }: {
    value?: string;
    mentions?: string[];
    people: Person[];
    rows?: number;
    placeholder?: string;
    disabled?: boolean;
    /** Ctrl+Enter / Cmd+Enter. */
    onsubmit?: () => void;
  } = $props();

  let el = $state<HTMLTextAreaElement>();
  let picked = $state<Person[]>([]);
  let query = $state<string | null>(null);
  let anchor = 0;
  let active = $state(0);

  $effect(() => {
    mentions = picked.filter((p) => value.includes('@' + p.name)).map((p) => p.id);
  });
  // A cleared field (after sending) forgets the picks.
  $effect(() => {
    if (!value) picked = [];
  });

  const suggestions = $derived(
    query === null
      ? []
      : people
          .filter((p) => p.name.toLowerCase().split(/\s+/).some((w) => w.startsWith(query!.toLowerCase())) || p.name.toLowerCase().startsWith(query!.toLowerCase()))
          .slice(0, 6)
  );

  function detect() {
    if (!el) return;
    const pos = el.selectionStart ?? 0;
    const before = value.slice(0, pos);
    // «@» at the start or after whitespace, followed by up to 30 non-space characters.
    const m = /(^|\s)@([^\s@]{0,30})$/.exec(before);
    if (m) {
      query = m[2];
      anchor = pos - m[2].length - 1;
      active = 0;
    } else {
      query = null;
    }
  }

  function pick(p: Person) {
    if (!el) return;
    const pos = el.selectionStart ?? value.length;
    const insert = `@${p.name} `;
    value = value.slice(0, anchor) + insert + value.slice(pos);
    if (!picked.some((x) => x.id === p.id)) picked = [...picked, p];
    query = null;
    const caret = anchor + insert.length;
    queueMicrotask(() => {
      el?.focus();
      el?.setSelectionRange(caret, caret);
    });
  }

  function keydown(e: KeyboardEvent) {
    if (suggestions.length) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        active = (active + 1) % suggestions.length;
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        active = (active - 1 + suggestions.length) % suggestions.length;
        return;
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault();
        pick(suggestions[active]);
        return;
      }
      if (e.key === 'Escape') {
        query = null;
        return;
      }
    }
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && onsubmit) {
      e.preventDefault();
      onsubmit();
    }
  }
</script>

<div class="mention">
  <textarea
    class="input"
    bind:this={el}
    bind:value
    {rows}
    {placeholder}
    {disabled}
    oninput={detect}
    onclick={detect}
    onkeydown={keydown}
    onblur={() => setTimeout(() => (query = null), 150)}
    aria-autocomplete="list"
  ></textarea>
  {#if suggestions.length}
    <ul class="pop card" role="listbox" aria-label="Упомянуть">
      {#each suggestions as p, i (p.id)}
        <li role="option" aria-selected={i === active}>
          <button type="button" class:on={i === active} onmousedown={(e) => { e.preventDefault(); pick(p); }}>
            <span class="ava">{p.name.split(/\s+/).map((w) => w[0]).slice(0, 2).join('').toUpperCase()}</span>{p.name}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
  <span class="hint faint small">@ — упомянуть коллегу{onsubmit ? ' · Ctrl+Enter — отправить' : ''}</span>
</div>

<style>
  .mention { position: relative; display: grid; gap: 2px; }
  textarea { resize: vertical; }
  .pop {
    /* Above the field: inputs usually sit at the bottom of scrolling containers. */
    position: absolute; z-index: 30; bottom: 100%; left: 0; margin: 0 0 4px; padding: 4px; list-style: none;
    min-width: 220px; box-shadow: var(--shadow-lg);
  }
  .pop button {
    display: flex; gap: 8px; align-items: center; width: 100%; border: 0; background: none; padding: 6px 8px;
    border-radius: var(--radius); cursor: pointer; text-align: left; font: inherit; color: var(--text);
  }
  .pop button.on, .pop button:hover { background: var(--accent-soft); }
  .ava {
    width: 22px; height: 22px; border-radius: 50%; background: var(--surface-2); color: var(--text-2);
    font-size: 10px; font-weight: 600; display: grid; place-items: center; flex: none;
  }
  .hint { justify-self: end; }
</style>
