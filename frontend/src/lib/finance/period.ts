// Reporting periods of the finance section.
import { localToday } from '$lib/api';

export type Period = 'month' | 'quarter' | 'year' | 'prev_year' | 'all' | 'custom';
export const PERIODS: [Period, string][] = [
  ['month', 'Месяц'], ['quarter', 'Квартал'], ['year', 'Год'], ['prev_year', 'Прошлый год'], ['all', 'Всё время'], ['custom', 'Период…']
];

const pad = (n: number) => String(n).padStart(2, '0');

/** [from, to] of a period ('' — open end). */
export function periodRange(p: Period): [string, string] {
  const t = localToday();
  const y = +t.slice(0, 4);
  const m = +t.slice(5, 7);
  if (p === 'month') return [`${y}-${pad(m)}-01`, t];
  if (p === 'quarter') return [`${y}-${pad(Math.floor((m - 1) / 3) * 3 + 1)}-01`, t];
  if (p === 'year') return [`${y}-01-01`, `${y}-12-31`];
  if (p === 'prev_year') return [`${y - 1}-01-01`, `${y - 1}-12-31`];
  return ['', ''];
}

export const MONTHS_SHORT = ['янв', 'фев', 'мар', 'апр', 'май', 'июн', 'июл', 'авг', 'сен', 'окт', 'ноя', 'дек'];

/** Money with a sign and no kopecks for whole sums: «+120 000 ₽». */
export function signed(n: number, kind: 'income' | 'expense'): string {
  const s = n.toLocaleString('ru-RU', { minimumFractionDigits: 0, maximumFractionDigits: 2 });
  return `${kind === 'income' ? '+' : '−'}${s} ₽`;
}
