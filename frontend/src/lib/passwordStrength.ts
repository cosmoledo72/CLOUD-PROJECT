// A rough estimate of a new password's strength for the hint under the field.
// The server only requires 10+ characters; this nudges people towards better ones.

export interface Strength {
  /** 0 — too short, 1 weak, 2 fair, 3 good, 4 strong. */
  score: 0 | 1 | 2 | 3 | 4;
  label: string;
}

const COMMON = ['password', 'parol', 'qwerty', 'йцукен', 'пароль', '123456', 'admin', 'welcome', 'letmein', '111111', 'abc123'];

export function passwordStrength(pw: string, minLength = 10): Strength {
  if (!pw) return { score: 0, label: '' };
  if (pw.length < minLength) return { score: 0, label: `ещё ${minLength - pw.length} симв.` };
  const lower = pw.toLowerCase();
  const kinds = [/[a-zа-яё]/, /[A-ZА-ЯЁ]/, /\d/, /[^\p{L}\d]/u].filter((r) => r.test(pw)).length;
  let points = kinds + (pw.length >= 14 ? 1 : 0) + (pw.length >= 18 ? 1 : 0);
  if (COMMON.some((c) => lower.includes(c))) points -= 2;
  if (/(.)\1\1/.test(pw)) points -= 1; // aaa, 111
  if (/(0123|1234|2345|3456|4567|5678|6789|abcd|qwer|asdf|йцук)/i.test(pw)) points -= 1;
  if (new Set(pw).size <= 4) points -= 2;
  const score = (points <= 1 ? 1 : points <= 2 ? 2 : points <= 4 ? 3 : 4) as Strength['score'];
  return { score, label: ['', 'слабый', 'средний', 'хороший', 'надёжный'][score] };
}
