const { chromium } = require('playwright');
const pages = process.argv.slice(3);
const mode = process.argv[2];
(async () => {
  const b = await chromium.launch();
  const ctx = await b.newContext({ viewport: { width: 1440, height: 900 }, colorScheme: mode, deviceScaleFactor: 1, locale: 'ru-RU', timezoneId: 'Asia/Sakhalin' });
  const p = await ctx.newPage();
  p.on('pageerror', (e) => console.log('ERR', e.message));
  await p.goto('http://localhost:5173/login?portal=demo');
  await p.fill('input[type=email]', 'o@example.com'); await p.fill('input[type=password]', 'owner-password-1');
  await p.click('button:has-text("Войти")'); await p.waitForURL((u) => !u.pathname.startsWith('/login'));
  for (const path of pages) {
    await p.goto('http://localhost:5173' + path); await p.waitForTimeout(2500);
    const name = path.replace(/[/?=&]/g, '_') || 'root';
    await p.screenshot({ path: `shots/au-${mode[0]}${name}.png`, fullPage: false });
  }
  await b.close();
})();
