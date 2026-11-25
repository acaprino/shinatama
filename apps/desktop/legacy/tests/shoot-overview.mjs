// README screenshots of the whole app (shinatama-overview.png, shinatama-widget.png): the real window at 1920x1080 on fixture projects, threads and PowerShell panes that replay agent output.
import { expect as plainExpect } from '@playwright/test';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { startSession } from './session-flow.mjs';
import { desktopSize, env0, launch, makeProject } from './shoot-lib.mjs';

process.env.SHINATAMA_XIOX = 'off';
const expect = plainExpect.configure({ timeout: 20000 });
const { fixture, browser, page, ipc, stop } = await launch();
const shot = (name, target = page) => target.screenshot({ path: resolve(env0.out, name) });
const e = '\x1b';
const color = n => s => `${e}[38;5;${n}m${s}${e}[0m`;
const [coral, green, yellow, cyan, gray, red] = [color(209), color(78), color(221), color(80), color(245), color(203)];
const bold = s => `${e}[1m${s}${e}[0m`;
const ok = s => `${green('+')} ${s}`;
// What each pane replays: the output of an agent, at most 52 columns wide.
const panes = {
  Codex: [
    `${bold('>_ OpenAI Codex')} ${gray('(gpt-5.5)')}`, '',
    `${cyan('reviewing')} src/retry.ts src/charge.ts`,
    `- retry.ts:42    backoff overflows after 31`, `                 attempts: clamp the exponent`,
    `- charge.ts:118  idempotency key reused across`, `                 customers`,
    green('2 findings, 0 blocking'),
  ],
  Claude: [
    `${coral('*')} ${bold('Claude Code')}  ${gray('Fable 5.2 - billing-service')}`, '',
    `${bold('> run docs/plans/billing-retries.md')}`, '',
    `I will hand the tasks to implementer, and keep`, `reviewer for the diff:`, '',
    `  1. exponential backoff in retry.ts`, `  2. jitter and a cap of 5 attempts`, `  3. idempotency key on the charge`, `  4. webhook retries use the same policy`, '',
    `${gray('[to implementer]')} Task 1: backoff in retry.ts.`, `  Tell me what you do at every step.`,
    `${gray('[implementer]')} READY. editing src/retry.ts`, `${gray('[implementer]')} npm test -- retry   ${green('14 passed')}`, '',
    ok(`Task 1  backoff               ${green('green')}`), ok(`Task 2  jitter and cap        ${green('green')}`), `${yellow('*')} Task 3  idempotency key       ${yellow('running')}`, '',
    `${gray('[to reviewer]')} the diff of tasks 1-2 is ready.`, `${gray('[reviewer]')} 2 findings, 0 blocking.`,
    `${gray('[watcher]')} explaining the retry policy to you:`, `  failures wait 1s, 2s, 4s ... capped at 30s,`, `  each wait gets a little random jitter.`,
  ],
  OpenCode: [
    `${bold('opencode')}  ${gray('janura-qwen - billing-service')}`, '',
    `${cyan('READY')}  waiting for work from planner`,
    `${cyan('STEP')}   editing src/retry.ts ${green('(+38 -12)')}`, `${cyan('STEP')}   npm test -- retry   ${green('14 passed')}`,
    `${cyan('STEP')}   editing src/charge.ts`, `${cyan('STEP')}   npm test -- charge  ${green('9 passed')}`,
    `${green('DONE')}   tasks 1-3 are green`,
  ],
  Pi: [
    `${bold('pi')}  ${gray('janura-qwen-mini - billing-service')}`, '',
    `tester: running the suite on the new policy`, `  checkout.spec.ts   ${green('ok')}`, `  webhooks.spec.ts   ${green('ok')}`, `  dunning.spec.ts    ${yellow('flaky, retrying')}`, `  dunning.spec.ts    ${green('ok')}`, `${green('27 passed')}  ${gray('0 failed')}`,
  ],
};
try {
  await desktopSize(browser, page);
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');

  const projects = [['billing-service', 'feature/retry-backoff', 7], ['invoices-api', 'main', 0], ['checkout-web', 'fix/flaky-cart-test', 3], ['ledger-core', 'develop', 12],
    ['notify-worker', 'main', 1], ['docs-site', 'docs/team-map', 4], ['mobile-app', 'release/2.8', 0], ['infra-terraform', 'main', 2]];
  const records = [];
  for (const [name, branch, changed] of projects) records.push(await ipc('project_select', { path: await makeProject(fixture, name, branch, changed) }));
  for (const index of [0, 3, 2]) await ipc('project_pin', { projectId: records[index].id, pinned: true }).catch(error => console.log('pin', String(error).slice(0, 100)));
  const billing = records[0];
  await ipc('project_select', { path: resolve(fixture, 'billing-service') });

  const titles = ['Billing rewrite', 'Refactor billing retries', 'Fix flaky checkout test', 'Invoice PDF layout', 'Migrate to Postgres 16', 'Webhook signature check', 'Dunning emails', 'Tax rounding bug'];
  for (const title of titles) {
    const thread = await ipc('thread_create', { request: { projectId: billing.id } });
    await ipc('thread_rename', { request: { projectId: billing.id, threadId: thread.id, expectedRevision: thread.revision, title } });
  }
  await page.reload(); await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  const sidebar = page.locator('.thread-sidebar');
  const anchors = sidebar.locator('button.thread-sidebar-anchor');
  await expect(anchors).toHaveCount(titles.length);
  const rows = await anchors.evaluateAll(nodes => nodes.map(n => ({ id: n.dataset.worksetId, text: n.textContent })));
  for (const title of ['Billing rewrite', 'Refactor billing retries', 'Webhook signature check', 'Dunning emails']) await ipc('thread_chat_set', { request: { worksetId: rows.find(r => r.text.includes(title)).id, enabled: true } });
  await page.reload(); await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  const grip = await sidebar.locator('.thread-sidebar-resize').boundingBox();
  await page.mouse.move(grip.x + grip.width / 2, grip.y + 300); await page.mouse.down();
  await page.mouse.move(grip.x + 120, grip.y + 300, { steps: 8 }); await page.mouse.up();
  if (await page.locator('aside.leftovers-sidebar').isVisible()) await page.locator('#leftovers-toggle').click();

  // Four PowerShell panes that replay what an agent prints (the replay is a script the shell runs, so only its output stays on screen).
  const demo = resolve(fixture, 'demo'); await mkdir(demo, { recursive: true });
  const ids = {};
  for (const [label, lines] of Object.entries(panes)) {
    const before = (await ipc('terminal_list')).map(t => t.id);
    await (await startSession(page, { project: resolve(fixture, 'billing-service'), harness: 'powershell' })).done(60000);
    await expect.poll(async () => (await ipc('terminal_list')).length, { timeout: 60000 }).toBe(before.length + 1);
    const id = (await ipc('terminal_list')).map(t => t.id).find(x => !before.includes(x));
    ids[label] = id;
    const file = resolve(demo, `${label}.ps1`);
    await writeFile(file, `function global:prompt { "${e}[38;5;209m>${e}[0m " }\r\nClear-Host\r\n$t = @'\r\n${lines.join('\r\n')}\r\n'@\r\n[Console]::Out.WriteLine($t)\r\n`, 'utf8');
    await delay(2000);
    await ipc('terminal_write', { id, data: [...Buffer.from(`iex (Get-Content -Raw '${file}')\r`)] });
    await delay(1000);
  }
  // Tile: Codex top-left, Claude Code on the right, Pi and OpenCode below Codex.
  const tab = id => page.locator(`.im-tile-tab[data-view-id="${id}"]`);
  await tab(ids.Claude).focus(); await page.keyboard.press('Alt+Shift+ArrowRight'); await delay(500);
  await tab(ids.OpenCode).focus(); await page.keyboard.press('Alt+Shift+ArrowDown'); await delay(500);
  await tab(ids.Pi).focus(); await page.keyboard.press('Alt+Shift+ArrowDown'); await delay(1500);
  await page.getByRole('button', { name: 'Overview', exact: true }).click();
  await delay(1500);
  await page.mouse.move(900, 1075);
  await shot('shinatama-overview-raw.png');
  console.log('shot overview');

  // The widget: Shinatama's own notification.
  const widget = browser.contexts()[0].pages().find(p => p.url().includes('widget.html'));
  if (!widget) throw new Error('no widget page');
  await ipc('attention_configure', { enabled: true, muted: true });
  let kind = 'input';
  try { await ipc('attention_preview', { kind }); } catch (error) { console.log('preview input refused', String(error).slice(0, 100)); kind = 'update'; await ipc('attention_preview', { kind }); }
  await delay(1500);
  console.log('widget text', JSON.stringify(await widget.evaluate(() => document.body.innerText)));
  const size = await widget.evaluate(() => ({ w: document.documentElement.scrollWidth, h: document.documentElement.scrollHeight }));
  const cdp = await widget.context().newCDPSession(widget);
  await cdp.send('Emulation.setDeviceMetricsOverride', { width: size.w, height: size.h, deviceScaleFactor: 2, mobile: false });
  await delay(600);
  await widget.screenshot({ path: resolve(env0.out, 'shinatama-widget-raw.png'), omitBackground: true });
  console.log('shot widget', kind, JSON.stringify(size));
} finally { await stop(); }
