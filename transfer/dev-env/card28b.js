const { chromium, devices } = require('playwright');
const mode = process.argv[2] || 'light';
const A = process.argv[3];
(async () => {
  const b = await chromium.launch();
  const opts = mode === 'mobile' ? { ...devices['iPhone 13'] } : { viewport: { width: 1440, height: 900 }, colorScheme: mode, deviceScaleFactor: 1 };
  const ctx = await b.newContext({ ...opts, locale: 'ru-RU', timezoneId: 'Asia/Sakhalin' });
  const p = await ctx.newPage();
  p.on('pageerror', (e) => console.log('ERR', e.message));
  async function login(email, pw) {
    await p.goto('http://localhost:5173/login?portal=demo');
    await p.fill('input[type=email]', email); await p.fill('input[type=password]', pw);
    await p.click('button:has-text("Войти")'); await p.waitForURL((u) => !u.pathname.startsWith('/login'));
  }
  const m = mode[0];
  await login('o@example.com', 'owner-password-1');
  await p.goto('http://localhost:5173/team/' + A); await p.waitForSelector('.hero h1'); await p.waitForTimeout(800);
  await p.screenshot({ path: `shots/c28b${m}-main.png` });
  if (mode !== 'mobile') {
    for (const [t, n] of [['Реквизиты для документов', 'docs'], ['Доступы', 'access'], ['Проекты и задачи', 'work'], ['Безопасность', 'sec']]) {
      await p.click(`.tabs-line button:has-text("${t}")`); await p.waitForTimeout(700);
      await p.screenshot({ path: `shots/c28b${m}-${n}.png` });
    }
    await p.goto('http://localhost:5173/team'); await p.waitForTimeout(1500);
    await p.screenshot({ path: `shots/c28b${m}-team.png` });
    await p.goto('http://localhost:5173/settings'); await p.waitForTimeout(1200);
    await p.screenshot({ path: `shots/c28b${m}-settings.png` });
    // colleague's view
    await p.click('button[title="Выйти"]'); await p.waitForTimeout(800);
    await login('b@example.com', 'boris-password-1');
    await p.goto('http://localhost:5173/team/' + A); await p.waitForTimeout(1500);
    await p.screenshot({ path: `shots/c28b${m}-colleague.png` });
  } else {
    await p.screenshot({ path: `shots/c28b${m}-full.png`, fullPage: true });
  }
  await b.close();
})();
