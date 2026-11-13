const { chromium, devices } = require('playwright');
const mode = process.argv[2] || 'light';
(async () => {
  const b = await chromium.launch();
  const opts = mode === 'mobile' ? { ...devices['iPhone 13'] } : { viewport: { width: 1440, height: 900 }, colorScheme: mode, deviceScaleFactor: 1 };
  const ctx = await b.newContext({ ...opts, locale: 'ru-RU' });
  const p = await ctx.newPage();
  p.on('pageerror', (e) => console.log('ERR', e.message));
  await p.goto('http://localhost:5173/login?portal=demo');
  await p.fill('input[type=email]', 'o@example.com'); await p.fill('input[type=password]', 'owner-password-1');
  await p.click('button:has-text("Войти")'); await p.waitForURL((u) => !u.pathname.startsWith('/login'));
  await p.goto('http://localhost:5173/admin/templates'); await p.waitForTimeout(1500);
  const el = await p.$('article.tpl:has-text("Проверка меток")');
  await el.scrollIntoViewIfNeeded();
  await el.screenshot({ path: `/home/claude/run/shots/t29${mode[0]}.png` });
  await b.close();
})();
