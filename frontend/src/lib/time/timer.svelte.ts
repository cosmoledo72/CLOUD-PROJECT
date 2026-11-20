// The employee's running timer (one at a time, kept on the server). The
// elapsed time ticks locally from the server's value; the timer is re-read
// when the window regains focus and every minute, so a timer started or
// stopped on another computer shows up here too.

import { api } from '#lib/api.ts';

export interface RunningTimer {
  task_id: string;
  task_number: number;
  task_title: string;
  project_id: string;
  project_name: string;
  started_at: string;
  elapsed_seconds: number;
}

class TimerStore {
  timer = $state<RunningTimer | null>(null);
  /** Seconds since the timer started (ticks every second). */
  seconds = $state(0);
  /** Bumped whenever time is recorded, so open task cards reload their entries. */
  version = $state(0);
  private base = 0;
  private baseAt = 0;
  private tick: ReturnType<typeof setInterval> | undefined;
  private poll: ReturnType<typeof setInterval> | undefined;
  private started = false;

  private set(t: RunningTimer | null) {
    this.timer = t;
    this.base = t?.elapsed_seconds ?? 0;
    this.baseAt = Date.now();
    this.seconds = this.base;
    clearInterval(this.tick);
    if (t) this.tick = setInterval(() => (this.seconds = this.base + Math.floor((Date.now() - this.baseAt) / 1000)), 1000);
  }

  /** Read the timer and keep it fresh (once per page load). */
  init() {
    if (this.started || typeof window === 'undefined') return;
    this.started = true;
    this.load();
    this.poll = setInterval(() => this.load(), 60_000);
    window.addEventListener('focus', () => this.load());
  }

  async load() {
    try {
      const r = await api.get<{ timer: RunningTimer | null }>('/time/timer');
      if (r.timer?.task_id !== this.timer?.task_id && this.timer) this.version++;
      this.set(r.timer);
    } catch { /* module off or signed out */ }
  }

  isOn(taskId: string) {
    return this.timer?.task_id === taskId;
  }

  async start(taskId: string): Promise<{ minutes: number } | null> {
    const r = await api.post<{ timer: RunningTimer; stopped: { id: string; minutes: number } | null }>(`/tasks/${taskId}/timer`, {});
    this.set(r.timer);
    if (r.stopped) this.version++;
    return r.stopped;
  }

  async stop(note = ''): Promise<{ minutes: number } | null> {
    const r = await api.post<{ entry: { id: string; minutes: number } | null }>('/time/timer/stop', { note });
    this.set(null);
    this.version++;
    return r.entry;
  }

  async discard() {
    await api.del('/time/timer');
    this.set(null);
  }
}

export const timer = new TimerStore();

/** 3725 s → «01:02:05». */
export function clock(s: number): string {
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  return [h, m, sec].map((x) => String(x).padStart(2, '0')).join(':');
}

/** 90 → «1 ч 30 мин», 45 → «45 мин». */
export function hm(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return h ? (m ? `${h} ч ${m} мин` : `${h} ч`) : `${m} мин`;
}

/** Hours with a comma: 90 → «1,5». */
export function hours(minutes: number): string {
  return (minutes / 60).toLocaleString('ru-RU', { maximumFractionDigits: 2 });
}

/**
 * Parse a duration typed by a person: «1:30», «1,5», «1.5 ч», «90 мин», «2ч 15м», «45».
 * A bare number up to 12 means hours, larger — minutes. Returns minutes or null.
 */
export function parseDuration(text: string): number | null {
  const s = text.trim().toLowerCase().replace(',', '.');
  if (!s) return null;
  let m = s.match(/^(\d{1,2}):(\d{1,2})$/);
  if (m) return +m[1] * 60 + +m[2];
  m = s.match(/^(?:(\d+(?:\.\d+)?)\s*ч[а-я]*)?\s*(?:(\d+)\s*м[а-я]*)?$/);
  if (m && (m[1] || m[2])) return Math.round((m[1] ? +m[1] * 60 : 0) + (m[2] ? +m[2] : 0));
  m = s.match(/^(\d+(?:\.\d+)?)$/);
  if (m) {
    const v = +m[1];
    return Math.round(v <= 12 ? v * 60 : v);
  }
  return null;
}
