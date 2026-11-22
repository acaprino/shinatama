// Poll the real conversation renderer while a reader focuses a session and selects message text.
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { chromium, expect } from '@playwright/test';
import { build } from 'vite';

const bundle = await build({
  configFile: false, logLevel: 'error', build: {
    write: false,
    lib: { entry: fileURLToPath(new URL('../src/thread-chat.ts', import.meta.url)), name: 'ThreadChat', formats: ['iife'], fileName: 'chat' },
  },
});
const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH;
const browser = await chromium.launch({ executablePath, channel: !executablePath && process.platform === 'win32' ? 'msedge' : undefined });
try {
  const page = await browser.newPage();
  await page.clock.install();
  // The canon's shell with its side panel (the inspector), where the conversation shows.
  await page.setContent('<main id="app" class="im-shell"><section class="im-panel im-inspector" aria-label="Side panel" hidden></section></main>');
  await page.addScriptTag({ content: bundle[0].output.find(chunk => chunk.type === 'chunk').code });
  await page.evaluate(() => {
    const session = name => ({ sessionId: name, title: name, memberName: name, state: 'running', connection: 'in_room' });
    const event = (id, kind = 'message') => ({ eventId: String(id), timestamp: '2025-01-01T00:00:00Z', kind, text: 'Message ' + id });
    window.fixture = { enabled: true, sessions: [session('alpha'), session('beta')], members: [], events: [event(1), event(2, 'join')] };
    window.makeEvent = event;
    window.chat = ThreadChat.createThreadChat({
      invoke: async () => structuredClone(window.fixture), notify() {}, title: thread => thread.id,
      tone: () => null, harnessLabel: name => name, focusSession() {}, seen() {},
    });
    window.chat.open({ id: 'first', projectId: 'project' });
  });
  const roster = page.getByRole('list', { name: 'Sessions of the thread' });
  const log = page.getByRole('list', { name: 'Conversation of the thread' });
  await expect(roster.getByRole('button')).toHaveCount(2);
  await page.locator('#thread-chat-to').selectOption('beta');
  await roster.getByRole('button').nth(1).focus();
  await page.evaluate(() => {
    const text = document.querySelector('.thread-chat-event p').firstChild;
    const range = new Range(); range.selectNodeContents(text);
    const selection = window.getSelection(); selection.removeAllRanges(); selection.addRange(range);
    window.retained = { button: document.activeElement, text, option: document.querySelector('#thread-chat-to option[value=beta]') };
    window.mutations = [];
    const observer = new MutationObserver(records => window.mutations.push(...records));
    for (const node of document.querySelectorAll('#thread-chat-to, #thread-chat-panel ul, #thread-chat-panel ol')) {
      observer.observe(node, { childList: true, characterData: true, attributes: true, subtree: true });
    }
  });
  await page.clock.runFor(3000);
  assert.equal(await page.evaluate(() => window.mutations.length), 0);
  await page.evaluate(() => { window.fixture.sessions.reverse(); window.fixture.sessions[0].wakes = 3; window.fixture.events.push(window.makeEvent(3)); });
  await page.clock.runFor(3000);
  await expect(roster.getByRole('button').first()).toBeFocused();
  await expect(roster.getByText(/woken 3/)).toBeVisible();
  await expect(log.locator('li')).toHaveCount(3);
  assert.ok(await page.evaluate(() => document.activeElement === window.retained.button
    && window.getSelection().anchorNode === window.retained.text
    && window.getSelection().toString() === 'Message 1'
    && document.querySelector('#thread-chat-to').selectedOptions[0] === window.retained.option));
  await page.locator('#thread-chat-filter').selectOption('messages');
  await expect(log.locator('li')).toHaveCount(2);
  await page.locator('#thread-chat-filter').selectOption('all');
  await expect(log.locator('li')).toHaveCount(3);
  await page.evaluate(() => {
    window.fixture.events = Array.from({ length: 400 }, (_, i) => window.makeEvent(i + 4));
    window.fixture.sessions = [];
  });
  await page.clock.runFor(3000);
  await expect(log.locator('li')).toHaveCount(400);
  assert.ok(await page.evaluate(() => Object.values(window.retained).every(node => !node.isConnected)));
  await page.evaluate(() => { window.fixture.events = []; window.chat.open({ id: 'second', projectId: 'project' }); });
  await expect(log.locator('li')).toHaveCount(0);
  await page.evaluate(() => window.chat.destroy());
  await expect(page.locator('#thread-chat-panel')).toHaveCount(0);
  console.log('Conversation polling retained focus, selection and unchanged rows, then pruned and reset them.');
} finally {
  await browser.close();
}
