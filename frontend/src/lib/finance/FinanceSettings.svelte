<script lang="ts">
  // Finance settings (finance editors): income and expense categories, the
  // invoice payment term.
  import { api, plainMessage, type FinanceCategory, type PayKind } from '$lib/api';
  import Modal from '$lib/Modal.svelte';

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let categories = $state<FinanceCategory[]>([]);
  let days = $state(10);
  let savedDays = $state(10);
  let newName = $state<Record<PayKind, string>>({ income: '', expense: '' });
  let error = $state('');
  let editing = $state<string | null>(null);
  let editName = $state('');

  async function load() {
    const [c, s] = await Promise.all([
      api.get<{ categories: FinanceCategory[] }>('/finance/categories'),
      api.get<{ invoice_due_days: number }>('/finance/settings')
    ]);
    categories = c.categories;
    days = savedDays = s.invoice_due_days;
  }
  let wasOpen = false;
  $effect(() => {
    if (open && !wasOpen) { error = ''; load().catch((e) => (error = plainMessage(e.message))); }
    wasOpen = open;
  });

  async function run(fn: () => Promise<unknown>) {
    error = '';
    try {
      await fn();
      await load();
    } catch (e) {
      error = plainMessage((e as Error).message);
    }
  }
  const add = (kind: PayKind) => run(async () => {
    await api.post('/finance/categories', { kind, name: newName[kind] });
    newName[kind] = '';
  });
  const rename = (c: FinanceCategory) => run(async () => {
    await api.patch(`/finance/categories/${c.id}`, { name: editName });
    editing = null;
  });
  const archive = (c: FinanceCategory) => run(() => api.patch(`/finance/categories/${c.id}`, { is_archived: !c.is_archived }));
  const saveDays = () => run(() => api.put('/finance/settings', { invoice_due_days: Number(days) }));
</script>

<Modal bind:open title="Настройки финансов" width={620}>
  <label class="field">Срок оплаты счёта, дней
    <div class="row">
      <input class="input days" type="number" min="0" max="365" bind:value={days} />
      <button class="btn" disabled={Number(days) === savedDays} onclick={saveDays}>Сохранить</button>
    </div>
    <span class="faint small">Счёт, не оплаченный за этот срок от даты счёта, считается просроченным.</span>
  </label>
  <div class="cols">
    {#each [['income', 'Статьи поступлений'], ['expense', 'Статьи расходов']] as [k, title] (k)}
      {@const kind = k as PayKind}
      <section>
        <h3>{title}</h3>
        <ul>
          {#each categories.filter((c) => c.kind === kind) as c (c.id)}
            <li class:arch={c.is_archived}>
              {#if editing === c.id}
                <input class="input sm" bind:value={editName} onkeydown={(e) => e.key === 'Enter' && rename(c)} />
                <button class="link" onclick={() => rename(c)}>ОК</button>
              {:else}
                <span class="nm">{c.name}</span>
                <button class="link" onclick={() => { editing = c.id; editName = c.name; }}>переименовать</button>
                <button class="link" onclick={() => archive(c)}>{c.is_archived ? 'вернуть' : 'в архив'}</button>
              {/if}
            </li>
          {/each}
        </ul>
        <div class="row">
          <input class="input sm" placeholder="Новая статья" bind:value={newName[kind]} onkeydown={(e) => e.key === 'Enter' && newName[kind].trim() && add(kind)} />
          <button class="btn sm" disabled={!newName[kind].trim()} onclick={() => add(kind)}>Добавить</button>
        </div>
      </section>
    {/each}
  </div>
  <p class="faint small">Статья в архиве не предлагается для новых платежей; прежние платежи сохраняют её.</p>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}<button class="btn primary" onclick={() => (open = false)}>Готово</button>{/snippet}
</Modal>

<style>
  .row { display: flex; gap: 8px; align-items: center; }
  .days { width: 100px; }
  .cols { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
  h3 { font-size: 13px; margin: 0 0 6px; color: var(--text-2); }
  ul { list-style: none; margin: 0 0 8px; padding: 0; display: grid; gap: 2px; }
  li { display: flex; gap: 8px; align-items: center; padding: 3px 0; font-size: 13.5px; }
  li.arch .nm { color: var(--text-3); text-decoration: line-through; }
  .nm { flex: 1; min-width: 0; }
  .link { border: 0; background: none; padding: 0; font: inherit; font-size: 12px; color: var(--accent); cursor: pointer; }
  .input.sm { height: 28px; padding: 0 8px; flex: 1; }
  p { margin: 0; }
  @media (max-width: 560px) { .cols { grid-template-columns: 1fr; } }
</style>
