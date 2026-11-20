<script lang="ts">
  // Catalogue of project labels: administrators rename, recolour and delete;
  // managers of projects (and administrators) add new ones.
  import Modal from '#lib/Modal.svelte';
  import LabelChip from '#lib/tasks/LabelChip.svelte';
  import { LABEL_COLORS } from '#lib/tasks/labels.svelte.ts';
  import { projectLabels } from './projectLabels.svelte';

  let { open = $bindable(false), onchange }: { open?: boolean; onchange?: () => void } = $props();

  let newName = $state('');
  let newColor = $state(LABEL_COLORS[4]);
  let error = $state('');

  async function act(fn: () => Promise<unknown>) {
    error = '';
    try {
      await fn();
      onchange?.();
    } catch (e) {
      error = (e as Error).message;
      projectLabels.load(true).catch(() => {});
    }
  }
</script>

<Modal bind:open title="Метки проектов" width={520}>
  <p class="faint small none">Метки общие для всего портала. Добавляют их руководители проектов и администраторы; переименовывает и удаляет администратор.</p>
  <div class="list">
    {#each projectLabels.list as l (l.id)}
      <div class="row">
        {#if projectLabels.canManage}
          <input type="color" value={l.color} aria-label="Цвет метки {l.name}" onchange={(e) => act(() => projectLabels.update(l.id, { color: e.currentTarget.value }))} />
          <input class="input sm" value={l.name} maxlength="40" aria-label="Название метки"
                 onchange={(e) => act(() => projectLabels.update(l.id, { name: e.currentTarget.value }))} />
        {:else}
          <span class="chip"><LabelChip label={l} /></span>
        {/if}
        <span class="faint small cnt">{l.project_count} пр.</span>
        {#if projectLabels.canManage}
          <button class="btn ghost sm danger" onclick={() => confirm(`Удалить метку «${l.name}»? Она снимется со всех проектов.`) && act(() => projectLabels.remove(l.id))}>Удалить</button>
        {/if}
      </div>
    {:else}
      <p class="faint none">Меток пока нет.</p>
    {/each}
  </div>
  {#if projectLabels.canCreate}
    <div class="row add">
      <LabelChip label={{ name: newName.trim() || 'Новая метка', color: newColor }} />
      <input class="input sm" placeholder="Название, например «Ожидаем оплату»" maxlength="40" bind:value={newName} aria-label="Название новой метки"
             onkeydown={(e) => e.key === 'Enter' && newName.trim() && act(async () => { await projectLabels.create(newName.trim(), newColor); newName = ''; })} />
      <div class="swatches">
        {#each LABEL_COLORS as c}<button class="sw" class:on={newColor === c} style="background:{c}" onclick={() => (newColor = c)} aria-label="Цвет {c}"></button>{/each}
      </div>
      <button class="btn sm" disabled={!newName.trim()} onclick={() => act(async () => { await projectLabels.create(newName.trim(), newColor); newName = ''; })}>Добавить</button>
    </div>
  {/if}
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}<button class="btn" onclick={() => (open = false)}>Готово</button>{/snippet}
</Modal>

<style>
  .none { margin: 0; }
  .list { display: grid; gap: 6px; max-height: 340px; overflow-y: auto; }
  .row { display: flex; align-items: center; gap: 8px; }
  .row .input { flex: 1; min-width: 0; }
  .chip { flex: 1; }
  .cnt { white-space: nowrap; }
  .row input[type='color'] { width: 30px; height: 28px; padding: 0; border: 1px solid var(--border); border-radius: 6px; background: none; flex: none; }
  .row.add { flex-wrap: wrap; border-top: 1px solid var(--border); padding-top: 10px; }
  .swatches { display: flex; gap: 4px; }
  .sw { width: 18px; height: 18px; border-radius: 50%; border: 2px solid transparent; cursor: pointer; padding: 0; }
  .sw.on { border-color: var(--text); }
</style>
