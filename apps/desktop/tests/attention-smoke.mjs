// Isolated Windows proof: real overlay and real terminal signals; no agent/provider calls.
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { createServer } from 'node:net';
import { startSession } from './session-flow.mjs';
if (process.platform !== 'win32') throw new Error('Windows required.');
const root = process.cwd(), output = resolve(root, '.prototype-evidence');
const surfaceOnly = process.argv.includes('--surface-only');
await mkdir(output, {recursive:true});
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-attention-'));
const project = resolve(fixture,'project'); await mkdir(project);
const reservation = createServer();
await new Promise((ok,no) => {reservation.once('error',no);reservation.listen(0,'127.0.0.1',ok);});
const port = reservation.address().port;
await new Promise(ok => reservation.close(ok));
const app = spawn(resolve(root,'target/debug/shinatama-desktop.exe'), [], {
  cwd:root, windowsHide:true, stdio:['ignore','pipe','pipe'], env:{...process.env,
    SHINATAMA_HOME:resolve(fixture,'data'), WEBVIEW2_USER_DATA_FOLDER:resolve(fixture,'webview'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`},
});
let browser, main, widget; const errors=[];
const evidence={date:'2026-11-03',checks:{},limits:['No provider or live harness run. Terminal controls have no input/approval/turn semantics. Physical audio listening is not automated.']};
const native = () => JSON.parse(execFileSync('powershell.exe',['-NoProfile','-NonInteractive','-File',resolve(root,'apps/desktop/tests/attention-window.ps1'),'-AppProcessId',String(app.pid)],{encoding:'utf8',windowsHide:true}));
const ipc = (command,args={},page=main) => page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args});
const rejected = async (command,args,page=main) => {try {await ipc(command,args,page);return false;} catch {return true;}};
const pathKey = value => value.replace(/^\\\\\?\\/,'').replaceAll('/','\\').toLowerCase();
try {
  for (const deadline=Date.now()+30000;Date.now()<deadline;) {
    if (app.exitCode!==null) throw new Error('Owned app exited during startup.');
    try {browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`,{timeout:1000});break;} catch {await delay(250);}
  }
  if(!browser) throw new Error('Native WebView startup deadline exceeded.');
  const context=browser.contexts()[0];
  await expect.poll(()=>context.pages().filter(page=>page.url().includes('tauri.localhost')).length).toBe(2);
  main=context.pages().find(page=>!page.url().includes('widget.html'));
  widget=context.pages().find(page=>page.url().includes('widget.html'));
  main.on('pageerror',error=>errors.push(error.message));widget.on('pageerror',error=>errors.push(error.message));
  await expect(main.locator('#runtime-status')).toContainText('Local runtime');
  expect(pathKey((await ipc('storage_location')).directory)).toBe(pathKey(resolve(fixture,'data')));
  expect((await ipc('attention_current')).current).toBeNull();
  let probe=native();expect(probe.windows.find(win=>win.title==='Shinatama — avvisi').visible).toBe(false);
  expect((await ipc('attention_current')).muted).toBe(false);
  const unmuted=(await ipc('attention_preview',{kind:'update'})).current;
  await ipc('attention_dismiss',{id:unmuted.id});
  evidence.checks.unmutedSystemSoundPathExercised=true;
  await main.locator('#toolbar-more').click();await main.locator('#desktop-alerts').click();
  await main.locator('#attention-sound').uncheck();
  await expect.poll(async()=>(await ipc('attention_current')).muted).toBe(true);
  await main.getByRole('button',{name:'Chiudi impostazioni avvisi'}).click();
  await main.reload();await expect(main.locator('#runtime-status')).toContainText('Local runtime');
  expect((await ipc('attention_current')).muted).toBe(true);
  evidence.checks.preferencesReappliedAfterFrontendReload=true;
  await main.locator('#toolbar-more').click();await main.locator('#desktop-alerts').click();
  const beforePreview=native().foreground;
  await main.locator('#attention-test').click();
  await expect(main.locator('#attention-settings')).not.toBeVisible();
  await expect(widget.locator('#attention-title')).toContainText('prova avviso');
  await expect.poll(() => widget.locator('#shinatama-portrait').evaluate(image => image.complete && image.naturalWidth >= 1024 && image.naturalHeight >= 1024)).toBe(true);
  const expressionBefore = await widget.locator('#shinatama-portrait').getAttribute('data-expression');
  expect(['smile','happy','curious']).toContain(expressionBefore);
  await ipc('attention_configure',{enabled:true,muted:true});
  expect(await widget.locator('#shinatama-portrait').getAttribute('data-expression')).toBe(expressionBefore);
  const current=(await ipc('attention_current')).current;
  probe=native();const visible=probe.windows.find(win=>win.title==='Shinatama — avvisi');
  expect(visible.visible).toBe(true);expect(visible.noActivate).toBe(true);expect(visible.topmost).toBe(true);
  expect(probe.foreground).not.toBe(visible.handle);
  evidence.foregroundUnchangedDuringPreview=probe.foreground===beforePreview;
  await ipc('attention_configure',{enabled:true,muted:true});
  expect(native().foreground).not.toBe(visible.handle);
  evidence.checks.nativeVisibleTopmostNoActivate=true;
  expect(await rejected('attention_configure',{enabled:false,muted:true},widget)).toBe(true);
  expect(await rejected('terminal_start',{cwd:project,rows:24,cols:80},widget)).toBe(true);
  expect(await rejected('attention_dismiss',{id:'stale'},widget)).toBe(true);
  expect((await ipc('attention_current')).current.id).toBe(current.id);
  await widget.screenshot({path:resolve(output,'attention-widget-native.png')});
  await widget.locator('#attention-dismiss').click();
  expect(await rejected('attention_open',{id:current.id},widget)).toBe(true);
  await ipc('attention_preview',{kind:'update'});
  await expect.poll(() => widget.locator('#shinatama-portrait').getAttribute('data-expression')).not.toBe(expressionBefore);
  evidence.checks.generatedPortraitStablePerNoticeAndNoImmediateRepeat=true;
  await ipc('attention_configure',{enabled:false,muted:true});
  expect((await ipc('attention_current')).current).toBeNull();
  expect(native().windows.find(win=>win.title==='Shinatama — avvisi').visible).toBe(false);
  expect(await rejected('attention_preview',{kind:'input'})).toBe(true);
  await ipc('attention_configure',{enabled:true,muted:true});
  evidence.checks.staleRequestsPrivilegesDisable=true;
  if (!surfaceOnly) {
  await ipc('project_select',{path:project});await main.reload();await expect(main.locator('#runtime-status')).toContainText('Local runtime');
  // Real pointer/key events supply the normal latency measurements used by admission.
  await (await startSession(main,{project,harness:'powershell'})).done(30000);
  await expect.poll(async()=>(await ipc('terminal_list')).length,{timeout:30000}).toBe(1);
  const session=(await ipc('terminal_list'))[0];
  await delay(1500);
  await ipc('terminal_write',{id:session.id,data:[...Buffer.from("[Console]::Write(([char]27) + ']9;SHINATAMA_PRIVATE_PAYLOAD' + [char]7)\r")]});
  await expect.poll(async()=>(await ipc('attention_current')).current?.sessionId,{timeout:15000}).toBe(session.id);
  const signal=(await ipc('attention_current')).current;
  expect(signal.kind).toBe('update');expect(JSON.stringify(signal)).not.toContain('PRIVATE_PAYLOAD');
  await expect(widget.locator('#attention-body')).toContainText('segnale di attenzione');
  await ipc('terminal_write',{id:session.id,data:[...Buffer.from("[Console]::Write([char]7)\r")]});
  await delay(500);expect((await ipc('attention_current')).current.id).toBe(signal.id);
  expect(native().foreground).not.toBe(visible.handle);
  await widget.locator('#attention-open').click();
  await expect.poll(async()=>(await ipc('attention_current')).current).toBeNull();
  const active=main.locator(`.wm-tab[data-view-id="${session.id}"]`);await expect(active).toHaveAttribute('aria-selected','true');
  await ipc('terminal_write',{id:session.id,data:[...Buffer.from('exit\r')]});
  await expect.poll(async()=>(await ipc('attention_current')).current?.kind,{timeout:15000}).toBe('complete');
  await widget.locator('#attention-open').click();
  await expect.poll(async()=>(await ipc('attention_current')).current).toBeNull();
  await expect(active).toHaveAttribute('aria-selected','true');
  evidence.checks.realPtyOscBellDedupRedactionClickAndExit=true;
  }
  if (surfaceOnly) evidence.limits.push('Live PTY notification/click/EOF proof not run in surface-only mode; the full run was refused by normal resource admission.');
  expect(errors).toEqual([]);evidence.checks.noWebviewErrors=true;
  await ipc('terminal_shutdown');await ipc('workspace_allow_close');
  const exited=new Promise(ok=>app.once('exit',ok));
  await ipc('plugin:window|close',{label:'main'}).catch(()=>{});
  await Promise.race([exited,delay(10000).then(()=>{throw new Error('Main/overlay close deadline exceeded.');})]);
  evidence.checks.overlayClosesWithMain=true;
  evidence.status=surfaceOnly?'surface_pass_live_deferred':'full_pass';
  await writeFile(resolve(output,'attention-widget.json'),JSON.stringify(evidence,null,2));
  console.log(surfaceOnly?'PASS: native surface, non-activating window, sound path, mute/preferences, stale/privilege/disable guards and app close. Live PTY proof remains deferred.':'PASS: native overlay and real PTY OSC/BEL/exit integration.');
} catch(error) {
  if(main && !main.isClosed()) {
    console.error(await main.evaluate(async()=>({notice:document.querySelector('#notice')?.textContent,resources:await window.__TAURI_INTERNALS__.invoke('resources_status'),project:document.querySelector('#project')?.textContent,dialogs:[...document.querySelectorAll('dialog[open]')].map(dialog=>dialog.textContent?.slice(0,500))})));
    await main.screenshot({path:resolve(output,'attention-failure.png')}).catch(()=>{});
  }
  throw error;
} finally {
  if(main && !main.isClosed()) {await ipc('terminal_shutdown').catch(()=>{});await ipc('workspace_allow_close').catch(()=>{});await main.evaluate(()=>window.__TAURI_INTERNALS__.invoke('plugin:window|close')).catch(()=>{});}
  if(browser) await browser.close().catch(()=>{});
  if(app.exitCode===null) app.kill();
}
