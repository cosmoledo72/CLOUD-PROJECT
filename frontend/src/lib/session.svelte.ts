// Global session state (Svelte 5 runes).

import { api, ApiError, type Me, type ModuleKey } from './api';

class Session {
  me = $state<Me | null>(null);
  loaded = $state(false);

  get crm() {
    return !!this.me && this.me.crm_role !== 'none';
  }

  /** Sees the finance section (payments, analytics, receivables). */
  get finance() {
    return !!this.me && this.has('finance') && (this.me.finance_role ?? 'none') !== 'none';
  }

  get financeEdit() {
    return this.finance && this.me?.finance_role === 'edit';
  }

  get crmManager() {
    return this.crm && (this.isAdmin || this.me?.crm_role === 'manager');
  }

  /** Is the portal module on (Администрирование → Модули)? */
  has(module: ModuleKey): boolean {
    return !!this.me && (this.me.modules ?? []).includes(module);
  }

  /** Start page: the first section whose module is on. */
  get home(): string {
    const order: [ModuleKey, string][] = [['projects', '/projects'], ['kb', '/kb'], ['chats', '/chats'], ['storage', '/files'], ['crm', '/crm'], ['team', '/team']];
    return order.find(([m]) => this.has(m) && (m !== 'crm' || this.crm))?.[1] ?? '/settings';
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
