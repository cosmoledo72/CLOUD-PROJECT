<script lang="ts">
  // Task card: one dialog for creating and editing a task from any section
  // (project, "All tasks", notification links). Fields are saved with
  // «Сохранить»; labels and checklist items are saved immediately.
  import {
    api, localToday, priorityLabel, statusLabel, timeAgo,
    type ChecklistItem, type Dependency, type Task, type TaskCardData, type TaskComment, type TaskStatus
  } from '$lib/api';
  import Avatar from '$lib/Avatar.svelte';
  import MentionInput from '$lib/MentionInput.svelte';
  import Modal from '$lib/Modal.svelte';
  import { session } from '$lib/session.svelte';
  import { isOverdue, rangeLabel } from './dates';
  import { LABEL_COLORS, labels } from './labels.svelte';
  import LabelChip from './LabelChip.svelte';
  import CalendarDays from '@lucide/svelte/icons/calendar-days';
  import GripVertical from '@lucide/svelte/icons/grip-vertical';
  import ListChecks from '@lucide/svelte/icons/list-checks';
  import MessageSquare from '@lucide/svelte/icons/message-square';
  import Plus from '@lucide/svelte/icons/plus';
  import Tag from '@lucide/svelte/icons/tag';
  import X from '@lucide/svelte/icons/x';

  let {
    open = $bindable(false),
    taskId = null,
    projectId = '',
    milestone = false,
    showProject = true,
    onchanged
  }: {
    open?: boolean;
    /** Task to show; `null` creates a new task in `projectId`. */
    taskId?: string | null;
    projectId?: string;
    /** New task is a milestone. */
    milestone?: boolean;
    /** Show the project link (off inside the project page). */
    showProject?: boolean;
    /** Something changed: reload lists. `shifted` — tasks moved by auto-scheduling. */
    onchanged?: (shifted?: Task[]) => void;
  } = $props();

  type Draft = {
    title: string; description: string; status: TaskStatus; priority: number; is_milestone: boolean;
    assignee_id: string; start_date: string; due_date: string; progress: number;
  };
  const blank = (ms: boolean): Draft => ({
    title: '', description: '', status: 'open', priority: 2, is_milestone: ms, assignee_id: '', start_date: '', due_date: '', progress: 0
  });

  let card = $state<TaskCardData | null>(null);
  let members = $state<TaskCardData['members']>([]);
  let draft = $state<Draft>(blank(false));
  let projectTasks = $state<Task[]>([]);
  let deps = $state<Dependency[]>([]);
  let error = $state('');
  let busy = $state(false);
  let loadedFor = $state<string | null>(null);

  const isNew = $derived(!taskId);
  const task = $derived(card?.task ?? null);
  const pid = $derived(task?.project_id ?? projectId);
  const canEdit = $derived(isNew ? true : !!card?.can_edit);
  const canManage = $derived(isNew ? true : !!card?.can_manage);
  const meId = $derived(session.me?.id);
  const today = localToday();

  function fill(t: Task) {
    draft = {
      title: t.title, description: t.description, status: t.status, priority: t.priority, is_milestone: t.is_milestone,
      assignee_id: t.assignee_id ?? '', start_date: t.start_date ?? '', due_date: t.due_date ?? '', progress: t.progress
    };
  }

  async function load(id: string | null) {
    error = '';
    labels.load().catch(() => {});
    if (!id) {
      card = null;
      draft = blank(milestone);
      comments = [];
      const r = await api.get<{ members: { user_id: string; display_name: string; role: 'manager' | 'member' | 'viewer' }[] }>(`/projects/${projectId}`);
      members = r.members;
      return;
    }
    const r = await api.get<TaskCardData>(`/tasks/${id}`);
    if (taskId !== id) return;
    card = r;
    members = r.members;
    fill(r.task);
    loadComments(id);
    loadProjectTasks(r.task.project_id);
  }

  async function loadProjectTasks(projId: string) {
    try {
      const r = await api.get<{ tasks: Task[]; dependencies: Dependency[] }>(`/projects/${projId}/tasks`);
      projectTasks = r.tasks;
      deps = r.dependencies;
    } catch {
      /* dependencies are optional in the card */
    }
  }

  async function refreshTask() {
    if (!taskId) return;
    const r = await api.get<TaskCardData>(`/tasks/${taskId}`);
    card = r;
  }

  // (Re)load whenever the dialog opens for another task.
  $effect(() => {
    if (!open) {
      loadedFor = null;
      return;
    }
    const key = taskId ?? `new:${projectId}:${milestone}`;
    if (loadedFor === key) return;
    loadedFor = key;
    card = null;
    newItem = '';
    labelMenu = false;
    load(taskId).catch((e) => (error = e.message));
  });

  async function guarded(fn: () => Promise<unknown>) {
    error = '';
    try {
      await fn();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  // --- Fields ------------------------------------------------------------------
  async function save() {
    busy = true;
    await guarded(async () => {
      const body = {
        title: draft.title, description: draft.description, priority: draft.priority, is_milestone: draft.is_milestone,
        assignee_id: draft.assignee_id || null, start_date: draft.start_date || null, due_date: draft.due_date || null
      };
      if (taskId) {
        const r = await api.patch<{ shifted: Task[] }>(`/tasks/${taskId}`, { ...body, status: draft.status, progress: draft.progress });
        onchanged?.(r.shifted);
      } else {
        await api.post(`/projects/${projectId}/tasks`, body);
        onchanged?.();
      }
      open = false;
    });
    busy = false;
  }

  async function remove() {
    if (!taskId || !confirm('Удалить задачу вместе с подзадачами?')) return;
    await guarded(async () => {
      await api.del(`/tasks/${taskId}`);
      open = false;
      onchanged?.();
    });
  }

  // --- Labels ------------------------------------------------------------------
  let labelMenu = $state(false);
  let newLabel = $state('');
  let newColor = $state(LABEL_COLORS[1]);
  const taskLabels = $derived((task?.label_ids ?? []).map((id) => labels.byId(id)).filter((l) => !!l));

  async function setLabels(ids: string[]) {
    if (!taskId) return;
    await guarded(async () => {
      const r = await api.put<{ task: Task }>(`/tasks/${taskId}/labels`, { label_ids: ids });
      if (card) card.task = r.task;
      onchanged?.();
    });
  }
  function toggleLabel(id: string) {
    const cur = task?.label_ids ?? [];
    setLabels(cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id]);
  }
  async function addLabel() {
    const name = newLabel.trim();
    if (!name) return;
    await guarded(async () => {
      const l = await labels.create(name, newColor);
      newLabel = '';
      await setLabels([...(task?.label_ids ?? []), l.id]);
    });
  }

  // --- Checklist ---------------------------------------------------------------
  let newItem = $state('');
  let newItemWho = $state('');
  let newItemDue = $state('');
  const items = $derived(card?.checklist ?? []);
  const doneCount = $derived(items.filter((i) => i.done).length);

  async function afterChecklist(list: ChecklistItem[]) {
    if (card) {
      card.checklist = list;
      card.task.checklist_total = list.length;
      card.task.checklist_done = list.filter((i) => i.done).length;
    }
    onchanged?.();
  }
  async function addItem() {
    const body = newItem.trim();
    if (!body || !taskId) return;
    await guarded(async () => {
      const r = await api.post<{ checklist: ChecklistItem[] }>(`/tasks/${taskId}/checklist`, {
        body, assignee_id: newItemWho || null, due_date: newItemDue || null
      });
      newItem = '';
      newItemDue = '';
      await afterChecklist(r.checklist);
    });
  }
  async function patchItem(it: ChecklistItem, patch: Record<string, unknown>) {
    await guarded(async () => {
      const r = await api.patch<{ checklist: ChecklistItem[] }>(`/checklist-items/${it.id}`, patch);
      await afterChecklist(r.checklist);
    });
  }
  async function deleteItem(it: ChecklistItem) {
    await guarded(async () => {
      const r = await api.del<{ checklist: ChecklistItem[] }>(`/checklist-items/${it.id}`);
      await afterChecklist(r.checklist);
    });
  }
  function renameItem(it: ChecklistItem, value: string) {
    const v = value.trim();
    if (v && v !== it.body) patchItem(it, { body: v });
  }
  const canTick = (it: ChecklistItem) => canEdit || it.assignee_id === meId;

  // Drag to reorder: the dropped item lands between its new neighbours.
  let dragId = $state<string | null>(null);
  function dropOn(target: ChecklistItem) {
    const from = items.find((i) => i.id === dragId);
    dragId = null;
    if (!from || from.id === target.id) return;
    const rest = items.filter((i) => i.id !== from.id);
    const idx = rest.findIndex((i) => i.id === target.id);
    const movingDown = items.indexOf(from) < items.indexOf(target);
    const before = movingDown ? rest[idx] : rest[idx - 1];
    const after = movingDown ? rest[idx + 1] : rest[idx];
    const order = before && after ? (before.sort_order + after.sort_order) / 2 : before ? before.sort_order + 1 : (after?.sort_order ?? 1) - 1;
    patchItem(from, { sort_order: order });
  }

  // --- Dependencies and time ---------------------------------------------------------
  let newDep = $state('');
  let timeMinutes = $state(60);
  let timeDate = $state(localToday());
  const predecessors = $derived(taskId ? deps.filter((d) => d.task_id === taskId) : []);
  const titleOf = (tid: string) => {
    const t = projectTasks.find((x) => x.id === tid);
    return t ? `#${t.number} ${t.title}` : '?';
  };
  async function addDep() {
    if (!taskId || !newDep) return;
    await guarded(async () => {
      const r = await api.post<{ shifted: Task[] }>(`/tasks/${taskId}/dependencies`, { depends_on_id: newDep });
      newDep = '';
      await loadProjectTasks(pid);
      onchanged?.(r.shifted);
    });
  }
  async function removeDep(depId: string) {
    await guarded(async () => {
      await api.del(`/tasks/${taskId}/dependencies/${depId}`);
      await loadProjectTasks(pid);
      onchanged?.();
    });
  }
  async function logTime() {
    await guarded(async () => {
      await api.post(`/tasks/${taskId}/time`, { work_date: timeDate, minutes: timeMinutes });
      await refreshTask();
      onchanged?.();
    });
  }

  // --- Comments --------------------------------------------------------------------
  let comments = $state<TaskComment[]>([]);
  let commentText = $state('');
  let commentMentions = $state<string[]>([]);
  let sending = $state(false);
  async function loadComments(id: string) {
    try {
      const r = await api.get<{ comments: TaskComment[] }>(`/tasks/${id}/comments`);
      if (taskId === id) comments = r.comments;
    } catch {
      comments = [];
    }
  }
  async function sendComment() {
    const id = taskId;
    if (!id || !commentText.trim() || sending) return;
    sending = true;
    await guarded(async () => {
      await api.post(`/tasks/${id}/comments`, { body: commentText, mentions: commentMentions });
      commentText = '';
      await loadComments(id);
      if (card) card.task.comment_count = comments.length;
      onchanged?.();
    });
    sending = false;
  }
  async function deleteComment(c: TaskComment) {
    const id = taskId;
    if (!id || !confirm('Удалить комментарий?')) return;
    await guarded(async () => {
      await api.del(`/task-comments/${c.id}`);
      await loadComments(id);
      if (card) card.task.comment_count = comments.length;
      onchanged?.();
    });
  }
  const isProjectManager = $derived(session.isAdmin || members.some((m) => m.user_id === meId && m.role === 'manager'));
  const mentionPeople = $derived(members.filter((m) => m.user_id !== meId).map((m) => ({ id: m.user_id, name: m.display_name })));

  const title = $derived(
    isNew ? (milestone ? 'Новая веха' : 'Новая задача') : task ? `#${task.number} · ${task.is_milestone ? 'Веха' : 'Задача'}` : 'Задача'
  );
  const late = $derived(!!task && isOverdue(task, today));
</script>

<Modal bind:open {title} width={720}>
  {#if !isNew && !card}
    {#if error}<div class="error-box">{error}</div>{:else}<p class="faint">Загрузка…</p>{/if}
  {:else}
    {#if task}
      <div class="chips">
        {#if showProject}<a class="proj" href="/projects/{task.project_id}?tab=list" onclick={() => (open = false)}>{task.project_name}</a>{/if}
        {#if task.priority >= 3}<span class="prio p{task.priority}">{priorityLabel[task.priority]}</span>{/if}
        {#if task.start_date || task.due_date}
          <span class="pill-date" class:late title={late ? 'Срок прошёл' : 'Сроки'}><CalendarDays size={13} strokeWidth={2} />{rangeLabel(task.start_date, task.due_date)}</span>
        {/if}
        {#each taskLabels as l (l.id)}
          <LabelChip label={l} onremove={canEdit ? () => toggleLabel(l.id) : undefined} />
        {/each}
        {#if canEdit}
          <button class="btn ghost sm" onclick={() => (labelMenu = !labelMenu)} aria-expanded={labelMenu}><Tag size={14} strokeWidth={2} />Метка</button>
        {/if}
      </div>
      {#if canEdit && labelMenu}
        <div class="lbl-menu" role="group" aria-label="Метки задачи">
          <div class="lbl-opts">
            {#each labels.list as l (l.id)}
              <label class="lbl-opt"><input type="checkbox" checked={task.label_ids.includes(l.id)} onchange={() => toggleLabel(l.id)} /><LabelChip label={l} /></label>
            {:else}
              <p class="faint small none">Меток пока нет — создайте первую.</p>
            {/each}
          </div>
          <div class="lbl-new">
            <input class="input sm" placeholder="Новая метка" maxlength="40" bind:value={newLabel}
                   onkeydown={(e) => e.key === 'Enter' && addLabel()} aria-label="Название новой метки" />
            <div class="swatches" role="radiogroup" aria-label="Цвет метки">
              {#each LABEL_COLORS as c}
                <button class="sw" class:on={newColor === c} style="background:{c}" onclick={() => (newColor = c)} role="radio" aria-checked={newColor === c} aria-label="Цвет {c}"></button>
              {/each}
            </div>
            <button class="btn sm" disabled={!newLabel.trim()} onclick={addLabel}>Добавить</button>
          </div>
        </div>
      {/if}
    {/if}

    <label class="field">Название <input class="input" bind:value={draft.title} disabled={!canEdit} /></label>
    <label class="field">Описание <textarea class="input" rows="3" bind:value={draft.description} disabled={!canEdit}></textarea></label>
    <div class="two">
      <label class="field">Исполнитель
        <select class="input" bind:value={draft.assignee_id} disabled={!canEdit}>
          <option value="">—</option>
          {#each members as m}<option value={m.user_id}>{m.display_name}</option>{/each}
        </select>
      </label>
      <label class="field">Приоритет
        <select class="input" bind:value={draft.priority} disabled={!canEdit}>
          {#each [1, 2, 3, 4] as p}<option value={p}>{priorityLabel[p]}</option>{/each}
        </select>
      </label>
      {#if !draft.is_milestone}
        <label class="field">Начало <input class="input" type="date" bind:value={draft.start_date} disabled={!canEdit} /></label>
      {/if}
      <label class="field">{draft.is_milestone ? 'Дата' : 'Срок'} <input class="input" type="date" bind:value={draft.due_date} disabled={!canEdit} /></label>
      {#if !isNew}
        <label class="field">Статус
          <select class="input" bind:value={draft.status} disabled={!canEdit}>
            {#each Object.entries(statusLabel) as [v, l]}<option value={v}>{l}</option>{/each}
          </select>
        </label>
        <label class="field">Готовность, % <input class="input" type="number" min="0" max="100" bind:value={draft.progress} disabled={!canEdit} /></label>
      {/if}
    </div>

    {#if task}
      <section class="sec">
        <h3><ListChecks size={16} strokeWidth={2} />Чек-лист {#if items.length}<span class="faint">{doneCount} из {items.length}</span>{/if}</h3>
        {#if items.length}
          <div class="progress" role="progressbar" aria-valuemin="0" aria-valuemax={items.length} aria-valuenow={doneCount} aria-label="Выполнено пунктов">
            <span style="width:{(doneCount / items.length) * 100}%"></span>
          </div>
        {/if}
        <ul class="checklist">
          {#each items as it (it.id)}
            <li class:done={it.done} class:dragging={dragId === it.id}
                ondragover={(e) => { if (dragId) e.preventDefault(); }} ondrop={() => dropOn(it)}>
              {#if canEdit}
                <span class="grip" draggable="true" role="button" tabindex="-1" aria-label="Перетащить пункт"
                      ondragstart={(e) => { dragId = it.id; e.dataTransfer?.setData('text/plain', it.id); }} ondragend={() => (dragId = null)}>
                  <GripVertical size={14} strokeWidth={2} />
                </span>
              {/if}
              <input type="checkbox" checked={it.done} disabled={!canTick(it)} onchange={(e) => patchItem(it, { done: e.currentTarget.checked })}
                     aria-label="Выполнено: {it.body}" />
              {#if canEdit}
                <input class="body" value={it.body} maxlength="500" aria-label="Текст пункта"
                       onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} onblur={(e) => renameItem(it, e.currentTarget.value)} />
                <select class="who" value={it.assignee_id ?? ''} onchange={(e) => patchItem(it, { assignee_id: e.currentTarget.value || null })} aria-label="Исполнитель пункта">
                  <option value="">Без исполнителя</option>
                  {#each members as m}<option value={m.user_id}>{m.display_name}</option>{/each}
                </select>
                <input class="due" class:late={!it.done && !!it.due_date && it.due_date < today} type="date" value={it.due_date ?? ''}
                       onchange={(e) => patchItem(it, { due_date: e.currentTarget.value || null })} aria-label="Срок пункта" />
                <button class="btn ghost sm del" onclick={() => deleteItem(it)} aria-label="Удалить пункт"><X size={14} strokeWidth={2} /></button>
              {:else}
                <span class="body ro">{it.body}</span>
                {#if it.assignee_id}<span class="meta"><Avatar id={it.assignee_id} name={it.assignee_name ?? '?'} size={20} />{it.assignee_name}</span>{/if}
                {#if it.due_date}<span class="meta" class:late={!it.done && it.due_date < today}>{rangeLabel(null, it.due_date)}</span>{/if}
              {/if}
            </li>
          {/each}
        </ul>
        {#if canEdit}
          <div class="add-item">
            <Plus size={16} strokeWidth={2} />
            <input class="input" placeholder="Добавить пункт и нажать Enter" maxlength="500" bind:value={newItem}
                   onkeydown={(e) => e.key === 'Enter' && addItem()} aria-label="Новый пункт чек-листа" />
            <select class="input who-new" bind:value={newItemWho} aria-label="Исполнитель нового пункта">
              <option value="">Без исполнителя</option>
              {#each members as m}<option value={m.user_id}>{m.display_name}</option>{/each}
            </select>
            <input class="input due-new" type="date" bind:value={newItemDue} aria-label="Срок нового пункта" />
            <button class="btn" disabled={!newItem.trim()} onclick={addItem}>Добавить</button>
          </div>
        {:else if !items.length}
          <p class="faint none">Пунктов нет.</p>
        {/if}
      </section>

      <section class="sec">
        <h3>Предшествующие задачи</h3>
        {#each predecessors as p}
          <div class="row dep">
            <span>{titleOf(p.depends_on_id)}</span><span class="faint">({p.kind}{p.lag_days ? ` +${p.lag_days} дн.` : ''})</span>
            <div class="spacer"></div>
            {#if canManage}<button class="btn ghost sm danger" onclick={() => removeDep(p.depends_on_id)}>Убрать</button>{/if}
          </div>
        {:else}
          <p class="faint none">Нет</p>
        {/each}
        {#if canManage}
          <div class="row">
            <select class="input" bind:value={newDep}>
              <option value="">Задача должна начаться после…</option>
              {#each projectTasks.filter((t) => t.id !== taskId && !predecessors.some((p) => p.depends_on_id === t.id)) as t}
                <option value={t.id}>#{t.number} {t.title}</option>
              {/each}
            </select>
            <button class="btn" disabled={!newDep} onclick={addDep}>Связать</button>
          </div>

          <h3>Учёт времени <span class="faint">— всего {(task.spent_minutes / 60).toFixed(1)} ч</span></h3>
          <div class="row">
            <input class="input" type="date" bind:value={timeDate} aria-label="Дата" />
            <input class="input mins" type="number" min="1" max="1440" step="15" bind:value={timeMinutes} aria-label="Минуты" />
            <span class="faint">мин</span>
            <button class="btn" onclick={logTime}>Записать</button>
          </div>
        {/if}
      </section>

      <section class="sec">
        <h3><MessageSquare size={16} strokeWidth={2} />Обсуждение {#if comments.length}<span class="faint">{comments.length}</span>{/if}</h3>
        <div class="comments">
          {#each comments as c (c.id)}
            <div class="comment">
              <div class="c-head">
                <b>{c.author_name ?? 'Удалённый пользователь'}</b>
                <span class="faint small" title={new Date(c.created_at).toLocaleString('ru-RU')}>{timeAgo(c.created_at)}</span>
                <div class="spacer"></div>
                {#if c.author_id === meId || isProjectManager}
                  <button class="btn ghost sm" onclick={() => deleteComment(c)} aria-label="Удалить комментарий">×</button>
                {/if}
              </div>
              <div class="c-body">{c.body}</div>
            </div>
          {:else}
            <p class="faint none">Комментариев нет.</p>
          {/each}
        </div>
        {#if canEdit}
          <MentionInput bind:value={commentText} bind:mentions={commentMentions} people={mentionPeople}
                        placeholder="Комментарий, вопрос исполнителю…" onsubmit={sendComment} />
          <div class="row"><div class="spacer"></div><button class="btn" disabled={!commentText.trim() || sending} onclick={sendComment}>Отправить</button></div>
        {/if}
      </section>

      {#if card?.created_at}
        <p class="faint small none">Создал(а) {card.created_by_name ?? '—'} · {new Date(card.created_at).toLocaleDateString('ru-RU')}</p>
      {/if}
    {/if}
    {#if error}<div class="error-box">{error}</div>{/if}
  {/if}

  {#snippet footer()}
    {#if task && canManage}<button class="btn danger" onclick={remove}>Удалить</button><div class="spacer"></div>{/if}
    <button class="btn" onclick={() => (open = false)}>Закрыть</button>
    {#if canEdit && (isNew || card)}<button class="btn primary" disabled={!draft.title.trim() || busy} onclick={save}>Сохранить</button>{/if}
  {/snippet}
</Modal>

<style>
  .chips { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; margin-top: -4px; }
  .proj { font-size: 12.5px; padding: 2px 8px; border-radius: 999px; background: var(--surface-2); color: var(--text-2); text-decoration: none; }
  .proj:hover { color: var(--accent); }
  .prio { font-size: 12px; font-weight: 600; padding: 2px 8px; border-radius: 999px; }
  .prio.p3 { background: var(--warn-soft); color: var(--warn); }
  .prio.p4 { background: var(--danger); color: #fff; }
  .pill-date { display: inline-flex; align-items: center; gap: 4px; font-size: 12.5px; padding: 2px 9px; border-radius: 999px; background: var(--surface-2); color: var(--text-2); white-space: nowrap; }
  .pill-date.late { background: var(--danger-soft); color: var(--danger); font-weight: 600; }
  .lbl-menu { border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface-2); padding: 8px; display: grid; gap: 6px; }
  .lbl-menu > .lbl-opts { display: flex; flex-wrap: wrap; gap: 4px 8px; }
  .lbl-opt { display: flex; align-items: center; gap: 6px; padding: 2px 4px; border-radius: 6px; cursor: pointer; }
  .lbl-opt:hover { background: var(--surface); }
  .lbl-new { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; border-top: 1px solid var(--border); padding-top: 8px; }
  .lbl-new .input { width: 200px; }
  .swatches { display: flex; gap: 5px; flex-wrap: wrap; }
  .sw { width: 20px; height: 20px; border-radius: 50%; border: 2px solid transparent; padding: 0; cursor: pointer; }
  .sw.on { border-color: var(--text); box-shadow: inset 0 0 0 2px var(--surface); }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .sec { display: grid; gap: 8px; border-top: 1px solid var(--border); padding-top: 12px; }
  h3 { margin: 0; display: flex; align-items: center; gap: 6px; }
  h3 .faint { font-weight: 500; font-size: 13px; }
  .progress { height: 6px; border-radius: 3px; background: var(--surface-2); overflow: hidden; }
  .progress span { display: block; height: 100%; background: var(--ok); border-radius: 3px; transition: width .2s; }
  .checklist { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .checklist li { display: flex; align-items: center; gap: 8px; padding: 3px 4px; border-radius: 6px; min-height: 34px; }
  .checklist li:hover { background: var(--surface-2); }
  .checklist li.dragging { opacity: .5; }
  .grip { color: var(--text-3); cursor: grab; display: grid; }
  .checklist input[type='checkbox'] { width: 16px; height: 16px; flex: none; accent-color: var(--ok); }
  .body { flex: 1; min-width: 0; border: 1px solid transparent; background: transparent; color: var(--text); font: inherit; padding: 3px 6px; border-radius: 5px; }
  .body:hover, .body:focus { border-color: var(--border); background: var(--surface); }
  .body.ro { padding: 3px 0; }
  .done .body { color: var(--text-3); text-decoration: line-through; }
  .who, .due { border: 1px solid transparent; background: transparent; color: var(--text-2); font: inherit; font-size: 12.5px; border-radius: 5px; padding: 2px 4px; max-width: 150px; }
  .who:hover, .due:hover, .who:focus, .due:focus { border-color: var(--border); background: var(--surface); }
  .due.late, .meta.late { color: var(--danger); font-weight: 600; }
  .meta { display: inline-flex; align-items: center; gap: 5px; font-size: 12.5px; color: var(--text-2); white-space: nowrap; }
  .del { color: var(--text-3); }
  .add-item { display: flex; align-items: center; gap: 8px; color: var(--text-3); }
  .add-item .input:first-of-type { flex: 1; min-width: 0; }
  .who-new { width: 160px; flex: none; }
  .due-new { width: 140px; flex: none; }
  .dep { padding: 4px 0; border-bottom: 1px solid var(--border); }
  .none { margin: 0; }
  .mins { width: 90px; }
  .comments { display: grid; gap: 8px; max-height: 280px; overflow-y: auto; }
  .comment { background: var(--surface-2); border-radius: var(--radius); padding: 8px 10px; }
  .c-head { display: flex; gap: 8px; align-items: baseline; }
  .c-body { white-space: pre-wrap; overflow-wrap: anywhere; margin-top: 2px; }
  @media (max-width: 600px) {
    .two { grid-template-columns: 1fr; }
    .add-item { flex-wrap: wrap; }
    .add-item .input:first-of-type { flex-basis: calc(100% - 30px); }
    .who-new, .due-new { flex: 1; width: auto; }
    .checklist li { flex-wrap: wrap; }
    .checklist .body { flex-basis: calc(100% - 60px); }
    .checklist .who { flex: 1 1 0; min-width: 0; max-width: none; margin-left: 26px; }
    .checklist .due { flex: 0 0 128px; }
  }
</style>
