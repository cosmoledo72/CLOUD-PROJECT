// Shared catalogue of project labels (one per portal), loaded once per page session.

import { api, type ProjectLabel } from '#lib/api.ts';

class ProjectLabels {
  list = $state<ProjectLabel[]>([]);
  /** Administrators and managers of at least one project add labels. */
  canCreate = $state(false);
  /** Administrators rename and delete. */
  canManage = $state(false);
  loaded = $state(false);
  #pending: Promise<void> | null = null;

  byId(id: string): ProjectLabel | undefined {
    return this.list.find((l) => l.id === id);
  }

  /** Labels of a project in catalogue order. */
  of(ids: string[] | undefined): ProjectLabel[] {
    return (ids ?? []).map((i) => this.byId(i)).filter((l): l is ProjectLabel => !!l);
  }

  load(force = false): Promise<void> {
    if (this.loaded && !force) return Promise.resolve();
    this.#pending ??= api
      .get<{ labels: ProjectLabel[]; can_create: boolean; can_manage: boolean }>('/project-labels')
      .then((r) => {
        this.list = r.labels;
        this.canCreate = r.can_create;
        this.canManage = r.can_manage;
        this.loaded = true;
      })
      .finally(() => (this.#pending = null));
    return this.#pending;
  }

  async create(name: string, color: string): Promise<ProjectLabel> {
    const r = await api.post<{ id: string; name: string; color: string }>('/project-labels', { name, color });
    await this.load(true);
    return this.byId(r.id) ?? { ...r, sort_order: 0, project_count: 0 };
  }

  async update(id: string, patch: Partial<Pick<ProjectLabel, 'name' | 'color'>>) {
    await api.patch(`/project-labels/${id}`, patch);
    await this.load(true);
  }

  async remove(id: string) {
    await api.del(`/project-labels/${id}`);
    await this.load(true);
  }
}

export const projectLabels = new ProjectLabels();
