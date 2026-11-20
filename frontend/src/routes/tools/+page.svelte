<script lang="ts">
  // «Инструменты»: tiles of the engineering tools, grouped (lib/tools/registry.ts).
  import { session } from '#lib/session.svelte.ts';
  import { STATUS_LABEL, TOOL_GROUPS, TOOLS } from '#lib/tools/registry.ts';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
</script>

<div class="page tools">
  <div class="page-head"><h1>Инструменты</h1></div>
  {#if session.me?.role === 'guest'}
    <div class="card pad"><p class="muted none">Раздел доступен сотрудникам портала.</p></div>
  {:else}
    <p class="muted lead">Расчёты и подготовка документации прямо в портале — с данными проектов и сохранением результата в папку проекта.</p>
    {#each TOOL_GROUPS as g (g.key)}
      {@const items = TOOLS.filter((t) => t.group === g.key)}
      {#if items.length}
        <section class="grp">
          <h2>{g.title}</h2>
          <div class="grid">
            {#each items as t (t.slug)}
              {@const Icon = t.icon}
              <a class="card tile" href="/tools/{t.slug}" style="--c: {t.color}">
                <span class="ic"><Icon size={22} /></span>
                <span class="body">
                  <span class="ttl">{t.title}</span>
                  <span class="sum">{t.summary}</span>
                </span>
                <span class="foot">
                  {#if STATUS_LABEL[t.status]}<span class="badge-s s-{t.status}">{STATUS_LABEL[t.status]}</span>{/if}
                  <span class="go"><ArrowRight size={16} /></span>
                </span>
              </a>
            {/each}
          </div>
        </section>
      {/if}
    {/each}
  {/if}
</div>

<style>
  .tools { max-width: 1240px; }
  .pad { padding: 20px; }
  .none { margin: 0; }
  .lead { margin: -10px 0 18px; }
  .grp { margin-bottom: 22px; }
  h2 { font-size: 12px; font-weight: 600; text-transform: uppercase; letter-spacing: .06em; color: var(--text-3); margin: 0 0 10px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 12px; }
  .tile {
    display: grid; grid-template-rows: auto 1fr auto; gap: 12px; padding: 16px; min-height: 168px;
    color: var(--text); text-decoration: none; transition: border-color .15s, box-shadow .15s, transform .15s;
  }
  .tile:hover { border-color: color-mix(in srgb, var(--c) 45%, var(--border)); box-shadow: var(--shadow-md); text-decoration: none; transform: translateY(-1px); }
  .tile:focus-visible { outline: none; box-shadow: var(--ring); }
  .ic {
    width: 42px; height: 42px; border-radius: 10px; display: grid; place-items: center;
    background: color-mix(in srgb, var(--c) 12%, var(--surface)); color: var(--c);
  }
  .body { display: grid; gap: 4px; }
  .ttl { font-weight: 600; font-size: 15px; line-height: 1.3; }
  .sum { font-size: 13px; color: var(--text-2); line-height: 1.4; }
  .foot { display: flex; align-items: center; justify-content: space-between; }
  .badge-s { font-size: 11.5px; font-weight: 600; padding: 2px 8px; border-radius: 999px; }
  .s-soon { background: var(--surface-3); color: var(--text-2); }
  .s-beta { background: var(--warn-soft); color: var(--warn); }
  .go { color: var(--text-3); display: inline-grid; margin-left: auto; transition: color .15s, transform .15s; }
  .tile:hover .go { color: var(--c); transform: translateX(2px); }
</style>
