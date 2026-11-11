<script lang="ts">
  // Typed form for project requisites, grouped by section. Used in the
  // project page, the "new project" dialog and the admin field editor preview.
  import { formatFieldValue, type CompanyRef, type FieldDef, type FieldValues } from './api';
  import CompanyPicker from './crm/CompanyPicker.svelte';
  import { session } from './session.svelte';

  let {
    defs,
    values = $bindable({}),
    users = [],
    readonly = false,
    requiredOnly = false,
    exclude = []
  }: {
    defs: FieldDef[];
    values?: FieldValues;
    users?: { id: string; display_name: string }[];
    readonly?: boolean;
    requiredOnly?: boolean;
    /** Keys shown elsewhere in the form (e.g. the project name requisite). */
    exclude?: string[];
  } = $props();

  const visible = $derived(
    defs.filter(
      (d) =>
        (readonly ? !d.is_archived || values[d.key] != null : !d.is_archived) &&
        (!requiredOnly || d.is_required) &&
        !exclude.includes(d.key)
    )
  );
  const sections = $derived.by(() => {
    const out: { name: string; fields: FieldDef[] }[] = [];
    for (const d of visible) {
      let s = out.find((x) => x.name === d.section);
      if (!s) out.push((s = { name: d.section, fields: [] }));
      s.fields.push(d);
    }
    return out;
  });

  const wide = (d: FieldDef) => d.field_type === 'textarea' || d.field_type === 'company';
  const str = (v: unknown) => (v === null || v === undefined ? '' : String(v));
  const companyOf = (v: unknown): CompanyRef | null =>
    v && typeof v === 'object' ? (v as CompanyRef) : typeof v === 'string' && v ? { id: null, name: v, inn: null } : null;

  // Picking a directory entry also fills the INN requisite when there is one;
  // leaving the directory entry clears the INN it brought.
  const innDef = $derived(defs.find((d) => d.key === 'client_inn' && d.field_type === 'text' && !d.is_archived));
  function picked(c: CompanyRef) {
    if (innDef && c.inn) values[innDef.key] = c.inn;
  }
  function setCompany(key: string, v: CompanyRef | null) {
    const prev = companyOf(values[key]);
    if (innDef && prev?.id && prev.inn && !v?.id && values[innDef.key] === prev.inn) values[innDef.key] = null;
    values[key] = v;
  }
</script>

{#if visible.length === 0}
  <p class="faint empty">Реквизиты не заданы.</p>
{:else}
  <div class="req">
    {#each sections as s (s.name)}
      <fieldset>
        {#if !requiredOnly}<legend>{s.name}</legend>{/if}
        <div class="grid">
          {#each s.fields as d (d.id)}
            {#if readonly}
              <div class="item" class:wide={wide(d)}>
                <div class="lbl">{d.label}{#if d.is_archived} <span class="badge grey">архив</span>{/if}</div>
                {#if d.field_type === 'company' && companyOf(values[d.key])?.id && session.crm}
                  <div class="val"><a href="/crm/companies/{companyOf(values[d.key])?.id}">{companyOf(values[d.key])?.name}</a></div>
                {:else}
                  <div class="val" class:empty-val={values[d.key] == null}>{formatFieldValue(d, values[d.key], users)}</div>
                {/if}
              </div>
            {:else if d.field_type === 'checkbox'}
              <label class="check item" title={d.help_text}>
                <input type="checkbox" checked={values[d.key] === true} onchange={(e) => (values[d.key] = e.currentTarget.checked)} />
                {d.label}{#if d.is_required}<span class="req-mark" aria-label="обязательное">*</span>{/if}
              </label>
            {:else}
              <label class="field item" class:wide={wide(d)}>
                <span>{d.label}{#if d.is_required}<span class="req-mark" aria-label="обязательное">*</span>{/if}</span>
                {#if d.field_type === 'textarea'}
                  <textarea class="input" rows="2" value={str(values[d.key])} oninput={(e) => (values[d.key] = e.currentTarget.value)}></textarea>
                {:else if d.field_type === 'select'}
                  <select class="input" value={str(values[d.key])} onchange={(e) => (values[d.key] = e.currentTarget.value || null)}>
                    <option value="">—</option>
                    {#each d.options as o}<option value={o}>{o}</option>{/each}
                  </select>
                {:else if d.field_type === 'user'}
                  <select class="input" value={str(values[d.key])} onchange={(e) => (values[d.key] = e.currentTarget.value || null)}>
                    <option value="">—</option>
                    {#each users as u}<option value={u.id}>{u.display_name}</option>{/each}
                  </select>
                {:else if d.field_type === 'company'}
                  <CompanyPicker onpick={picked}
                                 bind:value={() => companyOf(values[d.key]), (v) => setCompany(d.key, v)} />
                {:else if d.field_type === 'date'}
                  <input class="input" type="date" value={str(values[d.key])} onchange={(e) => (values[d.key] = e.currentTarget.value || null)} />
                {:else if d.field_type === 'number'}
                  <input class="input" inputmode="decimal" value={str(values[d.key])} oninput={(e) => (values[d.key] = e.currentTarget.value)} />
                {:else}
                  <input class="input" value={str(values[d.key])} oninput={(e) => (values[d.key] = e.currentTarget.value)} />
                {/if}
                {#if d.help_text}<span class="faint small">{d.help_text}</span>{/if}
              </label>
            {/if}
          {/each}
        </div>
      </fieldset>
    {/each}
  </div>
{/if}

<style>
  .req { display: grid; gap: 16px; }
  fieldset { border: 0; padding: 0; margin: 0; min-width: 0; }
  legend { font-weight: 600; font-size: 13px; padding: 0 0 8px; color: var(--text); }
  .grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px 16px; }
  .wide { grid-column: 1 / -1; }
  .item { min-width: 0; }
  .lbl { font-size: 12px; color: var(--text-3); margin-bottom: 2px; }
  .val { white-space: pre-wrap; overflow-wrap: anywhere; }
  .empty-val { color: var(--text-3); }
  .req-mark { color: var(--danger); margin-left: 3px; }
  .check { align-self: end; padding-bottom: 6px; color: var(--text); }
  .empty { margin: 0; }
  @media (max-width: 560px) { .grid { grid-template-columns: 1fr; } }
</style>
