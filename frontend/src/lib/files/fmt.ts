// Formatting for file lists: «только что», «12 мин назад», «сегодня, 14:05»,
// «вчера, 16:40», then the date; sizes like «729,6 КБ».

export function plural(n: number, one: string, few: string, many: string): string {
  const m10 = n % 10;
  const m100 = n % 100;
  if (m10 === 1 && m100 !== 11) return one;
  if (m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14)) return few;
  return many;
}

export function relTime(iso: string, now = Date.now()): string {
  const d = new Date(iso);
  const diff = (now - d.getTime()) / 1000;
  const hm = d.toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' });
  if (diff < 60) return 'только что';
  if (diff < 3600) return `${Math.floor(diff / 60)} мин назад`;
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((day(new Date(now)) - day(d)) / 86_400_000);
  if (days === 0) return `сегодня, ${hm}`;
  if (days === 1) return `вчера, ${hm}`;
  return d.toLocaleDateString('ru-RU', { day: '2-digit', month: '2-digit', year: 'numeric' });
}

export function sizeLabel(bytes: number): string {
  if (bytes < 1024) return `${bytes} Б`;
  const units = ['КБ', 'МБ', 'ГБ', 'ТБ'];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toLocaleString('ru-RU', { maximumFractionDigits: v < 100 ? 1 : 0 })} ${units[i]}`;
}

export const itemsLabel = (n: number) => (n ? `${n} ${plural(n, 'объект', 'объекта', 'объектов')}` : 'пусто');

/** Absolute address of a portal page (for «copy link»). */
export const portalUrl = (path: string) => `${location.origin}${path}`;

/** Download the selection as one ZIP archive. */
export async function downloadZip(files: string[], folders: string[]): Promise<void> {
  const res = await fetch('/api/files/zip', {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ files, folders })
  });
  if (!res.ok) {
    let msg = res.status === 413 ? 'Выбрано больше 1 ГБ — скачайте частями' : res.statusText;
    try {
      const j = await res.json();
      if (res.status !== 413) msg = j.message ?? msg;
    } catch { /* not JSON */ }
    throw new Error(msg);
  }
  const blob = await res.blob();
  const cd = res.headers.get('Content-Disposition') ?? '';
  const m = /filename\*=UTF-8''([^;]+)/i.exec(cd);
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = m ? decodeURIComponent(m[1]) : 'Файлы.zip';
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
}
