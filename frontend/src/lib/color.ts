// Colour helpers for portal branding: contrast checks and text colour choice.

export function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace('#', '');
  const n = parseInt(h.length === 3 ? h.split('').map((c) => c + c).join('') : h, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function rgbToHex([r, g, b]: [number, number, number]): string {
  return '#' + [r, g, b].map((v) => Math.round(v).toString(16).padStart(2, '0')).join('');
}

/** Linear mix of two colours, `t` = share of `a` (0..1); mirrors CSS color-mix in srgb. */
export function mix(a: string, b: string, t: number): string {
  const ca = hexToRgb(a);
  const cb = hexToRgb(b);
  return rgbToHex([0, 1, 2].map((i) => ca[i] * t + cb[i] * (1 - t)) as [number, number, number]);
}

/** WCAG relative luminance. */
export function luminance(hex: string): number {
  const [r, g, b] = hexToRgb(hex).map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG contrast ratio between two colours (1..21). */
export function contrast(a: string, b: string): number {
  const [l1, l2] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (l1 + 0.05) / (l2 + 0.05);
}

export const DARK_TEXT = '#111827';
export const LIGHT_TEXT = '#ffffff';

/** Text colour (white or near-black) with the better contrast on `bg`. */
export function textOn(bg: string): string {
  return contrast(bg, LIGHT_TEXT) >= contrast(bg, DARK_TEXT) ? LIGHT_TEXT : DARK_TEXT;
}

/** Accent as rendered in dark mode (must match `--accent` in app.css). */
export const darkAccent = (brand: string) => mix(brand, '#ffffff', 0.62);

export const isHex = (s: string) => /^#[0-9a-fA-F]{6}$/.test(s);
