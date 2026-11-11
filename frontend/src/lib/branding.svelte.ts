// Portal branding: loading and applying it to the document (or to a preview element).

import { api } from './api';
import { darkAccent, isHex, textOn } from './color';

export interface Branding {
  tenant_slug: string;
  display_name: string;
  accent_color: string;
  sidebar_style: 'light' | 'dark' | 'accent';
  color_scheme: 'auto' | 'light' | 'dark';
  corner_style: 'sharp' | 'default' | 'round';
  login_title: string;
  login_subtitle: string;
  logo_plate: boolean;
  login_logo_plate: boolean;
  login_bg_overlay: number;
  /** dark: light text over a darkening veil; light: dark text over a lightening veil. */
  login_bg_tone: 'dark' | 'light';
  /** Base under a transparent background image: brand gradient, light, white or #rrggbb. */
  login_bg_base: string;
  font_family: FontKey;
  font_size: 'small' | 'default' | 'large';
  has_logo: boolean;
  has_icon: boolean;
  has_login_bg: boolean;
  asset_version: number;
  logo_url: string | null;
  icon_url: string | null;
  login_bg_url: string | null;
  /** Self-registration is open (public lookup only). */
  registration_open?: boolean;
}

/** Base colours offered under the login panel's background. */
export const LOGIN_BASES = [
  { key: 'brand', label: 'Фирменный цвет' },
  { key: 'light', label: 'Светлая', color: '#f5f7fa' },
  { key: 'white', label: 'Белая', color: '#ffffff' }
] as const;

/**
 * Background and text colour of the login page's left panel:
 * a veil (darkening or lightening) over the image over the base colour.
 */
export function loginHero(b: Pick<Branding, 'login_bg_tone' | 'login_bg_base' | 'login_bg_overlay'>, imageUrl: string | null): string {
  const base =
    b.login_bg_base === 'light' ? '#f5f7fa'
    : b.login_bg_base === 'white' ? '#ffffff'
    : isHex(b.login_bg_base) ? b.login_bg_base
    : 'linear-gradient(155deg, color-mix(in srgb, var(--brand) 92%, white), color-mix(in srgb, var(--brand) 72%, black))';
  const light = b.login_bg_tone === 'light';
  const o = imageUrl ? b.login_bg_overlay ?? 0 : 0;
  const veil = light ? `rgb(255 255 255 / ${o}%)` : `rgb(0 0 0 / ${o}%)`;
  const layers = [
    `linear-gradient(${veil}, ${veil})`,
    ...(imageUrl ? [`url('${imageUrl}') center / cover no-repeat`] : []),
    base.startsWith('linear') ? base : `linear-gradient(${base}, ${base})`
  ];
  // Text: dark tone on the brand gradient keeps the brand's readable text colour.
  const fg = light ? 'color-mix(in srgb, var(--brand) 45%, #0b1726)' : b.login_bg_base === 'brand' && !imageUrl ? 'var(--on-brand)' : '#ffffff';
  return `--hero-bg: ${layers.join(', ')}; --hero-fg: ${fg}`;
}

/**
 * Portal fonts. All of them support Cyrillic. "system" needs no download; the
 * others are loaded from Google Fonts on demand (only the chosen one).
 */
export const FONTS = {
  system: { label: 'Системный', stack: "system-ui, -apple-system, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif", google: null },
  inter: { label: 'Inter', stack: "'Inter', system-ui, sans-serif", google: 'Inter:wght@400;500;600;700' },
  roboto: { label: 'Roboto', stack: "'Roboto', system-ui, sans-serif", google: 'Roboto:wght@400;500;700' },
  pt_sans: { label: 'PT Sans', stack: "'PT Sans', system-ui, sans-serif", google: 'PT+Sans:wght@400;700' },
  open_sans: { label: 'Open Sans', stack: "'Open Sans', system-ui, sans-serif", google: 'Open+Sans:wght@400;500;600;700' },
  ibm_plex: { label: 'IBM Plex Sans', stack: "'IBM Plex Sans', system-ui, sans-serif", google: 'IBM+Plex+Sans:wght@400;500;600;700' },
  montserrat: { label: 'Montserrat', stack: "'Montserrat', system-ui, sans-serif", google: 'Montserrat:wght@400;500;600;700' },
  golos: { label: 'Golos Text', stack: "'Golos Text', system-ui, sans-serif", google: 'Golos+Text:wght@400;500;600;700' },
  manrope: { label: 'Manrope', stack: "'Manrope', system-ui, sans-serif", google: 'Manrope:wght@400;500;600;700' },
  onest: { label: 'Onest', stack: "'Onest', system-ui, sans-serif", google: 'Onest:wght@400;500;600;700' },
  noto_sans: { label: 'Noto Sans', stack: "'Noto Sans', system-ui, sans-serif", google: 'Noto+Sans:wght@400;500;600;700' }
} as const;
export type FontKey = keyof typeof FONTS;
export const FONT_SIZES = { small: { label: 'Мельче', px: 13 }, default: { label: 'Обычный', px: 14 }, large: { label: 'Крупнее', px: 15 } } as const;

/** Load a Google font once (a <link> per family in <head>). */
export function loadFont(key: FontKey) {
  const f = FONTS[key] ?? FONTS.system;
  if (!f.google || typeof document === 'undefined') return;
  const id = `wh-font-${key}`;
  if (document.getElementById(id)) return;
  const link = document.createElement('link');
  link.id = id;
  link.rel = 'stylesheet';
  link.href = `https://fonts.googleapis.com/css2?family=${f.google}&subset=cyrillic&display=swap`;
  document.head.appendChild(link);
}

export const DEFAULT_ACCENT = '#1f4e79';
const DEFAULT_FAVICON = '/favicon.svg';

/**
 * Apply branding tokens to an element. On `document.documentElement` this
 * themes the whole app; on any other element (with the `brand-scope` class)
 * it themes only that subtree, which the branding editor uses for previews.
 */
export function applyBranding(
  el: HTMLElement,
  b: Pick<Branding, 'accent_color' | 'color_scheme' | 'corner_style'> & Partial<Pick<Branding, 'font_family' | 'font_size'>>
) {
  const brand = isHex(b.accent_color) ? b.accent_color.toLowerCase() : DEFAULT_ACCENT;
  el.style.setProperty('--brand', brand);
  el.style.setProperty('--on-brand', textOn(brand));
  el.style.setProperty('--on-brand-dark', textOn(darkAccent(brand)));
  el.dataset.theme = b.color_scheme;
  el.dataset.corners = b.corner_style;
  const font = FONTS[b.font_family ?? 'inter'] ?? FONTS.inter;
  loadFont(b.font_family ?? 'inter');
  el.style.setProperty('--font', font.stack);
  el.style.setProperty('--fs', `${(FONT_SIZES[b.font_size ?? 'default'] ?? FONT_SIZES.default).px}px`);
}

function setFavicon(href: string) {
  let link = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
  if (!link) {
    link = document.createElement('link');
    link.rel = 'icon';
    document.head.appendChild(link);
  }
  link.href = href;
}

class BrandingStore {
  current = $state<Branding | null>(null);

  /** Apply to the whole document (theme, favicon, title). */
  set(b: Branding) {
    this.current = b;
    applyBranding(document.documentElement, b);
    setFavicon(b.icon_url ?? b.logo_url ?? DEFAULT_FAVICON);
    document.title = b.display_name;
  }

  async loadOwn() {
    try {
      this.set(await api.get<Branding>('/branding'));
    } catch {
      /* keep defaults */
    }
  }

  /** Branding for the login page; returns null for unknown portals. */
  async loadPublic(slug: string): Promise<Branding | null> {
    try {
      const b = await api.get<Branding>(`/public/branding/${encodeURIComponent(slug)}`);
      this.set(b);
      return b;
    } catch {
      return null;
    }
  }

  reset() {
    this.current = null;
    const root = document.documentElement;
    ['--brand', '--on-brand', '--on-brand-dark'].forEach((p) => root.style.removeProperty(p));
    delete root.dataset.theme;
    delete root.dataset.corners;
    setFavicon(DEFAULT_FAVICON);
    document.title = 'CLOUD-PROJECT';
  }
}

export const branding = new BrandingStore();
