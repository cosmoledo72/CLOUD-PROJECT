// Sign-in pages (iteration 34): the product's look without a portal, the first
// start, «Запомнить меня», the password field and the strength hint.
import { expect, test } from '@playwright/test';
import { call, newPortal, PASSWORD, trackErrors } from './helpers';

const system = (firstRun: boolean, signup: boolean) => ({
  contentType: 'application/json',
  body: JSON.stringify({ first_run: firstRun, signup_enabled: signup })
});

test('первый запуск: оформление CLOUD-PROJECT и создание портала @phone', async ({ page }) => {
  const errors = trackErrors(page);
  await page.route('**/api/public/system', (r) => r.fulfill(system(true, true)));
  await page.goto('/login');
  await expect(page.getByText('Первый запуск')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Создайте портал организации' })).toBeVisible();
  await expect(page.locator('section.hero img')).toHaveAttribute('src', /cloud-project-logo/);
  await expect(page.getByRole('button', { name: 'У меня уже есть портал' })).toHaveCount(0);

  await page.getByPlaceholder('ООО «Партнер-Гарант»').fill('ООО «Партнер-Гарант»');
  await expect(page.getByPlaceholder('partner-garant')).toHaveValue('partner-garant');
  await expect(page.getByText(/login\?portal=partner-garant/)).toBeVisible();

  const pw = page.locator('input[autocomplete="new-password"]');
  await pw.fill('short');
  await expect(page.getByText('ещё 5 симв.')).toBeVisible();
  await pw.fill('qwerty123456');
  await expect(page.getByText('Пароль слабый')).toBeVisible();
  await pw.fill('Мост-через-Тобол-2026!');
  await expect(page.getByText('Пароль надёжный')).toBeVisible();
  await page.getByRole('button', { name: 'Показать пароль' }).click();
  await expect(pw).toHaveAttribute('type', 'text');
  expect(errors).toEqual([]);
});

test('первый запуск на сервере, где создание порталов выключено', async ({ page }) => {
  await page.route('**/api/public/system', (r) => r.fulfill(system(true, false)));
  await page.goto('/login');
  await expect(page.getByText('Создание порталов на этом сервере выключено')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Создать портал' })).toBeDisabled();
});

test('«Создать портал» скрыта, когда создание порталов выключено', async ({ page }) => {
  const p = await newPortal();
  await page.route('**/api/public/system', (r) => r.fulfill(system(false, false)));
  await page.goto(`/login?portal=${p.slug}`);
  await expect(page.getByRole('heading', { name: 'Вход на портал' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Создать портал' })).toHaveCount(0);
  await page.unroute('**/api/public/system');
  await page.goto(`/login?portal=${p.slug}`);
  await expect(page.getByRole('button', { name: 'Создать портал' })).toBeVisible();
});

test('«Запомнить меня» и запомненная почта', async ({ page, context }) => {
  const p = await newPortal();
  await page.goto(`/login?portal=${p.slug}`);
  await page.locator('input[type=email]').fill(p.email);
  await page.locator('input[type=password]').fill(PASSWORD);
  await page.getByLabel('Запомнить меня').check();
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await page.waitForURL((u) => !u.pathname.startsWith('/login'));
  const sid = (await context.cookies()).find((c) => c.name === 'wh_sid')!;
  const days = (sid.expires * 1000 - Date.now()) / 86400_000;
  expect(days).toBeGreaterThan(29);

  // Next visit: portal, e-mail and the choice are remembered.
  await call(p, 'POST', 'auth/logout');
  await context.clearCookies();
  await page.goto('/login');
  await expect(page.locator('input[type=email]')).toHaveValue(p.email);
  await expect(page.getByLabel('Запомнить меня')).toBeChecked();
});

test('тёмная тема: тёмная панель и светлый логотип CLOUD-PROJECT', async ({ browser }) => {
  const ctx = await browser.newContext({ colorScheme: 'dark' });
  const page = await ctx.newPage();
  await page.route('**/api/public/system', (r) => r.fulfill(system(true, true)));
  await page.goto('/login');
  await expect(page.locator('section.hero img')).toHaveAttribute('src', /cloud-project-logo-white/);
  await expect(page.locator('section.hero')).not.toHaveClass(/light/);
  await ctx.close();
});
