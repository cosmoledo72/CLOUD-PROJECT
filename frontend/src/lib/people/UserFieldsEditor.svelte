<script lang="ts">
  // Administrator: the portal's own requisites of employees (label and type,
  // in order). A removed field is deleted with its values.
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import Plus from '@lucide/svelte/icons/plus';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import Modal from '#lib/Modal.svelte';
  import { api, plainMessage } from '#lib/api.ts';
  import type { UserFieldDef, UserFieldType } from './types';

  let { open = $bindable(false), fields, onsaved }: { open?: boolean; fields: UserFieldDef[]; onsaved: () => void } = $props();

  type Row = { id?: string; label: string; field_type: UserFieldType };
  let rows = $state<Row[]>([]);
  let error = $state('');
  let busy = $state(false);
  const TYPES: [UserFieldType, string][] = [['text', 'Строка'], ['textarea', 'Текст'], ['date', 'Дата'], ['number', 'Число']];

  $effect(() => {
    if (open) {
      rows = fields.map((f) => ({ id: f.id, label: f.label, field_type: f.field_type }));
      error = '';
    }
  });
  const removed = $derived(fields.filter((f) => !rows.some((r) => r.id === f.id)));

  function moveRow(i: number, d: number) {
    const j = i + d;
    if (j < 0 || j >= rows.length) return;
    const next = [...rows];
    [next[i], next[j]] = [next[j], next[i]];
    rows = next;
  }
  async function save() {
    if (removed.length && !confirm(`Удалить ${removed.map((f) => `«${f.label}»`).join(', ')} вместе со значениями у всех сотрудников?`)) return;
    busy = true;
    error = '';
    try {
      await api.put('/admin/user-fields', { fields: rows.map((r) => ({ ...r, label: r.label.trim() })) });
      open = false;
      onsaved();
    } catch (e) {
      error = plainMessage((e as Error).message);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Свои реквизиты сотрудников" bind:open width={560}>
  <p class="muted small intro">Поля появятся на вкладке «Реквизиты для документов» у всех сотрудников. Заполняют их сами сотрудники или администратор.</p>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#if rows.length}
    <ul class="rows">
      {#each rows as r, i (r.id ?? i)}
        <li>
          <input class="input" bind:value={r.label} maxlength="120" placeholder="Название поля" aria-label="Название поля {i + 1}" />
          <select class="input type" bind:value={r.field_type} aria-label="Тип поля {i + 1}">
            {#each TYPES as [k, l]}<option value={k}>{l}</option>{/each}
          </select>
          <button class="btn ghost icon" disabled={i === 0} onclick={() => moveRow(i, -1)} aria-label="Выше"><ArrowUp size={15} /></button>
          <button class="btn ghost icon" disabled={i === rows.length - 1} onclick={() => moveRow(i, 1)} aria-label="Ниже"><ArrowDown size={15} /></button>
          <button class="btn ghost icon danger-text" onclick={() => (rows = rows.filter((_, k) => k !== i))} aria-label="Удалить"><Trash2 size={15} /></button>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="faint small">Своих полей пока нет.</p>
  {/if}
  <button class="btn sm" onclick={() => (rows = [...rows, { label: '', field_type: 'text' }])} disabled={rows.length >= 40}><Plus size={15} />Поле</button>
  {#snippet footer()}
    <button class="btn" onclick={() => (open = false)}>Отмена</button>
    <button class="btn primary" disabled={busy || rows.some((r) => !r.label.trim())} onclick={save}>Сохранить</button>
  {/snippet}
</Modal>

<style>
  .intro { margin: 0 0 12px; }
  .rows { list-style: none; margin: 0 0 10px; padding: 0; display: grid; gap: 6px; }
  .rows li { display: flex; gap: 6px; align-items: center; }
  .rows .input { flex: 1; min-width: 0; }
  .rows .type { flex: none; width: 110px; }
  .icon { padding: 0 8px; }
  .danger-text { color: var(--danger); }
</style>
