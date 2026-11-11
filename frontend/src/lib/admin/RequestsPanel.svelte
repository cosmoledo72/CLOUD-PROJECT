<script lang="ts">
  // Registration requests: approve (with role) or reject (with a reason); history.
  import { api, formatDate, plainMessage, type RegistrationRequest, type RegistrationSettings } from '$lib/api';
  import Modal from '$lib/Modal.svelte';

  let { settings, onchange }: { settings: RegistrationSettings | null; onchange: () => void } = $props();

  let items = $state<RegistrationRequest[]>([]);
  let history = $state(false);
  let roles = $state<Record<string, 'member' | 'guest'>>({});
  let error = $state('');
  let notice = $state('');
  let busy = $state<string | null>(null);

  async function load() {
    const r = await api.get<{ requests: RegistrationRequest[]; default_role: 'member' | 'guest' }>(
      `/admin/registration/requests${history ? '?status=all' : ''}`
    );
    items = r.requests;
    for (const x of r.requests) roles[x.id] ??= r.default_role;
  }
  $effect(() => {
    void history;
    load().catch((e) => (error = plainMessage(e.message)));
  });

  async function approve(r: RegistrationRequest) {
    error = notice = '';
    busy = r.id;
    try {
      const res = await api.post<{ emailed: boolean }>(`/admin/registration/requests/${r.id}/approve`, { role: roles[r.id] });
      notice = `${r.display_name} принят(а) в портал${res.emailed ? ' — письмо отправлено' : ''}.`;
      await load();
      onchange();
    } catch (e) {
      error = plainMessage((e as Error).message);
    } finally {
      busy = null;
    }
  }

  let rejectOf = $state<RegistrationRequest | null>(null);
  let rejectOpen = $state(false);
  let reason = $state('');
  function openReject(r: RegistrationRequest) {
    rejectOf = r;
    reason = '';
    rejectOpen = true;
  }
  async function reject() {
    const r = rejectOf;
    if (!r) return;
    error = notice = '';
    try {
      await api.post(`/admin/registration/requests/${r.id}/reject`, { reason });
      rejectOpen = false;
      notice = `Заявка ${r.email} отклонена.`;
      await load();
      onchange();
    } catch (e) {
      error = plainMessage((e as Error).message);
    }
  }

  const modeText = $derived(
    !settings ? '' :
    settings.mode === 'off' ? 'Регистрация закрыта: в портал попадают только по приглашению.' :
    settings.mode === 'request' ? 'Регистрация по заявке: каждую заявку одобряет администратор.' :
    `Адреса ${settings.domains.map((d) => '@' + d).join(', ')} входят сразу после подтверждения почты, остальные — по заявке.`
  );
  const pending = $derived(items.filter((x) => x.status === 'pending'));
  const decided = $derived(items.filter((x) => x.status !== 'pending'));
  const statusLabel: Record<string, string> = { unconfirmed: 'почта не подтверждена', approved: 'принят', rejected: 'отклонена' };
</script>

<div class="card mode">
  <p class="muted">{modeText}</p>
  <a class="btn sm" href="/admin/portal#registration">Настроить регистрацию</a>
</div>

{#if error}<div class="error-box">{error}</div>{/if}
{#if notice}<div class="ok-box">{notice}</div>{/if}

{#if pending.length === 0}
  <div class="card empty"><p class="muted">Новых заявок нет.</p></div>
{:else}
  <div class="reqs">
    {#each pending as r (r.id)}
      <article class="card req">
        <div class="who">
          <div class="name">{r.display_name}</div>
          <div class="faint small">{r.email} · {formatDate(r.created_at)}</div>
          <div class="badges">
            {#if r.email_confirmed}<span class="badge ok">почта подтверждена</span>{:else}<span class="badge grey">почта не проверялась</span>{/if}
            {#if r.domain_match}<span class="badge">свой домен</span>{/if}
          </div>
          {#if r.comment}<p class="comment">{r.comment}</p>{/if}
        </div>
        <div class="act">
          <select class="input sm" bind:value={roles[r.id]} aria-label="Роль для {r.display_name}">
            <option value="member">Сотрудник</option>
            <option value="guest">Гость</option>
          </select>
          <button class="btn primary sm" disabled={busy === r.id} onclick={() => approve(r)}>Принять</button>
          <button class="btn ghost sm danger" onclick={() => openReject(r)}>Отклонить</button>
        </div>
      </article>
    {/each}
  </div>
{/if}

<label class="hist"><input type="checkbox" bind:checked={history} /> Показать рассмотренные и неподтверждённые (за 90 дней)</label>
{#if history && decided.length}
  <div class="card scroll">
    <table class="list">
      <thead><tr><th>Кто</th><th>Подана</th><th>Итог</th><th>Решение</th></tr></thead>
      <tbody>
        {#each decided as r (r.id)}
          <tr>
            <td>{r.display_name}<div class="faint small">{r.email}</div></td>
            <td class="muted">{formatDate(r.created_at)}</td>
            <td><span class="badge" class:ok={r.status === 'approved'} class:warn={r.status === 'rejected'} class:grey={r.status === 'unconfirmed'}>{statusLabel[r.status]}</span>
              {#if r.reject_reason}<div class="faint small">{r.reject_reason}</div>{/if}</td>
            <td class="muted">{r.decided_by_name ?? (r.status === 'approved' ? 'автоматически (домен)' : '—')}{#if r.decided_at}<div class="faint small">{formatDate(r.decided_at)}</div>{/if}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<Modal bind:open={rejectOpen} title={`Отклонить заявку: ${rejectOf?.display_name ?? ''}`}>
  <label class="field"><span>Причина <span class="faint">(уйдёт заявителю письмом, необязательно)</span></span>
    <textarea class="input" rows="3" bind:value={reason} maxlength="1000"></textarea>
  </label>
  {#snippet footer()}
    <button class="btn" onclick={() => (rejectOpen = false)}>Отмена</button>
    <button class="btn primary danger" onclick={reject}>Отклонить</button>
  {/snippet}
</Modal>

<style>
  .ok-box, .error-box { margin-bottom: 12px; }
  .mode { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 10px 14px; margin-bottom: 12px; flex-wrap: wrap; }
  .mode p { margin: 0; }
  .empty { padding: 20px; margin-bottom: 12px; }
  .empty p { margin: 0; }
  .reqs { display: grid; gap: 10px; margin-bottom: 12px; }
  .req { padding: 14px 16px; display: flex; gap: 16px; justify-content: space-between; align-items: flex-start; flex-wrap: wrap; }
  .who { min-width: 0; flex: 1 1 320px; display: grid; gap: 4px; }
  .name { font-weight: 600; font-size: 15px; }
  .badges { display: flex; gap: 6px; flex-wrap: wrap; }
  .comment { margin: 4px 0 0; padding: 6px 10px; background: var(--surface-2); border-radius: 6px; font-size: 13.5px; white-space: pre-wrap; }
  .act { display: flex; gap: 6px; align-items: center; }
  .input.sm { height: 28px; width: auto; padding: 0 6px; }
  .hist { display: flex; align-items: center; gap: 8px; font-size: 13.5px; color: var(--text-2); margin: 4px 0 10px; }
  .scroll { overflow-x: auto; }
  textarea.input { height: auto; padding-top: 6px; resize: vertical; }
</style>
