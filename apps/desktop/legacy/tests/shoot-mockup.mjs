// Screenshots the steps of docs/mockups/thread-team.html at 1920x1080 (to compare the real app with the mockup); usage: node shoot-mockup.mjs out.png 3 5 7.
import { chromium } from '@playwright/test';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
const browser = await chromium.launch({ executablePath: 'C:/Program Files/Google/Chrome/Application/chrome.exe' });
const page = await browser.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: 1 });
await page.goto(pathToFileURL(resolve('docs/mockups/thread-team.html')).href);
await page.waitForTimeout(600);
for (const n of process.argv.slice(3)) {
  await page.getByRole('button', { name: new RegExp(`^${n} · `) }).click();
  await page.waitForTimeout(700);
  await page.screenshot({ path: process.argv[2].replace('.png', `-${n}.png`) });
  console.log('--- step', n);
  console.log(await page.evaluate(() => document.querySelector('#view, main, body').innerText.replace(/\n+/g, ' | ').slice(0, 2500)));
}
await browser.close();
