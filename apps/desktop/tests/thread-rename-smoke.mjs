// One private-app proof; root coordinates the build and native execution.
// Only the synthetic PowerShell session created here receives input.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { startSession } from './session-flow.mjs';

if (process.platform !== 'win32') throw new Error('This proof requires Windows.');
const workspace = process.cwd();
const packageExecutable = process.env.SHINATAMA_SMOKE_APP_EXE;
if (packageExecutable && !isAbsolute(packageExecutable)) throw new Error('Use an absolute package executable path.');
const executable = packageExecutable || process.env.SHINATAMA_EXE || resolve(workspace, 'target/debug/shinatama-desktop.exe');
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-thread-rename-'));
const data = resolve(fixture, 'data'); const projectPath = resolve(fixture, 'project');
await mkdir(projectPath);
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const evidence = { check: 'native-thread-rename', status: 'running', checks: {},
  scope: 'Fresh private app state and one synthetic PowerShell session. Saved thread (workset) title only; no agent, account or provider.' };
const save = () => writeFile(resolve(output, 'thread-rename.json'), `${JSON.stringify(evidence, null, 2)}\n`);
await save();
const reservation = createServer();
await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen({host:'127.0.0.1',port:0,exclusive:true}, accept); });
const port = reservation.address().port;
await new Promise((accept, reject) => reservation.close(error => error ? reject(error) : accept()));
const app = spawn(executable, [], { cwd: packageExecutable ? dirname(executable) : workspace,
  windowsHide:true, stdio:'ignore', env:{...process.env,SHINATAMA_HOME:data,
    WEBVIEW2_USER_DATA_FOLDER:resolve(fixture,'webview-profile'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`} });
let browser, page, owned = false, spawnError, frameClosed = false, pageErrors = 0;
app.once('error', error => { spawnError = error; });
const pathKey = path => path.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').replace(/\\+$/, '').toLowerCase();
function within(promise, milliseconds, label) {
  let timer;
  return Promise.race([promise,new Promise((_,reject) => { timer=setTimeout(() => reject(new Error(`${label} timed out.`)),milliseconds); })]).finally(() => clearTimeout(timer));
}
async function reply(command, args = {}) {
  return within(page.evaluate(async ({command,args}) => {
    try { return {ok:true,value:await window.__TAURI_INTERNALS__.invoke(command,args)}; }
    catch (issue) { return {ok:false,code:issue?.code ?? 'unavailable'}; }
  },{command,args}),15000,command);
}
async function ipc(command, args = {}) {
  const deadline = Date.now()+15000;
  do {
    const result = await reply(command,args);
    if (result.ok) return result.value;
    // The workspace store answers 'unavailable' ("busy; retry explicitly") while the window's own save is in flight.
    if (!['busy','unavailable'].includes(result.code)) throw new Error(`${command} refused (${result.code}).`);
    await delay(100);
  } while (Date.now()<deadline);
  throw new Error(`${command} remained busy.`);
}
const sessions = () => ipc('terminal_list');
const ids = async () => (await sessions()).map(item => item.id).sort();
let layoutScope = { workspaceId: 'main' };
const layout = async () => {
  const value=await ipc(layoutScope.projectId ? 'workspace_load' : 'global_workspace_load',{request:layoutScope});
  // A reload refreshes measured pane dimensions. Compare the saved arrangement,
  // tab order, focus and resource identities, excluding that measurement cache.
  return value ? JSON.parse(JSON.stringify(JSON.parse(value.layoutJson),
    (key,item)=>key==='lastUsableSize' ? undefined : item)) : null;
};
function exited(milliseconds) {
  if (app.exitCode !== null || app.signalCode !== null) return Promise.resolve();
  return new Promise((accept,reject) => {
    const done=()=>{clearTimeout(timer);accept();};
    const timer=setTimeout(()=>{app.removeListener('exit',done);reject(new Error('Owned app exit timed out.'));},milliseconds);
    app.once('exit',done);
  });
}
async function closeFrame() {
  const closing=Promise.all([page.waitForEvent('close',{timeout:15000}),exited(15000)]); void closing.catch(()=>{});
  await page.getByRole('button',{name:'Close window',exact:true}).click();
  await closing; frameClosed=true;
}
try {
  const deadline=Date.now()+30000;
  while (Date.now()<deadline) {
    if (spawnError || app.exitCode !== null) throw new Error('Owned rename app failed to start.');
    try { browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`,{timeout:1000}); break; }
    catch { await delay(250); }
  }
  if (!browser) throw new Error('Owned WebView startup timed out.');
  const context=browser.contexts()[0];
  const mainDeadline=Date.now()+10000;
  while (Date.now()<mainDeadline && !page) {
    page=context.pages().find(candidate=>{
      try { const url=new URL(candidate.url()); return url.hostname==='tauri.localhost' && ['/', '/index.html'].includes(url.pathname); }
      catch { return false; }
    });
    if (!page) await delay(100);
  }
  if (!page) throw new Error('Owned main workspace WebView was not found.');
  page.setDefaultTimeout(15000);
  await page.waitForURL(url=>url.hostname==='tauri.localhost');
  await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  const storage=await ipc('storage_location');
  if (pathKey(storage.directory)!==pathKey(data)) throw new Error('Private fixture identity mismatch; no UI was touched.');
  owned=true; page.on('pageerror',()=>{pageErrors++;});
  expect(await sessions()).toEqual([]);

  await (await startSession(page,{project:projectPath,harness:'powershell'})).done(90000);
  await expect.poll(ids).toHaveLength(1);
  const session=(await sessions())[0]; expect(session.shell).toBe('PowerShell');
  const project=(await ipc('project_recent')).find(item=>pathKey(item.path)===pathKey(projectPath));
  if (!project) throw new Error('Synthetic project was not saved.');
  const sidebar=page.locator('.thread-sidebar');
  const anchor=sidebar.locator('button.thread-sidebar-anchor');
  await expect(anchor).toHaveCount(1); await expect(anchor).toHaveAttribute('aria-pressed','true');
  const worksetId=await anchor.getAttribute('data-workset-id'); expect(worksetId).toMatch(/^w_[a-z2-7]{12}$/);
  // One layout for the window (ADR-0055 v2): the global record `main`.
  await expect.poll(async()=> (await layout())?.views.some(view=>view.id===session.id) ?? false).toBe(true);
  await expect.poll(()=>page.locator('.workspace-name').evaluate(label=>label.dataset.layoutDirty+label.dataset.layoutPending)).toBe('falsefalse');
  const request={projectId:project.id,archived:false};
  const stored=async()=> (await ipc('workset_list',{request})).rows.find(row=>row.id===worksetId);
  const before=await stored(); expect(before).toBeTruthy();
  const baseline=await layout(); const baselineIds=await ids();
  const rename=sidebar.locator('.thread-sidebar-more').first();
  const form=sidebar.locator('.thread-sidebar-rename'); const input=form.getByLabel('Name',{exact:true});
  const saveButton=form.getByRole('button',{name:'Save',exact:true});
  const unchanged=expect.objectContaining({id:worksetId,title:before.title,revision:before.revision,sessions:1});
  await rename.click(); await input.focus();
  await input.fill('   '); await input.press('Enter');
  await expect(form).toBeVisible(); expect(await stored()).toEqual(unchanged);
  await input.fill('Discard this'); await input.press('Escape');
  await expect(form).toBeHidden(); expect(await stored()).toEqual(unchanged);
  await rename.click(); await input.fill('Roadmap review'); await input.press('Enter');
  await expect(form).toBeHidden();
  await expect(anchor.locator('.thread-sidebar-title')).toHaveText('Roadmap review');
  const renamed=await stored();
  expect(renamed).toMatchObject({id:worksetId,projectId:project.id,title:'Roadmap review',archived:false,sessions:1,revision:before.revision+1});
  expect(await ids()).toEqual(baselineIds); expect(await layout()).toEqual(baseline);
  evidence.checks.inlineEnterEscapePreserveSessionThreadAndLayout=true;

  // Another window renames the thread first: the stale local edit is refused and the list shows the stored title.
  await rename.click(); await input.fill('Stale local edit');
  const current=await ipc('workset_update',{request:{worksetId,expectedRevision:renamed.revision,title:'Other window title'}});
  await input.press('Enter');
  await expect(page.locator('.im-notice', { hasText: 'This thread changed' })).toBeVisible();
  await expect(anchor.locator('.thread-sidebar-title')).toHaveText('Other window title');
  await expect(form).toBeHidden();
  expect((await stored()).title).toBe('Other window title');
  for (const rejected of [
    {worksetId:'w_aaaaaaaaaaaa',expectedRevision:current.revision,title:'Wrong thread'},
    {worksetId,expectedRevision:current.revision,title:'Unknown field',projectId:project.id},
    {worksetId,expectedRevision:current.revision},
    {worksetId,expectedRevision:renamed.revision,title:'Stale revision'},
    {worksetId,expectedRevision:current.revision,title:'🙂'.repeat(65)},
  ]) expect((await reply('workset_update',{request:rejected})).ok).toBe(false);
  expect((await stored()).title).toBe('Other window title');
  evidence.checks.conflictIsRefusedAndInvalidScopeOrPayloadCannotOverwrite=true;

  await rename.click(); await input.fill('Release planning'); await saveButton.click();
  await expect(form).toBeHidden(); await expect(anchor.locator('.thread-sidebar-title')).toHaveText('Release planning');
  await page.reload(); await expect(page.locator('#runtime-status')).toContainText('Local runtime');
  await expect(anchor.locator('.thread-sidebar-title')).toHaveText('Release planning');
  expect(await ids()).toEqual(baselineIds); expect(await layout()).toEqual(baseline);
  expect((await stored()).id).toBe(worksetId);
  await ipc('terminal_write',{id:session.id,data:Array.from(Buffer.from("Write-Output ('SHINATAMA_' + 'RENAME_HEALTHY')"+String.fromCharCode(13)))});
  await expect(page.locator('.xterm-accessibility')).toContainText('SHINATAMA_RENAME_HEALTHY');
  evidence.checks.reloadPersistsTitleAndSameShellRemainsResponsive=true;
  expect(pageErrors).toBe(0);
  // The tab's own close control confirms before stopping a running session.
    await expect(page.locator('.im-tile-tab')).toHaveCount(1);
  await page.locator('.im-tile-tab-close').click({force:true});
  await page.locator('#confirm-dialog button[value="stop"]').click();
  await expect.poll(ids).toEqual([]);
  await closeFrame(); expect(app.exitCode).toBe(0);
  evidence.checks.ownedStopAndFrameExit=true; evidence.status='passed'; await save();
  console.log('PASS: thread rename preserves its session, live shell and layout; conflict refused and reload persisted.');
} catch (error) {
  evidence.status='failed'; evidence.failure=String(error.message).split('\n')[0].slice(0,512); await save(); throw error;
} finally {
  if (owned && page && !page.isClosed() && !frameClosed) {
    try { for (const session of await sessions()) await ipc('terminal_stop',{id:session.id}); await closeFrame(); } catch {}
  }
  if (app.exitCode===null && app.signalCode===null) {app.kill();await exited(3000).catch(()=>{});}
  await browser?.close().catch(()=>{});
}
