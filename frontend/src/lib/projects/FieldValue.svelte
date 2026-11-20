<script lang="ts">
  // One requisite value of a project in the list: deadline as a coloured pill,
  // person with an avatar, money, code in monospace, customer, yes/no.
  import { formatDate, formatFieldValue, type FieldDef } from '#lib/api.ts';
  import { avatarHue, companyOf, deadline, initials, isCode, isDeadline, isMoney, money } from '#lib/fieldLook.ts';

  let {
    def,
    value,
    users,
    today,
    active = true,
    short = false
  }: {
    def: FieldDef;
    value: unknown;
    users: { id: string; display_name: string }[];
    today: string;
    /** Deadlines of inactive projects are shown without urgency. */
    active?: boolean;
    /** Compact deadline text (cards, narrow cells). */
    short?: boolean;
  } = $props();

  const empty = $derived(value === null || value === undefined || value === '');
  const person = $derived(def.field_type === 'user' ? users.find((u) => u.id === value) : undefined);
  const dl = $derived(def.field_type === 'date' && isDeadline(def) && !empty ? deadline(today, String(value)) : null);
  const dd = (s: string) => {
    const [y, m, d] = s.split('-');
    return y === today.slice(0, 4) ? `${d}.${m}` : `${d}.${m}.${y}`;
  };
</script>

{#if empty}
  <span class="none">—</span>
{:else if dl && active && (dl.tone === 'late' || dl.tone === 'soon')}
  <span class="pill {dl.tone}" title={formatDate(String(value))}>{short ? dl.text : `${dd(String(value))} · ${dl.text}`}</span>
{:else if dl}
  <span class="pill plain">{formatDate(String(value))}</span>
{:else if def.field_type === 'user'}
  {#if person}<span class="person"><span class="av" style="--h:{avatarHue(person.id)}">{initials(person.display_name)}</span><span class="nm">{person.display_name}</span></span>{:else}<span class="none">—</span>{/if}
{:else if def.field_type === 'company'}
  <span class="clip">{companyOf(value)?.name}</span>
{:else if def.field_type === 'checkbox'}
  {#if value}<span class="yes">Да</span>{:else}<span class="none">Нет</span>{/if}
{:else if isMoney(def)}
  <span class="num">{money(value)}</span>
{:else if isCode(def, value)}
  <span class="mono">{String(value)}</span>
{:else}
  <span class="clip">{formatFieldValue(def, value, users)}</span>
{/if}

<style>
  .none { color: var(--text-3); }
  .pill { display: inline-block; padding: 1px 8px; border-radius: 999px; font-size: 12px; font-weight: 500; white-space: nowrap; font-variant-numeric: tabular-nums; }
  .pill.plain { background: var(--surface-2); color: var(--text-2); font-weight: 400; }
  .pill.soon { background: var(--warn-soft); color: var(--warn); }
  .pill.late { background: var(--danger-soft, #fbe7e5); color: var(--danger); }
  .person { display: inline-flex; align-items: center; gap: 7px; min-width: 0; max-width: 100%; }
  .av { width: 24px; height: 24px; border-radius: 50%; flex: none; display: inline-grid; place-items: center; font-size: 10px; font-weight: 600;
        color: #fff; background: hsl(var(--h) 42% 42%); }
  .nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .clip { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .yes { color: var(--ok); font-weight: 500; }
  .num { font-variant-numeric: tabular-nums; white-space: nowrap; font-weight: 500; }
  .mono { font-family: ui-monospace, 'JetBrains Mono', Consolas, monospace; font-size: .93em; white-space: nowrap; }
</style>
