// Run after C05 commands/component wiring and the normal desktop binary have been rebuilt.
// --with-git exercises real initialization under the fixed Git-helper supervisor, not C09R calibration.
import { chromium, expect } from '@playwright/test';
import { execFile, spawn } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { lstat, mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir as systemTmpdir } from 'node:os';
import { realpathSync } from 'node:fs';
import { basename, delimiter, dirname, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { setTimeout as delay } from 'node:timers/promises';
import { startSession } from './session-flow.mjs';
import { promisify } from 'node:util';
// The temporary folder by its long name: a runner's is C:\Users\RUNNER~1\..., and the app answers with the long one.
const tmpdir = () => realpathSync.native(systemTmpdir());

if (process.platform !== 'win32') throw new Error('This new-project proof qualifies the real Windows app only.');
const workspace = process.cwd();
const withGit = process.argv.slice(2).includes('--with-git');
if (process.argv.slice(2).some(argument => argument !== '--with-git')) throw new Error('Supported optional flag: --with-git.');
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-new-project-smoke-'));
const data = resolve(fixture, 'data');
const rootA = resolve(fixture, 'root-a'); const rootB = resolve(fixture, 'root-b');
const futureRoot = resolve(fixture, 'future-schema-root');
const seed = resolve(rootA, 'existing-terminal-folder');
await mkdir(seed, { recursive: true }); await mkdir(rootB); await mkdir(futureRoot);
const projectName = 'My project à'; const createdPath = resolve(rootB, projectName);
const output = resolve(workspace, '.prototype-evidence'); await mkdir(output, { recursive: true });
const resultPath = resolve(output, 'new-project.json');
const evidence = { check: 'C05-connected-Windows-new-project', checks: {}, status: 'running', git: { requested: withGit, status: withGit ? 'pending' : 'not-requested' },
  gaps: [...(withGit ? [] : ['Real git init is not exercised without --with-git.']), 'Real synced/network drives, the complete U3 key matrix and C09R adaptive resource calibration remain separate qualifications.'] };
await writeFile(resultPath, `${JSON.stringify(evidence, null, 2)}\n`);
const port = 19234; const live = new Set(); let launches = 0;
let gitExecutable = null;
const key = path => path.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').toLowerCase();
function within(promise, milliseconds, label) {
  let timer;
  const deadline = new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${label} exceeded its deadline.`)), milliseconds); });
  return Promise.race([promise, deadline]).finally(() => clearTimeout(timer));
}
const invoke = (instance, command, args) => within(instance.page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args }), 20000, command);
async function selectGit() {
  const environmentValue = name => Object.entries(process.env).find(([key]) => key.toLowerCase() === name.toLowerCase())?.[1];
  const override = environmentValue('SHINATAMA_TEST_GIT_EXE');
  const candidates = (override ? [override] : [
    ...['ProgramFiles', 'ProgramFiles(x86)'].flatMap(variable => environmentValue(variable) ? [resolve(environmentValue(variable), 'Git/cmd/git.exe')] : []),
    ...(environmentValue('LOCALAPPDATA') ? [resolve(environmentValue('LOCALAPPDATA'), 'Programs/Git/cmd/git.exe')] : []),
    ...(environmentValue('PATH') ?? '').split(delimiter).filter(Boolean).slice(0, 32).map(path => resolve(path, 'git.exe')),
  ]).filter(path => Buffer.byteLength(path, 'utf8') <= 4096 && !/[\u0000-\u001f\u007f]/.test(path)).slice(0, 35);
  // Inspect metadata from drive root to leaf; never follow a reparse point or probe
  // a network/removable drive. This reads no Git configuration or binary output.
  const script = String.raw`
$ErrorActionPreference = 'Stop'
$candidates = [Environment]::GetEnvironmentVariable('SHINATAMA_TEST_GIT_CANDIDATES') | ConvertFrom-Json
foreach ($candidate in $candidates) {
    if ($candidate -notmatch '^[A-Za-z]:[\\/]' -or [IO.Path]::GetFileName($candidate) -ine 'git.exe') { continue }
    $candidate = [IO.Path]::GetFullPath($candidate)
    $root = [IO.Path]::GetPathRoot($candidate)
    if ([IO.DriveInfo]::new($root).DriveType -ne [IO.DriveType]::Fixed) { continue }
    $parts = [System.Collections.Generic.List[string]]::new()
    $next = $candidate
    while ($next -and $parts.Count -le 64) { $parts.Add($next); $next = [IO.Path]::GetDirectoryName($next) }
    if ($parts.Count -gt 64) { continue }
    $allowed = $true
    for ($index = $parts.Count - 1; $index -ge 0; $index--) {
        try { $entry = Get-Item -LiteralPath $parts[$index] -Force -ErrorAction Stop } catch { $allowed = $false; break }
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or ($index -eq 0 -and $entry.PSIsContainer) -or ($index -gt 0 -and !$entry.PSIsContainer)) { $allowed = $false; break }
    }
    if ($allowed) { @{ executable = $candidate } | ConvertTo-Json -Compress; exit 0 }
}
throw 'No regular local native Git executable qualified for this explicit test.'
`;
  const run = promisify(execFile);
  const { stdout } = await run('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], { windowsHide: true, encoding: 'utf8', timeout: 10000,
    maxBuffer: 16384, env: { ...process.env, SHINATAMA_TEST_GIT_CANDIDATES: JSON.stringify(candidates) } });
  const selected = JSON.parse(stdout.replace(/^\uFEFF/, '').trim()).executable;
  const information = await stat(selected);
  if (!information.isFile() || information.size > 64 * 1024 * 1024) throw new Error('Selected Git executable is not a size-bounded regular file.');
  const digest = createHash('sha256');
  for await (const chunk of createReadStream(selected, { highWaterMark: 65536 })) digest.update(chunk);
  evidence.git = { requested: true, status: 'selected-not-yet-exercised', executable: selected, executableSha256: digest.digest('hex').toUpperCase(),
    selection: 'First and only Git candidate in the test app PATH; the real bridge resolves this admitted regular local executable.',
    supervisionScope: 'Fixed contained Git helper limits; C09R adaptive resource calibration is not qualified.' };
  return selected;
}
function appEnvironment(profile) {
  const environment = { ...process.env, SHINATAMA_HOME: data, WEBVIEW2_USER_DATA_FOLDER: profile,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` };
  if (gitExecutable) {
    const windows = process.env.SystemRoot || process.env.WINDIR;
    if (!windows) throw new Error('Windows system directory is unavailable for the isolated test PATH.');
    for (const key of Object.keys(environment)) if (key.toLowerCase() === 'path') delete environment[key];
    environment.PATH = [dirname(gitExecutable), resolve(windows, 'System32'), resolve(windows, 'System32/WindowsPowerShell/v1.0'), windows].join(delimiter);
  }
  return environment;
}
function exitWithin(app, milliseconds) {
  if (app.exitCode !== null || app.signalCode !== null) return Promise.resolve();
  return new Promise((resolvePromise, reject) => {
    const done = () => { clearTimeout(timer); resolvePromise(); };
    const timer = setTimeout(() => { app.removeListener('exit', done); reject(new Error('Owned new-project app exit deadline exceeded.')); }, milliseconds);
    app.once('exit', done);
  });
}
async function forceClose(instance) {
  if (instance.page && !instance.page.isClosed()) await invoke(instance, 'terminal_shutdown').catch(() => {});
  if (instance.app.exitCode === null && instance.app.signalCode === null) { instance.app.kill(); await exitWithin(instance.app, 3000).catch(() => {}); }
  await instance.browser?.close().catch(() => {}); live.delete(instance);
}
async function openApp() {
  await new Promise((resolvePromise, reject) => {
    const probe = createServer();
    probe.once('error', () => reject(new Error('The new-project CDP port is occupied; no existing app was attached.')));
    probe.listen({ host: '127.0.0.1', port, exclusive: true }, () => probe.close(error => error ? reject(error) : resolvePromise()));
  });
  const app = spawn(resolve(workspace, 'target/debug/shinatama-desktop.exe'), [], { cwd: workspace, windowsHide: true, stdio: 'ignore',
    env: appEnvironment(resolve(fixture, `webview-${++launches}`)) });
  const instance = { app, browser: null, page: null, errors: [] }; live.add(instance);
  let spawnError; app.once('error', error => { spawnError = error; });
  try {
    const deadline = Date.now() + 30000;
    while (Date.now() < deadline) {
      if (spawnError) throw new Error(`Cannot start the owned new-project app: ${spawnError.code}`);
      if (app.exitCode !== null) throw new Error(`New-project app exited with code ${app.exitCode}.`);
      try { instance.browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; }
      catch { await delay(250); }
    }
    if (!instance.browser) throw new Error('New-project app startup exceeded 30 seconds.');
    const context = instance.browser.contexts()[0];
    // The attention widget is a page too; the workspace is the one that is not widget.html.
    for (const deadline = Date.now() + 10000; !(instance.page = context.pages().find(candidate => candidate.url().includes('tauri.localhost') && !candidate.url().includes('widget.html'))) && Date.now() < deadline;) await delay(100);
    if (!instance.page) throw new Error('The workspace page did not load.');
    instance.page.on('pageerror', error => instance.errors.push(error.message));
    await instance.page.waitForURL(url => url.hostname === 'tauri.localhost', { timeout: 10000 });
    await expect(instance.page.locator('#runtime-status')).toContainText('Local runtime', { timeout: 10000 });
    expect(key((await invoke(instance, 'storage_location')).directory)).toBe(key(data));
    evidence.runtime = await instance.browser.version(); return instance;
  } catch (error) { await forceClose(instance); throw error; }
}
async function closeApp(instance) {
  const hasTerminals = (await invoke(instance, 'terminal_list')).length > 0;
  const closing = Promise.all([instance.page.waitForEvent('close', { timeout: 10000 }), exitWithin(instance.app, 10000)]);
  void closing.catch(() => {});
  await instance.page.getByRole('button', { name: 'Close window', exact: true }).click();
  if (hasTerminals) await instance.page.getByRole('alertdialog', { name: 'Close workspace?', exact: true }).getByRole('button', { name: 'Stop', exact: true }).click();
  await closing; expect(instance.app.exitCode).toBe(0); expect(instance.errors).toEqual([]);
  await instance.browser.close().catch(() => {}); live.delete(instance);
}
async function openCreator(instance, fromPicker = false) {
  if (fromPicker) {
    await instance.page.locator('#project').click(); await instance.page.getByRole('button', { name: 'Project roots…', exact: true }).click();
    await expect(instance.page.locator('#refresh-projects')).toBeEnabled();
    await instance.page.locator('#refresh-projects').focus();
  }
  else await instance.page.locator('#project').focus();
  await instance.page.keyboard.press('n');
  await expect(instance.page.locator('#new-project-dialog')).toBeVisible();
  await expect(instance.page.locator('#new-project-root')).toBeEnabled();
  await expect(instance.page.locator('#new-project-git')).not.toBeChecked();
}
async function rootFolders() {
  const entries = await readdir(rootB);
  if (entries.includes('.shinatama-create.lock')) {
    const lock = await lstat(resolve(rootB, '.shinatama-create.lock'));
    expect(lock.isFile()).toBe(true); expect(lock.isSymbolicLink()).toBe(false); expect(lock.size).toBe(0);
  }
  return entries.filter(entry => entry !== '.shinatama-create.lock').sort();
}

try {
  if (withGit) gitExecutable = await selectGit();
  let instance = await openApp(); const page = instance.page;
  await page.locator('#project').click(); await page.getByRole('button', { name: 'Project roots…', exact: true }).click(); await page.locator('#folder').fill(seed);
  await page.getByRole('button', { name: 'Use folder', exact: true }).click();
  await expect(page.locator('#project-dialog')).not.toBeVisible();
  await page.locator('#project').click(); await page.getByRole('button', { name: 'Project roots…', exact: true }).click();
  await page.locator('#root-path').fill(rootA); await page.locator('#add-root').click();
  await expect(page.locator('.catalog-root-status')).toHaveCount(1, { timeout: 10000 });
  await page.locator('#root-path').fill(rootB); await page.locator('#add-root').click();
  await expect(page.locator('.catalog-root-status')).toHaveCount(2, { timeout: 10000 });
  await page.locator('.catalog-root-status').nth(1).getByRole('button', { name: /^Make default/ }).click();
  await expect(page.locator('.catalog-root-status').nth(1).locator('.root-default')).toHaveAttribute('aria-pressed', 'true', { timeout: 10000 });
  await page.locator('#project-form').getByRole('button', { name: 'Close', exact: true }).filter({ hasText: 'Close' }).click();
  const configuration = await invoke(instance, 'new_project_options');
  const defaultId = configuration.defaultRootId;
  expect(key(configuration.roots.find(root => root.id === defaultId).path)).toBe(key(rootB));

  // Neither filter nor ordinary text input can be stolen by the n shortcut.
  await page.locator('#project').click(); await page.getByRole('button', { name: 'Project roots…', exact: true }).click(); await page.locator('#project-filter').fill(''); await page.locator('#project-filter').focus();
  await page.keyboard.press('n'); await expect(page.locator('#project-filter')).toHaveValue('n');
  await expect(page.locator('#new-project-dialog')).not.toBeVisible();
  await page.locator('#folder').fill('typed-'); await page.locator('#folder').focus(); await page.keyboard.press('n');
  await expect(page.locator('#folder')).toHaveValue('typed-n'); await expect(page.locator('#new-project-dialog')).not.toBeVisible();
  await page.locator('#project-form').getByRole('button', { name: 'Close', exact: true }).filter({ hasText: 'Close' }).click();
  await (await startSession(page, { project: seed, harness: 'powershell' })).done(30000); await expect(page.locator('.xterm-accessibility')).toContainText('PS ', { timeout: 20000 });
  const originalSession = (await invoke(instance, 'terminal_list'))[0];
  await page.locator('.xterm-helper-textarea').focus(); await page.keyboard.type("Write-Output ('C05_KEY_' + '");
  await page.keyboard.press('n'); await expect(page.locator('#new-project-dialog')).not.toBeVisible();
  await page.keyboard.type("')"); await page.keyboard.press('Enter');
  await expect(page.locator('.xterm-accessibility')).toContainText('C05_KEY_n');
  evidence.checks.nDoesNotStealFilterTextInputOrTerminalInput = true;

  // A root path being typed keeps its n; the visible button still opens the creator.
  await page.locator('#project').click(); await page.getByRole('button', { name: 'Project roots…', exact: true }).click();
  const unsavedRoot = resolve(fixture, 'unsaved-root');
  await page.locator('#root-path').fill(unsavedRoot); await page.locator('#root-path').focus(); await page.keyboard.press('n');
  await expect(page.locator('#new-project-dialog')).not.toBeVisible(); await expect(page.locator('#root-path')).toHaveValue(`${unsavedRoot}n`);
  await page.locator('#create-project').click(); await expect(page.locator('#new-project-dialog')).toBeVisible();
  await expect(page.locator('#new-project-root')).toHaveValue(defaultId); await expect(page.locator('#new-project-git')).not.toBeChecked();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click(); expect(await rootFolders()).toEqual([]);
  evidence.checks.visibleCreateButtonWorksWithoutDiscardingRootDrafts = true;

  // App chrome and an idle picker outside its input both reach the same real dialog.
  await openCreator(instance); await expect(page.locator('#new-project-root')).toHaveValue(defaultId);
  await page.locator('#new-project-name').fill('cancelled-project'); await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect(await rootFolders()).toEqual([]); await expect(page.locator('#project')).toHaveAttribute('data-project', 'existing-terminal-folder');
  await openCreator(instance, true); await expect(page.locator('#new-project-root')).toHaveValue(defaultId);
  for (const invalidName of ['CON', 'aux.txt', 'LPT1', 'trailing.']) {
    await page.locator('#new-project-name').fill(invalidName); await page.getByRole('button', { name: 'Create project', exact: true }).click();
    expect(await page.locator('#new-project-name').evaluate(input => input.validationMessage.length > 0)).toBe(true);
    await expect(page.locator('#new-project-dialog')).toBeVisible(); expect(await rootFolders()).toEqual([]);
  }
  evidence.checks.explicitDefaultRootCancelAndWindowsNameValidation = true;
  await page.locator('#new-project-name').fill(projectName); await page.getByRole('button', { name: 'Create project', exact: true }).click();
  await expect(page.locator('#new-project-dialog')).not.toBeVisible(); await expect(page.locator('#project')).toHaveAttribute('data-project', projectName);
  expect(await rootFolders()).toEqual([projectName]); expect(await readdir(createdPath)).toEqual([]); // Git is explicitly false; no .git or app settings.
  const selected = (await invoke(instance, 'project_recent'))[0]; expect(key(selected.path)).toBe(key(createdPath)); expect(selected.id).toMatch(/^p_[a-z2-7]{12}$/);
  let expectedSelection = selected;
  expect(await invoke(instance, 'terminal_list')).toEqual([originalSession]);
  evidence.checks.unicodeSpaceNameCreatedInChosenDefaultWithoutGitOrAutomaticTerminal = true;

  const marker = resolve(createdPath, 'preserved-marker.txt'); const markerBytes = Buffer.from('C05 existing project must remain unchanged\n');
  expect(dirname(createdPath)).toBe(rootB); await writeFile(marker, markerBytes, { flag: 'wx' });
  await openCreator(instance); await page.locator('#new-project-name').fill('my PROJECT À');
  await page.getByRole('button', { name: 'Create project', exact: true }).click();
  await expect(page.locator('#new-project-error')).toBeVisible(); await expect(page.locator('#new-project-dialog')).toBeVisible();
  expect(await readFile(marker)).toEqual(markerBytes); expect(await rootFolders()).toEqual([projectName]);
  expect((await invoke(instance, 'project_recent'))[0]).toEqual(selected);
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  evidence.checks.caseInsensitiveDuplicatePreservesExistingFolderAndMarker = true;

  const stale = await invoke(instance, 'new_project_options');
  let requestId = randomUUID(); await invoke(instance, 'catalog_begin', { requestId });
  await invoke(instance, 'catalog_roots_set', { requestId, request: { expectedRevision: stale.revision, roots: stale.roots,
    defaultRootIndex: stale.roots.findIndex(root => root.id === stale.defaultRootId) } });
  requestId = randomUUID(); await invoke(instance, 'catalog_begin', { requestId });
  let rejected = false;
  try { await invoke(instance, 'new_project_create', { requestId, request: { expectedRevision: stale.revision, rootId: defaultId, name: 'stale-snapshot', gitInit: false } }); }
  catch (error) { rejected = true; expect(typeof error.message).toBe('string'); expect(error.message.length).toBeGreaterThan(0); }
  expect(rejected).toBe(true); expect(await rootFolders()).toEqual([projectName]); expect(await readFile(marker)).toEqual(markerBytes);
  evidence.checks.staleRootRevisionRejectedBeforeCreatingAnyFolder = true;
  if (withGit) {
    const gitName = 'Git project Ω'; const gitPath = resolve(rootB, gitName);
    await openCreator(instance); await page.locator('#new-project-name').fill(gitName); await page.locator('#new-project-git').check();
    await page.getByRole('button', { name: 'Create project', exact: true }).click();
    await expect.poll(async () => {
      if (await page.locator('#new-project-error').isVisible()) throw new Error(`Real Git creation failed: ${await page.locator('#new-project-error').textContent()}`);
      return page.locator('#new-project-dialog').isVisible();
    }, { timeout: 15000 }).toBe(false);
    expect(await readFile(resolve(gitPath, '.git/HEAD'), 'utf8')).toBe('ref: refs/heads/main\n');
    expect(await readdir(gitPath)).toEqual(['.git']);
    expect(await readdir(resolve(gitPath, '.git'))).not.toContain('hooks');
    expect(await readdir(resolve(gitPath, '.git'))).not.toContain('description');
    expect(await rootFolders()).toEqual([projectName, gitName].sort());
    expectedSelection = (await invoke(instance, 'project_recent'))[0];
    expect(key(expectedSelection.path)).toBe(key(gitPath)); expect(await invoke(instance, 'terminal_list')).toEqual([originalSession]);
    evidence.git.status = 'passed-real-initialization'; evidence.git.branch = 'main'; evidence.git.noHooksOrTemplateDescriptionCopied = true;
    evidence.checks.realOptionalGitCreatesMainWithoutHooksOrTemplates = true;
  }
  await closeApp(instance);

  instance = await openApp(); await expect(instance.page.locator('#project')).toHaveAttribute('data-project', expectedSelection.name);
  const restored = (await invoke(instance, 'project_recent'))[0]; expect(restored.id).toBe(expectedSelection.id); expect(key(restored.path)).toBe(key(expectedSelection.path));
  expect(await invoke(instance, 'terminal_list')).toEqual([]); expect(await readFile(marker)).toEqual(markerBytes);
  evidence.checks.selectionAndIdentityPersistAcrossRestartWithoutAutomaticTerminals = true;
  // Upgrade only this synthetic database while the actual app remains open.
  // The new root is empty: even a newly created operational lock is a failure.
  const current = await invoke(instance, 'new_project_options');
  requestId = randomUUID(); await invoke(instance, 'catalog_begin', { requestId });
  await invoke(instance, 'catalog_roots_set', { requestId, request: { expectedRevision: current.revision,
    roots: [...current.roots, { path: futureRoot }], defaultRootIndex: current.roots.length } });
  const futureOptions = await invoke(instance, 'new_project_options');
  const futureRootId = futureOptions.roots.find(root => key(root.path) === key(futureRoot)).id;
  const databasePath = resolve(data, 'shinatama.db'); expect((await stat(databasePath)).isFile()).toBe(true);
  const database = new DatabaseSync(databasePath);
  try {
    database.exec('PRAGMA busy_timeout=5000');
    const before = database.prepare('SELECT * FROM project ORDER BY id').all();
    database.exec('PRAGMA user_version=999');
    requestId = randomUUID(); await invoke(instance, 'catalog_begin', { requestId });
    let futureRejected = false;
    try { await invoke(instance, 'new_project_create', { requestId, request: { expectedRevision: futureOptions.revision, rootId: futureRootId, name: 'future-must-not-exist', gitInit: false } }); }
    catch (error) { futureRejected = true; expect(typeof error.message).toBe('string'); expect(error.recoveryPath).toBeUndefined(); }
    expect(futureRejected).toBe(true); expect(await readdir(futureRoot)).toEqual([]);
    expect(database.prepare('SELECT * FROM project ORDER BY id').all()).toEqual(before);
    expect(database.prepare('PRAGMA user_version').get().user_version).toBe(999);
  } finally { database.close(); }
  await closeApp(instance); evidence.checks.alreadyOpenAppRejectsFutureSchemaBeforeAnyFolderLockOrProjectWrite = true;
  evidence.status = 'passed'; await writeFile(resultPath, `${JSON.stringify(evidence, null, 2)}\n`);
  console.log(`PASS C05 local Windows: scoped n, explicit default, cancelled/invalid/stale/future-schema creation leaves no project, Unicode creation, duplicate preserves existing work, restart retains identity. Real Git: ${withGit ? 'main verified without hooks/templates' : 'not requested'}.`);
} catch (error) {
  evidence.status = 'failed'; evidence.failure = error.message;
  await writeFile(resultPath, `${JSON.stringify(evidence, null, 2)}\n`); throw error;
} finally {
  for (const instance of [...live]) await forceClose(instance);
  if (dirname(fixture) === resolve(tmpdir()) && basename(fixture).startsWith('shinatama-new-project-smoke-')) await rm(fixture, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
}
