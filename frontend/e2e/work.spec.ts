// Everyday work: project with a task and its Gantt bar, files, knowledge base,
// portal modules.
import { expect, test } from '@playwright/test';
import { call, login, newPortal, trackErrors } from './helpers';

test('проект, задача и полоса на Ганте', async ({ page }) => {
  const errors = trackErrors(page);
  const p = await newPortal();
  await login(page, p);
  await page.goto('/projects');
  await page.getByRole('button', { name: 'Новый проект' }).first().click();
  const dlg = page.getByRole('dialog');
  await dlg.getByLabel('Название').fill('Обследование моста');
  await dlg.getByRole('button', { name: 'Создать' }).click();
  await page.waitForURL(/\/projects\/[0-9a-f-]{36}/);
  await expect(page.getByRole('heading', { name: 'Обследование моста' })).toBeVisible();

  await page.getByRole('button', { name: 'Задача', exact: true }).click();
  const card = page.getByRole('dialog');
  await card.getByLabel('Название').fill('Обмерные работы');
  const today = new Date();
  const iso = (d: Date) => d.toISOString().slice(0, 10);
  await card.getByLabel('Начало').fill(iso(today));
  await card.getByLabel('Срок').fill(iso(new Date(today.getTime() + 7 * 86400_000)));
  await card.getByRole('button', { name: 'Сохранить' }).click();
  await expect(card).toBeHidden();
  await page.getByRole('tab', { name: /Задачи/ }).click();
  await expect(page.getByText('Обмерные работы').first()).toBeVisible();

  await page.getByRole('tab', { name: /Гант/ }).click();
  await expect(page.locator('g.bar[aria-label="Обмерные работы"]')).toBeVisible();
  expect(errors).toEqual([]);
});

test('хранилище: папка и загрузка файла', async ({ page }) => {
  const errors = trackErrors(page);
  const p = await newPortal();
  await login(page, p);
  await page.goto('/files');
  await page.getByRole('button', { name: 'Создать папку' }).click();
  const dlg = page.getByRole('dialog');
  await dlg.getByPlaceholder('Название').fill('Отчёты 2026');
  await dlg.getByRole('button', { name: 'Создать' }).click();
  await page.getByRole('link', { name: 'Отчёты 2026' }).click();
  await expect(page.getByRole('heading', { name: 'Отчёты 2026' })).toBeVisible();
  await page.locator('input[type=file]').first().setInputFiles({ name: 'акт осмотра.txt', mimeType: 'text/plain', buffer: Buffer.from('Акт осмотра') });
  await expect(page.getByText('акт осмотра.txt')).toBeVisible();
  const usage = await call<{ used: number }>(p, 'GET', 'storage/usage');
  expect(usage.used).toBeGreaterThan(0);
  expect(errors).toEqual([]);
});

test('база знаний: новая страница по ссылке ?new=1, сохранение Ctrl+S', async ({ page }) => {
  const errors = trackErrors(page);
  const dialogs: string[] = [];
  page.on('dialog', (d) => {
    dialogs.push(d.message());
    d.accept();
  });
  const p = await newPortal();
  await login(page, p);
  await page.goto('/kb?new=1');
  await page.locator('input.title[placeholder="Название страницы"]').fill('Регламент выезда');
  await page.locator('.ProseMirror').click();
  await page.keyboard.type('Перед выездом проверить снаряжение.');
  await page.keyboard.press('Control+s');
  await page.waitForURL(/\/kb\?p=/);
  await page.reload();
  await expect(page.getByText('Перед выездом проверить снаряжение.')).toBeVisible();
  await page.goto('/projects');
  expect(dialogs).toEqual([]);
  expect(errors).toEqual([]);
});

test('модули портала: отключённая CRM пропадает из меню', async ({ page }) => {
  const p = await newPortal();
  await login(page, p);
  await expect(page.locator('nav a[href="/crm/deals"], nav a[href^="/crm"]').first()).toBeVisible();
  await page.goto('/admin/modules');
  await page.getByLabel('Модуль «CRM»').uncheck();
  await page.getByRole('button', { name: 'Сохранить' }).click();
  await expect(page.getByText('Модули сохранены')).toBeVisible();
  await page.reload();
  await expect(page.locator('nav a[href^="/crm"]')).toHaveCount(0);
  await call(p, 'PUT', 'admin/modules', { disabled: [] });
});

test('телефон: меню и переход в проекты @phone', async ({ page }) => {
  const p = await newPortal();
  await login(page, p);
  await page.getByRole('button', { name: 'Меню' }).click();
  await page.getByRole('link', { name: 'Проекты' }).click();
  await expect(page).toHaveURL(/\/projects/);
});

test('панель уведомлений перекрывает заголовки страницы', async ({ page }) => {
  const p = await newPortal();
  await call(p, 'POST', 'projects', { name: 'Проект для проверки слоёв' });
  await login(page, p);
  await page.goto('/projects');
  await expect(page.getByText('Проект для проверки слоёв')).toBeVisible();
  await page.locator('button[aria-label^="Уведомления"]:visible').first().click();
  const panel = page.locator('.panel[role=dialog]');
  await expect(panel).toBeVisible();
  // Every sampled point of the panel must belong to the panel, not to the page underneath.
  const foreign = await panel.evaluate((el) => {
    const r = el.getBoundingClientRect();
    let n = 0;
    for (let x = r.left + 4; x < r.right - 4; x += 16)
      for (let y = r.top + 4; y < r.bottom - 4; y += 16) {
        const hit = document.elementFromPoint(x, y);
        if (hit && !el.contains(hit)) n++;
      }
    return n;
  });
  expect(foreign).toBe(0);
});

test('телефон: страницы настроек не шире экрана @phone', async ({ page }) => {
  const p = await newPortal();
  await call(p, 'POST', 'admin/project-fields/preset', {});
  await login(page, p);
  for (const path of ['/admin/organization', '/admin/requisites', '/crm/settings']) {
    await page.goto(path);
    await page.waitForLoadState('networkidle');
    const width = await page.evaluate(() => document.documentElement.scrollWidth);
    expect(width, path).toBeLessThanOrEqual(page.viewportSize()!.width + 1);
  }
});
