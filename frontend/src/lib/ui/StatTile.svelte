<script lang="ts">
  // Key figure tile used on every page: label, value, optional hint and an
  // icon chip on the right. A tile with `onclick` is a button (filter or
  // drill-down); `on` marks it as the active filter.
  import type { Component, Snippet } from 'svelte';

  type Tone = 'accent' | 'ok' | 'warn' | 'danger' | 'violet' | 'neutral';
  let {
    label, value, icon, tone = 'neutral', valueTone, hint, hintTone, title, onclick, on = false, dim = false, children
  }: {
    label: string;
    value: string | number;
    icon?: Component<{ size?: number }>;
    tone?: Tone;
    /** Colour of the value: ok (income), danger (expense, overdue), accent. */
    valueTone?: 'ok' | 'danger' | 'accent' | 'warn';
    hint?: string;
    hintTone?: 'ok' | 'danger' | 'warn';
    title?: string;
    onclick?: () => void;
    on?: boolean;
    /** Faded (a zero that needs no attention). */
    dim?: boolean;
    /** Custom hint line (instead of `hint`). */
    children?: Snippet;
  } = $props();
</script>

<svelte:element this={onclick ? 'button' : 'div'} class="card stat" class:click={!!onclick} class:on class:dim {title}
  {onclick} type={onclick ? 'button' : undefined} role={onclick ? undefined : 'group'} aria-pressed={onclick ? on : undefined}>
  <span class="s-label">{label}</span>
  {#if icon}{@const Icon = icon}<i class="chip-icon s-icon {tone}"><Icon size={17} /></i>{/if}
  <b class="s-value {valueTone ?? ''}">{value}</b>
  {#if children}<span class="s-hint">{@render children()}</span>
  {:else if hint}<span class="s-hint {hintTone ?? ''}">{hint}</span>{/if}
</svelte:element>

<style>
  .stat {
    display: grid; grid-template-columns: minmax(0, 1fr) auto; grid-template-rows: auto auto 1fr; column-gap: 10px; row-gap: 2px;
    padding: 12px 14px; text-align: left; font: inherit; color: inherit; min-width: 0; align-content: start;
  }
  .click { cursor: pointer; }
  .click:hover { border-color: color-mix(in srgb, var(--accent) 45%, var(--border)); }
  .on { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent) inset; }
  .dim .s-value { color: var(--text-3); }
  .s-label { grid-column: 1; font-size: 12.5px; color: var(--text-3); line-height: 1.35; }
  .s-icon { grid-column: 2; grid-row: 1 / span 2; align-self: start; width: 32px; height: 32px; border-radius: 9px; }
  .s-icon.violet { background: color-mix(in srgb, #7c3aed 13%, var(--surface)); color: #7c3aed; }
  .s-value { grid-column: 1; font-size: 21px; font-weight: 650; letter-spacing: -.01em; font-variant-numeric: tabular-nums; line-height: 1.3;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .s-hint { grid-column: 1 / -1; font-size: 12px; color: var(--text-3); line-height: 1.35; }
  .ok { color: var(--ok); }
  .danger { color: var(--danger); }
  .accent { color: var(--accent); }
  .warn { color: var(--warn); }
  .s-icon.ok { color: var(--ok); }
  .s-icon.danger { color: var(--danger); }
  .s-icon.accent { color: var(--accent); }
  .s-icon.warn { color: var(--warn); }
  @media (max-width: 640px) {
    .stat { padding: 10px 12px; }
    .s-value { font-size: 18px; }
    .s-icon { width: 28px; height: 28px; }
  }
</style>
