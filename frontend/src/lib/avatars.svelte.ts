// Who has a photo (user id → upload time). Loaded once per page load; every
// Avatar shows the photo instead of initials when there is one.
import { api } from '$lib/api';

class Avatars {
  map = $state<Record<string, number>>({});
  private pending: Promise<void> | null = null;

  ensure(): Promise<void> {
    this.pending ??= api
      .get<{ avatars: Record<string, number> }>('/users/avatars')
      .then((r) => { this.map = r.avatars; })
      .catch(() => { this.pending = null; });
    return this.pending;
  }

  url(id: string): string | null {
    const v = this.map[id];
    return v ? `/api/users/${id}/avatar?v=${v}` : null;
  }

  /** After the photo changed (`at` — upload time, null — removed). */
  set(id: string, at: string | null) {
    const next = { ...this.map };
    if (at) next[id] = Math.floor(new Date(at).getTime() / 1000);
    else delete next[id];
    this.map = next;
  }
}

export const avatars = new Avatars();
