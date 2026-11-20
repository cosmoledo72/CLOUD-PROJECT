// Sign-in: password, the second factor, recovery codes (iteration 32).
import { expect, test } from '@playwright/test';
import { call, enable2fa, fillLogin, newPortal, totp, trackErrors } from './helpers';

test('вход с кодом из приложения @phone', async ({ page }) => {
  const errors = trackErrors(page);
  const p = await newPortal();
  const { secret } = await enable2fa(p);
  await fillLogin(page, p);
  await expect(page.getByRole('heading', { name: 'Подтверждение входа' })).toBeVisible();
  const code = page.getByLabel('Код', { exact: true });
  await code.fill(totp(secret) === '000000' ? '111111' : '000000');
  await page.getByRole('button', { name: 'Подтвердить' }).click();
  await expect(page.locator('.error-box')).toBeVisible();
  await code.fill(totp(secret));
  await page.getByRole('button', { name: 'Подтвердить' }).click();
  await page.waitForURL((u) => !u.pathname.startsWith('/login'));
  expect(errors).toEqual([]);
});

test('вход по резервному коду: код срабатывает один раз', async ({ page, browser }) => {
  const p = await newPortal();
  const { codes } = await enable2fa(p);
  await fillLogin(page, p);
  await page.getByRole('button', { name: 'Войти по резервному коду' }).click();
  await expect(page.getByRole('heading', { name: 'Вход по резервному коду' })).toBeVisible();
  await page.getByPlaceholder('XXXX-XXXX').fill(codes[0].toLowerCase());
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await page.waitForURL((u) => !u.pathname.startsWith('/login'));
  const me = await call<{ recovery_codes_left: number }>(p, 'GET', 'auth/me');
  expect(me.recovery_codes_left).toBe(9);

  // The same code again: refused.
  const other = await browser.newPage();
  await fillLogin(other, p);
  await other.getByRole('button', { name: 'Войти по резервному коду' }).click();
  await other.getByPlaceholder('XXXX-XXXX').fill(codes[0]);
  await other.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(other.locator('.error-box')).toBeVisible();
  await other.close();
});

test('включение 2FA в профиле показывает 10 резервных кодов', async ({ page }) => {
  const errors = trackErrors(page);
  const p = await newPortal();
  await fillLogin(page, p);
  await page.waitForURL((u) => !u.pathname.startsWith('/login'));
  await page.goto('/settings');
  await page.getByRole('button', { name: 'Настроить' }).click();
  const secret = (await page.locator('code').first().textContent())!.trim();
  await page.getByPlaceholder('000000').fill(totp(secret));
  await page.getByRole('button', { name: 'Подтвердить' }).click();

  const dialog = page.getByRole('dialog');
  await expect(dialog.getByText('Двухфакторная защита включена')).toBeVisible();
  await expect(dialog.locator('.rc-code')).toHaveCount(10);
  const done = dialog.getByRole('button', { name: 'Готово' });
  await expect(done).toBeDisabled();
  await dialog.getByText('Я сохранил коды в надёжном месте').click();
  await done.click();
  await expect(page.getByText('осталось 10 из 10')).toBeVisible();

  // New set with the password.
  await page.getByRole('button', { name: 'Выпустить новые' }).click();
  const re = page.getByRole('dialog');
  await re.getByPlaceholder('Пароль').fill(p.password);
  await re.getByRole('button', { name: 'Выпустить' }).click();
  await expect(re.locator('.rc-code')).toHaveCount(10);
  expect(errors).toEqual([]);
});
