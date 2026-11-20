<script lang="ts">
  import { api, docKindLabel, vatLabel, type DocKind, type OrgRequisites } from '#lib/api.ts';
  import { accountValid, innValid, ogrnValid } from '#lib/crm/validate.ts';
  import { session } from '#lib/session.svelte.ts';

  let r = $state<OrgRequisites | null>(null);
  let vatRates = $state<string[]>([]);
  let saved = $state('');
  let error = $state('');
  let notice = $state('');

  type Num = { kind: DocKind; label: string; pattern: string; next: number; example: string; year: number };
  let numbering = $state<Num[]>([]);
  let numSaved = $state('');
  let numError = $state('');
  let numNotice = $state('');

  async function load() {
    const [o, n] = await Promise.all([
      api.get<{ requisites: OrgRequisites; vat_rates: string[] }>('/admin/organization'),
      api.get<{ numbering: Num[] }>('/admin/numbering')
    ]);
    r = o.requisites;
    vatRates = o.vat_rates;
    saved = JSON.stringify(r);
    numbering = n.numbering;
    numSaved = JSON.stringify(numbering);
  }
  $effect(() => {
    load().catch((e) => (error = e.message));
  });

  const dirty = $derived(!!r && JSON.stringify(r) !== saved);
  const digits = (s: string) => s.replace(/\s/g, '');
  const errs = $derived.by(() => {
    const e: Record<string, string> = {};
    if (!r) return e;
    const inn = digits(r.inn), ogrn = digits(r.ogrn), bik = digits(r.bik), acc = digits(r.account), corr = digits(r.corr_account);
    if (inn && !innValid(inn)) e.inn = 'Неверный ИНН: проверьте цифры';
    if (digits(r.kpp) && !/^\d{4}[\dA-Z]{2}\d{3}$/.test(digits(r.kpp))) e.kpp = 'КПП — 9 знаков';
    if (ogrn && !ogrnValid(ogrn)) e.ogrn = 'Неверный ОГРН';
    if (bik && !/^\d{9}$/.test(bik)) e.bik = 'БИК — 9 цифр';
    if (acc && (!/^\d{9}$/.test(bik) || !accountValid(bik, acc, false))) e.account = 'Счёт не сходится с БИК';
    if (corr && (!/^\d{9}$/.test(bik) || !accountValid(bik, corr, true))) e.corr_account = 'Корр. счёт не сходится с БИК';
    return e;
  });

  async function save() {
    if (!r) return;
    error = '';
    notice = '';
    try {
      await api.put('/admin/organization', r);
      await load();
      notice = 'Реквизиты сохранены.';
    } catch (e) {
      error = (e as Error).message;
    }
  }

  // Preview of a number pattern with the next number.
  function preview(pattern: string, n: number): string {
    const d = new Date();
    return pattern.replace(/\{(n(?::(\d))?|yyyy|yy|mm)\}/g, (_m, tok: string, w?: string) => {
      if (tok === 'yyyy') return String(d.getFullYear());
      if (tok === 'yy') return String(d.getFullYear() % 100).padStart(2, '0');
      if (tok === 'mm') return String(d.getMonth() + 1).padStart(2, '0');
      return w ? String(n).padStart(+w, '0') : String(n);
    });
  }
  const numDirty = $derived(JSON.stringify(numbering) !== numSaved);
  const numValid = $derived(numbering.every((x) => x.pattern.includes('{n') && x.pattern.length <= 60 && x.next >= 1));
  async function saveNumbering() {
    numError = '';
    numNotice = '';
    const before: Num[] = JSON.parse(numSaved);
    try {
      await api.put(
        '/admin/numbering',
        numbering.map((x) => {
          const old = before.find((b) => b.kind === x.kind);
          return { kind: x.kind, pattern: x.pattern, next: old && old.next !== x.next ? Number(x.next) : null };
        })
      );
      await load();
      numNotice = 'Нумерация сохранена.';
    } catch (e) {
      numError = (e as Error).message;
    }
  }

</script>

{#if r}
  <section class="card box">
    <div class="head">
      <h2>Реквизиты организации</h2>
      <p class="muted">Подставляются в счета, акты, договоры и КП как реквизиты исполнителя (метки <span class="mono">{'{{org.*}}'}</span>).</p>
    </div>
    {#if error}<div class="error-box">{error}</div>{/if}
    {#if notice}<div class="ok-box">{notice}</div>{/if}
    <div class="grid">
      <label class="field wide">Полное наименование
        <input class="input" bind:value={r.full_name} placeholder={'Общество с ограниченной ответственностью «…»'} />
        <span class="faint small">Краткое наименование — название портала: {session.me?.tenant_name}</span>
      </label>
      <label class="field">ИНН <input class="input mono" bind:value={r.inn} aria-invalid={!!errs.inn} />{#if errs.inn}<span class="err">{errs.inn}</span>{/if}</label>
      <label class="field">КПП <input class="input mono" bind:value={r.kpp} aria-invalid={!!errs.kpp} />{#if errs.kpp}<span class="err">{errs.kpp}</span>{/if}</label>
      <label class="field">ОГРН <input class="input mono" bind:value={r.ogrn} aria-invalid={!!errs.ogrn} />{#if errs.ogrn}<span class="err">{errs.ogrn}</span>{/if}</label>
      <label class="field">Ставка НДС по умолчанию
        <select class="input" bind:value={r.vat_rate}>
          {#each vatRates as v}<option value={v}>{v === 'none' ? 'Без НДС (не плательщик / УСН)' : vatLabel(v)}</option>{/each}
        </select>
      </label>
      <label class="field wide">Юридический адрес <input class="input" bind:value={r.legal_address} /></label>
      <label class="field wide">Почтовый адрес <input class="input" bind:value={r.postal_address} placeholder="если отличается" /></label>
      <label class="field">Телефон <input class="input" bind:value={r.phone} /></label>
      <label class="field">Email <input class="input" type="email" bind:value={r.email} /></label>
      <label class="field">Сайт <input class="input" bind:value={r.website} /></label>
      <span></span>
      <label class="field">Должность руководителя <input class="input" bind:value={r.director_title} /></label>
      <label class="field">Руководитель <input class="input" bind:value={r.director} placeholder="Иванов И. И." /></label>
      <label class="field">Главный бухгалтер <input class="input" bind:value={r.accountant} /></label>
      <span></span>
      <label class="field wide">Банк <input class="input" bind:value={r.bank_name} /></label>
      <label class="field">БИК <input class="input mono" bind:value={r.bik} aria-invalid={!!errs.bik} />{#if errs.bik}<span class="err">{errs.bik}</span>{/if}</label>
      <label class="field">Расчётный счёт <input class="input mono" bind:value={r.account} aria-invalid={!!errs.account} />{#if errs.account}<span class="err">{errs.account}</span>{/if}</label>
      <label class="field">Корреспондентский счёт <input class="input mono" bind:value={r.corr_account} aria-invalid={!!errs.corr_account} />{#if errs.corr_account}<span class="err">{errs.corr_account}</span>{/if}</label>
    </div>
    <div class="row">
      <div class="spacer"></div>
      <button class="btn" disabled={!dirty} onclick={load}>Отменить</button>
      <button class="btn primary" disabled={!dirty || Object.keys(errs).length > 0} onclick={save}>Сохранить</button>
    </div>
  </section>

  <section class="card box">
    <div class="head">
      <h2>Нумерация документов продаж</h2>
      <p class="muted">
        Номер присваивается при создании документа; счётчик каждого вида начинается заново с 1 января.
        В формате: <span class="mono">{'{n}'}</span> — порядковый номер, <span class="mono">{'{n:3}'}</span> — с нулями (007),
        <span class="mono">{'{yyyy}'}</span>, <span class="mono">{'{yy}'}</span> — год, <span class="mono">{'{mm}'}</span> — месяц.
      </p>
    </div>
    {#if numError}<div class="error-box">{numError}</div>{/if}
    {#if numNotice}<div class="ok-box">{numNotice}</div>{/if}
    <table class="list numbering">
      <thead><tr><th>Документ</th><th>Формат номера</th><th>Следующий номер в {numbering[0]?.year} г.</th><th>Будет выглядеть так</th></tr></thead>
      <tbody>
        {#each numbering as x (x.kind)}
          <tr>
            <td>{docKindLabel[x.kind]}</td>
            <td><input class="input mono pat" bind:value={x.pattern} maxlength="60" aria-label="Формат: {x.label}" /></td>
            <td><input class="input num" type="number" min="1" bind:value={x.next} aria-label="Следующий номер: {x.label}" /></td>
            <td class="mono example">{x.pattern.includes('{n') ? preview(x.pattern, x.next) : '— нужен {n}'}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="faint small">Чтобы продолжить нумерацию из бухгалтерской программы, укажите следующий номер — например, 128.</p>
    <div class="row">
      <div class="spacer"></div>
      <button class="btn" disabled={!numDirty} onclick={load}>Отменить</button>
      <button class="btn primary" disabled={!numDirty || !numValid} onclick={saveNumbering}>Сохранить</button>
    </div>
  </section>
{:else if error}
  <div class="error-box">{error}</div>
{/if}

<style>
  .box { padding: 18px; margin-bottom: 14px; display: grid; gap: 12px; }
  .head h2 { margin: 0 0 4px; }
  .head p { margin: 0; max-width: 760px; }
  .grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 10px 14px; }
  .wide { grid-column: span 2; }
  .err { color: var(--danger); font-size: 12px; }
  .pat { width: 200px; }
  .num { width: 110px; }
  p { margin: 0; }
  .ok-box, .error-box { margin: 0; }
  @media (max-width: 1100px) { .grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  @media (max-width: 600px) { .grid { grid-template-columns: 1fr; } .wide { grid-column: auto; } }
  /* Phone: one block per document kind — name and example on top, the two inputs below. */
  @media (max-width: 640px) {
    .numbering thead { display: none; }
    .numbering, .numbering tbody { display: block; }
    .numbering tr { display: grid; grid-template-columns: minmax(0, 1fr) 96px; gap: 6px 8px; padding: 10px 0; border-bottom: 1px solid var(--border); }
    .numbering td { padding: 0; border: 0; }
    .numbering td:first-child { font-weight: 500; }
    .numbering .example { grid-area: 1 / 2; text-align: right; color: var(--text-2); }
    .numbering td:nth-child(2) { grid-area: 2 / 1; }
    .numbering td:nth-child(3) { grid-area: 2 / 2; }
    .numbering .pat, .numbering .num { width: 100%; }
    .numbering tbody tr:hover td { background: none; }
  }
</style>
