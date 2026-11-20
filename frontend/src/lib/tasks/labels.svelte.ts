// Shared catalogue of task labels (one per portal), loaded once per page session.

import { api, type TaskLabel } from '#lib/api.ts';

/** Colours offered for a new label. */
export const LABEL_COLORS = ['#64748b', '#2563eb', '#0891b2', '#16a34a', '#ca8a04', '#ea580c', '#dc2626', '#c026d3', '#7c3aed'];

class Labels {
  list = $state<TaskLabel[]>([]);
  canManage = $state(false);
  loaded = $state(false);
  #pending: Promise<void> | null = null;

  byId(id: string): TaskLabel | undefined {
    return this.list.find((l) => l.id === id);
  }

  /** Load the catalogue unless it is already loaded (or `force`). */
  load(force = false): Promise<void> {
    if (this.loaded && !force) return Promise.resolve();
    this.#pending ??= api
      .get<{ labels: TaskLabel[]; can_manage: boolean }>('/task-labels')
      .then((r) => {
        this.list = r.labels;
        this.canManage = r.can_manage;
        this.loaded = true;
      })
      .finally(() => (this.#pending = null));
    return this.#pending;
  }

  async create(name: string, color: string): Promise<TaskLabel> {
    const r = await api.post<{ id: string; name: string; color: string }>('/task-labels', { name, color });
    await this.load(true);
    return this.byId(r.id) ?? { ...r, sort_order: 0, task_count: 0 };
  }

  async update(id: string, patch: Partial<Pick<TaskLabel, 'name' | 'color'>>) {
    await api.patch(`/task-labels/${id}`, patch);
    await this.load(true);
  }

  async remove(id: string) {
    await api.del(`/task-labels/${id}`);
    await this.load(true);
  }
}

export const labels = new Labels();
