// Client-side checks of Russian requisites (mirror of backend crm/validate.rs)
// for instant feedback while typing; the server re-validates everything.

const digits = (s: string) => /^\d+$/.test(s);

function weighted(d: number[], w: number[]): number {
  return w.reduce((sum, wi, i) => sum + wi * d[i], 0);
}

export function innValid(s: string): boolean {
  if (!digits(s)) return false;
  const d = [...s].map(Number);
  if (d.length === 10) return (weighted(d, [2, 4, 10, 3, 5, 9, 4, 6, 8]) % 11) % 10 === d[9];
  if (d.length === 12) {
    return (
      (weighted(d, [7, 2, 4, 10, 3, 5, 9, 4, 6, 8]) % 11) % 10 === d[10] &&
      (weighted(d, [3, 7, 2, 4, 10, 3, 5, 9, 4, 6, 8]) % 11) % 10 === d[11]
    );
  }
  return false;
}

export const kppValid = (s: string) => /^\d{4}[\dA-Z]{2}\d{3}$/.test(s);

export function ogrnValid(s: string): boolean {
  if (!digits(s)) return false;
  if (s.length === 13) return Number(BigInt(s.slice(0, 12)) % 11n % 10n) === Number(s[12]);
  if (s.length === 15) return Number(BigInt(s.slice(0, 14)) % 13n % 10n) === Number(s[14]);
  return false;
}

export const bikValid = (s: string) => /^\d{9}$/.test(s);

export function accountValid(bik: string, account: string, correspondent: boolean): boolean {
  if (!bikValid(bik) || !/^\d{20}$/.test(account)) return false;
  const prefix = correspondent ? '0' + bik.slice(4, 6) : bik.slice(6);
  const w = [7, 1, 3];
  const sum = [...(prefix + account)].reduce((acc, c, i) => acc + ((Number(c) * w[i % 3]) % 10), 0);
  return sum % 10 === 0;
}

/** Remove spaces and upper-case a code field. */
export const normCode = (s: string) => s.replace(/\s+/g, '').toUpperCase();
