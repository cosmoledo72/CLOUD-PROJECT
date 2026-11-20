// Interface tests (iteration 32, D7): key user scenarios in a real browser
// against the built portal behind nginx (or the vite dev server).
//
//   WEB=http://localhost:8090 API=http://localhost:8080 npm run test:ui
//
// Every test creates its own portal through the API, so tests are independent
// and can run against a database that already holds other data.
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  timeout: 60_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: process.env.WEB ?? 'http://localhost:8090',
    locale: 'ru-RU',
    timezoneId: 'Asia/Yekaterinburg',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure'
  },
  projects: [
    { name: 'desktop', use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 900 } } },
    { name: 'phone', use: { ...devices['Pixel 7'] }, grep: /@phone/ }
  ]
});
