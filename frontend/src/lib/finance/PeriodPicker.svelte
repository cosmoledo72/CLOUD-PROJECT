<script lang="ts">
  // Period buttons of the finance section (month … all time, or custom dates).
  import { PERIODS, periodRange, type Period } from './period';

  let { period = $bindable<Period>('year'), from = $bindable(''), to = $bindable(''), allowAll = true }:
    { period?: Period; from?: string; to?: string; allowAll?: boolean } = $props();

  function pick(p: Period) {
    period = p;
    if (p !== 'custom') [from, to] = periodRange(p);
  }
</script>

<div class="pp">
  <div class="seg" role="radiogroup" aria-label="Период">
    {#each PERIODS.filter(([k]) => allowAll || k !== 'all') as [k, l] (k)}
      <button role="radio" aria-checked={period === k} class:on={period === k} onclick={() => pick(k)}>{l}</button>
    {/each}
  </div>
  {#if period === 'custom'}
    <input class="input date" type="date" bind:value={from} aria-label="С" />
    <span class="faint">—</span>
    <input class="input date" type="date" bind:value={to} aria-label="По" />
  {/if}
</div>

<style>
  .pp { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .seg { display: inline-flex; border: 1px solid var(--border-strong); border-radius: var(--radius); overflow: hidden; flex-wrap: wrap; }
  .seg button { border: 0; background: var(--surface); padding: 5px 12px; cursor: pointer; font: inherit; color: var(--text-2); }
  .seg button.on { background: var(--accent); color: var(--on-accent); }
  .date { width: 150px; }
</style>
