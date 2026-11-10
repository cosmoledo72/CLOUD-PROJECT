// Global session state (Svelte 5 runes).

import { api, ApiError, type Me } from './api';

class Session {
  me = $state<Me | null>(null);
  loaded = $state(false);

  get crm() {
    return !!this.me && this.me.crm_role !== 'none';
  }

  get crmManager() {
    return this.crm && (this.isAdmin || this.me?.crm_role === 'manager');
  }

  get isAdmin() {
    return this.me?.role === 'owner' || this.me?.role === 'admin';
  }

  async load(): Promise<Me | null> {
    try {
      this.me = await api.get<Me>('/auth/me');
    } catch (e) {
      if (!(e instanceof ApiError && e.status === 401)) throw e;
      this.me = null;
    } finally {
      this.loaded = true;
    }
    return this.me;
  }

  async logout() {
    await api.post('/auth/logout');
    this.me = null;
  }
}

export const session = new Session();
