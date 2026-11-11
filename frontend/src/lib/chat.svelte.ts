// Chat state shared by the menu badge and the chat page: unread counters and
// live events delivered over the notification stream.

import { api } from './api';
import { session } from './session.svelte';

export interface ChatMessage {
  id: string;
  channel_id: string;
  author_id: string | null;
  author_name: string | null;
  body: string;
  created_at: string;
  edited_at: string | null;
}

export type ChatEvent =
  | { type: 'chat_message'; channel_id: string; message: ChatMessage }
  | { type: 'chat_edit'; channel_id: string; message: ChatMessage }
  | { type: 'chat_delete'; channel_id: string; id: string }
  | { type: 'chat_read'; channel_id: string; unread_total: number };

class Chats {
  unreadTotal = $state(0);
  /** Channel the user is looking at: its messages do not count as unread. */
  openChannel: string | null = null;
  private listeners = new Set<(ev: ChatEvent) => void>();

  async refreshUnread() {
    if (!session.has('chats')) {
      this.unreadTotal = 0;
      return;
    }
    try {
      this.unreadTotal = (await api.get<{ total: number }>('/chats/unread')).total;
    } catch {
      /* offline: keep the last value */
    }
  }

  subscribe(fn: (ev: ChatEvent) => void) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /** Called by the notification stream for every chat event. */
  onEvent(ev: ChatEvent) {
    if (ev.type === 'chat_message' && ev.channel_id !== this.openChannel && ev.message.author_id !== session.me?.id) this.unreadTotal += 1;
    if (ev.type === 'chat_read') this.unreadTotal = ev.unread_total;
    for (const fn of this.listeners) fn(ev);
  }
}

export const chats = new Chats();
