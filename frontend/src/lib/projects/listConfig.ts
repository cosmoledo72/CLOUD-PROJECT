// Settings of the project list ("Поля"): table columns and their order, the
// line under the project name, fields on cards and grouping. Stored on the
// server per user (view setting `projects_list`), with a portal default an
// administrator may set; without either, a built-in default is derived from
// the requisites marked "show in project card".

import { nameDef, type FieldDef } from '$lib/api';
import { isDeadline } from '$lib/fieldLook';

/** Built-in columns; anything else is a requisite key. */
export const BUILTIN = {
  $name: 'Проект',
  $status: 'Статус',
  $labels: 'Метки',
  $progress: 'Задачи',
  $dates: 'Сроки по задачам',
  $description: 'Описание',
  $budget: 'Бюджет',
  $paid: 'Оплачено',
  $due: 'Остаток к оплате'
} as const;
/** Money columns (finance module): shown only to those who may see project money. */
export const MONEY_KEYS = ['$budget', '$paid', '$due'] as const;
export type Builtin = keyof typeof BUILTIN;

export interface ListConfig {
  /** Table columns in order; `$name` is always present. */
  columns: string[];
  /** Shown under the project name in the table (a field key, `$description` or null). */
  subtitle: string | null;
  /** Fields on a card, in order (`$name` is the card title and is not listed). */
  card: string[];
  /** Group the list by: `$status`, `$labels`, a requisite key, or null. */
  group: string | null;
}

/** Requisites that can be shown (not archived, not the project name itself). */
export const usableDefs = (defs: FieldDef[]) => {
  const nd = nameDef(defs);
  return defs.filter((d) => !d.is_archived && d.id !== nd?.id);
};

/** Fields the list may be grouped by. */
export const groupable = (defs: FieldDef[]) =>
  usableDefs(defs).filter((d) => d.field_type === 'select' || d.field_type === 'user' || d.field_type === 'company' || d.field_type === 'checkbox');

export function defaultConfig(defs: FieldDef[]): ListConfig {
  const usable = usableDefs(defs);
  const listed = usable.filter((d) => d.show_in_list && d.field_type !== 'textarea').map((d) => d.key);
  const hasCode = usable.some((d) => d.key === 'code');
  const rest = listed.filter((k) => k !== 'code');
  const hasDeadline = usable.some((d) => listed.includes(d.key) && isDeadline(d));
  return {
    columns: [...(hasCode ? ['code'] : []), '$name', ...rest, '$status', '$progress', ...(hasDeadline ? [] : ['$dates'])],
    subtitle: null,
    card: [...(hasCode ? ['code'] : []), ...rest, '$progress'],
    group: null
  };
}

/** Keep a stored config valid against the current requisites. */
export function normalize(raw: unknown, defs: FieldDef[]): ListConfig {
  const base = defaultConfig(defs);
  if (!raw || typeof raw !== 'object') return base;
  const r = raw as Partial<ListConfig>;
  const keys = new Set([...usableDefs(defs).map((d) => d.key), ...Object.keys(BUILTIN)]);
  const clean = (a: unknown) => [...new Set(Array.isArray(a) ? a.filter((k): k is string => typeof k === 'string' && keys.has(k)) : [])];
  const columns = clean(r.columns);
  if (!columns.includes('$name')) columns.unshift('$name');
  const card = clean(r.card).filter((k) => k !== '$name');
  const subtitle = typeof r.subtitle === 'string' && keys.has(r.subtitle) && r.subtitle !== '$name' ? r.subtitle : null;
  const groupKeys = new Set(['$status', '$labels', ...groupable(defs).map((d) => d.key)]);
  const group = typeof r.group === 'string' && groupKeys.has(r.group) ? r.group : null;
  return { columns: Array.isArray(r.columns) ? columns : base.columns, subtitle, card: Array.isArray(r.card) ? card : base.card, group };
}

export const sameConfig = (a: ListConfig, b: ListConfig) => JSON.stringify(a) === JSON.stringify(b);
