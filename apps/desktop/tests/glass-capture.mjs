// Register U9: the glass as it is on screen. Opens a PowerShell session in the real desktop app
// (isolated data folder), prints text in the default and ANSI colours, then photographs the window
// from the screen, over whatever is behind it: put a dark or a bright window there first. The app
// window is raised above every other window for a few seconds while it runs.
// Raising can fail (seen in two of six runs): if the title bar colour printed is not the app's, the
// picture is of the window in front; discard the run and repeat it.
// Usage: node apps/desktop/tests/glass-capture.mjs [alpha ...]   (hex, e.g. 94 c7: tries those
// terminal opacities live; with none, photographs the build as it is). Output: captures and the
// colours of terminal, title bar and sidebar, under %LOCALAPPDATA%\Temp\shinatama-glass-*.
import { createRequire } from 'node:module';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { createServer } from 'node:net';

const here = dirname(fileURLToPath(import.meta.url)); const root = resolve(here, '../../..');
const { chromium } = createRequire(resolve(root, 'package.json'))('@playwright/test');
// The data folder must be outside any Git project, or the app refuses it as storage.
const fixture = await mkdtemp(resolve(process.env.LOCALAPPDATA, 'Temp', 'shinatama-glass-'));
const out = resolve(fixture, 'captures'); await mkdir(out, { recursive: true });
const project = resolve(fixture, 'projects', 'shinatama'); await mkdir(project, { recursive: true });
const reservation = createServer(); await new Promise(accept => reservation.listen(0, '127.0.0.1', accept));
const port = reservation.address().port; await new Promise(accept => reservation.close(accept));
const app = spawn(resolve(root, 'target/debug/shinatama-desktop.exe'), [], { cwd: root, stdio: 'ignore',
  env: { ...process.env, SHINATAMA_HOME: resolve(fixture, 'data'), WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, 'webview'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` } });
const log = []; let browser;
try {
  for (let attempt = 0; attempt < 60 && !browser; attempt++) { try { browser = await chromium.connectOverCDP('http://127.0.0.1:' + port, { timeout: 1000 }); } catch { await delay(500); } }
  const context = browser.contexts()[0];
  const main = () => context.pages().find(page => !page.url().includes('widget.html') && /localhost/.test(page.url()));
  for (const deadline = Date.now() + 30000; !main() && Date.now() < deadline;) await delay(250);
  const page = main(); await page.waitForSelector('#runtime-status'); await delay(2500);
  await page.locator('#project').click(); await page.getByRole('button', { name: 'Project roots…' }).click();
  await page.locator('#folder').fill(project); await page.getByRole('button', { name: 'Use folder', exact: true }).click(); await delay(800);
  await page.locator('#new-terminal').click(); await page.locator('#session-menu [data-harness="powershell"]').click();
  await page.waitForSelector('.terminal-view', { timeout: 90000 }); await delay(3500);
  await page.locator('.xterm-helper-textarea').first().focus();
  await page.keyboard.type("Get-ChildItem C:\\Windows | Select-Object -First 9; Write-Host 'muted grey' -ForegroundColor DarkGray; Write-Host 'blue' -ForegroundColor Blue; Write-Host 'red' -ForegroundColor Red; Write-Host 'green' -ForegroundColor Green");
  await page.keyboard.press('Enter'); await delay(2500);
  log.push('container background: ' + await page.evaluate(() => document.querySelector('.terminal-view')?.style.background));
  const capture = name => execFileSync('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', resolve(here, 'glass-capture.ps1'),
    '-ProcessId', String(app.pid), '-Out', resolve(out, name + '.png')], { encoding: 'utf8' }).trim();
  for (const alpha of process.argv.slice(2).length ? process.argv.slice(2) : [null]) {
    if (alpha) await page.evaluate(alpha => { for (const view of document.querySelectorAll('.terminal-view')) { const base = getComputedStyle(view).backgroundColor.match(/[\d.]+/g).slice(0, 3).map(Number); view.style.background = `rgb(${base.join(' ')} / ${parseInt(alpha, 16) / 255})`; } }, alpha);
    await delay(400); const label = alpha ?? 'as-built';
    log.push(`${label} -> ${capture(label)}`);
  }
  log.push('captures in ' + out);
} catch (error) { log.push('FATAL ' + String(error.message).split('\n')[0]); }
finally { console.log(log.join('\n')); await browser?.close().catch(() => {}); app.kill(); }
