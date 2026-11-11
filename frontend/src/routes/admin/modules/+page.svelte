<script lang="ts">
  // Portal modules: switch whole sections on and off. Data of a switched-off
  // module is kept and comes back when the module is switched on again.
  import { api, type ModuleKey } from '$lib/api';
  import { session } from '$lib/session.svelte';
  import Boxes from '@lucide/svelte/icons/boxes';

  interface ModuleRow {
    key: ModuleKey;
    label: string;
    description: string;
    parent: ModuleKey | null;
    switched_on: boolean;
    enabled: boolean;
  }

  let modules = $state<ModuleRow[]>([]);
  let on = $state<Record<string, boolean>>({});
  let saved = $state<Record<string, boolean>>({});
  let error = $state('');
  let notice = $state('');
  let busy = $state(false);

  function apply(list: ModuleRow[]) {
    modules = list;
    on = Object.fromEntries(list.map((m) => [m.key, m.switched_on]));
    saved = { ...on };
  }
  $effect(() => {
    api.get<{ modules: ModuleRow[] }>('/admin/modules').then((r) => apply(r.modules)).catch((e) => (error = e.message));
  });

  const labelOf = (k: string | null) => modules.find((m) => m.key === k)?.label ?? '';
  const parentOff = (m: ModuleRow) => !!m.parent && !on[m.parent];
  const dirty = $derived(modules.some((m) => on[m.key] !== saved[m.key]));
  const turningOff = $derived(modules.filter((m) => saved[m.key] && !on[m.key]));

  async function save() {
    error = '';
    notice = '';
    busy = true;
    try {
      const disabled = modules.filter((m) => !on[m.key]).map((m) => m.key);
      const r = await api.put<{ modules: ModuleRow[] }>('/admin/modules', { disabled });
      apply(r.modules);
      await session.load();
      notice = 'Модули сохранены. Остальные сотрудники увидят изменения после обновления страницы.';
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }
</script>

<div class="card intro">
  <Boxes size={20} strokeWidth={1.75} />
  <p class="muted">
    Отключённый модуль пропадает из меню и разделов портала, перестаёт присылать уведомления, а сервер не принимает к нему
    обращений. <b>Данные не удаляются</b>: после включения всё вернётся на место.
  </p>
</div>

<div class="card list">
  {#each modules as m (m.key)}
    <label class="row" class:child={!!m.parent} class:muted-row={parentOff(m)}>
      <span class="text">
        <span class="name">{m.label}</span>
        <span class="faint small">{m.description}</span>
        {#if parentOff(m)}<span class="small warn">Не работает, пока выключен модуль «{labelOf(m.parent)}»</span>{/if}
      </span>
      <input type="checkbox" class="switch" bind:checked={on[m.key]} aria-label="Модуль «{m.label}»" />
    </label>
  {/each}
</div>

{#if turningOff.length}
  <p class="small warn">Будут отключены: {turningOff.map((m) => `«${m.label}»`).join(', ')}. Данные сохранятся.</p>
{/if}
{#if error}<div class="error-box">{error}</div>{/if}
{#if notice}<div class="ok-box" role="status">{notice}</div>{/if}
<div class="row-actions">
  <button class="btn primary" disabled={!dirty || busy} onclick={save}>Сохранить</button>
  {#if dirty}<button class="btn" onclick={() => (on = { ...saved })}>Отменить</button>{/if}
</div>

<style>
  .intro { display: flex; gap: 12px; align-items: flex-start; padding: 14px 16px; margin-bottom: 14px; color: var(--text-2); }
  .intro p { margin: 0; }
  .list { padding: 4px 0; max-width: 760px; }
  .row { display: flex; align-items: center; gap: 16px; padding: 12px 16px; border-bottom: 1px solid var(--border); cursor: pointer; }
  .row:last-child { border-bottom: 0; }
  .row.child { padding-left: 40px; }
  .row.muted-row .name { color: var(--text-3); }
  .text { flex: 1; display: grid; gap: 2px; min-width: 0; }
  .name { font-weight: 500; }
  .warn { color: var(--warn); }
  .switch {
    appearance: none; width: 40px; height: 22px; border-radius: 11px; background: var(--border); position: relative;
    cursor: pointer; flex: none; transition: background .15s; margin: 0;
  }
  .switch::after {
    content: ''; position: absolute; top: 3px; left: 3px; width: 16px; height: 16px; border-radius: 50%;
    background: #fff; box-shadow: 0 1px 2px rgb(0 0 0 / .25); transition: transform .15s;
  }
  .switch:checked { background: var(--accent); }
  .switch:checked::after { transform: translateX(18px); }
  .switch:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .row-actions { display: flex; gap: 8px; margin-top: 12px; }
  .ok-box { background: var(--ok-soft, var(--accent-soft)); color: var(--ok); padding: 8px 12px; border-radius: var(--radius); margin-top: 10px; }
  .error-box { margin-top: 10px; }
</style>
