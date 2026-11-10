// Notification center state: the list, the unread counter and the live
// stream (server-sent events) that keeps them current in every open tab.

import { api, type NotificationItem } from './api';
import { crmDue } from './crm/due.svelte';
import { chats, type ChatEvent } from './chat.svelte';

class Notifications {
  items = $state<NotificationItem[]>([]);
  unread = $state(0);
  hasMore = $state(false);
  loaded = $state(false);
  private source: EventSource | null = null;
  private retry: ReturnType<typeof setTimeout> | undefined;

  async load() {
    const r = await api.get<{ items: NotificationItem[]; unread: number; has_more: boolean }>('/notifications?limit=30');
    this.items = r.items;
    this.unread = r.unread;
    this.hasMore = r.has_more;
    this.loaded = true;
  }

  async more() {
    const last = this.items.at(-1);
    if (!last) return;
    const r = await api.get<{ items: NotificationItem[]; has_more: boolean }>(
      `/notifications?limit=30&before=${encodeURIComponent(last.created_at)}`
    );
    this.items = [...this.items, ...r.items];
    this.hasMore = r.has_more;
  }

  async markRead(ids: string[]) {
    if (!ids.length) return;
    const now = new Date().toISOString();
    for (const n of this.items) if (ids.includes(n.id) && !n.read_at) n.read_at = now;
    const r = await api.post<{ unread: number }>('/notifications/read', { ids });
    this.unread = r.unread;
  }

  async markAll() {
    const now = new Date().toISOString();
    for (const n of this.items) n.read_at ??= now;
    const r = await api.post<{ unread: number }>('/notifications/read', { all: true });
    this.unread = r.unread;
  }

  /** Open the live stream; the browser reconnects by itself after drops. */
  start() {
    if (this.source || typeof EventSource === 'undefined') return;
    this.load().catch(() => {});
    const es = new EventSource('/api/notifications/stream');
    this.source = es;
    es.addEventListener('message', (e) => this.onEvent(e.data));
    // After a reconnect, catch up on anything missed in between.
    es.addEventListener('hello', () => {
      if (this.loaded) this.load().catch(() => {});
    });
    es.onerror = () => {
      // A closed stream (e.g. the session ended) is retried slowly.
      if (es.readyState === EventSource.CLOSED) {
        this.source = null;
        clearTimeout(this.retry);
        this.retry = setTimeout(() => this.start(), 30_000);
      }
    };
  }

  stop() {
    clearTimeout(this.retry);
    this.source?.close();
    this.source = null;
    this.items = [];
    this.unread = 0;
    this.loaded = false;
  }

  private onEvent(raw: string) {
    let ev: { type: string; item?: NotificationItem; unread?: number };
    try {
      ev = JSON.parse(raw);
    } catch {
      return;
    }
    if (ev.type.startsWith('chat_')) {
      chats.onEvent(ev as unknown as ChatEvent);
      return;
    }
    if (ev.type === 'notification' && ev.item) {
      if (!this.items.some((n) => n.id === ev.item!.id)) {
        this.items = [ev.item, ...this.items];
        this.unread += 1;
      }
      if (ev.item.kind.startsWith('crm_task')) crmDue.refresh();
    } else if (ev.type === 'read' && typeof ev.unread === 'number') {
      this.unread = ev.unread;
      if (ev.unread === 0) {
        const now = new Date().toISOString();
        for (const n of this.items) n.read_at ??= now;
      }
    } else if (ev.type === 'resync') {
      this.load().catch(() => {});
      chats.refreshUnread();
    }
  }
}

export const notifications = new Notifications();
