// Shared helpers of the interface tests: portals through the API, sign-in, TOTP.
import { createHmac, randomBytes } from 'node:crypto';
import { expect, request, type APIRequestContext, type Page } from '@playwright/test';

export const API = (process.env.API ?? 'http://localhost:8080').replace(/\/$/, '') + '/api';
export const PASSWORD = 'ui-test-password-1';

export interface Portal {
  slug: string;
  email: string;
  password: string;
  /** API client signed in as the owner. */
  api: APIRequestContext;
}

/** A new portal with its owner, signed in through the API. */
export async function newPortal(prefix = 'ui'): Promise<Portal> {
  const slug = prefix + randomBytes(4).toString('hex');
  const email = 'owner@example.com';
  const api = await request.newContext({ baseURL: API + '/' });
  const r = await api.post('auth/signup', {
    data: { tenant_slug: slug, tenant_name: 'Тест ' + slug, email, display_name: 'Ольга Тестова', password: PASSWORD }
  });
  expect(r.status(), await r.text()).toBe(200);
  return { slug, email, password: PASSWORD, api };
}

/** Call the API as the portal owner; fails the test on an unexpected status. */
export async function call<T = unknown>(p: Portal, method: string, path: string, data?: unknown, status = 200): Promise<T> {
  const r = await p.api.fetch(path.replace(/^\//, ''), { method, data });
  expect(r.status(), `${method} ${path}: ${await r.text()}`).toBe(status);
  const text = await r.text();
  return (text ? JSON.parse(text) : null) as T;
}

/** Fill the login form (portal from the link). */
export async function fillLogin(page: Page, p: Portal) {
  await page.goto(`/login?portal=${p.slug}`);
  await page.locator('input[type=email]').fill(p.email);
  await page.locator('input[type=password]').fill(p.password);
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
}

/** Sign in through the interface and wait for the portal. */
export async function login(page: Page, p: Portal) {
  await fillLogin(page, p);
  await page.waitForURL((u) => !u.pathname.startsWith('/login'));
}

function base32(s: string): Buffer {
  const abc = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
  let bits = '';
  for (const ch of s.replace(/=+$/, '').toUpperCase()) bits += abc.indexOf(ch).toString(2).padStart(5, '0');
  const out: number[] = [];
  for (let i = 0; i + 8 <= bits.length; i += 8) out.push(parseInt(bits.slice(i, i + 8), 2));
  return Buffer.from(out);
}

/** Current 6-digit TOTP code (RFC 6238, SHA-1, 30 s). */
export function totp(secret: string, at = Date.now()): string {
  const msg = Buffer.alloc(8);
  msg.writeBigUInt64BE(BigInt(Math.floor(at / 1000 / 30)));
  const h = createHmac('sha1', base32(secret)).update(msg).digest();
  const o = h[h.length - 1] & 15;
  const n = (h.readUInt32BE(o) & 0x7fffffff) % 1_000_000;
  return n.toString().padStart(6, '0');
}

/** Turn on 2FA for the owner through the API; returns the secret and the recovery codes. */
export async function enable2fa(p: Portal): Promise<{ secret: string; codes: string[] }> {
  const s = await call<{ secret: string }>(p, 'POST', 'auth/totp/setup');
  const r = await call<{ recovery_codes: string[] }>(p, 'POST', 'auth/totp/enable', { code: totp(s.secret) });
  return { secret: s.secret, codes: r.recovery_codes };
}

/** Page errors collected during a test (none are expected). */
export function trackErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  return errors;
}
