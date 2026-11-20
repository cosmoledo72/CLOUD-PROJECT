<script lang="ts">
  // Page of one tool. While a tool is being built it shows what it will do.
  import { page } from '$app/state';
  import EmptyState from '#lib/EmptyState.svelte';
  import { session } from '#lib/session.svelte.ts';
  import { toolBySlug } from '#lib/tools/registry.ts';
  import Construction from '@lucide/svelte/icons/construction';
  import SearchX from '@lucide/svelte/icons/search-x';
  import CircleCheck from '@lucide/svelte/icons/circle-check';

  const tool = $derived(toolBySlug(page.params.slug ?? ''));
</script>

<div class="page tool">
  <div class="page-head">
    <a href="/tools" class="faint">Инструменты</a><span class="faint">/</span>
    <h1>{tool?.title ?? 'Инструмент не найден'}</h1>
  </div>
  {#if session.me?.role === 'guest'}
    <div class="card pad"><p class="muted none">Раздел доступен сотрудникам портала.</p></div>
  {:else if !tool}
    <div class="card"><EmptyState icon={SearchX} title="Такого инструмента нет" text="Вернитесь к списку инструментов." /></div>
  {:else}
    {@const Icon = tool.icon}
    <div class="card hero" style="--c: {tool.color}">
      <span class="ic"><Icon size={28} /></span>
      <div class="txt">
        <p class="sum">{tool.summary}</p>
        <span class="wip"><Construction size={15} />В разработке</span>
      </div>
    </div>
    <section class="card plan">
      <h2>Что будет в инструменте</h2>
      <ul>
        {#each tool.plan as p}<li><CircleCheck size={16} />{p}</li>{/each}
      </ul>
      <p class="faint small">Пожелания к инструменту можно оставить в базе знаний или в чате — они учитываются при разработке.</p>
    </section>
  {/if}
</div>

<style>
  .tool { max-width: 900px; }
  .pad { padding: 20px; }
  .none { margin: 0; }
  .hero { display: flex; gap: 16px; align-items: center; padding: 18px 20px; margin-bottom: 14px; }
  .ic {
    width: 56px; height: 56px; border-radius: 14px; display: grid; place-items: center; flex: none;
    background: color-mix(in srgb, var(--c) 12%, var(--surface)); color: var(--c);
  }
  .txt { display: grid; gap: 8px; }
  .sum { margin: 0; font-size: 15px; }
  .wip { display: inline-flex; align-items: center; gap: 6px; justify-self: start; font-size: 12.5px; font-weight: 600; padding: 3px 10px; border-radius: 999px; background: var(--warn-soft); color: var(--warn); }
  .plan { padding: 18px 20px; }
  h2 { font-size: 15px; margin: 0 0 12px; }
  ul { list-style: none; margin: 0 0 14px; padding: 0; display: grid; gap: 9px; }
  li { display: flex; gap: 10px; align-items: flex-start; line-height: 1.4; }
  li :global(svg) { color: var(--text-3); flex: none; margin-top: 2px; }
  .small { margin: 0; }
</style>
