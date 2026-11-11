// How requisite values are presented: deadlines, money, codes, people.
// Shared by the project passport (RequisitesView) and the project list.

import type { CompanyRef, FieldDef } from './api';

const DAY = 86_400_000;

/** Whole days from `a` to `b` (YYYY-MM-DD). */
export const daysBetween = (a: string, b: string) => Math.round((Date.parse(b) - Date.parse(a)) / DAY);

export function plural(n: number, one: string, few: string, many: string): string {
  const m10 = n % 10, m100 = n % 100;
  return m10 === 1 && m100 !== 11 ? one : m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14) ? few : many;
}
export const daysWord = (n: number) => `${n} ${plural(n, 'день', 'дня', 'дней')}`;

export type Tone = 'late' | 'soon' | 'ok' | 'past';

/** «через 37 дней» / «сегодня» / «3 дня назад». */
export function relative(today: string, date: string): { text: string; tone: Tone } {
  const n = daysBetween(today, date);
  if (n === 0) return { text: 'сегодня', tone: 'soon' };
  if (n > 0) return { text: `через ${daysWord(n)}`, tone: n <= 14 ? 'soon' : 'ok' };
  return { text: `${daysWord(-n)} назад`, tone: 'past' };
}

/** Like `relative`, but a past date is overdue. */
export function deadline(today: string, date: string): { text: string; tone: Tone; days: number } {
  const n = daysBetween(today, date);
  if (n < 0) return { text: `просрочено на ${daysWord(-n)}`, tone: 'late', days: n };
  return { ...relative(today, date), days: n };
}

/** A date field that is a deadline («Срок сдачи…», key with deadline). */
export const isDeadline = (d: FieldDef) => d.field_type === 'date' && /срок|deadline|сдач/i.test(d.label + ' ' + d.key);
/** A number field that holds money (by its label). */
export const isMoney = (d: FieldDef) => d.field_type === 'number' && /руб|₽|стоимост|сумм|цена|аванс/i.test(d.label);
/** Short code-like text: digits, no spaces (project code, INN, cadastral number). */
export const isCode = (d: FieldDef, v: unknown) =>
  d.field_type === 'text' && typeof v === 'string' && v.length <= 40 && /\d/.test(v) && !/\s/.test(v.trim());
export const isAddress = (d: FieldDef) => /address|adres/.test(d.key) || d.label.toLowerCase().includes('адрес');

export const money = (v: unknown) =>
  typeof v === 'number' ? `${v.toLocaleString('ru-RU', { minimumFractionDigits: v % 1 ? 2 : 0, maximumFractionDigits: 2 })} ₽` : String(v);
/** «18,5 млн ₽» for summaries. */
export function moneyShort(v: number): string {
  if (Math.abs(v) >= 1e6) return `${(v / 1e6).toLocaleString('ru-RU', { maximumFractionDigits: 1 })} млн ₽`;
  if (Math.abs(v) >= 1e3) return `${(v / 1e3).toLocaleString('ru-RU', { maximumFractionDigits: 0 })} тыс. ₽`;
  return money(v);
}

export const companyOf = (v: unknown): CompanyRef | null =>
  v && typeof v === 'object' ? (v as CompanyRef) : typeof v === 'string' && v ? { id: null, name: v, inn: null } : null;

export const initials = (n: string) =>
  n.split(/\s+/).filter(Boolean).map((w) => w[0]).slice(0, 2).join('').toUpperCase();

/** Stable avatar colour for a person (a few muted hues). */
export function avatarHue(id: string): number {
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return [212, 268, 168, 24, 330, 140][h % 6];
}

export const shortDate = (d: string) =>
  new Date(d + 'T00:00:00').toLocaleDateString('ru-RU', { day: 'numeric', month: 'short', year: 'numeric' }).replace(' г.', '');
