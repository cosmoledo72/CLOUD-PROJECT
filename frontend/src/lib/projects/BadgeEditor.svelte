<script lang="ts">
  // Choose the project's badge: colour and either the first letter or an icon.
  import { api, type Project } from '#lib/api.ts';
  import Modal from '#lib/Modal.svelte';
  import { autoBadgeColor, BADGE_COLORS, BADGE_ICONS, badgeLetter } from './badge';
  import ProjectBadge from './ProjectBadge.svelte';

  let { open = $bindable(false), project, onsaved }: {
    open?: boolean;
    project: Pick<Project, 'id' | 'name' | 'badge_color' | 'badge_icon'>;
    onsaved?: (color: string | null, icon: string | null) => void;
  } = $props();

  let color = $state<string | null>(null);
  let icon = $state<string | null>(null);
  let error = $state('');
  let busy = $state(false);
  let wasOpen = false;
  $effect(() => {
    if (open && !wasOpen) {
      color = project.badge_color;
      icon = project.badge_icon;
      error = '';
    }
    wasOpen = open;
  });
  const auto = $derived(autoBadgeColor(project.id));

  async function save() {
    busy = true;
    error = '';
    try {
      await api.patch(`/projects/${project.id}`, { badge_color: color, badge_icon: icon });
      onsaved?.(color, icon);
      open = false;
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }
</script>

<Modal bind:open title="Значок проекта" width={460}>
  <div class="preview">
    <ProjectBadge id={project.id} name={project.name} {color} {icon} size={44} />
    <div class="pname">{project.name}</div>
  </div>
  <div class="grp">
    <span class="lbl">Цвет</span>
    <div class="colors" role="radiogroup" aria-label="Цвет значка">
      <button class="sw auto" class:on={color === null} style="background:{auto}" onclick={() => (color = null)} role="radio" aria-checked={color === null} title="Автоматически">A</button>
      {#each BADGE_COLORS as c}
        <button class="sw" class:on={color === c} style="background:{c}" onclick={() => (color = c)} role="radio" aria-checked={color === c} aria-label="Цвет {c}"></button>
      {/each}
    </div>
  </div>
  <div class="grp">
    <span class="lbl">Знак</span>
    <div class="icons" role="radiogroup" aria-label="Знак на значке">
      <button class="ib letter" class:on={icon === null} onclick={() => (icon = null)} role="radio" aria-checked={icon === null} title="Первая буква названия">{badgeLetter(project.name)}</button>
      {#each Object.entries(BADGE_ICONS) as [key, i] (key)}
        {@const Icon = i.icon}
        <button class="ib" class:on={icon === key} onclick={() => (icon = key)} role="radio" aria-checked={icon === key} title={i.label} aria-label={i.label}><Icon size={18} /></button>
      {/each}
    </div>
  </div>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (open = false)}>Отмена</button>
    <button class="btn primary" onclick={save} disabled={busy}>Сохранить</button>
  {/snippet}
</Modal>

<style>
  .preview { display: flex; align-items: center; gap: 12px; padding: 4px 0 8px; }
  .pname { font-weight: 600; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .grp { display: grid; gap: 6px; }
  .lbl { font-size: 12.5px; color: var(--text-3); }
  .colors, .icons { display: flex; flex-wrap: wrap; gap: 6px; }
  .sw { width: 26px; height: 26px; border-radius: 7px; border: 2px solid transparent; cursor: pointer; padding: 0; color: #fff; font-size: 11px; font-weight: 600; }
  .sw.on { box-shadow: 0 0 0 2px var(--surface), 0 0 0 4px var(--text); }
  .ib {
    width: 34px; height: 34px; display: grid; place-items: center; border: 1px solid var(--border); border-radius: 8px;
    background: var(--surface); color: var(--text-2); cursor: pointer; padding: 0; font-weight: 600;
  }
  .ib:hover { border-color: var(--border-strong); color: var(--text); }
  .ib.on { border-color: var(--accent); color: var(--accent); background: var(--accent-soft); }
</style>
