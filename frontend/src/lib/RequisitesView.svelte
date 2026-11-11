<script lang="ts">
  // Read-only "project passport": key requisites as tiles on top (fields marked
  // "show in project card"), the rest as section cards with label–value rows.
  // Empty fields are folded into one "not filled" line per section.
  import { formatDate, formatFieldValue, localToday, nameDef, type CompanyRef, type FieldDef, type FieldValues } from './api';
  import {
    companyOf, daysBetween, deadline as dl, initials, isAddress, isCode, isDeadline, isMoney, money, relative as rel, shortDate
  } from './fieldLook';
  import { session } from './session.svelte';
  import Check from '@lucide/svelte/icons/check';
  import ClipboardList from '@lucide/svelte/icons/clipboard-list';
  import Copy from '@lucide/svelte/icons/copy';
  import FileSignature from '@lucide/svelte/icons/file-signature';
  import Info from '@lucide/svelte/icons/info';
  import MapPin from '@lucide/svelte/icons/map-pin';
  import UsersRound from '@lucide/svelte/icons/users-round';
  import type { Component } from 'svelte';

  let {
    defs,
    values,
    users = [],
    startDate = null,
    onfill
  }: {
    defs: FieldDef[];
    values: FieldValues;
    users?: { id: string; display_name: string }[];
    /** Start of the project period (for the deadline progress bar). */
    startDate?: string | null;
    /** Open the editor (shown to those who may edit). */
    onfill?: () => void;
  } = $props();

  const today = localToday();
  const isEmpty = (v: unknown) =>
    v === null || v === undefined || v === '' || (typeof v === 'object' && !(v as CompanyRef).name && !(v as CompanyRef).id);
  const nd = $derived(nameDef(defs));
  // Archived fields only when they still hold a value; the name requisite is the page title.
  const shown = $derived(defs.filter((d) => d.id !== nd?.id && (!d.is_archived || !isEmpty(values[d.key]))));
  const tiles = $derived(shown.filter((d) => d.show_in_list && !d.is_archived && d.field_type !== 'textarea'));
  const sections = $derived.by(() => {
    const out: { name: string; filled: FieldDef[]; empty: FieldDef[] }[] = [];
    for (const d of shown) {
      if (tiles.includes(d)) continue;
      let s = out.find((x) => x.name === d.section);
      if (!s) out.push((s = { name: d.section, filled: [], empty: [] }));
      (isEmpty(values[d.key]) ? s.empty : s.filled).push(d);
    }
    return out;
  });

  // Section icon by its name; a neutral one otherwise.
  function iconOf(section: string): Component {
    const s = section.toLowerCase();
    if (s.includes('объект') || s.includes('адрес')) return MapPin;
    if (s.includes('договор') || s.includes('оплат') || s.includes('финанс')) return FileSignature;
    if (s.includes('участник') || s.includes('команд') || s.includes('ответствен')) return UsersRound;
    if (s.includes('общ')) return Info;
    return ClipboardList;
  }

  // --- Value helpers ---------------------------------------------------------------
  const userName = (v: unknown) => users.find((u) => u.id === v)?.display_name ?? null;
  const relative = (date: string) => rel(today, date);
  const deadline = (date: string) => dl(today, date);
  const days = daysBetween;

  /** Start of the period: the earliest of the project start and the other filled
   *  dates (e.g. the contract date), so the bar is meaningful for imported projects. */
  const periodStart = $derived.by(() => {
    const ds = defs
      .filter((d) => d.field_type === 'date' && !isDeadline(d) && typeof values[d.key] === 'string' && values[d.key])
      .map((d) => String(values[d.key]));
    if (startDate) ds.push(startDate);
    return ds.length ? ds.sort()[0] : null;
  });
  /** Share of the period from its start to the date that has passed. */
  function elapsed(date: string): number | null {
    const start = periodStart;
    if (!start || start >= date) return null;
    const total = days(start, date);
    return Math.max(0, Math.min(100, Math.round((days(start, today) / total) * 100)));
  }

  let copied = $state<string | null>(null);
  async function copy(key: string, text: string) {
    try {
      await navigator.clipboard.writeText(text);
      copied = key;
      setTimeout(() => copied === key && (copied = null), 1500);
    } catch {
      /* clipboard unavailable */
    }
  }
  const mapUrl = (addr: string) => `https://yandex.ru/maps/?text=${encodeURIComponent(addr)}`;
</script>

{#snippet copyBtn(key: string, text: string)}
  <button class="copy" onclick={() => copy(key, text)} title="Копировать" aria-label="Копировать {text}">
    {#if copied === key}<Check size={13} strokeWidth={2.25} />{:else}<Copy size={13} strokeWidth={2} />{/if}
  </button>
{/snippet}

{#snippet value(d: FieldDef, big: boolean)}
  {@const v = values[d.key]}
  {#if d.field_type === 'company'}
    {@const c = companyOf(v)}
    {#if c?.id && session.crm}<a class="chip" href="/crm/companies/{c.id}">{c.name} ↗</a>{:else}<span class:chip-plain={big}>{c?.name}</span>{/if}
  {:else if d.field_type === 'user'}
    {@const n = userName(v)}
    <span class="person"><span class="av">{initials(n ?? '?')}</span>{n ?? 'Удалённый сотрудник'}</span>
  {:else if d.field_type === 'checkbox'}
    {#if v}<span class="badge on">Да</span>{:else}<span class="badge">Нет</span>{/if}
  {:else if d.field_type === 'date'}
    {@const s = String(v)}
    {#if big}{shortDate(s)}{:else}{formatDate(s)}{#if isDeadline(d)}{@const r = deadline(s)}<span class="rel {r.tone}">{r.text}</span>{/if}{/if}
  {:else if isMoney(d)}
    <span class="num">{money(v)}</span>
  {:else if isCode(d, v)}
    <span class="mono">{String(v)}</span>{@render copyBtn(d.key, String(v))}
  {:else if isAddress(d) && typeof v === 'string'}
    <span class="pre">{v}</span> <a class="map" href={mapUrl(v)} target="_blank" rel="noopener noreferrer">на карте</a>
  {:else}
    <span class="pre">{formatFieldValue(d, v, users)}</span>
  {/if}
{/snippet}

{#if shown.length === 0}
  <p class="faint none">Реквизиты не заданы.</p>
{:else}
  {#if tiles.length}
    <div class="tiles">
      {#each tiles as d (d.id)}
        {@const v = values[d.key]}
        <div class="card tile">
          <div class="t-lbl">{d.label}</div>
          {#if isEmpty(v)}
            <div class="t-val faint">—</div>
            {#if onfill}<button class="link small" onclick={onfill}>заполнить</button>{/if}
          {:else}
            <div class="t-val" class:t-text={d.field_type === 'company' || d.field_type === 'user' || d.field_type === 'select'}>{@render value(d, true)}</div>
            {#if d.field_type === 'company' && companyOf(v)?.inn}
              <div class="t-sub">ИНН {companyOf(v)?.inn}</div>
            {:else if d.field_type === 'date'}
              {@const r = isDeadline(d) ? deadline(String(v)) : relative(String(v))}
              <div class="t-sub rel {r.tone}">{r.text}</div>
              {#if isDeadline(d) && elapsed(String(v)) !== null}
                <div class="bar {r.tone}" title="Прошло {elapsed(String(v))}% срока"><span style="width:{elapsed(String(v))}%"></span></div>
              {/if}
            {/if}
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <div class="sections">
    {#each sections as s (s.name)}
      {@const Icon = iconOf(s.name)}
      <section class="card sec">
        <h3><span class="ico"><Icon size={15} strokeWidth={2} /></span>{s.name}</h3>
        {#if s.filled.length}
          <dl>
            {#each s.filled as d (d.id)}
              <dt>{d.label}{#if d.is_archived} <span class="arch">архив</span>{/if}</dt>
              <dd class:long={d.field_type === 'textarea'}>{@render value(d, false)}</dd>
            {/each}
          </dl>
        {/if}
        {#if s.empty.length}
          <p class="empty">
            Не заполнено: {s.empty.map((d) => d.label).join(', ')}{#if onfill}&nbsp;· <button class="link" onclick={onfill}>заполнить</button>{/if}
          </p>
        {/if}
      </section>
    {/each}
  </div>
{/if}

<style>
  .none { margin: 0; }
  .tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(190px, 1fr)); gap: 12px; margin-bottom: 14px; }
  .tile { padding: 14px 16px; display: grid; align-content: start; gap: 3px; min-width: 0; }
  .t-lbl { font-size: 11.5px; text-transform: uppercase; letter-spacing: .05em; color: var(--text-3); }
  .t-val { font-size: 19px; font-weight: 600; line-height: 1.3; overflow-wrap: anywhere; display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .t-val.t-text { font-size: 15.5px; }
  .t-sub { font-size: 12.5px; color: var(--text-2); }
  .bar { height: 5px; border-radius: 3px; background: var(--surface-2); overflow: hidden; margin-top: 6px; }
  .bar span { display: block; height: 100%; background: var(--accent); }
  .bar.soon span { background: var(--warn); }
  .bar.late span { background: var(--danger); }
  /* Masonry-like columns: short sections do not leave holes under them. */
  .sections { columns: 2 380px; column-gap: 14px; }
  .sec { padding: 16px 18px; min-width: 0; break-inside: avoid; margin-bottom: 14px; }
  h3 { margin: 0 0 8px; font-size: 12.5px; text-transform: uppercase; letter-spacing: .05em; color: var(--text-2); display: flex; align-items: center; gap: 8px; }
  .ico { width: 26px; height: 26px; border-radius: 7px; background: var(--accent-soft); color: var(--accent); display: grid; place-items: center; }
  dl { margin: 0; display: grid; grid-template-columns: minmax(120px, 34%) 1fr; }
  dt, dd { margin: 0; padding: 8px 0; border-top: 1px solid var(--border); min-width: 0; }
  dt:first-of-type, dd:first-of-type { border-top: 0; }
  dt { color: var(--text-2); font-size: 13.5px; padding-right: 12px; }
  dd { overflow-wrap: anywhere; display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  dd.long { display: block; }
  .pre { white-space: pre-wrap; }
  .arch { font-size: 11px; color: var(--text-3); border: 1px solid var(--border); border-radius: 4px; padding: 0 4px; }
  .empty { margin: 10px 0 0; font-size: 12.5px; color: var(--text-3); }
  .link { border: 0; background: none; padding: 0; font: inherit; color: var(--accent); cursor: pointer; justify-self: start; }
  .link:hover { text-decoration: underline; }
  .chip { display: inline-flex; align-items: center; padding: 2px 10px; border-radius: 999px; background: var(--accent-soft); color: var(--accent); font-weight: 500; font-size: 14px; }
  .chip:hover { text-decoration: none; filter: brightness(.97); }
  .person { display: inline-flex; align-items: center; gap: 8px; }
  .av { width: 24px; height: 24px; border-radius: 50%; background: var(--accent); color: var(--on-accent); display: inline-grid; place-items: center; font-size: 10.5px; font-weight: 600; flex: none; }
  .badge { display: inline-block; padding: 1px 9px; border-radius: 999px; font-size: 12.5px; font-weight: 600; background: var(--surface-2); color: var(--text-2); }
  .badge.on { background: var(--warn-soft); color: var(--warn); }
  .mono { font-family: ui-monospace, 'JetBrains Mono', Consolas, monospace; font-size: .93em; }
  .num { font-variant-numeric: tabular-nums; font-weight: 500; }
  .copy { border: 1px solid var(--border); background: var(--surface); color: var(--text-3); border-radius: 6px; width: 24px; height: 22px; display: inline-grid; place-items: center; cursor: pointer; padding: 0; }
  .copy:hover { color: var(--accent); border-color: var(--accent); }
  .map { font-size: 12.5px; color: var(--accent); white-space: nowrap; }
  .rel { font-size: 12.5px; color: var(--text-2); }
  .rel.soon { color: var(--warn); font-weight: 600; }
  .rel.late { color: var(--danger); font-weight: 600; }
  dd .rel { margin-left: 4px; }
  @media (max-width: 560px) {
    .tiles { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
    .tile { padding: 11px 12px; }
    .tile:last-child:nth-child(odd) { grid-column: span 2; }
    .t-val { font-size: 16px; }
    .t-val.t-text { font-size: 14px; }
    .sections { columns: 1; }
    dl { grid-template-columns: 1fr; }
    dt { padding-bottom: 0; }
    dd { border-top: 0; padding-top: 2px; }
  }
</style>
