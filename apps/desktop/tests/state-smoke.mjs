// Real embedded-app restarts against synthetic, private SQLite state only.
// Node's built-in SQLite is a test observer, never an application dependency.
import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir as systemTmpdir } from 'node:os';
import { realpathSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { setTimeout as delay } from 'node:timers/promises';
import { mainPage } from './main-page.mjs';
// The temporary folder by its long name: a runner's is C:\Users\RUNNER~1\..., and the app answers with the long one.
const tmpdir = () => realpathSync.native(systemTmpdir());

if (process.platform !== 'win32') throw new Error('This embedded WebView2 state check is Windows-only.');
const root = process.cwd();
// The schema the app writes is shinatama_state::schema::CURRENT_VERSION; the test reads it there,
// so a new migration does not leave the expectation behind (it said 3 while the app wrote 6).
const schemaSource = await readFile(resolve(root, 'crates/state/src/schema.rs'), 'utf8');
const expectedSchema = Number(process.env.SHINATAMA_EXPECT_SCHEMA || schemaSource.match(/pub const CURRENT_VERSION: u32 = (\d+);/)?.[1]);
if (!Number.isSafeInteger(expectedSchema) || expectedSchema < 1 || expectedSchema >= 999) {
  throw new Error('The expected test schema must be an integer from 1 to 998.');
}
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-state-smoke-'));
const data = resolve(fixture, 'data');
const first = resolve(fixture, 'first-project');
const second = resolve(fixture, 'second-project');
const legacyPath = resolve(data, 'projects.json');
const databasePath = resolve(data, 'shinatama.db');
const evidenceDirectory = resolve(root, '.prototype-evidence');
await mkdir(data); await mkdir(first); await mkdir(second); await mkdir(evidenceDirectory, { recursive: true });
const legacy = Buffer.from(`${JSON.stringify({ schema_version: 1, projects: [
  { path: first, name: 'first-project', last_opened: 1700000001000 },
  { path: second, name: 'second-project', last_opened: 1700000000000 },
] }, null, 2)}\n`);
await writeFile(legacyPath, legacy);
const port = 19230;
const live = new Set();
let launches = 0;
const evidence = { check: 'C02-embedded-state', expectedSchema, checks: {}, status: 'running' };
const evidencePath = resolve(evidenceDirectory, 'state-storage.json');
await writeFile(evidencePath, `${JSON.stringify(evidence, null, 2)}\n`);

function pathKey(path) {
  if (path.startsWith('\\\\?\\UNC\\')) path = `\\\\${path.slice(8)}`;
  else if (path.startsWith('\\\\?\\')) path = path.slice(4);
  return path.replaceAll('/', '\\').toLowerCase();
}

function invoke(instance, command, args) {
  return instance.page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
}

async function waitForExit(app, milliseconds) {
  if (app.exitCode !== null || app.signalCode !== null) return;
  await new Promise((resolve, reject) => {
    const completed = () => { clearTimeout(timer); resolve(); };
    const timer = setTimeout(() => {
      app.removeListener('exit', completed);
      reject(new Error('The owned state-test application did not exit within its deadline.'));
    }, milliseconds);
    app.once('exit', completed);
  });
}

async function forceClose(instance) {
  if (instance.page && !instance.page.isClosed()) {
    await invoke(instance, 'terminal_shutdown').catch(() => {});
  }
  if (instance.app.exitCode === null && instance.app.signalCode === null) {
    instance.app.kill(); await waitForExit(instance.app, 3000).catch(() => {});
  }
  if (instance.browser) await instance.browser.close().catch(() => {});
  live.delete(instance);
}

async function openApp(readOnly = false) {
  const app = spawn(resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
    cwd: root, windowsHide: true, stdio: 'ignore',
    env: { ...process.env, SHINATAMA_HOME: data,
      WEBVIEW2_USER_DATA_FOLDER: resolve(fixture, `webview-${++launches}`),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
  });
  const instance = { app, browser: null, page: null, errors: [] };
  let spawnError;
  app.once('error', error => { spawnError = error; });
  live.add(instance);
  try {
    const deadline = Date.now() + 30000;
    while (Date.now() < deadline) {
      if (spawnError) throw new Error(`Cannot start the owned state-test application: ${spawnError.code}`);
      if (app.exitCode !== null) throw new Error(`The owned state-test application exited with code ${app.exitCode}.`);
      try { instance.browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; }
      catch { await delay(250); }
    }
    if (!instance.browser) throw new Error('The state-test WebView2 startup exceeded 30 seconds.');
    const context = instance.browser.contexts()[0];
    instance.page = await mainPage(instance.browser);
    instance.page.on('pageerror', error => instance.errors.push(error.message));
    await instance.page.waitForURL(url => url.hostname === 'tauri.localhost', { timeout: 10000 });
    expect(new URL(instance.page.url()).hostname).toBe('tauri.localhost');
    await expect(instance.page.getByRole('button', { name: 'Close window', exact: true })).toBeEnabled({ timeout: 10000 });
    const storage = await invoke(instance, 'storage_location');
    expect(pathKey(storage.directory)).toBe(pathKey(data));
    expect(pathKey(storage.database_path)).toBe(pathKey(databasePath));
    expect(storage.read_only).toBe(readOnly);
    await expect(instance.page.locator('#runtime-status')).toContainText(readOnly ? 'Read-only storage · update required' : 'Local runtime');
    expect(await invoke(instance, 'terminal_list')).toEqual([]);
    return instance;
  } catch (error) { await forceClose(instance); throw error; }
}

async function closeApp(instance) {
  expect(await invoke(instance, 'terminal_list')).toEqual([]);
  await Promise.all([
    instance.page.waitForEvent('close', { timeout: 10000 }),
    waitForExit(instance.app, 10000),
    instance.page.getByRole('button', { name: 'Close window', exact: true }).click({ timeout: 10000 }),
  ]);
  expect(instance.app.exitCode).toBe(0);
  await instance.browser.close().catch(() => {});
  expect(instance.errors).toEqual([]);
  live.delete(instance);
}

async function selectFolder(instance, path) {
  await instance.page.locator('#project').click(); await instance.page.getByRole('button', { name: 'Project roots…', exact: true }).click();
  await instance.page.locator('#folder').fill(path);
  await instance.page.getByRole('button', { name: 'Use folder', exact: true }).click();
  await expect(instance.page.locator('#project-dialog')).not.toBeVisible();
}

function snapshotDatabase() {
  const database = new DatabaseSync(databasePath, { readOnly: true });
  try {
    return {
      version: database.prepare('PRAGMA user_version').get().user_version,
      journal: database.prepare('PRAGMA journal_mode').get().journal_mode,
      integrity: database.prepare('PRAGMA integrity_check').get().integrity_check,
      projects: database.prepare('SELECT * FROM project ORDER BY id').all(),
      settings: database.prepare('SELECT key,value FROM setting ORDER BY key').all(),
    };
  } finally { database.close(); }
}

try {
  let instance = await openApp();
  const imported = await invoke(instance, 'project_recent');
  expect(imported.map(project => project.name)).toEqual(['first-project', 'second-project']);
  expect(imported.map(project => project.last_opened)).toEqual([1700000001000, 1700000000000]);
  expect(imported.every(project => project.available && /^p_[a-z2-7]{12}$/.test(project.id))).toBe(true);
  expect(new Set(imported.map(project => project.id)).size).toBe(2);
  await expect(instance.page.locator('#project')).toHaveAttribute('data-project', 'first-project');
  await instance.page.locator('#project').click(); await instance.page.getByRole('button', { name: 'Project roots…', exact: true }).click();
  await expect(instance.page.locator('.recent-project strong')).toHaveText(['first-project', 'second-project']);
  await instance.page.locator('#project-form').getByRole('button', { name: 'Close', exact: true }).filter({ hasText: 'Close' }).click();
  await selectFolder(instance, second);
  await expect(instance.page.locator('#project')).toHaveAttribute('data-project', 'second-project');
  const selected = await invoke(instance, 'project_recent');
  expect(selected.map(project => project.id)).toEqual([imported[1].id, imported[0].id]);
  expect(selected[0].last_opened).toBeGreaterThan(imported[0].last_opened);
  expect(await readFile(legacyPath)).toEqual(legacy);
  await closeApp(instance);
  expect(await readFile(legacyPath)).toEqual(legacy);
  const initialSnapshot = snapshotDatabase();
  expect(initialSnapshot.version).toBe(expectedSchema);
  expect(initialSnapshot.journal).toBe('wal');
  expect(initialSnapshot.integrity).toBe('ok');
  expect(initialSnapshot.projects).toHaveLength(2);
  expect(initialSnapshot.settings.filter(row => row.key === 'migration.projects_json.v1')).toHaveLength(1);
  evidence.checks.legacyImportedOnceWithStableIdentityAndOriginalBytesPreserved = true;
  evidence.checks.uiSelectionPersistsOnlyInWalSqlite = true;

  const corruptedLegacy = Buffer.from('deliberately invalid legacy JSON after completed SQLite import\n');
  await writeFile(legacyPath, corruptedLegacy);
  instance = await openApp();
  await expect(instance.page.locator('#project')).toHaveAttribute('data-project', 'second-project');
  const restored = await invoke(instance, 'project_recent');
  expect(restored).toEqual(selected);
  await instance.page.locator('#project').click(); await instance.page.getByRole('button', { name: 'Project roots…', exact: true }).click();
  await expect(instance.page.locator('.recent-project strong')).toHaveText(['second-project', 'first-project']);
  await instance.page.locator('#project-form').getByRole('button', { name: 'Close', exact: true }).filter({ hasText: 'Close' }).click();
  expect(await readFile(legacyPath)).toEqual(corruptedLegacy);
  await closeApp(instance);
  // The window's own state (ui.*) is saved again at each start: the thread sidebar lists the
  // toolbar's project (workset-sidebar.ts setProject) and the layout gets a new revision. So is the
  // running app's bookkeeping (operational.*): the model proxy starts with Shinatama and reserves
  // its resources. The data must come back untouched; the read-only run below still compares the
  // file byte for byte.
  const withoutWindowState = snapshot => ({ ...snapshot, settings: snapshot.settings.filter(row => !/^(ui|operational)\./.test(row.key)) });
  expect(withoutWindowState(snapshotDatabase())).toEqual(withoutWindowState(initialSnapshot));
  evidence.checks.restartUsesVerifiedSqliteDespiteCorruptLegacyJson = true;

  // Only the synthetic database is changed, while its application is closed.
  const future = new DatabaseSync(databasePath);
  try { future.exec('PRAGMA user_version=999; PRAGMA wal_checkpoint(TRUNCATE);'); }
  finally { future.close(); }
  const futureBytes = await readFile(databasePath);
  const beforeReadOnly = snapshotDatabase();
  expect(beforeReadOnly.version).toBe(999);
  instance = await openApp(true);
  await expect(instance.page.locator('#project')).toHaveAttribute('data-project', 'second-project');
  await expect(instance.page.locator('#notice')).toContainText('999');
  expect(await invoke(instance, 'project_recent')).toEqual(selected);
  const rejected = await invoke(instance, 'project_select', { path: first })
    .then(() => ({ rejected: false, message: '' }), error => ({ rejected: true, message: String(error) }));
  expect(rejected.rejected).toBe(true); expect(rejected.message).toContain('999');
  await instance.page.locator('#project').click(); await instance.page.getByRole('button', { name: 'Project roots…', exact: true }).click();
  await instance.page.locator('#folder').fill(first);
  await instance.page.getByRole('button', { name: 'Use folder', exact: true }).click();
  await expect(instance.page.locator('#project-error')).toContainText('999');
  await expect(instance.page.locator('#project-dialog')).toBeVisible();
  await expect(instance.page.locator('#project')).toHaveAttribute('data-project', 'second-project');
  await instance.page.locator('#project-form').getByRole('button', { name: 'Close', exact: true }).filter({ hasText: 'Close' }).click();
  await closeApp(instance);
  expect(await readFile(databasePath)).toEqual(futureBytes);
  expect(snapshotDatabase()).toEqual(beforeReadOnly);
  expect(await readFile(legacyPath)).toEqual(corruptedLegacy);
  expect(await readdir(first)).toEqual([]); expect(await readdir(second)).toEqual([]);
  evidence.checks.newerSchemaShowsReadOnlyAndRejectsApiAndUiWrites = true;
  evidence.checks.readOnlyClosePreservesDatabaseBytesAndAllRows = true;
  evidence.checks.projectFoldersUntouchedAndNoAutomaticTerminals = true;
  evidence.status = 'passed';
  await writeFile(evidencePath, `${JSON.stringify(evidence, null, 2)}\n`);
  console.log(`PASS: embedded app imports legacy history once into schema ${expectedSchema} WAL SQLite; stable IDs and selection survive restart; corrupt legacy is ignored after import; newer schema stays read-only and byte-preserved; themed close works on all three launches.`);
} catch (error) {
  evidence.status = 'failed'; evidence.failure = error.message;
  await writeFile(evidencePath, `${JSON.stringify(evidence, null, 2)}\n`);
  throw error;
} finally {
  for (const instance of [...live]) await forceClose(instance);
  // The only recursive cleanup target is the uniquely allocated test directory.
  if (dirname(fixture) === resolve(tmpdir()) && basename(fixture).startsWith('shinatama-state-smoke-')) {
    await rm(fixture, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  }
}
