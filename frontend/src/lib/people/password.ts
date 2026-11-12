/** Random readable password (no ambiguous characters). */
export function generatePassword(len = 14): string {
  const alphabet = 'abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789';
  const bytes = crypto.getRandomValues(new Uint32Array(len));
  return Array.from(bytes, (b) => alphabet[b % alphabet.length]).join('');
}

/** Phone numbers: digits, spaces, `+ - ( )` and an extension («доб. 214»). Mirrors the server check. */
export function validPhone(p: string): boolean {
  const s = p.trim();
  if (s.length > 40) return false;
  const i = s.toLowerCase().indexOf('доб');
  let base = s;
  if (i >= 0) {
    const ext = s.slice(i).replace(/^[^\d]*/, '');
    if (!/^\d+$/.test(ext)) return false;
    base = s.slice(0, i).replace(/[ ,]+$/, '');
    if (!/\d/.test(base)) return false;
  }
  return /^[\d +\-()]*$/.test(base) && (base === '' || /\d/.test(base));
}
