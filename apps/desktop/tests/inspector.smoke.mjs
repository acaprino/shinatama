// Smoke test of the inspector: its page on Vite with the mocked commands (src/mock/files.ts, sessions.ts, inspector.ts), driven by what a
// person and a screen reader see (roles and names, never a class). The project files (the tree that reads a folder when it opens, the search
// by name, a file edited and saved, a file that changed on disk, the question before unsaved changes are lost, Markdown's Preview), what
// agents left on disk, the running sessions, the stack of panels of a session (its facts, its conversation, a search in it, a raw entry,
// an export, a hop) with the way back, the archive browser, the settings of a thread, both schemes. Screenshots go under tests/out/.
//   node apps/desktop/tests/inspector.smoke.mjs
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { createServer } from 'vite';

const here = fileURLToPath(new URL('.', import.meta.url));
const out = `${here}out/`;
mkdirSync(out, { recursive: true });
const server = await createServer({ configFile: `${here}../vite.config.ts`, server: { port: 1439, strictPort: false, watch: null }, logLevel: 'error' });
await server.listen();
const url = server.resolvedUrls.local[0];
const browser = await chromium.launch({ args: ['--disable-lcd-text'] });
const failures = [];

async function open(scheme = 'dark', view = 'Workspace') {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 }, colorScheme: scheme, deviceScaleFactor: 1 });
  await page.addInitScript((value) => { try { localStorage.setItem('shinatama.theme', value); } catch { /* no storage */ } }, scheme);
  page.on('pageerror', (error) => failures.push(`pageerror: ${error}`));
  page.on('console', (message) => {
    // The workspace's terminal tab hands `fill` to an element of the page (a warning of React, not of this area): said once by its owner.
    if ((message.type() === 'error' || message.type() === 'warning') && !/non-boolean attribute/.test(message.text())) failures.push(`console: ${message.text()}`);
  });
  await page.goto(url);
  await page.getByText(/^Local runtime/).waitFor();
  await page.waitForTimeout(600);
  await clearNotices(page);
  if (view !== 'Workspace') await page.getByRole('button', { name: view }).or(page.getByRole('tab', { name: view })).first().click();
  return page;
}
/** A command of the palette (Ctrl+K), by what the person types. */
async function command(page, text) {
  await page.getByText(/^Local runtime/).click();
  await page.keyboard.press('Control+k');
  await page.getByRole('combobox').or(page.getByRole('searchbox')).first().fill(text);
  await page.keyboard.press('Enter');
}
/** The notices of the start (the terminals that were reconnected) sit over the foot of the inspector: closed before the panels are used. */
async function clearNotices(page) {
  const close = page.getByRole('button', { name: /^Close the notice/ });
  for (let at = 0; at < 6 && await close.count(); at++) { await close.first().click(); await page.waitForTimeout(250); }
}
const inspector = (page, title) => page.getByRole('heading', { name: title, exact: true });
const confirm = (page) => page.getByRole('alertdialog').or(page.getByRole('dialog'));
const editor = (page, path) => page.getByRole('textbox', { name: `Source of ${path}` });

try {
  // --- the project files ----------------------------------------------------------------------------------------------------------
  let page = await open();
  await command(page, 'Project files');
  await inspector(page, 'Project files').waitFor();
  assert.equal(await page.getByRole('combobox', { name: 'Project' }).textContent(), 'shinatama', 'the project the window works in');
  const tree = page.getByRole('tree', { name: 'Folder tree' });
  await tree.getByRole('treeitem', { name: /^src/ }).click();
  await tree.getByRole('treeitem', { name: /^retry\.ts/ }).click();
  await editor(page, 'src/retry.ts').waitFor();
  assert.ok(await page.getByText('TypeScript', { exact: true }).isVisible(), 'the language comes from the name of the file');
  assert.ok(await page.getByText(/^9 lines · edit and press Save to write it/).isVisible());
  assert.equal(await page.getByRole('button', { name: 'Save' }).isDisabled(), true, 'nothing to save yet');
  await editor(page, 'src/retry.ts').click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('// edited');
  assert.ok(await page.getByText('Unsaved changes').isVisible());
  await page.getByRole('button', { name: 'Save' }).waitFor();
  await page.keyboard.press('Control+s');
  await page.getByText('Saved.', { exact: true }).waitFor();
  assert.equal(await page.getByText('Unsaved changes').count(), 0);
  await page.screenshot({ path: `${out}inspector-files.png` });

  // A file that changed on disk behind the panel's back is replaced only when the person says so.
  await page.evaluate(async () => (await import('/src/mock/files.ts')).changeOnDisk('src/retry.ts', 'changed by an agent'));
  await editor(page, 'src/retry.ts').click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type(' again');
  await page.keyboard.press('Control+s');
  await confirm(page).getByText('The file changed on disk').waitFor();
  assert.ok(await confirm(page).getByRole('button', { name: 'Overwrite src/retry.ts' }).isVisible(), 'the button carries the verb and the object');
  await confirm(page).getByRole('button', { name: 'Cancel' }).click();
  await page.getByText('Not saved: the file on disk is newer. Your text is still here.').waitFor();
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await confirm(page).getByRole('button', { name: 'Overwrite src/retry.ts' }).click();
  await page.getByText('Saved.', { exact: true }).waitFor();

  // The search by name replaces the tree with what it found.
  await page.getByRole('searchbox', { name: 'Find a file by name' }).fill('read');
  await page.getByText('1 match for “read”.').waitFor();
  assert.equal(await tree.getByRole('treeitem', { name: /^retry\.ts/ }).count(), 0, 'only the matches');
  await tree.getByRole('treeitem', { name: /^README\.md/ }).click();
  await page.getByRole('radio', { name: 'Preview' }).or(page.getByRole('button', { name: 'Preview' })).first().click();
  await page.getByRole('heading', { name: 'shinatama' }).or(page.getByText('The window.')).first().waitFor();
  assert.equal(await page.getByRole('textbox', { name: 'Source of README.md' }).count(), 0, 'the preview takes the place of the source');
  await page.getByRole('searchbox', { name: 'Find a file by name' }).fill('');
  await tree.getByRole('treeitem', { name: /^data\.bin/ }).waitFor();
  await tree.getByRole('treeitem', { name: /^data\.bin/ }).click();
  await page.getByText(/data\.bin is not text \(2,048 bytes\); it is not opened or run/).waitFor();

  // Closing with something unsaved asks first, and Cancel keeps the files open.
  await tree.getByRole('treeitem', { name: /^src/ }).click();
  await tree.getByRole('treeitem', { name: /^retry\.ts/ }).click();
  await editor(page, 'src/retry.ts').click();
  await page.keyboard.type('x');
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await confirm(page).getByText('Discard unsaved changes?').waitFor();
  assert.ok(await confirm(page).getByText(/src\/retry\.ts: Closing discards them/).isVisible(), 'it names the file and the consequence');
  await confirm(page).getByRole('button', { name: 'Cancel' }).click();
  assert.ok(await inspector(page, 'Project files').isVisible(), 'Cancel keeps the files open');
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await confirm(page).getByRole('button', { name: 'Discard changes' }).click();
  await inspector(page, 'Project files').waitFor({ state: 'detached' });

  // --- what agents left on disk, and the sessions that run ---------------------------------------------------------------------------
  await command(page, 'What agents left');
  await inspector(page, 'Leftovers').waitFor();
  assert.ok(await page.getByText('shinatama-wt-retry', { exact: true }).isVisible());
  assert.ok(await page.getByText('Attention: too many new worktrees').isVisible(), 'what asks for attention is said in words');
  assert.ok(await page.getByRole('button', { name: 'Show in Explorer' }).first().isVisible());
  assert.ok(await page.getByRole('button', { name: /^Go to Claude Code/ }).isVisible(), 'the only agent that was open then');
  assert.ok(await page.getByRole('button', { name: 'Copy clean command' }).isVisible());
  await page.getByRole('button', { name: 'Check now' }).click();
  await page.getByText('Xiox is checking shinatama…').waitFor();
  await page.screenshot({ path: `${out}inspector-leftovers.png` });
  await command(page, 'Shinatama windows');
  await inspector(page, 'Running sessions').waitFor();
  assert.equal(await inspector(page, 'Leftovers').count(), 0, 'a panel opened from a command takes the inspector');
  assert.equal(await page.getByRole('button', { name: /^Back to/ }).count(), 0, 'and has no way back to the one it replaced');
  assert.ok(await page.getByText('Possible conflict').isVisible());
  assert.ok(await page.getByText('Both Claude Code (aaaa) and Codex (bbbb) changed src/retry.ts').isVisible());
  assert.ok(await page.getByRole('button', { name: 'Focus Claude Code aaaa' }).isVisible());
  await page.screenshot({ path: `${out}inspector-instances.png` });

  // --- the settings of a thread, from its row --------------------------------------------------------------------------------------------
  await page.getByRole('button', { name: 'Settings of Terminal history' }).click();
  await inspector(page, 'Settings of Terminal history').waitFor();
  assert.equal(await page.getByRole('textbox', { name: 'Name' }).inputValue(), 'Terminal history');
  assert.ok(await page.getByRole('heading', { name: 'Sessions (2)' }).isVisible());
  assert.ok(await page.getByRole('button', { name: 'Show Claude Code' }).isVisible());
  await page.getByRole('textbox', { name: 'Name' }).fill('History of the terminal');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await inspector(page, 'Settings of History of the terminal').waitFor();
  await page.getByRole('button', { name: 'Rename Codex' }).click();
  await page.getByRole('textbox', { name: 'New name of Codex' }).fill('Reviewer');
  await page.keyboard.press('Enter');
  await page.getByRole('button', { name: 'Show Reviewer' }).waitFor();
  await page.getByRole('button', { name: 'Delete thread' }).click();
  await confirm(page).getByText('Delete the thread History of the terminal?').waitFor();
  assert.ok(await confirm(page).getByText(/with its 2 sessions, closed ones included/).isVisible(), 'the dialog names the consequence');
  await confirm(page).getByRole('button', { name: 'Cancel' }).click();
  await page.getByText('Sessions talk to each other', { exact: true }).click();
  await page.getByRole('button', { name: 'Open the conversation' }).waitFor();
  await page.screenshot({ path: `${out}inspector-thread.png` });
  await page.getByRole('button', { name: 'Archive', exact: true }).click();
  await page.getByText(/is archived\. It is listed under Show archived threads\./).waitFor();
  await inspector(page, 'Settings of History of the terminal').waitFor({ state: 'detached' });
  await page.close();

  // --- the stack of a session ----------------------------------------------------------------------------------------------------------------
  page = await open('dark', 'Sessions');
  await page.getByRole('row', { name: /Retry backoff for the charge worker/ }).click();
  await inspector(page, 'Session').waitFor();
  await page.getByRole('button', { name: 'Open conversation' }).click();
  await inspector(page, 'Session detail').waitFor();
  assert.ok(await page.getByRole('button', { name: 'Back to Session' }).isVisible(), 'a panel opened from inside another has the way back');
  await page.getByRole('tab', { name: 'Overview' }).click();
  assert.ok(await page.getByText('2.1.4').isVisible(), 'the harness version');
  assert.ok(await page.getByText('31 over 12 turns'.replace('31 ', '31 ')).isVisible() || await page.getByText(/31 over 12 turns/).isVisible(), 'the model usage as recorded');
  assert.ok(await page.getByText(/Not observed yet/).isVisible(), 'what the model proxy did for it');
  await page.getByRole('tab', { name: 'Lineage' }).click();
  assert.ok(await page.getByRole('button', { name: 'cc-writers' }).isEnabled(), 'a child that is recorded can be opened');
  await page.getByRole('tab', { name: 'Transcript' }).click();
  assert.ok(await page.getByText(/1–5 of 7 events/).isVisible());
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await page.getByText(/6–7 of 7 events/).waitFor();
  await page.getByRole('button', { name: 'Previous', exact: true }).click();
  await page.getByText(/1–5 of 7 events/).waitFor();

  // A search in the session opens its transcript at the event.
  await page.getByRole('button', { name: 'Find in conversation' }).click();
  await inspector(page, 'Find in this session').waitFor();
  await page.getByRole('searchbox', { name: 'Literal text' }).fill('zzz');
  await page.getByRole('button', { name: 'Search this session' }).click();
  await page.getByText('No matching canonical text in this session.').waitFor();
  await page.getByRole('searchbox', { name: 'Literal text' }).fill('add a test');
  await page.getByRole('button', { name: 'Search this session' }).click();
  await page.getByRole('list', { name: 'Session search matches' }).getByRole('button').first().click();
  await inspector(page, 'Session detail').waitFor();
  await page.getByText('6–7 of 7 events'.replace('6–7', '7–7')).or(page.getByText(/7–7 of 7 events/)).first().waitFor();
  assert.ok(await page.getByText('Thanks. Add a test for the idempotency key.').isVisible(), 'the page of the match');

  // One raw entry only behind a warning that is checked.
  await page.getByRole('button', { name: /^RAW entry/ }).first().click();
  await inspector(page, 'RAW native entry').waitFor();
  assert.equal(await page.getByRole('button', { name: 'Load this RAW entry' }).isDisabled(), true, 'not before the warning is checked');
  assert.equal(await page.getByText(/native record/).count(), 0, 'opening reads nothing');
  await page.getByRole('checkbox', { name: /I understand/ }).click();
  await page.getByRole('button', { name: 'Load this RAW entry' }).click();
  await page.getByText(/"role": "assistant"/).waitFor();
  await page.getByRole('checkbox', { name: /I understand/ }).click();
  assert.equal(await page.getByText(/"role": "assistant"/).count(), 0, 'unchecking clears what was read');
  await page.getByRole('button', { name: 'Back to Session detail' }).click();

  // Export says what it writes, and what it did not.
  await page.getByRole('button', { name: 'Export…' }).click();
  await inspector(page, 'Export session').waitFor();
  await page.getByRole('radio', { name: 'Portable', exact: true }).click();
  await page.getByRole('button', { name: 'Save as…' }).click();
  await page.getByText('Saved 48,210 bytes.').waitFor();
  assert.ok(await page.getByText('C:\\Users\\me\\Documents\\retry-backoff.stx.jsonl').isVisible());
  assert.ok(await page.getByText('3 reasoning, 2 metadata records').isVisible(), 'what was left out');
  await page.screenshot({ path: `${out}inspector-export.png` });
  await page.getByRole('button', { name: 'Back to Session detail' }).click();

  // The hop shows the exact seed before it sends anything.
  await page.getByRole('button', { name: 'Hop…' }).click();
  await inspector(page, 'Hop to another agent').waitFor();
  assert.equal(await page.getByRole('button', { name: /^Hop to / }).count(), 0, 'nothing can be sent before a preview');
  await page.getByRole('radio', { name: 'Pi', exact: true }).click();
  await page.getByRole('button', { name: 'Prepare preview' }).click();
  await page.getByText('Exact seed').waitFor();
  assert.ok(await page.getByText(/Carry the retry policy to the webhook handler too\./).isVisible(), 'the seed, as it will be sent');
  assert.ok(await page.getByText('Reasoning', { exact: true }).isVisible(), 'what is lost');
  assert.ok(await page.getByRole('button', { name: 'Copy hand-off prompt' }).isVisible());
  await page.getByRole('radio', { name: 'OpenCode', exact: true }).click();
  assert.equal(await page.getByText('Exact seed').count(), 0, 'another destination needs its own preview');
  await page.getByRole('radio', { name: 'Pi', exact: true }).click();
  await page.getByRole('button', { name: 'Prepare preview' }).click();
  await page.getByText('Exact seed').waitFor();
  await page.screenshot({ path: `${out}inspector-hop.png` });

  // The way back, one panel at a time: the conversation at the match, the search whose match it was, the conversation, the facts.
  await page.getByRole('button', { name: 'Back to Session detail' }).click();
  await inspector(page, 'Session detail').waitFor();
  assert.ok(await page.getByText(/7–7 of 7 events/).isVisible(), 'the page of the match is as it was left');
  await page.getByRole('button', { name: 'Back to Find in this session' }).click();
  await inspector(page, 'Find in this session').waitFor();
  assert.ok(await page.getByRole('list', { name: 'Session search matches' }).isVisible());
  await page.getByRole('button', { name: 'Back to Session detail' }).click();
  await page.getByRole('button', { name: 'Back to Session', exact: true }).click();
  await inspector(page, 'Session').waitFor();
  assert.equal(await page.getByRole('button', { name: /^Back to/ }).count(), 0, 'nothing below the first panel');
  await page.getByRole('button', { name: 'Open conversation' }).click();
  await inspector(page, 'Session detail').waitFor();
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await inspector(page, 'Session').waitFor();

  // --- the archive browser -----------------------------------------------------------------------------------------------------------------------------------
  await page.getByRole('navigation', { name: 'Stores' }).getByText('Archive browser…').click();
  await inspector(page, 'Archive browser').waitFor();
  assert.equal(await page.getByRole('list', { name: 'Session metadata' }).count(), 0, 'an archive is chosen, never assumed');
  await page.getByRole('button', { name: 'Load archives' }).click();
  await page.getByRole('combobox', { name: 'Archive' }).click();
  await page.getByRole('option', { name: /Claude Code/ }).click();
  const list = page.getByRole('list', { name: 'Session metadata' });
  await list.waitFor();
  assert.equal(await list.getByRole('button').count(), 3, 'the first page of an archive of four');
  await page.getByText('1–3 of 4 · 3 shown').waitFor();
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await page.getByText('4–4 of 4 · 1 shown').waitFor();
  await page.getByRole('button', { name: 'Previous', exact: true }).click();
  await page.getByRole('radio', { name: 'Projects / sessions' }).or(page.getByRole('button', { name: 'Projects / sessions' })).first().click();
  await page.getByRole('heading', { name: /C:\\work\\xiox/ }).waitFor();
  await page.getByRole('searchbox', { name: 'Filter this page' }).fill('retry');
  assert.equal(await page.getByRole('heading', { name: /C:\\work\\xiox/ }).count(), 0, 'the filter narrows the page');
  await page.getByRole('button', { name: /Retry backoff/ }).click();
  await page.getByRole('button', { name: 'Open detail' }).click();
  await inspector(page, 'Session detail').waitFor();
  await page.getByRole('button', { name: 'Back to Archive browser' }).click();
  await page.getByRole('combobox', { name: 'Agent' }).click();
  await page.getByRole('option', { name: 'Pi' }).click();
  await page.getByRole('textbox', { name: 'Archive folder' }).fill('C:\\pi');
  await page.getByRole('button', { name: 'Add archive' }).click();
  await page.getByText(/Archive registered/).waitFor();
  await page.screenshot({ path: `${out}inspector-archives.png` });

  // --- the search index ----------------------------------------------------------------------------------------------------------------------------------
  await page.getByRole('navigation', { name: 'Stores' }).getByText('Search index…').click();
  await inspector(page, 'Search index').waitFor();
  assert.ok(await page.getByText('1,284 sessions indexed · 3 with deleted source · 30.0 MiB').isVisible());
  await page.getByRole('button', { name: 'Index sessions' }).last().click();
  await page.getByText(/Finished: 2\/2 archives/).waitFor();
  await page.close();

  // --- light ---------------------------------------------------------------------------------------------------------------------------------------------------
  page = await open('light');
  await command(page, 'Project files');
  await inspector(page, 'Project files').waitFor();
  await page.getByRole('tree', { name: 'Folder tree' }).getByRole('treeitem', { name: /^src/ }).click();
  await page.getByRole('tree', { name: 'Folder tree' }).getByRole('treeitem', { name: /^retry\.ts/ }).click();
  await editor(page, 'src/retry.ts').waitFor();
  await page.screenshot({ path: `${out}inspector-files-light.png` });
  await page.close();
} catch (error) {
  failures.push(String(error?.stack ?? error));
} finally {
  await browser.close();
  await server.close();
}

if (failures.length) {
  console.error(failures.join('\n'));
  process.exit(1);
}
console.log('inspector smoke: ok');
