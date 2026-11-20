<script lang="ts">
  // "Администрирование → Портал → Регистрация": who may join the portal by themselves.
  import { api, plainMessage, type RegistrationMode, type RegistrationSettings } from '#lib/api.ts';

  type Group = { id: string; name: string };
  let s = $state<RegistrationSettings | null>(null);
  let mode = $state<RegistrationMode>('request');
  let domains = $state('');
  let role = $state<'member' | 'guest'>('member');
  let groupIds = $state<string[]>([]);
  let groups = $state<Group[]>([]);
  let error = $state('');
  let notice = $state('');
  let busy = $state(false);

  let section: HTMLElement | undefined = $state();
  function apply(r: RegistrationSettings) {
    const first = !s;
    s = r;
    // Opened by a link to #registration: scroll here once the card has its height.
    if (first && location.hash === '#registration') setTimeout(() => section?.scrollIntoView({ block: 'start', behavior: 'smooth' }), 50);
    mode = r.mode;
    domains = r.domains.join(', ');
    role = r.role;
    groupIds = [...r.group_ids];
  }
  $effect(() => {
    api.get<RegistrationSettings>('/admin/registration').then(apply).catch((e) => (error = plainMessage(e.message)));
    api.get<{ groups: Group[] }>('/groups').then((r) => (groups = r.groups)).catch(() => {});
  });

  const domainList = $derived(domains.split(/[\s,;]+/).map((d) => d.trim()).filter(Boolean));
  const dirty = $derived(
    !!s && (mode !== s.mode || domainList.join(',') !== s.domains.join(',') || role !== s.role ||
            [...groupIds].sort().join(',') !== [...s.group_ids].sort().join(','))
  );

  async function save() {
    error = notice = '';
    busy = true;
    try {
      apply(await api.put<RegistrationSettings>('/admin/registration', { mode, domains: domainList, role, group_ids: groupIds }));
      notice = 'Настройки регистрации сохранены.';
    } catch (e) {
      error = plainMessage((e as Error).message);
    } finally {
      busy = false;
    }
  }
  const toggle = (id: string) => (groupIds = groupIds.includes(id) ? groupIds.filter((g) => g !== id) : [...groupIds, id]);
  const loginLink = $derived(typeof location !== 'undefined' ? `${location.origin}/register` : '/register');
</script>

<section class="card sec" id="registration" bind:this={section}>
  <h2>Регистрация сотрудников</h2>
  {#if s}
    <div class="modes" role="radiogroup" aria-label="Режим регистрации">
      <label class="opt" class:on={mode === 'off'}>
        <input type="radio" bind:group={mode} value="off" />
        <span><b>Только по приглашению</b><span class="faint small">Кнопки «Зарегистрироваться» на странице входа нет.</span></span>
      </label>
      <label class="opt" class:on={mode === 'request'}>
        <input type="radio" bind:group={mode} value="request" />
        <span><b>По заявке</b><span class="faint small">Любой может подать заявку, администратор одобряет или отклоняет её.</span></span>
      </label>
      <label class="opt" class:on={mode === 'domain'} class:disabled={!s.mail_available}>
        <input type="radio" bind:group={mode} value="domain" disabled={!s.mail_available && mode !== 'domain'} />
        <span><b>Свободно для корпоративной почты</b>
          <span class="faint small">Адреса указанных доменов входят сразу после подтверждения почты, остальные — по заявке.{#if !s.mail_available} Нужна настроенная <a href="/admin/mail">почта портала</a>.{/if}</span></span>
      </label>
    </div>
    {#if mode === 'domain'}
      <label class="field">Домены почты
        <input class="input" bind:value={domains} placeholder="partner-garant.ru" />
        <span class="faint small">Через запятую. Поддомены указываются отдельно.</span>
      </label>
    {/if}
    {#if mode !== 'off'}
      <div class="row2">
        <label class="field">Роль новых сотрудников
          <select class="input" bind:value={role}>
            <option value="member">Сотрудник</option>
            <option value="guest">Гость</option>
          </select>
        </label>
        {#if groups.length}
          <div class="field">
            <span>Добавлять в группы</span>
            <div class="chips">
              {#each groups as g (g.id)}
                <button type="button" class="chip" class:on={groupIds.includes(g.id)} aria-pressed={groupIds.includes(g.id)} onclick={() => toggle(g.id)}>{g.name}</button>
              {/each}
            </div>
          </div>
        {/if}
      </div>
      {#if !s.mail_available}
        <p class="faint small">Почта портала не настроена: адрес заявителя не проверяется, и каждая заявка сразу уходит администраторам.</p>
      {/if}
      <p class="faint small">Ссылка на регистрацию: <span class="mono">{loginLink}</span> (на странице входа тоже есть кнопка).</p>
    {/if}
    {#if error}<div class="error-box">{error}</div>{/if}
    {#if notice}<div class="ok-box">{notice}</div>{/if}
    <div class="row">
      {#if s.pending}<a href="/admin/users?tab=requests">Заявок ждут решения: {s.pending}</a>{/if}
      <div class="spacer"></div>
      <button class="btn primary" disabled={busy || !dirty || (mode === 'domain' && !domainList.length)} onclick={save}>Сохранить</button>
    </div>
  {:else if error}
    <div class="error-box">{error}</div>
  {/if}
</section>

<style>
  .sec { padding: 16px; display: grid; gap: 12px; }
  .sec h2, .sec p { margin: 0; }
  .modes { display: grid; gap: 6px; }
  .opt { display: flex; gap: 10px; align-items: flex-start; padding: 9px 12px; border: 1px solid var(--border); border-radius: 8px; cursor: pointer; }
  .opt.on { border-color: var(--accent); background: var(--accent-soft); }
  .opt.disabled { opacity: .7; }
  .opt input { margin-top: 3px; }
  .opt > span { display: grid; gap: 2px; }
  .row2 { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.5fr); gap: 12px; align-items: start; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { border: 1px solid var(--border); background: var(--surface); color: var(--text-2); border-radius: 999px; padding: 3px 10px; font: inherit; font-size: 13px; cursor: pointer; }
  .chip.on { background: var(--accent-soft); border-color: var(--accent); color: var(--accent); font-weight: 500; }
  .ok-box, .error-box { margin: 0; }
  @media (max-width: 560px) { .row2 { grid-template-columns: 1fr; } }
</style>
