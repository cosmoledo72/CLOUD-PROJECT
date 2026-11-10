// Date helpers for task cards and lists. Dates are ISO `YYYY-MM-DD` strings.

const MONTHS = ['янв', 'фев', 'мар', 'апр', 'мая', 'июн', 'июл', 'авг', 'сен', 'окт', 'ноя', 'дек'];

function parts(d: string): [number, number, number] {
  const [y, m, day] = d.split('-').map(Number);
  return [y, m, day];
}

/** «6 ноя», with the year when it differs from `refYear`. */
export function shortDate(d: string, refYear = new Date().getFullYear()): string {
  const [y, m, day] = parts(d);
  return `${day} ${MONTHS[m - 1]}${y !== refYear ? ` ${y}` : ''}`;
}

/** Compact range for a date «pill»: «2–6 ноя», «28 окт – 3 ноя», «до 6 ноя». */
export function rangeLabel(start: string | null, due: string | null, refYear = new Date().getFullYear()): string {
  if (!start && !due) return '';
  if (!start || start === due) return shortDate((due ?? start)!, refYear);
  if (!due) return `с ${shortDate(start, refYear)}`;
  const [ys, ms, ds] = parts(start);
  const [yd, md, dd] = parts(due);
  if (ys === yd && ms === md) return `${ds}–${dd} ${MONTHS[md - 1]}${yd !== refYear ? ` ${yd}` : ''}`;
  return `${shortDate(start, refYear)} – ${shortDate(due, refYear)}`;
}

/** Overdue: an unfinished task whose due date is before today. */
export function isOverdue(t: { due_date: string | null; status: string }, today: string): boolean {
  return !!t.due_date && t.status !== 'done' && t.due_date < today;
}
