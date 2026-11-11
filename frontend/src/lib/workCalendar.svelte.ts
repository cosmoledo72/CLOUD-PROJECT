// Working calendar on the client: special days (holidays, transferred days
// off, working weekends, shortened days) loaded per year from /api/calendar
// and cached for the session. Day numbers are UTC days since the epoch, the
// same as the Gantt chart uses.

import { api } from './api';

export type DayKind = 'holiday' | 'off' | 'work' | 'short';
export interface CalendarDay {
  date: string;
  kind: DayKind;
  title: string;
  custom: boolean;
  official_kind: DayKind | null;
}

const DAY_MS = 86_400_000;
export const toDay = (s: string) => {
  const [y, m, d] = s.split('-').map(Number);
  return Date.UTC(y, m - 1, d) / DAY_MS;
};
export const fromDay = (n: number) => new Date(n * DAY_MS).toISOString().slice(0, 10);

const isWorkKind = (k: DayKind) => k === 'work' || k === 'short';
const weekend = (n: number) => {
  const w = new Date(n * DAY_MS).getUTCDay();
  return w === 0 || w === 6;
};

class WorkCalendarStore {
  /** Special days by day number; reassigned on every load so readers react. */
  days = $state<Map<number, CalendarDay>>(new Map());
  private loaded = new Set<number>();
  private pending = new Map<number, Promise<void>>();

  /** Make sure the years covering [from, to] (day numbers) are loaded. */
  async ensure(from: number, to: number): Promise<void> {
    const y0 = new Date(from * DAY_MS).getUTCFullYear();
    const y1 = new Date(to * DAY_MS).getUTCFullYear();
    const jobs: Promise<void>[] = [];
    for (let y = Math.max(1990, y0); y <= Math.min(2150, y1); y++) jobs.push(this.loadYear(y));
    await Promise.all(jobs);
  }

  loadYear(y: number, force = false): Promise<void> {
    if (!force && this.loaded.has(y)) return Promise.resolve();
    if (!force && this.pending.has(y)) return this.pending.get(y)!;
    const job = api
      .get<{ days: CalendarDay[] }>(`/calendar?year=${y}`)
      .then((r) => {
        const next = new Map(this.days);
        for (const [k, v] of next) if (new Date(k * DAY_MS).getUTCFullYear() === y) next.delete(k);
        for (const d of r.days) next.set(toDay(d.date), d);
        this.days = next;
        this.loaded.add(y);
      })
      .catch(() => {})
      .finally(() => this.pending.delete(y));
    this.pending.set(y, job);
    return job;
  }

  /** Forget cached years (after an administrator changed the calendar). */
  invalidate() {
    this.loaded.clear();
    this.days = new Map();
  }

  info(n: number): CalendarDay | undefined {
    return this.days.get(n);
  }

  isWork(n: number): boolean {
    const d = this.days.get(n);
    return d ? isWorkKind(d.kind) : !weekend(n);
  }

  /** First working day on or after `n` (`dir` = -1: on or before). */
  snap(n: number, dir: 1 | -1 = 1): number {
    let d = n;
    for (let i = 0; i < 400 && !this.isWork(d); i++) d += dir;
    return d;
  }

  /** Move `k` working days from working day `n` (negative — back). */
  add(n: number, k: number): number {
    let d = n;
    const step = k >= 0 ? 1 : -1;
    for (let left = Math.abs(k); left > 0; ) {
      d += step;
      if (this.isWork(d)) left--;
    }
    return d;
  }

  /** Working days in [s, e] inclusive. */
  count(s: number, e: number): number {
    let c = 0;
    for (let d = s; d <= e; d++) if (this.isWork(d)) c++;
    return c;
  }
}

export const workCalendar = new WorkCalendarStore();

/** «5 раб. дн. · 7 кал. дн.» (or «7 дн.» in calendar mode) for a task's dates. */
export function durationLabel(start: string | null, due: string | null, mode: 'work' | 'calendar'): string {
  const s0 = start ?? due;
  const e0 = due ?? start;
  if (!s0 || !e0) return '';
  const s = toDay(s0);
  const e = Math.max(s, toDay(e0));
  const cal = e - s + 1;
  if (mode === 'calendar') return `${cal} дн.`;
  const w = workCalendar.count(s, e);
  return w === cal ? `${w} раб. дн.` : `${w} раб. дн. · ${cal} кал. дн.`;
}
