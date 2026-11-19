// README screenshots of the team map (team-map*.png): the real window at 1920x1080 with the team commands stubbed as in team-map-smoke.mjs.
import { expect as plainExpect } from '@playwright/test';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { SESSIONS, installStub } from './team-map-drive.mjs';
import { desktopSize, env0, launch, makeProject } from './shoot-lib.mjs';

const expect = plainExpect.configure({ timeout: 15000 });
const { fixture, browser, page, ipc, stop } = await launch();
const shot = name => page.screenshot({ path: resolve(env0.out, name) });
try {
  await desktopSize(browser, page);
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');

  const projects = [['billing-service', 'feature/retry-backoff', 7], ['invoices-api', 'main', 0], ['checkout-web', 'fix/flaky-cart-test', 3], ['ledger-core', 'develop', 12],
    ['notify-worker', 'main', 1], ['docs-site', 'docs/team-map', 4], ['mobile-app', 'release/2.8', 0], ['infra-terraform', 'main', 2]];
  const records = [];
  for (const [name, branch, changed] of projects) records.push(await ipc('project_select', { path: await makeProject(fixture, name, branch, changed) }));
  const billing = records[0];
  await page.reload(); await expect(page.locator('#runtime-status')).toContainText('Local runtime');

  const titles = ['Billing rewrite', 'Refactor billing retries', 'Fix flaky checkout test', 'Invoice PDF layout', 'Migrate to Postgres 16', 'Webhook signature check', 'Dunning emails', 'Tax rounding bug'];
  for (const title of titles) {
    const thread = await ipc('thread_create', { request: { projectId: billing.id } });
    await ipc('thread_rename', { request: { projectId: billing.id, threadId: thread.id, expectedRevision: thread.revision, title } });
  }
  await ipc('project_select', { path: resolve(fixture, 'billing-service') });
  await page.reload(); await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  await page.screenshot({ path: resolve(env0.out, 'debug-sidebar.png') });
  const sidebar = page.locator('.thread-sidebar');
  const anchors = sidebar.locator('button.thread-sidebar-anchor');
  await expect(anchors).toHaveCount(titles.length);
  const rows = await anchors.evaluateAll(nodes => nodes.map(n => ({ id: n.dataset.worksetId, text: n.textContent })));
  console.log(JSON.stringify(rows));
  const worksetOf = title => rows.find(r => r.text.includes(title))?.id;
  const teamId = worksetOf('Billing rewrite');
  for (const title of ['Billing rewrite', 'Refactor billing retries', 'Webhook signature check', 'Dunning emails']) await ipc('thread_chat_set', { request: { worksetId: worksetOf(title), enabled: true } });
  await page.reload(); await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  const grip = await sidebar.locator('.thread-sidebar-resize').boundingBox();
  await page.mouse.move(grip.x + grip.width / 2, grip.y + 300); await page.mouse.down();
  await page.mouse.move(grip.x + 120, grip.y + 300, { steps: 8 }); await page.mouse.up();
  if (await page.locator('aside.leftovers-sidebar').isVisible()) await page.locator('#leftovers-toggle').click();
  await expect(page.locator('aside.leftovers-sidebar')).toBeHidden();

  // Saved teams (the real store), drawn as small maps.
  const names = 'abcdefghijklmnopqrstuvwxyz234567';
  let counter = 0; const id = () => 'a_' + Array.from({ length: 12 }, () => names[(counter++ * 7 + 3) % 32]).join('');
  const model = (program, m) => ({ program, connection: null, model: m });
  const agent = (name, program, m, x, y, notes = '') => ({ id: id(), session: null, model: model(program, m), name, notes, share: 'split', x, y });
  const work = (from, to, report = 'done', words = '') => ({ from: from.id, to: to.id, kind: 'work', report, words });
  const sees = (from, to, words = '') => ({ from: from.id, to: to.id, kind: 'observes', report: 'done', words });
  const save = (name, agents, links) => ipc('team_template_save', { request: { name, formation: { revision: 0, agents, links } } });
  {
    const [p, i, r, w] = [agent('planner', 'claude_code', 'Fable 5.2', 300, 40), agent('implementer', 'opencode', 'janura-qwen', 80, 260), agent('reviewer', 'claude_code', 'Sonnet 5.5', 520, 260), agent('watcher', 'codex', 'gpt-5.5', 700, 40)];
    await save('Plan, build, review', [p, i, r, w], [work(p, i, 'steps'), work(p, r), sees(w, p, 'explain it to me')]);
  }
  {
    const [p, a, b] = [agent('lead', 'claude_code', 'Fable 5.2', 300, 40), agent('worker-1', 'opencode', 'janura-qwen', 90, 260), agent('worker-2', 'pi', 'janura-qwen-mini', 520, 260)];
    await save('Lead + two workers', [p, a, b], [work(p, a, 'strict'), work(p, b, 'strict')]);
  }
  {
    const [a, b] = [agent('author', 'claude_code', 'Sonnet 5.5', 120, 150), agent('critic', 'codex', 'gpt-5.5', 560, 150)];
    await save('Writer and critic', [a, b], [work(a, b, 'steps', 'tell me before you merge')]);
  }
  {
    const [a, b, c] = [agent('spec', 'claude_code', 'Fable 5.2', 60, 150), agent('build', 'opencode', 'janura-qwen', 340, 150), agent('verify', 'pi', 'janura-qwen-mini', 620, 150)];
    await save('Spec, build, verify', [a, b, c], [work(a, b, 'steps'), work(b, c, 'strict')]);
  }
  {
    const [a, b, c, d] = [agent('captain', 'claude_code', 'Fable 5.2', 320, 30), agent('dev-1', 'opencode', 'janura-qwen', 60, 250), agent('dev-2', 'pi', 'janura-qwen-mini', 320, 250), agent('auditor', 'codex', 'gpt-5.5', 600, 250)];
    await save('Squad and auditor', [a, b, c, d], [work(a, b), work(a, c), sees(d, b), sees(d, c)]);
  }

  // The thread whose map is drawn: five sessions.
  SESSIONS.length = 0;
  SESSIONS.push(
    { sessionId: 's_aaaaaaaaaaaa', harness: 'claude_code', title: 'Fable 5.2', state: 'running', memberName: 'claude-1', connection: 'in_room', wakes: 0 },
    { sessionId: 's_bbbbbbbbbbbb', harness: 'opencode', title: 'janura-qwen', state: 'running', memberName: 'opencode-1', connection: 'in_room', wakes: 1 },
    { sessionId: 's_cccccccccccc', harness: 'pi', title: 'janura-qwen-mini', state: 'running', memberName: 'pi-1', connection: 'in_room', wakes: 0 },
    { sessionId: 's_dddddddddddd', harness: 'claude_code', title: 'Sonnet 5.5', state: 'running', memberName: 'claude-2', connection: 'in_room', wakes: 0 },
    { sessionId: 's_eeeeeeeeeeee', harness: 'codex', title: 'gpt-5.5', state: 'running', memberName: null, connection: 'launched', wakes: 0 });
  await installStub(page);
  const teamOf = wid => sidebar.locator('li.thread-sidebar-row', { has: page.locator(`button.thread-sidebar-anchor[data-workset-id="${wid}"]`) }).locator('.thread-sidebar-team');
  await teamOf(teamId).click();
  const map = page.locator('.team-map[data-view-id]:visible').first();
  const canvas = map.locator('.team-canvas');
  await expect(map).toBeVisible();
  const bar = name => map.locator('.team-bar').getByRole('button', { name, exact: true });
  const byTitle = t => map.locator('.team-card').filter({ has: page.locator('.team-card-name', { hasText: new RegExp(`^${t}$`) }) });
  const byId = i => map.locator(`.team-card[data-agent="${i}"]`);
  const chip = t => map.locator(`.team-session[data-session="${SESSIONS.find(x => x.title === t).sessionId}"]`);
  const box = map.locator('.team-box-link');
  const agentBox = map.locator('.team-box-agent');
  const menu = map.locator('.team-templates');
  const closeBoxes = async () => {
    if (await box.isVisible()) await box.getByRole('button', { name: 'Close the arrow box' }).click();
    if (await agentBox.isVisible()) await agentBox.getByRole('button', { name: 'Close the agent box' }).click();
  };
  const pull = async (from, to) => { await closeBoxes(); await byId(from).locator('.team-port').dragTo(byId(to)); };
  const place = async (title, left, top, newName) => {
    const area = await map.locator('.team-scroll').boundingBox();
    console.log('scroll', JSON.stringify(area));
    await chip(title).dragTo(canvas, { targetPosition: { x: Math.min(left + 95, Math.round(area.width) - 140), y: top + 39 } });
    const agentId = await byTitle(title).getAttribute('data-agent');
    await byTitle(title).locator('.team-card-name').click();
    await expect(agentBox).toBeVisible();
    await agentBox.locator('input[data-first="1"]').fill(newName);
    await closeBoxes();
    return agentId;
  };
  const planner = await place('Fable 5.2', 300, 230, 'planner');
  const implementer = await place('janura-qwen', 50, 50, 'implementer');
  const tester = await place('janura-qwen-mini', 520, 50, 'tester');
  const reviewer = await place('Sonnet 5.5', 300, 420, 'reviewer');
  const watcher = await place('gpt-5.5', 820, 40, 'watcher');
  await pull(planner, implementer);
  await box.locator('.team-seg button[data-report="steps"]').click();
  await pull(planner, tester);
  await pull(planner, reviewer);
  await pull(watcher, planner);
  await box.locator('.team-kinds button[data-kind="observes"]').click();
  await box.locator('input').fill('to explain it to me in plain words');
  await page.mouse.move(1000, 1000);
  await delay(800);
  await shot('team-map-raw.png');
  console.log('shot map');

  await closeBoxes();
  // Tucked in beside planner, so that the map also fits next to the Conversation panel.
  await byId(watcher).locator('.team-card-name').dragTo(canvas, { targetPosition: { x: 700, y: 160 } });
  await delay(500);
  await bar('Templates ▾').click();
  await expect(menu).toBeVisible();
  await expect(menu.locator('.team-template')).toHaveCount(5);
  await delay(500);
  await shot('team-map-templates-raw.png');
  console.log('shot templates');
  await menu.getByRole('button', { name: 'Close the templates' }).click();

  await bar('Apply').click();
  await expect(map.locator('.team-state')).toContainText('revision 1');
  await page.evaluate(() => {
    window.__teamStub.release();
    window.__teamStub.events = [
      { eventId: '1', timestamp: '2026-11-18T10:00:00Z', kind: 'message', actor: 'shinatama', actorHost: true, recipients: ['claude-1'], text: 'TEAM revision 1. You are planner, the lead: you give work to implementer (tell what you are doing at every step), tester and reviewer. watcher observes you.', signal: null, reason: null },
      { eventId: '2', timestamp: '2026-11-18T10:00:01Z', kind: 'message', actor: 'shinatama', actorHost: true, recipients: ['opencode-1'], text: 'TEAM revision 1. You are implementer, a worker: wait for work from planner, do it, and tell what you are doing at every step.', signal: null, reason: null },
      { eventId: '3', timestamp: '2026-11-18T10:00:03Z', kind: 'message', actor: 'shinatama', actorHost: true, recipients: [], text: 'TEAM revision 1. New: planner gives work to implementer, tester, reviewer; watcher observes planner', signal: null, reason: null },
      { eventId: '4', timestamp: '2026-11-18T10:01:12Z', kind: 'message', actor: 'claude-1', recipients: ['opencode-1'], text: 'Run docs/plans/billing-retries.md. Start with task 1 and tell me when each task is green.', signal: null, reason: null },
      { eventId: '5', timestamp: '2026-11-18T10:01:20Z', kind: 'message', actor: 'opencode-1', recipients: ['claude-1'], text: 'READY. Starting task 1: exponential backoff in retry.ts.', signal: null, reason: null },
    ];
  });
  await expect(map.locator('.team-state')).toHaveText('revision 1 · everyone was told', { timeout: 20000 });
  await sidebar.locator('li.thread-sidebar-row', { has: page.locator(`button.thread-sidebar-anchor[data-workset-id="${teamId}"]`) }).locator('.thread-sidebar-chat').click();
  await page.mouse.move(1000, 1000);
  await delay(1200);
  await shot('team-map-apply-raw.png');
  console.log('shot apply');
} finally { await stop(); }
