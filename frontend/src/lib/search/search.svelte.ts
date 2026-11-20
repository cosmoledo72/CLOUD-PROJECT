// Portal-wide search: the Ctrl+K palette and the /search page share one API.
import { api } from '#lib/api.ts';

export type HitType = 'projects' | 'tasks' | 'files' | 'kb' | 'companies' | 'contacts' | 'deals' | 'comments' | 'messages' | 'people';

export interface Hit {
  type: HitType;
  id: string;
  title: string;
  subtitle: string;
  /** Matched text with ⟦…⟧ markers. */
  snippet: string | null;
  url: string;
  ext?: string;
  number?: number;
  status?: string;
  icon?: string;
}

export interface HitGroup {
  type: HitType;
  label: string;
  items: Hit[];
  more: boolean;
}

export interface SearchParams {
  q: string;
  type?: HitType | 'all';
  offset?: number;
  limit?: number;
  project?: string;
  since?: string;
}

export async function runSearch(p: SearchParams): Promise<HitGroup[]> {
  const u = new URLSearchParams({ q: p.q, type: p.type ?? 'all' });
  if (p.offset) u.set('offset', String(p.offset));
  if (p.limit) u.set('limit', String(p.limit));
  if (p.project) u.set('project', p.project);
  if (p.since) u.set('since', p.since);
  return (await api.get<{ groups: HitGroup[] }>(`/search?${u}`)).groups;
}

/** Query words for highlighting titles. */
export const queryWords = (q: string) => q.toLowerCase().split(/[^\p{L}\p{N}]+/u).filter(Boolean);

// Recent queries, remembered per browser.
const KEY = 'wh_search_recent';
export function recent(): string[] {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? '[]') as string[];
  } catch {
    return [];
  }
}
export function remember(q: string) {
  const t = q.trim();
  if (t.length < 2) return;
  try {
    localStorage.setItem(KEY, JSON.stringify([t, ...recent().filter((x) => x !== t)].slice(0, 6)));
  } catch { /* storage unavailable */ }
}

/** Palette state (opened from the sidebar, Ctrl+K or «/»). */
export const palette = $state({ open: false, q: '' });
