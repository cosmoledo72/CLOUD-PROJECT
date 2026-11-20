<script lang="ts">
  import StatTile from '#lib/ui/StatTile.svelte';
  import { onIntent } from '#lib/intent.ts';
  import Banknote from '@lucide/svelte/icons/banknote';
  import CalendarClock from '@lucide/svelte/icons/calendar-clock';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import HandCoins from '@lucide/svelte/icons/hand-coins';
  import EmptyState from '#lib/EmptyState.svelte';
  import FolderKanban from '@lucide/svelte/icons/folder-kanban';
  import Plus from '@lucide/svelte/icons/plus';
  import SearchX from '@lucide/svelte/icons/search-x';
  import { goto } from '$app/navigation';
  import { api, formatDate, formatFieldValue, localToday, nameDef, type ProjectMoney, type FieldDef, type FieldValues, type Project, type UserItem } from '#lib/api.ts';
  import Modal from '#lib/Modal.svelte';
  import RequisitesForm from '#lib/RequisitesForm.svelte';
  import { session } from '#lib/session.svelte.ts';
  import { avatarHue, daysBetween, initials, isDeadline, isMoney, money as fmtMoney, moneyShort } from '#lib/fieldLook.ts';
  import FieldValue from '#lib/projects/FieldValue.svelte';
  import FieldsMenu from '#lib/projects/FieldsMenu.svelte';
  import ProjectBadge from '#lib/projects/ProjectBadge.svelte';
  import ProjectLabelsDialog from '#lib/projects/ProjectLabelsDialog.svelte';
  import Tags from '@lucide/svelte/icons/tags';
  import { projectLabels } from '#lib/projects/projectLabels.svelte.ts';
  import LabelChip from '#lib/tasks/LabelChip.svelte';
  import { BUILTIN, MONEY_KEYS, groupable, normalize, type ListConfig } from '#lib/projects/listConfig.ts';
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import LayoutGrid from '@lucide/svelte/icons/layout-grid';
  import List from '@lucide/svelte/icons/list';
  import Users from '@lucide/svelte/icons/users';
  import ChartGantt from '@lucide/svelte/icons/chart-gantt';
  import X from '@lucide/svelte/icons/x';
  import ProjectBoard, { type BoardColumn } from '#lib/projects/ProjectBoard.svelte';
  import SummaryGantt from '#lib/projects/SummaryGantt.svelte';
  import ProjectFilters, { EMPTY_FILTER, filterCount, type ProjectFilter } from '#lib/projects/ProjectFilters.svelte';
  import type { ProjectPerson } from '#lib/api.ts';

  let projects = $state<Project[]>([]);
  /** Everyone taking part in the listed projects (from the list itself). */
  let people = $state<ProjectPerson[]>([]);
  let defs = $state<FieldDef[]>([]);
  let allDefs = $state<FieldDef[]>([]);
  let defsLoaded = $state(false);
  let users = $state<UserItem[]>([]);
  let createOpen = $state(false);
  let name = $state('');
  let description = $state('');
  let values = $state<FieldValues>({});
  let allFields = $state(false);
  let error = $state('');
  let query = $state('');

  async function load() {
    const [p, f, u] = await Promise.all([
      api.get<{ projects: Project[]; people: ProjectPerson[] }>('/projects'),
      api.get<{ fields: FieldDef[] }>('/project-fields'),
      api.get<{ users: UserItem[] }>('/users')
    ]);
    projects = p.projects;
    people = p.people;
    allDefs = f.fields;
    defs = f.fields.filter((d) => !d.is_archived);
    users = u.users;
    defsLoaded = true;
  }
  $effect(() => {
    load();
    projectLabels.load(true).catch(() => {});
  });

  // --- Project labels: chips, filter, grouping ---------------------------------------
  /** Selected labels: a project matches when it has any of them. */
  let labelsOpen = $state(false);
  let labelFilter = $state<string[]>([]);
  function toggleLabelFilter(id: string) {
    labelFilter = labelFilter.includes(id) ? labelFilter.filter((x) => x !== id) : [...labelFilter, id];
  }
  /** Labels present on at least one visible project, in catalogue order. */
  const usedLabels = $derived(projectLabels.list.filter((l) => projects.some((p) => p.label_ids.includes(l.id))));

  const hasRequired = $derived(defs.some((d) => d.is_required));
  // «Наименование объекта» is the project name: one input instead of two.
  const nd = $derived(nameDef(defs));


  function openCreate() {
    name = '';
    description = '';
    values = {};
    allFields = false;
    error = '';
    createOpen = true;
  }
  onIntent('new', () => { if (session.me?.role !== 'guest') openCreate(); });

  async function create() {
    error = '';
    // Send only filled-in values.
    const fields = Object.fromEntries(Object.entries(values).filter(([, v]) => v !== '' && v !== null && v !== false));
    try {
      const r = await api.post<{ id: string }>('/projects', { name, description, fields });
      goto(`/projects/${r.id}`);
    } catch (e) {
      error = (e as Error).message;
    }
  }


  const statusText = { active: 'Активный', paused: 'Приостановлен', closed: 'Закрыт' };
  const statusOrder: Project['status'][] = ['active', 'paused', 'closed'];
  const today = localToday();

  // --- Remembered per browser: view, status tab, collapsed groups ----------------------
  type View = 'rows' | 'cards' | 'managers' | 'labels' | 'gantt';
  function stored<T extends string>(key: string, allowed: readonly T[], dflt: T): T {
    try {
      const v = localStorage.getItem(key);
      return (allowed as readonly string[]).includes(v ?? '') ? (v as T) : dflt;
    } catch {
      return dflt;
    }
  }
  function store(key: string, v: string) {
    try { localStorage.setItem(key, v); } catch { /* storage unavailable */ }
  }
  let view = $state<View>(stored('wh_projects_view', ['rows', 'cards', 'managers', 'labels', 'gantt'] as const, 'rows'));
  function setView(v: View) {
    view = v;
    store('wh_projects_view', v);
  }
  // Narrow screens always get cards: a wide table does not fit.
  let narrow = $state(false);
  $effect(() => {
    const mq = matchMedia('(max-width: 720px)');
    narrow = mq.matches;
    const on = () => (narrow = mq.matches);
    mq.addEventListener('change', on);
    return () => mq.removeEventListener('change', on);
  });
  const shownView = $derived<View>(narrow && view === 'rows' ? 'cards' : view);
  const isBoard = $derived(shownView === 'managers' || shownView === 'labels');

  let status = $state<'all' | Project['status']>(stored('wh_projects_status', ['all', 'active', 'paused', 'closed'] as const, 'all'));
  $effect(() => store('wh_projects_status', status));
  /** Managers, members and customers chosen in «Фильтры» (remembered per browser). */
  let pfilter = $state<ProjectFilter>((() => {
    try {
      const v = JSON.parse(localStorage.getItem('wh_projects_filter') ?? 'null');
      return v && Array.isArray(v.managers) && Array.isArray(v.members) && Array.isArray(v.companies) ? v : { ...EMPTY_FILTER };
    } catch { return { ...EMPTY_FILTER }; }
  })());
  $effect(() => store('wh_projects_filter', JSON.stringify(pfilter)));
  /** Quick filter from the summary: overdue or due within 14 days. */
  let urgency = $state<'late' | 'soon' | null>(null);

  let collapsed = $state<string[]>((() => { try { return JSON.parse(localStorage.getItem('wh_projects_collapsed') ?? '[]'); } catch { return []; } })());
  function toggleGroup(id: string) {
    collapsed = collapsed.includes(id) ? collapsed.filter((x) => x !== id) : [...collapsed, id];
    store('wh_projects_collapsed', JSON.stringify(collapsed));
  }

  // --- Field settings (server, per user; portal default) ---------------------------
  let config = $state<ListConfig>({ columns: ['$name'], subtitle: null, card: [], group: null });
  let own = $state(false);
  let loadedRaw = $state<unknown>(undefined);
  let ready = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    api.get<{ value: unknown; own: boolean }>('/view-settings/projects_list')
      .then((r) => { loadedRaw = r.value; own = r.own; })
      .catch(() => { loadedRaw = null; });
  });
  // Apply once both the setting and the requisites are here (in any order).
  $effect(() => {
    if (ready || !defsLoaded || loadedRaw === undefined) return;
    config = normalize(loadedRaw, allDefs);
    ready = true;
  });
  // Save the user's changes (debounced) once loaded.
  let lastSaved = '';
  $effect(() => {
    const snap = JSON.stringify(config);
    if (!ready) { lastSaved = snap; return; }
    if (snap === lastSaved) return;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      lastSaved = snap;
      try {
        await api.put('/view-settings/projects_list', { value: JSON.parse(snap) });
        own = true;
      } catch { /* keep working with the local state */ }
    }, 600);
  });
  async function resetConfig() {
    const r = await api.del<{ value: unknown; own: boolean }>('/view-settings/projects_list');
    own = r.own;
    const next = normalize(r.value, allDefs);
    lastSaved = JSON.stringify(next);
    config = next;
  }
  let notice = $state('');
  async function makeDefault() {
    if (!confirm('Сделать текущий набор полей набором по умолчанию для всех сотрудников портала? Кто уже настроил свой, увидит свой.')) return;
    await api.put('/admin/view-settings/projects_list', { value: JSON.parse(JSON.stringify(config)) });
    notice = 'Набор полей сохранён по умолчанию для портала.';
    setTimeout(() => (notice = ''), 4000);
  }

  const defByKey = $derived(new Map(defs.map((d) => [d.key, d])));

  // --- Project money (finance module): budget, paid, due for the projects the user may see ---
  let moneyById = $state<Map<string, ProjectMoney>>(new Map());
  const hasMoney = $derived(moneyById.size > 0);
  $effect(() => {
    if (!session.has('finance') || session.me?.role === 'guest') return;
    api.get<{ projects: ProjectMoney[] }>('/finance/projects')
      .then((r) => (moneyById = new Map(r.projects.map((m) => [m.project_id, m]))))
      .catch(() => {});
  });
  const moneyVal = (p: Project, k: string): number | null => {
    const m = moneyById.get(p.id);
    if (!m) return null;
    return k === '$budget' ? m.budget : k === '$paid' ? m.paid : k === '$due' ? m.due : null;
  };
  const isMoneyCol = (k: string) => (MONEY_KEYS as readonly string[]).includes(k);
  const groupDefs = $derived(groupable(defs));

  // --- Due dates, urgency, money ---------------------------------------------------
  /** The contract deadline field (first "Срок…" date requisite), if any. */
  const dueDef = $derived(defs.find((d) => isDeadline(d)) ?? null);
  /** A project's due date: the contract deadline, else the end of its task schedule. */
  const dueOf = (p: Project): string | null => {
    const v = dueDef ? p.fields?.[dueDef.key] : null;
    return typeof v === 'string' && v ? v : p.end_date;
  };
  const urgencyOf = (p: Project): 'late' | 'soon' | null => {
    const d = dueOf(p);
    if (p.status !== 'active' || !d) return null;
    const n = daysBetween(today, d);
    return n < 0 ? 'late' : n <= 14 ? 'soon' : null;
  };
  const moneyDef = $derived(defs.find((d) => isMoney(d)) ?? null);
  const amountOf = (p: Project) => {
    const v = moneyDef ? p.fields?.[moneyDef.key] : null;
    return typeof v === 'number' ? v : 0;
  };

  // --- Filtering ---------------------------------------------------------------------
  const textOf = (v: unknown) => (v && typeof v === 'object' ? String((v as { name?: string }).name ?? '') : String(v ?? ''));
  const searched = $derived(
    projects.filter((p) => {
      const q = query.trim().toLowerCase();
      if (!q) return true;
      const people = Object.values(p.fields ?? {}).map((v) => users.find((u) => u.id === v)?.display_name ?? '');
      const marks = projectLabels.of(p.label_ids).map((l) => l.name);
      const hay = [p.name, p.description, ...Object.values(p.fields ?? {}).map(textOf), ...people, ...marks].join(' ').toLowerCase();
      return hay.includes(q);
    })
  );
  const counts = $derived({
    all: searched.length,
    active: searched.filter((p) => p.status === 'active').length,
    paused: searched.filter((p) => p.status === 'paused').length,
    closed: searched.filter((p) => p.status === 'closed').length
  });
  const late = $derived(searched.filter((p) => urgencyOf(p) === 'late'));
  const soon = $derived(searched.filter((p) => urgencyOf(p) === 'soon'));
  const toCollect = $derived(searched.filter((p) => p.status === 'active').reduce((s, p) => s + (moneyById.get(p.id)?.due ?? 0), 0));
  const inWork = $derived(searched.filter((p) => p.status === 'active').reduce((s, p) => s + amountOf(p), 0));
  const filtered = $derived(
    searched.filter(
      (p) =>
        (status === 'all' || p.status === status) &&
        (!urgency || urgencyOf(p) === urgency) &&
        (!labelFilter.length || p.label_ids.some((l) => labelFilter.includes(l))) &&
        (!pfilter.managers.length || p.manager_ids.some((m) => pfilter.managers.includes(m))) &&
        (!pfilter.members.length || p.member_ids.some((m) => pfilter.members.includes(m))) &&
        (!pfilter.companies.length || (!!p.company_name && pfilter.companies.includes(p.company_name)))
    )
  );

  // --- Filters: who and for whom ---------------------------------------------------------
  const personName = (id: string) => people.find((x) => x.id === id)?.display_name ?? '—';
  const managerPeople = $derived(people.filter((x) => projects.some((p) => p.manager_ids.includes(x.id))));
  const companies = $derived([...new Set(projects.map((p) => p.company_name).filter((c): c is string => !!c))].sort((a, b) => a.localeCompare(b, 'ru')));
  // Drop choices that are no longer in the list (a project closed, a member removed).
  $effect(() => {
    const ok = new Set(people.map((x) => x.id));
    if (!people.length) return;
    const next = {
      managers: pfilter.managers.filter((m) => ok.has(m)),
      members: pfilter.members.filter((m) => ok.has(m)),
      companies: pfilter.companies.filter((c) => companies.includes(c))
    };
    if (filterCount(next) !== filterCount(pfilter)) pfilter = next;
  });

  // --- Sorting -------------------------------------------------------------------------
  let sortKey = $state<string>('$name');
  let sortDir = $state<1 | -1>(1);
  function sortBy(k: string) {
    if (sortKey === k) sortDir = sortDir === 1 ? -1 : 1;
    else { sortKey = k; sortDir = 1; }
  }
  const progress = (p: Project) => (p.task_total ? p.task_done / p.task_total : 0);
  const coll = new Intl.Collator('ru', { numeric: true, sensitivity: 'base' });
  function sortVal(p: Project, k: string): string | number {
    switch (k) {
      case '$name': return p.name;
      case '$status': return statusOrder.indexOf(p.status);
      case '$labels': return projectLabels.of(p.label_ids).map((l) => l.name).join(', ');
      case '$progress': return progress(p);
      case '$dates': return p.end_date ?? '';
      case '$description': return p.description ?? '';
      case '$budget': case '$paid': case '$due': return moneyVal(p, k) ?? '';
      default: {
        const d = defByKey.get(k);
        const v = p.fields?.[k];
        if (!d || v == null || v === '') return '';
        if (typeof v === 'number') return v;
        if (d.field_type === 'date') return String(v);
        return formatFieldValue(d, v, users);
      }
    }
  }
  const sorted = $derived.by(() =>
    [...filtered].sort((a, b) => {
      const x = sortVal(a, sortKey);
      const y = sortVal(b, sortKey);
      if (x === '' && y !== '') return 1;
      if (y === '' && x !== '') return -1;
      const c = typeof x === 'number' && typeof y === 'number' ? x - y : coll.compare(String(x), String(y));
      return c * sortDir || coll.compare(a.name, b.name);
    })
  );

  // --- Grouping ------------------------------------------------------------------------
  type Group = { id: string; label: string; items: Project[]; sum: number };
  const groups = $derived.by((): Group[] => {
    const g = config.group;
    if (!g) return [{ id: '', label: '', items: sorted, sum: 0 }];
    const map = new Map<string, Group>();
    let order: string[] = [];
    const def = defByKey.get(g);
    if (g === '$status') order = statusOrder;
    else if (def?.field_type === 'select') order = def.options ?? [];
    if (g === '$labels') {
      // A project with several labels is listed under each of them.
      const put = (id: string, label: string, p: Project) => {
        if (!map.has(id)) map.set(id, { id, label, items: [], sum: 0 });
        const grp = map.get(id)!;
        grp.items.push(p);
        grp.sum += amountOf(p);
      };
      for (const p of sorted) {
        const ls = projectLabels.of(p.label_ids);
        if (!ls.length) put('', 'Без меток', p);
        for (const l of ls) put(l.id, l.name, p);
      }
      const pos = (id: string) => (id === '' ? 1e9 : projectLabels.list.findIndex((l) => l.id === id));
      return [...map.values()].sort((a, b) => pos(a.id) - pos(b.id));
    }
    for (const p of sorted) {
      let id: string;
      let label: string;
      if (g === '$status') { id = p.status; label = statusText[p.status]; }
      else {
        const v = p.fields?.[g];
        const t = v == null || v === '' ? '' : def ? formatFieldValue(def, v, users) : textOf(v);
        id = t === '—' ? '' : t;
        label = id || 'Не указано';
        if (def?.field_type === 'checkbox') { id = v ? 'yes' : 'no'; label = `${def.label}: ${v ? 'да' : 'нет'}`; }
      }
      if (!map.has(id)) map.set(id, { id, label, items: [], sum: 0 });
      const grp = map.get(id)!;
      grp.items.push(p);
      grp.sum += amountOf(p);
    }
    const rank = (id: string) => (id === '' ? 1e9 : order.includes(id) ? order.indexOf(id) : 1e6);
    return [...map.values()].sort((a, b) => rank(a.id) - rank(b.id) || coll.compare(a.label, b.label));
  });

  // --- Boards: by managers, by labels ----------------------------------------------------
  const boardColumns = $derived.by((): BoardColumn[] => {
    if (shownView === 'managers') {
      const cols = managerPeople
        .map((m) => ({ id: m.id, title: m.display_name, personId: m.id, items: sorted.filter((p) => p.manager_ids.includes(m.id)) }))
        .filter((c) => c.items.length);
      const none = sorted.filter((p) => !p.manager_ids.length);
      return none.length ? [...cols, { id: '', title: 'Без руководителя', items: none }] : cols;
    }
    if (shownView === 'labels') {
      const cols = projectLabels.list
        .map((l) => ({ id: l.id, title: l.name, color: l.color, items: sorted.filter((p) => p.label_ids.includes(l.id)) }))
        .filter((c) => c.items.length);
      const none = sorted.filter((p) => !p.label_ids.length);
      return none.length ? [...cols, { id: '', title: 'Без метки', items: none }] : cols;
    }
    return [];
  });

  // --- Bulk actions (rows view): managers of a project and administrators -----------------
  const canManage = (p: Project) => session.isAdmin || (!!session.me && p.manager_ids.includes(session.me.id));
  let selected = $state<string[]>([]);
  const manageable = $derived(sorted.filter(canManage));
  const selectedShown = $derived(selected.filter((id) => manageable.some((p) => p.id === id)));
  const allSelected = $derived(manageable.length > 0 && manageable.every((p) => selected.includes(p.id)));
  function toggleSelect(id: string) {
    selected = selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id];
  }
  function toggleAll() {
    selected = allSelected ? [] : manageable.map((p) => p.id);
  }
  let bulkBusy = $state(false);
  let bulkError = $state('');
  async function bulk(body: Record<string, string>, done: string) {
    if (!selectedShown.length) return;
    bulkBusy = true;
    bulkError = '';
    try {
      const r = await api.post<{ changed: number }>('/projects/bulk', { ids: selectedShown, ...body });
      selected = [];
      await load();
      notice = `${done}: ${r.changed} ${r.changed % 10 === 1 && r.changed % 100 !== 11 ? 'проект' : 'проектов'}.`;
      setTimeout(() => (notice = ''), 4000);
    } catch (e) {
      bulkError = (e as Error).message;
    } finally {
      bulkBusy = false;
    }
  }

  const labelOf = (k: string) => (k in BUILTIN ? BUILTIN[k as keyof typeof BUILTIN] : defByKey.get(k)?.label ?? k);
  const cols = $derived(config.columns.filter((k) => (k in BUILTIN && (!isMoneyCol(k) || hasMoney)) || defByKey.has(k)));
  const isRight = (k: string) => { const d = defByKey.get(k); return isMoneyCol(k) || (!!d && d.field_type === 'number'); };
  const subtitleOf = (p: Project) => {
    const k = config.subtitle;
    if (!k) return '';
    if (k === '$description') return p.description ?? '';
    const d = defByKey.get(k);
    const v = p.fields?.[k];
    return d && v != null && v !== '' ? formatFieldValue(d, v, users) : '';
  };

  // --- Cards -------------------------------------------------------------------------
  const cardKeys = $derived(config.card.filter((k) => (k in BUILTIN && (!isMoneyCol(k) || hasMoney)) || defByKey.has(k)));
  const cardPerson = $derived(cardKeys.find((k) => defByKey.get(k)?.field_type === 'user') ?? null);
  const cardDeadline = $derived(cardKeys.find((k) => { const d = defByKey.get(k); return !!d && isDeadline(d); }) ?? null);
  const cardGrid = $derived(
    cardKeys.filter((k) => k !== 'code' && k !== cardPerson && k !== cardDeadline && k !== '$progress' && k !== '$status' && k !== '$labels' && k !== '$description')
  );
  const hasCodeDef = $derived(defByKey.has('code'));
  const personOf = (p: Project) => (cardPerson ? users.find((u) => u.id === p.fields?.[cardPerson]) : undefined);
</script>

<div class="page">
  <div class="page-head">
    <h1>Проекты</h1>
    <div class="spacer"></div>
    {#if session.me?.role !== 'guest'}<button class="btn primary" onclick={openCreate}><Plus size={16} />Новый проект</button>{/if}
  </div>
  {#if projects.length}
  <div class="toolbar">
    {#if projects.length > 3}<input class="input search" placeholder="Поиск: название, шифр, заказчик, ГИП" bind:value={query} aria-label="Поиск проектов" />{/if}
    {#if projects.length}
      {#if !isBoard}
      <label class="grp-sel">
        <span class="faint small">Группировать</span>
        <select class="input" value={config.group ?? ''} onchange={(e) => (config = { ...config, group: e.currentTarget.value || null })} aria-label="Группировать">
          <option value="">нет</option>
          <option value="$status">по статусу</option>
          {#if projectLabels.list.length}<option value="$labels">по меткам</option>{/if}
          {#each groupDefs as d (d.id)}<option value={d.key}>{d.label}</option>{/each}
        </select>
      </label>
      {/if}
      <ProjectFilters bind:value={pfilter} managers={managerPeople} members={people} {companies} />
      {#if projectLabels.canCreate}<button class="btn" onclick={() => (labelsOpen = true)} title="Метки проектов"><Tags size={16} />Метки</button>{/if}
      {#if shownView === 'rows' || shownView === 'cards'}
        <FieldsMenu {defs} bind:config view={shownView} {own} isAdmin={session.isAdmin} money={hasMoney} onreset={resetConfig} onmakedefault={makeDefault} />
      {/if}
    {/if}
  </div>
  {/if}
  {#if notice}<div class="ok-box">{notice}</div>{/if}

  {#if projects.length === 0}
    <div class="card"><EmptyState icon={FolderKanban} title="Проектов пока нет"
      text="Создайте первый проект: реквизиты, задачи, график и документы будут в одном месте.">
      {#if session.me?.role !== 'guest'}<button class="btn primary" onclick={openCreate}><Plus size={16} />Новый проект</button>{/if}
    </EmptyState></div>
  {:else}
    <div class="stat-grid">
      <StatTile label="Активные проекты" value={counts.active} icon={FolderKanban} tone="accent" on={status === 'active' && !urgency}
        onclick={() => { status = 'active'; urgency = null; }} hint="в работе сейчас" />
      <StatTile label="Просрочено" value={late.length} icon={CircleAlert} tone={late.length ? 'danger' : 'neutral'} valueTone={late.length ? 'danger' : undefined}
        dim={!late.length} on={urgency === 'late'} onclick={() => (urgency = urgency === 'late' ? null : 'late')}
        title={dueDef ? `По реквизиту «${dueDef.label}», без него — по срокам задач` : 'По срокам задач'} hint="срок сдачи прошёл" />
      <StatTile label="Сдача в ближайшие 14 дней" value={soon.length} icon={CalendarClock} tone={soon.length ? 'warn' : 'neutral'} valueTone={soon.length ? 'warn' : undefined}
        dim={!soon.length} on={urgency === 'soon'} onclick={() => (urgency = urgency === 'soon' ? null : 'soon')} hint="по сроку сдачи" />
      {#if hasMoney && toCollect > 0}
        <StatTile label="К получению от заказчиков" value={moneyShort(toCollect)} icon={HandCoins} tone="ok" hint="бюджет минус поступления"
          title="Остаток к оплате по активным проектам: бюджет − поступления" />
      {/if}
      {#if moneyDef}
        <StatTile label="В работе по договорам" value={moneyShort(inWork)} icon={Banknote} tone="neutral" hint="«{moneyDef.label}» активных проектов"
          title="Сумма «{moneyDef.label}» по активным проектам" />
      {/if}
    </div>

    <div class="tabs" role="tablist" aria-label="Статус">
      {#each [['all', 'Все'], ['active', 'Активные'], ['paused', 'Приостановленные'], ['closed', 'Закрытые']] as [k, l] (k)}
        <button role="tab" aria-selected={status === k} class:on={status === k} onclick={() => (status = k as typeof status)}>
          {l} <span class="n">{counts[k as keyof typeof counts]}</span>
        </button>
      {/each}
      {#if urgency}
        <button class="chip-x {urgency}" onclick={() => (urgency = null)}>{urgency === 'late' ? 'Только просроченные' : 'Только со сдачей ≤ 14 дн.'} ✕</button>
      {/if}
      {#if usedLabels.length}
        <span class="lf" role="group" aria-label="Фильтр по меткам">
          {#each usedLabels as l (l.id)}
            <button class="lf-b" class:on={labelFilter.includes(l.id)} aria-pressed={labelFilter.includes(l.id)} onclick={() => toggleLabelFilter(l.id)} title="Показать проекты с меткой «{l.name}»">
              <LabelChip label={l} />
            </button>
          {/each}
          {#if labelFilter.length}<button class="lf-x" onclick={() => (labelFilter = [])} title="Сбросить фильтр по меткам">✕</button>{/if}
        </span>
      {/if}
      <span class="tabs-sp"></span>
      <div class="seg views" role="radiogroup" aria-label="Вид">
        {#if !narrow}<button role="radio" aria-checked={view === 'rows'} class:on={view === 'rows'} title="Строками" onclick={() => setView('rows')}><List size={16} /></button>{/if}
        <button role="radio" aria-checked={shownView === 'cards'} class:on={shownView === 'cards'} title="Карточками" onclick={() => setView('cards')}><LayoutGrid size={16} /></button>
        <button role="radio" aria-checked={view === 'managers'} class:on={view === 'managers'} title="По руководителям проектов" onclick={() => setView('managers')}><Users size={16} /></button>
        {#if projectLabels.list.length}<button role="radio" aria-checked={view === 'labels'} class:on={view === 'labels'} title="По меткам" onclick={() => setView('labels')}><Tags size={16} /></button>{/if}
        <button role="radio" aria-checked={view === 'gantt'} class:on={view === 'gantt'} title="Сводный Гант" onclick={() => setView('gantt')}><ChartGantt size={16} /></button>
      </div>
    </div>

    {#if filterCount(pfilter)}
      <div class="fchips">
        {#each pfilter.managers as m (m)}<button class="fchip" onclick={() => (pfilter = { ...pfilter, managers: pfilter.managers.filter((x) => x !== m) })}>Руководитель: {personName(m)} <X size={12} /></button>{/each}
        {#each pfilter.members as m (m)}<button class="fchip" onclick={() => (pfilter = { ...pfilter, members: pfilter.members.filter((x) => x !== m) })}>Участник: {personName(m)} <X size={12} /></button>{/each}
        {#each pfilter.companies as c (c)}<button class="fchip" onclick={() => (pfilter = { ...pfilter, companies: pfilter.companies.filter((x) => x !== c) })}>Заказчик: {c} <X size={12} /></button>{/each}
        <button class="fclear" onclick={() => (pfilter = { managers: [], members: [], companies: [] })}>Сбросить</button>
      </div>
    {/if}

    {#if !sorted.length}
      <div class="card"><EmptyState icon={SearchX} title="Ничего не найдено" text="Измените условия поиска или фильтры." /></div>
    {:else if isBoard}
      <ProjectBoard columns={boardColumns} {dueOf} {urgencyOf} {amountOf} showSum={!!moneyDef} hideLabelId={shownView === 'labels'} />
    {:else if shownView === 'gantt'}
      <SummaryGantt {groups} grouped={!!config.group} {dueOf} {urgencyOf} {personName} {today} />
    {:else if shownView === 'rows'}
      {#snippet th(k: string)}
        <th class:right={isRight(k)} class:c-name={k === '$name'} aria-sort={sortKey === k ? (sortDir === 1 ? 'ascending' : 'descending') : 'none'}>
          <button class="sort" onclick={() => sortBy(k)}>{k === 'code' ? 'Шифр' : labelOf(k)}
            {#if sortKey === k}{#if sortDir === 1}<ArrowUp size={12} />{:else}<ArrowDown size={12} />{/if}{/if}
          </button>
        </th>
      {/snippet}
      <div class="card table">
        <table class="rows">
          <thead><tr>
            {#if manageable.length}<th class="c-sel"><input type="checkbox" checked={allSelected} indeterminate={!allSelected && selectedShown.length > 0} onchange={toggleAll} aria-label="Выбрать все проекты" /></th>{/if}
            {#each cols as k (k)}{@render th(k)}{/each}
          </tr></thead>
          {#each groups as g (g.id)}
            <tbody>
              {#if config.group}
                {@const shut = collapsed.includes(g.id)}
                <tr class="ghead">
                  <td colspan={cols.length + (manageable.length ? 1 : 0)}>
                    <button class="gbtn" onclick={() => toggleGroup(g.id)} aria-expanded={!shut}>
                      <span class="chev" class:open={!shut}><ChevronRight size={14} /></span>{g.label} <span class="n">{g.items.length}</span>
                      {#if moneyDef && g.sum}<span class="gsum">{moneyShort(g.sum)}</span>{/if}
                    </button>
                  </td>
                </tr>
              {/if}
              {#if !config.group || !collapsed.includes(g.id)}
                {#each g.items as p (p.id)}
                  {@const pct = Math.round(progress(p) * 100)}
                  {@const u = urgencyOf(p)}
                  <tr class="prow" class:closed={p.status === 'closed'} class:sel={selected.includes(p.id)} onclick={(e) => { if (!(e.target as HTMLElement).closest('a,button,.c-sel')) goto(`/projects/${p.id}`); }}>
                    {#if manageable.length}
                      <td class="c-sel">{#if canManage(p)}<input type="checkbox" checked={selected.includes(p.id)} onchange={() => toggleSelect(p.id)} aria-label="Выбрать «{p.name}»" />{/if}</td>
                    {/if}
                    {#each cols as k (k)}
                      {#if k === '$name'}
                        <td class="c-name">
                          <div class="nm-row">
                            <ProjectBadge id={p.id} name={p.name} color={p.badge_color} icon={p.badge_icon} size={26} />
                            <div class="nm-main">
                              <a href="/projects/{p.id}" title={p.description || p.name}>{p.name}</a>
                              {#if subtitleOf(p)}<div class="subl">{subtitleOf(p)}</div>{/if}
                              {#if !cols.includes('$labels') && p.label_ids.length}
                                <div class="marks">{#each projectLabels.of(p.label_ids) as l (l.id)}<LabelChip label={l} small />{/each}</div>
                              {/if}
                            </div>
                          </div>
                        </td>
                      {:else if k === '$labels'}
                        <td class="c-labels"><div class="marks">{#each projectLabels.of(p.label_ids) as l (l.id)}<LabelChip label={l} small />{:else}<span class="faint">—</span>{/each}</div></td>
                      {:else if k === '$status'}
                        <td><span class="st st-{p.status}"><i></i>{statusText[p.status]}</span></td>
                      {:else if k === '$progress'}
                        <td class="c-prog">
                          <span class="prog" title="Выполнено {pct}%">
                            <span class="ring" style="--p:{pct}"></span><span class="nums">{p.task_done}/{p.task_total}</span>
                          </span>
                        </td>
                      {:else if k === '$dates'}
                        <td class="small nums dates">
                          {#if p.start_date || p.end_date}{formatDate(p.start_date)} —<br /><span class:late={u === 'late' && !dueDef}>{formatDate(p.end_date)}</span>{:else}<span class="faint">—</span>{/if}
                        </td>
                      {:else if k === '$description'}
                        <td class="small c-desc">{p.description}</td>
                      {:else if isMoneyCol(k)}
                        {@const v = moneyVal(p, k)}
                        <td class="c-f right nums" class:due={k === '$due' && !!v}>{#if v != null}{fmtMoney(v)}{:else}<span class="faint">—</span>{/if}</td>
                      {:else if k === 'code' && p.fields?.code}
                        <td class="c-code"><span class="code mono">{p.fields.code}</span></td>
                      {:else}
                        {@const d = defByKey.get(k)}
                        <td class="c-f" class:right={isRight(k)}>{#if d}<FieldValue def={d} value={p.fields?.[k]} {users} {today} active={p.status === 'active'} />{/if}</td>
                      {/if}
                    {/each}
                  </tr>
                {/each}
              {/if}
            </tbody>
          {/each}
        </table>
      </div>
      {#if selectedShown.length}
        <div class="bulk" role="region" aria-label="Действия с выбранными проектами">
          <b>Выбрано: {selectedShown.length}</b>
          <select class="input" disabled={bulkBusy} aria-label="Сменить статус" onchange={(e) => { const v = e.currentTarget.value; e.currentTarget.value = ''; if (v) bulk({ status: v }, 'Статус изменён'); }}>
            <option value="">Сменить статус…</option>
            <option value="active">Активный</option>
            <option value="paused">Приостановлен</option>
            <option value="closed">Закрыт</option>
          </select>
          {#if projectLabels.list.length}
            <select class="input" disabled={bulkBusy} aria-label="Добавить метку" onchange={(e) => { const v = e.currentTarget.value; e.currentTarget.value = ''; if (v) bulk({ add_label: v }, 'Метка добавлена'); }}>
              <option value="">Добавить метку…</option>
              {#each projectLabels.list as l (l.id)}<option value={l.id}>{l.name}</option>{/each}
            </select>
            <select class="input" disabled={bulkBusy} aria-label="Снять метку" onchange={(e) => { const v = e.currentTarget.value; e.currentTarget.value = ''; if (v) bulk({ remove_label: v }, 'Метка снята'); }}>
              <option value="">Снять метку…</option>
              {#each projectLabels.list as l (l.id)}<option value={l.id}>{l.name}</option>{/each}
            </select>
          {/if}
          <span class="spacer"></span>
          {#if bulkError}<span class="berr">{bulkError}</span>{/if}
          <button class="btn" onclick={() => { selected = []; bulkError = ''; }}>Снять выделение</button>
        </div>
      {/if}
    {:else}
      {#each groups as g (g.id)}
        {#if config.group}
          {@const shut = collapsed.includes(g.id)}
          <button class="gbtn cards-g" onclick={() => toggleGroup(g.id)} aria-expanded={!shut}>
            <span class="chev" class:open={!shut}><ChevronRight size={14} /></span>{g.label} <span class="n">{g.items.length}</span>
            {#if moneyDef && g.sum}<span class="gsum">{moneyShort(g.sum)}</span>{/if}
          </button>
        {/if}
        {#if !config.group || !collapsed.includes(g.id)}
          <div class="grid">
            {#each g.items as p (p.id)}
              {@const pct = Math.round(progress(p) * 100)}
              {@const u = urgencyOf(p)}
              {@const person = personOf(p)}
              <a class="card pc tone-{p.status === 'active' ? (u ?? 'ok') : p.status}" href="/projects/{p.id}">
                <div class="in">
                  <div class="top">
                    {#if hasCodeDef && cardKeys.includes('code') && p.fields?.code}<span class="code mono">{p.fields.code}</span>{/if}
                    {#if p.status !== 'active'}<span class="st st-{p.status}"><i></i>{statusText[p.status]}</span>{/if}
                    <span class="spacer"></span>
                    {#if cardDeadline && defByKey.get(cardDeadline)}
                      <FieldValue def={defByKey.get(cardDeadline)!} value={p.fields?.[cardDeadline]} {users} {today} active={p.status === 'active'} short />
                    {/if}
                  </div>
                  <div class="ttl-row">
                    <ProjectBadge id={p.id} name={p.name} color={p.badge_color} icon={p.badge_icon} size={30} />
                    <h3>{p.name}</h3>
                  </div>
                  {#if p.label_ids.length}
                    <div class="marks">{#each projectLabels.of(p.label_ids) as l (l.id)}<LabelChip label={l} small />{/each}</div>
                  {/if}
                  {#if cardKeys.includes('$description') && p.description}<p class="muted desc">{p.description}</p>{/if}
                  {#if cardGrid.length}
                    <dl class="kv">
                      {#each cardGrid as k (k)}
                        <div>
                          <dt>{labelOf(k)}</dt>
                          <dd>
                            {#if k === '$dates'}{#if p.start_date || p.end_date}{formatDate(p.start_date)} — {formatDate(p.end_date)}{:else}<span class="faint">—</span>{/if}
                            {:else if isMoneyCol(k)}{#if moneyVal(p, k) != null}<span class="nums">{fmtMoney(moneyVal(p, k))}</span>{:else}<span class="faint">—</span>{/if}
                            {:else if defByKey.get(k)}<FieldValue def={defByKey.get(k)!} value={p.fields?.[k]} {users} {today} active={p.status === 'active'} short />{/if}
                          </dd>
                        </div>
                      {/each}
                    </dl>
                  {/if}
                </div>
                {#if person || cardKeys.includes('$progress')}
                  <div class="foot">
                    {#if person}
                      <span class="av" style="--h:{avatarHue(person.id)}">{initials(person.display_name)}</span>
                      <span class="pname" title={labelOf(cardPerson ?? '')}>{person.display_name}</span>
                    {/if}
                    <span class="spacer"></span>
                    {#if cardKeys.includes('$progress')}
                      <span class="lin" title="Выполнено {pct}%"><i style="width:{pct}%"></i></span>
                      <span class="nums">{p.task_done}/{p.task_total}</span>
                    {/if}
                  </div>
                {/if}
              </a>
            {/each}
          </div>
        {/if}
      {/each}
    {/if}
  {/if}
</div>

<ProjectLabelsDialog bind:open={labelsOpen} onchange={load} />

<Modal bind:open={createOpen} title="Новый проект" width={defs.length ? 640 : 480}>
  {#if nd}
    <label class="field">{nd.label}<span class="faint small">— это и название проекта</span>
      <textarea class="input" rows="2" bind:value={name}></textarea>
    </label>
  {:else}
    <label class="field">Название <input class="input" bind:value={name} /></label>
  {/if}
  <label class="field">Описание <textarea class="input" rows="2" bind:value={description}></textarea></label>
  {#if defs.length}
    <div class="req-head row">
      <h3>Реквизиты</h3>
      <div class="spacer"></div>
      {#if hasRequired}
        <label class="check small"><input type="checkbox" bind:checked={allFields} />Заполнить все</label>
      {/if}
    </div>
    <RequisitesForm {defs} bind:values {users} requiredOnly={hasRequired && !allFields} exclude={nd ? [nd.key] : []} />
    {#if !allFields && hasRequired}<p class="faint small">Остальные реквизиты можно заполнить позже на вкладке «Реквизиты» проекта.</p>{/if}
  {/if}
  {#if error}<div class="error-box">{error}</div>{/if}
  {#snippet footer()}
    <button class="btn" onclick={() => (createOpen = false)}>Отмена</button>
    <button class="btn primary" disabled={!name.trim()} onclick={create}>Создать</button>
  {/snippet}
</Modal>

<style>
  .search { width: 260px; }
  .grp-sel { display: flex; align-items: center; gap: 6px; }
  .grp-sel .input { width: auto; max-width: 180px; }
  .ok-box { margin-bottom: 12px; }
  .empty { padding: 40px; text-align: center; }
  p { margin: 0; }
  .req-head h3 { margin: 0; }
  /* Search and view tools under the title (the main button stays on the title line). */
  .toolbar { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; margin: -8px 0 14px; }

  /* Status tabs */
  .tabs { display: flex; gap: 4px; align-items: center; flex-wrap: wrap; margin-bottom: 12px; }
  .tabs button[role='tab'] { border: 0; background: none; padding: 5px 12px; border-radius: 999px; font: inherit; font-size: 13.5px; color: var(--text-2); cursor: pointer; display: inline-flex; gap: 6px; align-items: center; }
  .tabs button[role='tab']:hover { background: var(--surface-2); }
  .tabs button[role='tab'].on { background: var(--accent); color: var(--on-accent); }
  .tabs .n { font-size: 11.5px; opacity: .7; font-variant-numeric: tabular-nums; }
  .chip-x { margin-left: 6px; border: 0; border-radius: 999px; padding: 4px 10px; font: inherit; font-size: 12.5px; cursor: pointer; }
  .lf { display: inline-flex; flex-wrap: wrap; gap: 4px; align-items: center; margin-left: 10px; padding-left: 12px; border-left: 1px solid var(--border); }
  .lf-b { border: 1px solid transparent; background: none; padding: 1px; border-radius: 999px; cursor: pointer; display: inline-flex; opacity: .75; }
  .lf-b:hover { opacity: 1; }
  .lf-b.on { opacity: 1; border-color: var(--text-2); }
  .lf-x { border: 0; background: none; color: var(--text-3); cursor: pointer; font-size: 12px; padding: 2px 6px; }
  .nm-row { display: flex; align-items: flex-start; gap: 10px; }
  .nm-main { min-width: 0; }
  .marks { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 4px; }
  .c-labels .marks { margin-top: 0; }
  .ttl-row { display: flex; align-items: flex-start; gap: 10px; }
  .ttl-row h3 { margin: 0; flex: 1; min-width: 0; }
  .chip-x.late { background: var(--danger-soft); color: var(--danger); }
  .chip-x.soon { background: var(--warn-soft); color: var(--warn); }

  /* Filters, bulk */
  .fchips { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; margin: -4px 0 12px; }
  .fchip { display: inline-flex; align-items: center; gap: 5px; border: 1px solid var(--border); background: var(--surface); border-radius: 999px;
           padding: 3px 10px; font: inherit; font-size: 12.5px; color: var(--text-2); cursor: pointer; }
  .fchip:hover { border-color: var(--accent); color: var(--text); }
  .fclear { border: 0; background: none; color: var(--accent); font: inherit; font-size: 12.5px; cursor: pointer; }
  .c-sel { width: 1%; padding-right: 0 !important; }
  .rows th.c-sel { padding: 9px 0 9px 12px; }
  .rows td.c-sel { padding-left: 12px; }
  .c-sel input { width: 16px; height: 16px; cursor: pointer; vertical-align: middle; }
  .prow.sel { background: color-mix(in srgb, var(--accent) 7%, transparent); }
  .bulk { position: sticky; bottom: 12px; z-index: 5; margin-top: 12px; display: flex; flex-wrap: wrap; align-items: center; gap: 10px;
          background: var(--surface); border: 1px solid var(--accent); border-radius: 12px; padding: 10px 14px; box-shadow: 0 10px 30px rgba(0, 0, 0, .14); }
  .bulk .input { width: auto; }
  .bulk .spacer { flex: 1; }
  .berr { color: var(--danger); font-size: 13px; }
  .views button { padding: 5px 9px; }
  .tabs-sp { flex: 1; }

  /* Table */
  .table { padding: 0; overflow-x: auto; }
  .rows { width: 100%; border-collapse: collapse; font-size: 13.5px; }
  .rows th { text-align: left; font-weight: 500; color: var(--text-3); padding: 0; border-bottom: 1px solid var(--border); background: var(--surface-2); white-space: nowrap; position: sticky; top: 0; z-index: 1; }
  .rows th.right .sort { margin-left: auto; }
  .rows th.right { text-align: right; }
  .sort { border: 0; background: none; font: inherit; font-size: 12.5px; color: inherit; padding: 9px 10px; cursor: pointer; display: inline-flex; gap: 4px; align-items: center; }
  .sort:hover { color: var(--text); }
  .rows td { padding: 9px 10px; border-bottom: 1px solid var(--border); vertical-align: middle; }
  .prow { cursor: pointer; }
  .prow:hover { background: var(--surface-2); }
  .prow.closed td { color: var(--text-3); }
  .rows tbody:last-child tr:last-child td { border-bottom: 0; }
  .c-name { min-width: 220px; max-width: 420px; line-height: 1.35; }
  .c-name a { color: var(--text); font-weight: 600; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .prow.closed .c-name a { color: var(--text-2); }
  .subl { font-size: 12px; color: var(--text-3); margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 420px; }
  .c-f { max-width: 220px; line-height: 1.35; }
  .c-f.right { text-align: right; }
  .c-desc { max-width: 280px; color: var(--text-2); }
  .c-code { width: 1%; white-space: nowrap; }
  .code { background: var(--surface-2); border: 1px solid var(--border); padding: 1px 6px; border-radius: 4px; color: var(--text-2); font-size: 12px; }
  .c-prog { width: 1%; white-space: nowrap; }
  .prog { display: inline-flex; align-items: center; gap: 7px; font-size: 12px; color: var(--text-3); }
  .ring { width: 22px; height: 22px; border-radius: 50%; flex: none; display: inline-grid; place-items: center;
          background: conic-gradient(var(--ok) calc(var(--p) * 1%), var(--surface-2) 0); }
  .ring::after { content: ''; width: 14px; height: 14px; border-radius: 50%; background: var(--surface); }
  .prow:hover .ring::after { background: var(--surface-2); }
  .dates { line-height: 1.35; color: var(--text-2); white-space: nowrap; }
  .nums { font-variant-numeric: tabular-nums; white-space: nowrap; }
  .late { color: var(--danger); font-weight: 500; }
  td.due { color: var(--warn); font-weight: 500; }
  .st { display: inline-flex; align-items: center; gap: 6px; padding: 1px 9px 1px 7px; border-radius: 999px; font-size: 12px; font-weight: 500; white-space: nowrap; }
  .st i { width: 7px; height: 7px; border-radius: 50%; background: currentColor; }
  .st-active { background: var(--ok-soft); color: var(--ok); }
  .st-paused { background: var(--warn-soft); color: var(--warn); }
  .st-closed { background: var(--surface-2); color: var(--text-3); }

  /* Groups */
  .ghead td { background: var(--surface); padding: 0; border-bottom: 1px solid var(--border); }
  .gbtn { border: 0; background: none; font: inherit; font-size: 13px; font-weight: 600; color: var(--text-2); padding: 8px 10px; cursor: pointer;
          display: flex; align-items: center; gap: 6px; width: 100%; text-align: left; }
  .gbtn .n { font-weight: 400; color: var(--text-3); font-size: 12px; }
  .gsum { margin-left: auto; font-weight: 500; font-size: 12.5px; color: var(--text-2); font-variant-numeric: tabular-nums; }
  .chev { display: grid; place-items: center; transition: transform .15s; color: var(--text-3); }
  .chev.open { transform: rotate(90deg); }
  .cards-g { padding: 10px 2px 8px; }

  /* Cards */
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 12px; margin-bottom: 4px; }
  .pc { padding: 0; color: var(--text); display: flex; flex-direction: column; overflow: hidden; border-top: 3px solid var(--ok); }
  .pc:hover { text-decoration: none; border-color: var(--accent); border-top-color: var(--accent); }
  .pc.tone-soon { border-top-color: var(--warn); }
  .pc.tone-late { border-top-color: var(--danger); }
  .pc.tone-paused { border-top-color: var(--warn); }
  .pc.tone-closed { border-top-color: var(--border-strong); }
  .pc.tone-closed h3 { color: var(--text-2); }
  .in { padding: 13px 16px 12px; display: grid; gap: 9px; align-content: start; flex: 1; }
  .top { display: flex; align-items: center; gap: 8px; min-height: 22px; }
  .top .spacer { flex: 1; }
  .pc h3 { margin: 0; font-size: 15px; line-height: 1.3; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  .desc { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; font-size: 13px; }
  .kv { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px 14px; margin: 0; }
  .kv div { min-width: 0; }
  .kv dt { font-size: 11px; text-transform: uppercase; letter-spacing: .04em; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .kv dd { margin: 1px 0 0; font-size: 13.5px; font-weight: 500; min-width: 0; overflow: hidden; }
  .foot { display: flex; align-items: center; gap: 8px; border-top: 1px solid var(--border); padding: 9px 16px; font-size: 12.5px; color: var(--text-2); }
  .foot .spacer { flex: 1; }
  .av { width: 24px; height: 24px; border-radius: 50%; flex: none; display: inline-grid; place-items: center; font-size: 10px; font-weight: 600; color: #fff; background: hsl(var(--h) 42% 42%); }
  .pname { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .lin { width: 80px; height: 6px; border-radius: 3px; background: var(--surface-2); overflow: hidden; flex: none; }
  .lin i { display: block; height: 100%; background: var(--ok); }

  @media (max-width: 720px) {
    .search { width: 100%; order: 10; }
    .grp-sel span { display: none; }
    .grid { grid-template-columns: 1fr; }
  }
</style>
