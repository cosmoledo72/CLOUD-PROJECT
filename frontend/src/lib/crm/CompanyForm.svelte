<script lang="ts">
  // Company card form with live requisite validation and duplicate detection.
  import { api, type CompanyInput, type UserItem } from '../api';
  import { session } from '../session.svelte';
  import { accountValid, bikValid, innValid, kppValid, normCode, ogrnValid } from './validate';

  let { value = $bindable(), excludeId = null, users = [], groups = [] }: {
    value: CompanyInput;
    excludeId?: string | null;
    users?: UserItem[];
    groups?: { id: string; name: string }[];
  } = $props();

  // --- Field-level validation (instant feedback) -------------------------------
  const c = (s: string) => normCode(s ?? '');
  const errors = $derived({
    inn: c(value.inn) && !innValid(c(value.inn)) ? 'Неверная длина или контрольная сумма' : '',
    kpp: c(value.kpp) && !kppValid(c(value.kpp)) ? '9 символов' : '',
    ogrn: c(value.ogrn) && !ogrnValid(c(value.ogrn)) ? 'Неверная контрольная сумма' : '',
    bik: c(value.bik) && !bikValid(c(value.bik)) ? '9 цифр' : '',
    account: c(value.account)
      ? !c(value.bik) ? 'Укажите БИК' : !accountValid(c(value.bik), c(value.account), false) ? 'Не сходится ключ с БИК' : ''
      : '',
    corr_account: c(value.corr_account)
      ? !c(value.bik) ? 'Укажите БИК' : !accountValid(c(value.bik), c(value.corr_account), true) ? 'Не сходится ключ с БИК' : ''
      : ''
  });
  export function hasErrors() {
    return Object.values(errors).some(Boolean) || !value.name.trim();
  }

  // --- Duplicate check by INN -----------------------------------------------------
  let dupes = $state<{ id: string | null; name: string; kpp: string | null; same_kpp: boolean }[]>([]);
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const inn = c(value.inn);
    const kpp = c(value.kpp);
    clearTimeout(timer);
    if (!innValid(inn)) {
      dupes = [];
      return;
    }
    timer = setTimeout(async () => {
      const q = new URLSearchParams({ inn, kpp });
      if (excludeId) q.set('exclude', excludeId);
      const r = await api.get<{ matches: typeof dupes }>(`/crm/companies/check-inn?${q}`);
      dupes = r.matches;
    }, 300);
  });

  let tagText = $state(value.tags.join(', '));
  $effect(() => {
    value.tags = tagText.split(',').map((t) => t.trim()).filter(Boolean);
  });
</script>

<div class="form">
  <fieldset>
    <legend>Основное</legend>
    <div class="grid">
      <label class="field wide">Краткое наименование* <input class="input" bind:value={value.name} placeholder="ООО «Ромашка»" /></label>
      <label class="field wide">Полное наименование <input class="input" bind:value={value.full_name} /></label>
      <label class="field">ИНН
        <input class="input mono" class:bad={errors.inn} bind:value={value.inn} inputmode="numeric" maxlength="14" />
        {#if errors.inn}<span class="err">{errors.inn}</span>{/if}
      </label>
      <label class="field">КПП
        <input class="input mono" class:bad={errors.kpp} bind:value={value.kpp} maxlength="11" />
        {#if errors.kpp}<span class="err">{errors.kpp}</span>{/if}
      </label>
      <label class="field">ОГРН / ОГРНИП
        <input class="input mono" class:bad={errors.ogrn} bind:value={value.ogrn} inputmode="numeric" maxlength="17" />
        {#if errors.ogrn}<span class="err">{errors.ogrn}</span>{/if}
      </label>
      <label class="field">Руководитель <input class="input" bind:value={value.director} /></label>
      {#if dupes.length}
        <div class="dupes wide" role="status">
          {#each dupes as d}
            <div>
              {d.same_kpp ? 'Уже есть контрагент с этим ИНН и КПП:' : 'С этим ИНН есть контрагент (другой КПП — возможно, филиал):'}
              {#if d.id}<a href="/crm/companies/{d.id}" target="_blank">{d.name}</a>{:else}<i>{d.name}</i>{/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </fieldset>

  <fieldset>
    <legend>Контакты и адреса</legend>
    <div class="grid">
      <label class="field">Телефон <input class="input" bind:value={value.phone} type="tel" /></label>
      <label class="field">Email <input class="input" bind:value={value.email} type="email" /></label>
      <label class="field">Сайт <input class="input" bind:value={value.website} /></label>
      <label class="field">Метки (через запятую) <input class="input" bind:value={tagText} placeholder="заказчик, бюджет" /></label>
      <label class="field wide">Юридический адрес <input class="input" bind:value={value.legal_address} /></label>
      <label class="field wide">Почтовый адрес <input class="input" bind:value={value.postal_address} /></label>
    </div>
  </fieldset>

  <fieldset>
    <legend>Банковские реквизиты</legend>
    <div class="grid">
      <label class="field wide">Банк <input class="input" bind:value={value.bank_name} /></label>
      <label class="field">БИК
        <input class="input mono" class:bad={errors.bik} bind:value={value.bik} inputmode="numeric" maxlength="9" />
        {#if errors.bik}<span class="err">{errors.bik}</span>{/if}
      </label>
      <span></span>
      <label class="field">Расчётный счёт
        <input class="input mono" class:bad={errors.account} bind:value={value.account} inputmode="numeric" maxlength="24" />
        {#if errors.account}<span class="err">{errors.account}</span>{/if}
      </label>
      <label class="field">Корр. счёт
        <input class="input mono" class:bad={errors.corr_account} bind:value={value.corr_account} inputmode="numeric" maxlength="24" />
        {#if errors.corr_account}<span class="err">{errors.corr_account}</span>{/if}
      </label>
    </div>
  </fieldset>

  <fieldset>
    <legend>Ответственный и доступ</legend>
    <div class="grid">
      <label class="field">Ответственный
        <select class="input" bind:value={value.responsible_id} disabled={!session.crmManager && !!excludeId}>
          {#each users.filter((u) => u.is_active && u.crm_role !== 'none') as u}<option value={u.id}>{u.display_name}</option>{/each}
        </select>
      </label>
      <label class="field">Кто видит
        <select class="input" bind:value={value.visibility}>
          <option value="all">Все сотрудники с доступом к CRM</option>
          <option value="responsible">Только ответственный (и менеджеры)</option>
          <option value="group">Группа сотрудников</option>
        </select>
      </label>
      {#if value.visibility === 'group'}
        <label class="field">Группа
          <select class="input" bind:value={value.group_id}>
            <option value={null}>—</option>
            {#each groups as g}<option value={g.id}>{g.name}</option>{/each}
          </select>
        </label>
      {/if}
      <label class="field wide">Примечание <textarea class="input" rows="2" bind:value={value.notes}></textarea></label>
    </div>
  </fieldset>
</div>

<style>
  .form { display: grid; gap: 14px; }
  fieldset { border: 0; padding: 0; margin: 0; min-width: 0; }
  legend { font-weight: 600; font-size: 13px; padding-bottom: 8px; }
  .grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px 12px; }
  .wide { grid-column: span 3; }
  .bad { border-color: var(--danger); }
  .err { color: var(--danger); font-size: 12px; }
  .dupes { background: var(--warn-soft); color: var(--warn); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; display: grid; gap: 4px; }
  @media (max-width: 640px) {
    .grid { grid-template-columns: 1fr; }
    .wide { grid-column: auto; }
  }
</style>
