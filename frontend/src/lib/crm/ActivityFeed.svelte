<script lang="ts">
  // Interaction history with a quick "add entry" form.
  import { activityLabel, api, formatDateTime, type Activity, type ActivityKind, type Contact } from '../api';
  import { contactName } from '../api';
  import MentionInput from '../MentionInput.svelte';
  import { session } from '../session.svelte';

  let { activities, companyId = null, dealId = null, contacts = [], people = [], onchange }: {
    activities: Activity[];
    companyId?: string | null;
    dealId?: string | null;
    contacts?: Contact[];
    /** Colleagues who can be @mentioned. */
    people?: { id: string; name: string }[];
    onchange: () => void;
  } = $props();
  let mentions = $state<string[]>([]);

  const kinds: Exclude<ActivityKind, 'system'>[] = ['call', 'meeting', 'email', 'note'];
  let kind = $state<Exclude<ActivityKind, 'system'>>('call');
  let body = $state('');
  let contactId = $state('');
  let when = $state('');
  let error = $state('');
  let busy = $state(false);

  async function add() {
    error = '';
    busy = true;
    try {
      await api.post('/crm/activities', {
        company_id: companyId,
        deal_id: dealId,
        contact_id: contactId || null,
        kind,
        body,
        happened_at: when ? new Date(when).toISOString() : null,
        mentions
      });
      body = '';
      when = '';
      onchange();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  async function remove(a: Activity) {
    if (!confirm('Удалить запись из истории?')) return;
    try {
      await api.del(`/crm/activities/${a.id}`);
      onchange();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  const canDelete = (a: Activity) =>
    session.crmManager || (a.kind !== 'system' && a.author_id === session.me?.id);

  const icon: Record<ActivityKind, string> = {
    call: 'M5 4h4l2 5-2.5 1.5a11 11 0 0 0 5 5L15 13l5 2v4a2 2 0 0 1-2 2A16 16 0 0 1 3 6a2 2 0 0 1 2-2',
    meeting: 'M8 7a3 3 0 1 0 0-.01M16 7a3 3 0 1 0 0-.01M2 19c0-3 3-5 6-5s6 2 6 5M14 15c3 0 8 1 8 4',
    email: 'M3 6h18v12H3zM3 6l9 7 9-7',
    note: 'M5 3h10l4 4v14H5zM14 3v5h5M8 12h8M8 16h6',
    system: 'M12 3v4M12 17v4M3 12h4M17 12h4M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8'
  };
</script>

<div class="add card">
  <div class="kinds" role="radiogroup" aria-label="Тип записи">
    {#each kinds as k}
      <button class="chip" class:on={kind === k} role="radio" aria-checked={kind === k} onclick={() => (kind = k)}>{activityLabel[k]}</button>
    {/each}
  </div>
  <MentionInput bind:value={body} bind:mentions people={people.filter((p) => p.id !== session.me?.id)}
                placeholder={kind === 'call' ? 'О чём договорились' : 'Текст записи'} onsubmit={() => body.trim() && !busy && add()} />
  <div class="row opts">
    {#if contacts.length}
      <select class="input" bind:value={contactId} aria-label="Контактное лицо">
        <option value="">Контактное лицо —</option>
        {#each contacts as c}<option value={c.id}>{contactName(c)}</option>{/each}
      </select>
    {/if}
    <input class="input when" type="datetime-local" bind:value={when} aria-label="Когда" title="Когда (по умолчанию — сейчас)" />
    <div class="spacer"></div>
    <button class="btn primary" disabled={!body.trim() || busy} onclick={add}>Добавить</button>
  </div>
  {#if error}<div class="error-box">{error}</div>{/if}
</div>

{#if activities.length === 0}
  <p class="faint empty">Записей пока нет.</p>
{:else}
  <ol class="feed">
    {#each activities as a (a.id)}
      <li class="item k-{a.kind}">
        <span class="ic" aria-hidden="true">
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" stroke-linecap="round"><path d={icon[a.kind]} /></svg>
        </span>
        <div class="content">
          <div class="meta">
            <b>{activityLabel[a.kind]}</b>
            {#if a.contact_name}<span>· {a.contact_name}</span>{/if}
            {#if a.deal_title && !dealId}<span>· <a href="/crm/deals/{a.deal_id}">{a.deal_title}</a></span>{/if}
            <span class="faint">· {a.author_name ?? '—'} · {formatDateTime(a.happened_at)}</span>
          </div>
          <div class="body">{a.body}</div>
        </div>
        {#if canDelete(a)}<button class="btn ghost sm del" onclick={() => remove(a)} aria-label="Удалить">✕</button>{/if}
      </li>
    {/each}
  </ol>
{/if}

<style>
  .add { padding: 10px; display: grid; gap: 8px; margin-bottom: 12px; }
  .kinds { display: flex; gap: 4px; flex-wrap: wrap; }
  .chip { border: 1px solid var(--border-strong); background: var(--surface); border-radius: 999px; padding: 2px 10px; font-size: 12.5px; cursor: pointer; color: var(--text-2); }
  .chip.on { background: var(--accent-soft); color: var(--accent); border-color: var(--accent); }
  .opts { flex-wrap: wrap; }
  .opts select { width: auto; max-width: 240px; }
  .when { width: 200px; }
  .empty { margin: 0; }
  .feed { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .item { display: flex; gap: 10px; padding: 8px 4px; border-bottom: 1px solid var(--border); }
  .ic { width: 26px; height: 26px; border-radius: 50%; display: grid; place-items: center; flex: none; background: var(--accent-soft); color: var(--accent); }
  .k-system .ic { background: var(--surface-2); color: var(--text-3); }
  .k-call .ic { background: var(--ok-soft); color: var(--ok); }
  .k-meeting .ic { background: var(--warn-soft); color: var(--warn); }
  .content { flex: 1; min-width: 0; }
  .meta { font-size: 12.5px; display: flex; flex-wrap: wrap; gap: 4px; align-items: baseline; }
  .del { flex: none; align-self: flex-start; height: 20px; padding: 0 6px; color: var(--text-3); }
  .body { white-space: pre-wrap; overflow-wrap: anywhere; margin-top: 2px; }
  .k-system .body { color: var(--text-2); font-size: 13px; }
</style>
