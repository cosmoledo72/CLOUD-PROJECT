<script lang="ts">
  import GanttExportDialog from '$lib/projects/GanttExportDialog.svelte';
  import Download from '@lucide/svelte/icons/download';
  import ProjectFinance from '$lib/finance/ProjectFinance.svelte';
  import Wallet from '@lucide/svelte/icons/wallet';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import {
    api, formatDate, nameDef, roleLabel,
    localToday, type CompanyListItem, type DayMode, type BaselineEntry, type DocTemplate, type Dependency, type Schedule, type FieldDef, type FieldValues, type Member, type Project, type Task, type TaskStatus, type UserItem
  } from '$lib/api';
  import Gantt from '$lib/Gantt.svelte';
  import BadgeEditor from '$lib/projects/BadgeEditor.svelte';
  import ProjectBadge from '$lib/projects/ProjectBadge.svelte';
  import ProjectLabelPicker from '$lib/projects/ProjectLabelPicker.svelte';
  import TaskRows from '$lib/TaskRows.svelte';
  import TaskCard from '$lib/tasks/TaskCard.svelte';
  import ChartGantt from '@lucide/svelte/icons/chart-gantt';
  import ListTodo from '@lucide/svelte/icons/list-todo';
  import ClipboardList from '@lucide/svelte/icons/clipboard-list';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import UsersRound from '@lucide/svelte/icons/users-round';
  import FolderBrowser from '$lib/FolderBrowser.svelte';
  import Modal from '$lib/Modal.svelte';
  import RequisitesForm from '$lib/RequisitesForm.svelte';
  import RequisitesView from '$lib/RequisitesView.svelte';
  import { session } from '$lib/session.svelte';

  const id = $derived(page.params.id);
  let project = $state<Project | null>(null);
  let members = $state<Member[]>([]);
  let myRole = $state<'manager' | 'member' | 'viewer'>('viewer');
  let tasks = $state<Task[]>([]);
  let deps = $state<Dependency[]>([]);
  type Tab = 'requisites' | 'list' | 'gantt' | 'finance' | 'files' | 'members';
  // A project opens on its requisites; ?tab= links straight to another tab.
  let tab = $state<Tab>((['requisites', 'list', 'gantt', 'finance', 'files', 'members'] as const).find((t) => t === page.url.searchParams.get('tab')) ?? 'requisites');
  function setTab(t: Tab) {
    tab = t;
  }
  let error = $state('');

  const canEdit = $derived(myRole !== 'viewer');
  const isManager = $derived(myRole === 'manager');
  // Money of the project: finance users and the project's managers.
  const showFinance = $derived(session.has('finance') && (session.finance || isManager) && session.me?.role !== 'guest');

  async function loadProject() {
    const r = await api.get<{ project: Project; members: Member[]; my_role: typeof myRole }>(`/projects/${id}`);
    project = r.project;
    members = r.members;
    myRole = r.my_role;
  }
  let schedule = $state<Schedule>({ critical: [], slack: {}, project_end: null });
  let baseline = $state<BaselineEntry[]>([]);
  async function loadTasks() {
    const r = await api.get<{ tasks: Task[]; dependencies: Dependency[]; schedule: Schedule; baseline: BaselineEntry[] }>(`/projects/${id}/tasks`);
    tasks = r.tasks;
    deps = r.dependencies;
    schedule = r.schedule;
    baseline = r.baseline;
  }

  // --- Scheduling ------------------------------------------------------------
  let info = $state('');
  let infoTimer: ReturnType<typeof setTimeout> | undefined;
  function announceShift(shifted: Task[] | undefined) {
    if (!shifted?.length) return;
    const names = shifted.slice(0, 3).map((t) => `«${t.title}»`).join(', ');
    info = `Сдвинуты связанные задачи (${shifted.length}): ${names}${shifted.length > 3 ? ' и др.' : ''}`;
    clearTimeout(infoTimer);
    infoTimer = setTimeout(() => (info = ''), 8000);
  }
  async function toggleAuto() {
    if (!project) return;
    const value = !project.auto_schedule;
    await guarded(async () => {
      await api.patch(`/projects/${id}`, { auto_schedule: value });
      if (project) project.auto_schedule = value;
    });
  }
  async function setDayMode(mode: DayMode) {
    if (!project || project.day_mode === mode) return;
    await guarded(async () => {
      await api.patch(`/projects/${id}`, { day_mode: mode });
      if (project) project.day_mode = mode;
      await loadTasks();
      info = mode === 'work'
        ? 'Сроки считаются в рабочих днях: выходные и праздники пропускаются. Чтобы перенести задачи с нерабочих дней, нажмите «Выровнять по связям» или сдвиньте их на диаграмме.'
        : 'Сроки считаются в календарных днях.';
    });
  }
  async function alignAll() {
    await guarded(async () => {
      const r = await api.post<{ shifted: Task[] }>(`/projects/${id}/reschedule`);
      await loadTasks();
      if (r.shifted.length) announceShift(r.shifted);
      else info = 'Все связи соблюдены — сдвигать нечего.';
    });
  }
  async function saveBaseline() {
    if (project?.baseline_saved_at && !confirm('Заменить сохранённый базовый план текущими сроками?')) return;
    await guarded(async () => {
      await api.post(`/projects/${id}/baseline`);
      await Promise.all([loadTasks(), loadProject()]);
      info = 'Базовый план сохранён. Отклонения от него видны на диаграмме серыми полосами.';
    });
  }
  async function clearBaseline() {
    if (!confirm('Удалить базовый план?')) return;
    await guarded(async () => {
      await api.del(`/projects/${id}/baseline`);
      await Promise.all([loadTasks(), loadProject()]);
    });
  }
  const baselineEnd = $derived(baseline.reduce<string | null>((m, b) => {
    const d = b.due_date ?? b.start_date;
    return d && (!m || d > m) ? d : m;
  }, null));
  const endDelta = $derived.by(() => {
    if (!baselineEnd || !schedule.project_end) return 0;
    return Math.round((Date.parse(schedule.project_end) - Date.parse(baselineEnd)) / 86_400_000);
  });
  const violations = $derived(Object.values(schedule.slack).filter((v) => v < 0).length);
  $effect(() => {
    id;
    Promise.all([loadProject(), loadTasks()]).catch((e) => (error = e.message));
  });

  // Links from notifications open a task: /projects/<id>?task=<task id>.
  let openedFromLink = '';
  $effect(() => {
    const want = page.url.searchParams.get('task');
    if (!want || want === openedFromLink || !tasks.length) return;
    const t = tasks.find((x) => x.id === want);
    if (t) {
      openedFromLink = want;
      openTask(t);
    }
  });

  async function guarded(fn: () => Promise<unknown>) {
    error = '';
    try {
      await fn();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  // --- Gantt drag ---------------------------------------------------------
  async function reschedule(taskId: string, start: string, due: string) {
    const t = tasks.find((t) => t.id === taskId);
    if (!t) return;
    const prev = { start: t.start_date, due: t.due_date };
    t.start_date = start; // optimistic
    t.due_date = due;
    try {
      const r = await api.patch<{ task: Task; shifted: Task[] }>(`/tasks/${taskId}`, { start_date: start, due_date: due });
      Object.assign(t, r.task);
      announceShift(r.shifted);
      await loadTasks();
    } catch (e) {
      t.start_date = prev.start;
      t.due_date = prev.due;
      error = (e as Error).message;
    }
  }

  // --- Task card ------------------------------------------------------------------
  let cardOpen = $state(false);
  let cardTask = $state<string | null>(null);
  let cardMilestone = $state(false);
  function openNew(milestone = false) {
    cardTask = null;
    cardMilestone = milestone;
    cardOpen = true;
  }
  function openTask(t: Task) {
    cardTask = t.id;
    cardOpen = true;
  }
  async function onTaskChanged(shifted?: Task[]) {
    announceShift(shifted);
    await Promise.all([loadTasks(), loadProject()]).catch((e) => (error = e.message));
  }

  async function setStatus(t: Task, status: TaskStatus) {
    await guarded(async () => {
      const r = await api.patch<{ task: Task }>(`/tasks/${t.id}`, { status });
      Object.assign(t, r.task);
      await loadTasks();
    });
  }

  // --- Members ------------------------------------------------------------
  let users = $state<UserItem[]>([]);
  let addUser = $state('');
  let addRole = $state<'manager' | 'member' | 'viewer'>('member');
  $effect(() => {
    if ((tab === 'members' || tab === 'requisites') && users.length === 0) api.get<{ users: UserItem[] }>('/users').then((r) => (users = r.users));
  });

  // --- Document from template ----------------------------------------------------
  let genOpen = $state(false);
  let templates = $state<DocTemplate[]>([]);
  let genTemplate = $state('');
  let genName = $state('');
  let genMissing = $state<{ key: string; label: string; known: boolean }[]>([]);
  let genResult = $state<{ file_id: string; name: string; folder_id: string } | null>(null);
  let genBusy = $state(false);

  async function openGenerate() {
    genResult = null;
    genMissing = [];
    templates = (await api.get<{ templates: DocTemplate[] }>('/templates')).templates.filter((t) => t.scope === 'project');
    genTemplate = templates[0]?.id ?? '';
    genOpen = true;
  }
  // Default file name and empty-requisite check follow the selected template.
  $effect(() => {
    const t = templates.find((x) => x.id === genTemplate);
    if (!genOpen || !t) return;
    const code = project?.fields?.code;
    genName = code ? `${t.name} ${code}` : t.name;
    api.get<{ missing: typeof genMissing }>(`/projects/${id}/templates/${t.id}/check?today=${localToday()}`)
      .then((r) => (genMissing = r.missing))
      .catch(() => (genMissing = []));
  });
  async function generate(open: boolean) {
    genBusy = true;
    await guarded(async () => {
      const r = await api.post<{ file_id: string; name: string; folder_id: string }>(`/projects/${id}/generate`, {
        template_id: genTemplate, name: genName, today: localToday()
      });
      genResult = r;
      if (open) window.open(`/editor/${r.file_id}`, '_blank');
    });
    genBusy = false;
  }

  // --- Requisites -------------------------------------------------------------
  let defs = $state<FieldDef[]>([]);
  let reqEditing = $state(false);
  let reqValues = $state<FieldValues>({});
  let reqName = $state('');
  // «Наименование объекта» is the project name when the portal has that requisite.
  const nd = $derived(nameDef(defs));
  let reqDescription = $state('');
  $effect(() => {
    if (tab === 'requisites' && defs.length === 0) api.get<{ fields: FieldDef[] }>('/project-fields').then((r) => (defs = r.fields));
  });
  function editRequisites() {
    reqValues = { ...(project?.fields ?? {}) };
    reqName = project?.name ?? '';
    reqDescription = project?.description ?? '';
    reqEditing = true;
  }
  async function saveRequisites() {
    await guarded(async () => {
      // Description belongs to the project itself. The name too — unless the
      // «Наименование объекта» requisite is the name (the server keeps them equal).
      const patch: Record<string, string> = {};
      if (!nd) {
        const name = reqName.trim();
        if (!name) throw new Error('Укажите название проекта');
        if (name !== project?.name) patch.name = name;
      }
      if (reqDescription.trim() !== (project?.description ?? '')) patch.description = reqDescription.trim();
      if (Object.keys(patch).length) await api.patch(`/projects/${id}`, patch);
      // Send every editable field so that cleared values are removed on the server.
      const values: FieldValues = {};
      for (const d of defs.filter((d) => !d.is_archived)) {
        const v = reqValues[d.key];
        values[d.key] = v === '' || v === undefined ? null : v;
      }
      if (defs.some((d) => !d.is_archived)) await api.put<{ values: FieldValues }>(`/projects/${id}/fields`, { values });
      // The customer requisite may have changed the project's CRM link.
      await loadProject();
      reqEditing = false;
    });
  }
  async function upsertMember(userId: string, role: string) {
    await guarded(async () => {
      await api.post(`/projects/${id}/members`, { user_id: userId, role });
      addUser = '';
      await loadProject();
    });
  }
  async function removeMember(userId: string) {
    if (!confirm('Исключить участника из проекта?')) return;
    await guarded(async () => {
      await api.del(`/projects/${id}/members/${userId}`);
      await loadProject();
    });
  }

  async function setProjectStatus(status: string) {
    await guarded(async () => {
      await api.patch(`/projects/${id}`, { status });
      await loadProject();
    });
  }
  async function deleteProject() {
    if (!confirm('Удалить проект, все задачи и папку документов проекта?')) return;
    await guarded(async () => {
      await api.del(`/projects/${id}`);
      goto('/projects');
    });
  }
  let badgeOpen = $state(false);
  // --- CRM customer link ---------------------------------------------------------------
  let custOpen = $state(false);
  let custId = $state('');
  let custList = $state<CompanyListItem[]>([]);
  async function openCustomer() {
    custList = (await api.get<{ companies: CompanyListItem[] }>('/crm/companies')).companies;
    custId = project?.company_id ?? '';
    custOpen = true;
  }
  async function saveCustomer() {
    error = '';
    try {
      await api.put(`/projects/${id}/company`, { company_id: custId || null });
      custOpen = false;
      await loadProject();
    } catch (e) {
      error = (e as Error).message;
    }
  }
  // Gantt export (PDF / Excel / MS Project).
  let exportOpen = $state(false);
</script>

<div class="page">
  {#if project}
    <div class="page-head">
      <a href="/projects" class="faint">Проекты</a><span class="faint">/</span>
      {#if project.fields?.code}<span class="pcode mono">{project.fields.code}</span>{/if}
      <h1 class="ptitle">
        {#if isManager}
          <button class="pbadge" onclick={() => (badgeOpen = true)} title="Изменить значок проекта" aria-label="Изменить значок проекта">
            <ProjectBadge id={project.id} name={project.name} color={project.badge_color} icon={project.badge_icon} size={30} />
          </button>
        {:else}
          <span class="pbadge"><ProjectBadge id={project.id} name={project.name} color={project.badge_color} icon={project.badge_icon} size={30} /></span>
        {/if}
        {project.name}
      </h1>
      {#if project.company_id}
        {#if session.crm}<a class="badge cust" href="/crm/companies/{project.company_id}" title="Карточка заказчика в CRM"><span class="cust-l">Заказчик:</span> {project.company_name}</a>
        {:else}<span class="badge cust"><span class="cust-l">Заказчик:</span> {project.company_name}</span>{/if}
      {/if}
      {#if project.deal_id && session.crm}<a class="small" href="/crm/deals/{project.deal_id}">сделка</a>{/if}
      <div class="spacer"></div>
      {#if canEdit && project.folder_id && session.has('templates')}<button class="btn" onclick={openGenerate}>Документ из шаблона</button>{/if}
      {#if canEdit}
        <button class="btn" onclick={() => openNew(true)}>Веха</button>
        <button class="btn primary" onclick={() => openNew()}>Задача</button>
      {/if}
    </div>
    {#if project.description}<p class="muted desc">{project.description}</p>{/if}
    {#if isManager || project.label_ids.length}
      <div class="plabels">
        <ProjectLabelPicker {project} canEdit={isManager} onchange={(ids) => { if (project) project.label_ids = ids; }} />
      </div>
    {/if}

    <div class="tabs" role="tablist">
      <button role="tab" aria-selected={tab === 'requisites'} class:on={tab === 'requisites'} onclick={() => setTab('requisites')}><ClipboardList size={16} strokeWidth={1.75} />Реквизиты</button>
      <button role="tab" aria-selected={tab === 'list'} class:on={tab === 'list'} onclick={() => setTab('list')}><ListTodo size={16} strokeWidth={1.75} />Задачи <span class="cnt">{tasks.length}</span></button>
      {#if session.has('planning')}<button role="tab" aria-selected={tab === 'gantt'} class:on={tab === 'gantt'} onclick={() => setTab('gantt')}><ChartGantt size={16} strokeWidth={1.75} />Гант</button>{/if}
      {#if showFinance}<button role="tab" aria-selected={tab === 'finance'} class:on={tab === 'finance'} onclick={() => setTab('finance')}><Wallet size={16} strokeWidth={1.75} />Финансы</button>{/if}
      {#if project.folder_id}<button role="tab" aria-selected={tab === 'files'} class:on={tab === 'files'} onclick={() => setTab('files')}><FolderOpen size={16} strokeWidth={1.75} />Файлы</button>{/if}
      <button role="tab" aria-selected={tab === 'members'} class:on={tab === 'members'} onclick={() => setTab('members')}><UsersRound size={16} strokeWidth={1.75} />Участники <span class="cnt">{members.length}</span></button>
    </div>

    {#if error}<div class="error-box">{error}</div>{/if}
    {#if info}<div class="info-box" role="status">{info}</div>{/if}

    {#if tab === 'gantt' && session.has('planning')}
      <div class="sched row">
        {#if schedule.project_end}
          <span>Окончание: <b>{formatDate(schedule.project_end)}</b></span>
          {#if baselineEnd && endDelta !== 0}
            <span class="badge" class:danger={endDelta > 0} class:ok={endDelta < 0}>{endDelta > 0 ? '+' : ''}{endDelta} дн. к базовому плану</span>
          {/if}
        {/if}
        {#if violations > 0}<span class="badge danger">нарушено связей: {violations}</span>{/if}
        <div class="spacer"></div>
        {#if isManager}
          <div class="seg" role="radiogroup" aria-label="Как считать сроки" title="Как считать длительность задач и задержки связей">
            <button class="btn sm" role="radio" aria-checked={project.day_mode === 'work'} class:on={project.day_mode === 'work'} onclick={() => setDayMode('work')}>Рабочие дни</button>
            <button class="btn sm" role="radio" aria-checked={project.day_mode === 'calendar'} class:on={project.day_mode === 'calendar'} onclick={() => setDayMode('calendar')}>Календарные</button>
          </div>
        {:else}
          <span class="faint small">{project.day_mode === 'work' ? 'Сроки в рабочих днях' : 'Сроки в календарных днях'}</span>
        {/if}
        {#if isManager}
          <label class="check small" title="При переносе задачи зависящие от неё задачи сдвигаются автоматически">
            <input type="checkbox" checked={project.auto_schedule} onchange={toggleAuto} />Автосдвиг связанных задач
          </label>
        {/if}
        {#if canEdit && deps.length}<button class="btn sm" onclick={alignAll}>Выровнять по связям</button>{/if}
        {#if tasks.length}<button class="btn sm" onclick={() => (exportOpen = true)} title="PDF, Excel или MS Project"><Download size={14} />Экспорт</button>{/if}
        {#if isManager && tasks.length}
          <button class="btn sm" onclick={saveBaseline}>{project.baseline_saved_at ? 'Обновить базовый план' : 'Сохранить базовый план'}</button>
          {#if project.baseline_saved_at}
            <button class="btn ghost sm" onclick={clearBaseline} title="Сохранён {formatDate(project.baseline_saved_at)}">Сбросить</button>
          {/if}
        {/if}
      </div>
      <Gantt {tasks} dependencies={deps} {canEdit} critical={schedule.critical} slack={schedule.slack} {baseline}
             onchange={reschedule} onselect={openTask} />
      <GanttExportDialog bind:open={exportOpen} projectId={project.id} hasBaseline={!!project.baseline_saved_at} />
    {:else if tab === 'requisites'}
      {#if reqEditing}
        <div class="card req-card">
          <div class="proj-main">
            {#if !nd}<label class="field">Название проекта <input class="input" bind:value={reqName} maxlength="1000" required /></label>{/if}
            <label class="field">Описание <textarea class="input" rows="2" bind:value={reqDescription}></textarea></label>
          </div>
          <RequisitesForm {defs} bind:values={reqValues} {users} />
          <div class="row req-actions">
            <div class="spacer"></div>
            <button class="btn" onclick={() => (reqEditing = false)}>Отмена</button>
            <button class="btn primary" onclick={saveRequisites}>Сохранить</button>
          </div>
        </div>
      {:else}
        <div class="row req-bar">
          {#if !defs.some((d) => d.field_type === 'company' && !d.is_archived)}
            <span class="faint">Заказчик:</span>
            {#if project.company_id}
              {#if session.crm}<a href="/crm/companies/{project.company_id}">{project.company_name}</a>{:else}<b>{project.company_name}</b>{/if}
            {:else}<span class="faint">не привязан к контрагенту CRM</span>{/if}
            {#if isManager && session.crm}<button class="btn ghost sm" onclick={openCustomer}>{project.company_id ? 'Изменить' : 'Выбрать из CRM'}</button>{/if}
          {/if}
          <div class="spacer"></div>
          {#if isManager}<button class="btn" onclick={editRequisites}>Редактировать</button>{/if}
        </div>
        {#if defs.length === 0}
          <div class="card req-card">
            <p class="faint none">Реквизиты проектов не настроены.{#if isManager}{' '}Администратор портала задаёт их в разделе «Администрирование → Реквизиты проектов».{/if}</p>
          </div>
        {:else}
          <RequisitesView {defs} values={project.fields} {users} startDate={project.start_date ?? project.created_at.slice(0, 10)}
                          onfill={isManager ? editRequisites : undefined} />
        {/if}
      {/if}
    {:else if tab === 'finance' && showFinance}
      <ProjectFinance projectId={project.id} />
    {:else if tab === 'files' && project.folder_id}
      <FolderBrowser folderId={project.folder_id} root={project.folder_id} embedded />
    {:else if tab === 'list' || tab === 'gantt'}
      <TaskRows {tasks} {canEdit} meId={session.me?.id} onopen={openTask} onstatus={setStatus} />
    {:else}
      <div class="card scroll">
        <table class="list">
          <thead><tr><th>Участник</th><th>Email</th><th>Роль в проекте</th><th></th></tr></thead>
          <tbody>
            {#each members as m (m.user_id)}
              <tr>
                <td>{m.display_name}</td>
                <td class="muted">{m.email}</td>
                <td>
                  {#if isManager}
                    <select class="input sm" value={m.role} onchange={(e) => upsertMember(m.user_id, e.currentTarget.value)}>
                      <option value="manager">Руководитель</option><option value="member">Участник</option><option value="viewer">Наблюдатель</option>
                    </select>
                  {:else}{roleLabel[m.role]}{/if}
                </td>
                <td style="text-align:right">{#if isManager}<button class="btn ghost sm danger" onclick={() => removeMember(m.user_id)}>Исключить</button>{/if}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        {#if isManager}
          <div class="row addm">
            <select class="input" bind:value={addUser}>
              <option value="">Добавить участника…</option>
              {#each users.filter((u) => u.is_active && !members.some((m) => m.user_id === u.id)) as u}
                <option value={u.id}>{u.display_name} ({u.email})</option>
              {/each}
            </select>
            <select class="input role" bind:value={addRole}>
              <option value="manager">Руководитель</option><option value="member">Участник</option><option value="viewer">Наблюдатель</option>
            </select>
            <button class="btn primary" disabled={!addUser} onclick={() => upsertMember(addUser, addRole)}>Добавить</button>
          </div>
        {/if}
      </div>
      {#if isManager}
        <div class="row danger-zone">
          <span class="muted">Статус проекта:</span>
          <select class="input role" value={project.status} onchange={(e) => setProjectStatus(e.currentTarget.value)}>
            <option value="active">Активный</option><option value="paused">Приостановлен</option><option value="closed">Закрыт</option>
          </select>
          <div class="spacer"></div>
          <button class="btn danger" onclick={deleteProject}>Удалить проект</button>
        </div>
      {/if}
    {/if}
  {:else if error}
    <div class="error-box">{error}</div>
  {/if}
</div>

{#if project}
  <BadgeEditor bind:open={badgeOpen} {project} onsaved={(c, i) => { if (project) { project.badge_color = c; project.badge_icon = i; } }} />
{/if}
<TaskCard bind:open={cardOpen} taskId={cardTask} projectId={id} milestone={cardMilestone} showProject={false} onchanged={onTaskChanged} />

<Modal bind:open={genOpen} title="Документ из шаблона" width={560}>
  {#if templates.length === 0}
    <p class="muted">Шаблонов пока нет. Администратор загружает их в разделе «Администрирование → Шаблоны документов».</p>
  {:else if genResult}
    <div class="ok-box">Создан документ «{genResult.name}» в папке проекта.</div>
    {#if genMissing.length}<p class="faint small">Пустые метки заменены пустым текстом — их можно дописать в редакторе.</p>{/if}
  {:else}
    <div class="tpl-list" role="radiogroup" aria-label="Шаблон">
      {#each templates as t (t.id)}
        <label class="tpl-opt" class:on={genTemplate === t.id}>
          <input type="radio" name="tpl" value={t.id} bind:group={genTemplate} />
          <span class="tpl-ext {t.extension}">{t.extension === 'docx' ? 'W' : 'X'}</span>
          <span class="tpl-txt"><b>{t.name}</b>{#if t.description}<span class="faint small">{t.description}</span>{/if}</span>
          <span class="faint small">{t.category}</span>
        </label>
      {/each}
    </div>
    <label class="field">Имя файла <input class="input" bind:value={genName} /></label>
    {#if genMissing.length}
      <div class="warn-box">
        <b>Не заполнены реквизиты — в документе будут пустые места:</b>
        <ul>{#each genMissing as m}<li>{#if m.known}{m.label}{:else}Неизвестная метка <span class="mono">{'{{' + m.key + '}}'}</span> — исправьте шаблон{/if}</li>{/each}</ul>
        {#if isManager}<button class="btn sm" onclick={() => { genOpen = false; setTab('requisites'); editRequisites(); }}>Заполнить реквизиты</button>{/if}
      </div>
    {/if}
  {/if}
  {#snippet footer()}
    {#if genResult}
      <a class="btn" href="/files/{genResult.folder_id}">Открыть папку</a>
      <a class="btn primary" href="/editor/{genResult.file_id}" target="_blank">Открыть документ</a>
    {:else}
      <button class="btn" onclick={() => (genOpen = false)}>Отмена</button>
      {#if templates.length}
        <button class="btn" disabled={!genTemplate || !genName.trim() || genBusy} onclick={() => generate(false)}>Создать</button>
        <button class="btn primary" disabled={!genTemplate || !genName.trim() || genBusy} onclick={() => generate(true)}>Создать и открыть</button>
      {/if}
    {/if}
  {/snippet}
</Modal>

<Modal bind:open={custOpen} title="Заказчик проекта">
  <label class="field">Контрагент
    <select class="input" bind:value={custId}>
      <option value="">— не привязан —</option>
      {#each custList as c}<option value={c.id}>{c.name}{c.inn ? ` (ИНН ${c.inn})` : ''}</option>{/each}
    </select>
  </label>
  <p class="faint small">Наименование, ИНН заказчика в реквизитах проекта заполнятся из карточки контрагента.</p>
  {#snippet footer()}
    <button class="btn" onclick={() => (custOpen = false)}>Отмена</button>
    <button class="btn primary" onclick={saveCustomer}>Сохранить</button>
  {/snippet}
</Modal>

<style>
  .desc { margin: -8px 0 12px; }
  .ptitle { display: flex; align-items: flex-start; gap: 10px; }
  .pbadge { flex: none; border: 0; background: none; padding: 0; margin-top: 1px; display: inline-grid; border-radius: 8px; }
  button.pbadge { cursor: pointer; }
  button.pbadge:hover { box-shadow: 0 0 0 3px var(--accent-soft); }
  .plabels { margin: -6px 0 12px; }
  .cust { text-decoration: none; }
  .pcode { background: var(--surface-2); border: 1px solid var(--border); padding: 1px 6px; border-radius: 4px; color: var(--text-2); }
  .tpl-list { display: grid; gap: 6px; max-height: 300px; overflow-y: auto; }
  .tpl-opt { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--radius); cursor: pointer; }
  .tpl-opt.on { border-color: var(--accent); background: var(--accent-soft); }
  .tpl-opt input { position: absolute; opacity: 0; pointer-events: none; }
  .tpl-opt:has(input:focus-visible) { outline: 2px solid var(--accent); }
  .tpl-ext { width: 24px; height: 24px; border-radius: 5px; display: grid; place-items: center; color: #fff; font-weight: 700; font-size: 12px; flex: none; }
  .tpl-ext.docx { background: var(--word); }
  .tpl-ext.xlsx { background: var(--cell); }
  .tpl-txt { display: grid; flex: 1; min-width: 0; }
  .warn-box { background: var(--warn-soft); color: var(--warn); padding: 10px 12px; border-radius: var(--radius); font-size: 13px; display: grid; gap: 6px; justify-items: start; }
  .warn-box ul { margin: 0; padding-left: 18px; }
  .sched { flex-wrap: wrap; margin-bottom: 10px; font-size: 13px; color: var(--text-2); gap: 10px; }
  .sched b { color: var(--text); }
  .info-box { background: var(--accent-soft); color: var(--accent); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; margin-bottom: 10px; }
  .req-card { padding: 18px; display: grid; gap: 14px; }
  .req-card p { margin: 0; }
  .tabs { display: flex; gap: 4px; border-bottom: 1px solid var(--border); margin-bottom: 14px; }
  .tabs button { display: inline-flex; align-items: center; gap: 6px; padding: 7px 12px; border: 0; background: none; color: var(--text-2); border-bottom: 2px solid transparent; margin-bottom: -1px; cursor: pointer; }
  .tabs button.on { color: var(--accent); border-color: var(--accent); }
  .tabs { overflow-x: auto; }
  .tabs button { white-space: nowrap; }
  .cnt { font-size: 11px; background: var(--surface-2); color: var(--text-3); border-radius: 999px; padding: 0 6px; min-width: 18px; text-align: center; }
  .tabs button.on .cnt { background: var(--accent-soft); color: var(--accent); }

  .scroll { overflow-x: auto; }
  .link { background: none; border: 0; padding: 0; color: var(--text); cursor: pointer; text-align: left; }
  .link:hover { color: var(--accent); text-decoration: underline; }
  .input.sm { height: 26px; width: auto; padding: 0 6px; }
  .role { width: 170px; flex: none; }
  .addm { padding: 12px; }
  .danger-zone { margin-top: 16px; }
  .proj-main { display: grid; gap: 12px; padding-bottom: 14px; border-bottom: 1px solid var(--border); }
  .req-bar { margin-bottom: 12px; min-height: 34px; }
  .none { margin: 0; }
  .cust-l { opacity: .7; font-weight: 400; }
</style>
