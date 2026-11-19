// Drives an embedded, feature-gated proof binary. Node and CDP are test drivers,
// never dependencies or open ports of the normally launched application.
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { basename, dirname, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('TL1 currently qualifies Windows only.');
const project = process.cwd();
const output = resolve(project, '.prototype-evidence/tl1');
const workspace = resolve(output, 'workspace');
const port = 19228;
const cli = process.env.SHINATAMA_TL1_AGENT || resolve(process.env.USERPROFILE, '.local/bin/claude.exe');
const agentVersion = execFileSync(cli, ['--version'], { encoding: 'utf8', windowsHide: true, timeout: 10000, maxBuffer: 4096 }).trim();
await mkdir(resolve(workspace, 'notes'), { recursive: true });
await mkdir(resolve(workspace, 'src'), { recursive: true });
const markdown = '# TL1 local workspace\n\nMarkdown source is a real local file.\n\n## Nested panes\n\n- Rust owns the PTY\n- WebView2 displays local content\n\n```rust\nfn main() { println!("TL1"); }\n```\n\n<script>window.__TL1_INJECTED__ = true</script>\n\n![remote](https://example.invalid/tl1.png)\n';
await writeFile(resolve(workspace, 'notes/proof.md'), markdown, 'utf8');
await writeFile(resolve(workspace, 'src/main.rs'), 'fn main() { println!("REAL_FILE_TL1"); }\n', 'utf8');
await writeFile(resolve(workspace, 'too-large.txt'), 'x'.repeat(65537));
await writeFile(resolve(workspace, 'binary.dat'), Buffer.from([0xff, 0xfe, 0x00]));
await writeFile(resolve(workspace, '.tl1-settings.json'), '{"disableAllHooks":true}\n', 'utf8');
await writeFile(resolve(workspace, '.tl1-mcp.json'), '{"mcpServers":{}}\n', 'utf8');
await writeFile(resolve(output, 'outside.txt'), 'TL1_OUTSIDE_DENIED', 'utf8');
const temporaryHome = await mkdtemp(resolve(tmpdir(), 'shinatama-tl1-home-'));
const app = spawn(resolve(project, 'target/debug/shinatama-desktop.exe'), [], {
  cwd: workspace, windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'],
  env: { ...process.env, SHINATAMA_HOME: temporaryHome, SHINATAMA_TL1_ROOT: workspace,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
    WEBVIEW2_USER_DATA_FOLDER: resolve(output, 'webview-profile') },
});
let diagnostics = '';
app.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-4096); });
let browser;
let page;
function processNames() {
  // Executable names only: no command lines, environment or account files.
  return JSON.parse(execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
    `$all = @(Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name); $ids = [System.Collections.Generic.HashSet[int]]::new(); [void]$ids.Add(${app.pid}); do { $changed = $false; foreach ($p in $all) { if ($ids.Contains([int]$p.ParentProcessId) -and $ids.Add([int]$p.ProcessId)) { $changed = $true } } } while ($changed); ConvertTo-Json -Compress -InputObject @($all | Where-Object { $ids.Contains([int]$_.ProcessId) } | Select-Object -ExpandProperty Name)`], { encoding: 'utf8', windowsHide: true }));
}
const evidence = { gate: 'TL1', platform: 'win32-x64', fixtureOnly: true,
  runtime: '', agent: 'Claude Code native interactive CLI',
  agentVersion,
  modelPromptSubmitted: false,
  checks: {}, gaps: ['Windows without WebView2 was not tested; installer handling remains open in ADR-0048.',
    'The proof renderer supports inert headings, paragraphs, lists and fenced code only; full Markdown and Mermaid fidelity remain TL4/TL6.'] };
await writeFile(resolve(output, 'result.json'), `${JSON.stringify({ ...evidence, status: 'running' }, null, 2)}\n`);
const timeout = (message) => { throw new Error(message); };
try {
  for (let attempt = 0; attempt < 60; attempt++) {
    if (app.exitCode !== null) throw new Error(`TL1 binary exited with code ${app.exitCode}; ${diagnostics}`);
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; }
    catch { await delay(500); }
  }
  if (!browser) timeout('TL1 WebView2 did not expose the test endpoint.');
  evidence.runtime = browser.version();
  const context = browser.contexts()[0];
  page = context.pages()[0] || await context.waitForEvent('page');
  const errors = [];
  const requests = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => {
    if (!/^https?:/.test(request.url())) return;
    const hostname = new URL(request.url()).hostname;
    if (!['tauri.localhost', 'ipc.localhost'].includes(hostname)) requests.push(hostname);
  });
  await expect(page.locator('#runtime')).toContainText('Embedded Tauri', { timeout: 30000 });
  expect(new URL(page.url()).hostname).toBe('tauri.localhost');
  const invoke = (command, args) => page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
  const terminalText = () => page.evaluate(() => window.__TL1__.text());
  const terminalContains = pattern => terminalText().then(value => typeof pattern === 'string' ? value.includes(pattern) : pattern.test(value));
  const sessionId = () => page.evaluate(() => window.__TL1__.sessionId());
  const rejected = async relativePath => {
    const error = await invoke('tl1_read', { relativePath }).then(() => null, error => String(error));
    expect(error).not.toBeNull();
  };
  await page.locator('[data-path="notes/proof.md"]').click();
  await expect(page.locator('#source')).toHaveText(markdown);
  await expect(page.locator('#preview h1')).toHaveText('TL1 local workspace');
  await expect(page.locator('#preview h2')).toHaveText('Nested panes');
  await expect(page.locator('#preview li')).toHaveCount(2);
  await expect(page.locator('#preview pre')).toContainText('fn main()');
  expect(await page.locator('#preview script, #preview img, #preview iframe, #preview a').count()).toBe(0);
  expect(await page.evaluate(() => window.__TL1_INJECTED__)).toBeUndefined();
  expect(requests).toEqual([]);
  evidence.checks.inertMarkdownSourceAndPreview = true;
  await page.locator('[data-path="src/main.rs"]').click();
  await expect(page.locator('#source')).toContainText('REAL_FILE_TL1');
  await writeFile(resolve(workspace, 'src/main.rs'), '// CHANGED_ON_DISK_TL1\n', 'utf8');
  await page.locator('[data-path="src/main.rs"]').click();
  await expect(page.locator('#source')).toHaveText('// CHANGED_ON_DISK_TL1\n');
  await rejected('../outside.txt'); await rejected('too-large.txt'); await rejected('binary.dat');
  evidence.checks.actualDiskNavigationAndBoundedRead = true;
  await page.locator('[data-path="notes/proof.md"]').click();
  await page.locator('#start').click();
  await expect.poll(() => terminalContains('PS '), { timeout: 30000 }).toBe(true);
  const id = await sessionId();
  expect(id).toBeTruthy();
  await page.locator('.xterm-helper-textarea').focus();
  await page.keyboard.type("Write-Output ('TL1_' + 'SHELL_READY')"); await page.keyboard.press('Enter');
  await expect.poll(() => terminalContains('TL1_SHELL_READY')).toBe(true);
  const input = async command => { await page.locator('.xterm-helper-textarea').focus(); await page.keyboard.type(command); await page.keyboard.press('Enter'); };
  // No model prompt is sent. /help is a native local command, not a model request.
  const quote = value => `'${value.replaceAll("'", "''")}'`;
  // File arguments also avoid legacy Windows PowerShell's native JSON quoting.
  const command = `$env:DISABLE_AUTOUPDATER='1'; $env:CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC='1'; & ${quote(cli)} --setting-sources project --settings '.tl1-settings.json' --strict-mcp-config --mcp-config '.tl1-mcp.json' --no-chrome`;
  await input(command);
  let ready = false;
  let sawTrust = false;
  for (let attempt = 0; attempt < 90; attempt++) {
    const screen = await terminalText();
    const current = screen.slice(-18000);
    if (/select.*(?:login|log.in)|choose.*(?:login|log.in)|sign in with|login method|paste.*authentication|browser.*sign.in|OAuth code/i.test(current)) timeout('Claude native TUI needs interactive login. No credentials or model prompt were supplied.');
    if (/choose the text style|choose.*theme|select.*theme/i.test(current)) { await page.keyboard.press('Enter'); await delay(600); }
    else if (/do you trust|trust this folder|project you created or one you trust|yes, i trust/i.test(current) && !sawTrust) { sawTrust = true; await page.keyboard.press('Enter'); await delay(700); }
    else if (/(?:Claude Code v|Claude Code \w*\d|Welcome (?:back|to Claude Code)|Try ")/.test(current) && /(?:for shortcuts|\? for|bypass permissions|accept edits|No recent activity|What should Claude|❯)/i.test(current)) { ready = true; break; }
    await delay(500);
  }
  if (!ready) timeout('Claude did not reach a recognized native interactive prompt within 45 seconds. Raw terminal output is deliberately not saved.');
  expect(processNames().some(name => /^claude\.exe$/i.test(name))).toBe(true);
  evidence.checks.realInteractiveAgentInConPty = true;
  await input('/help');
  await expect.poll(() => terminalContains(/(?:Claude Code help|Available commands|Keyboard shortcuts|General commands|Commands|Getting started)/i), { timeout: 15000 }).toBe(true);
  evidence.checks.localAgentHelpCommand = true;
  // Resize two nested levels, using keyboard and actual pointer drags, while Claude is live.
  const originalSize = await page.evaluate(() => window.__TL1__.size());
  const terminalSplit = page.getByRole('separator', { name: 'Resize terminal pane', exact: true });
  await terminalSplit.focus(); await page.keyboard.press('ArrowDown'); await page.keyboard.press('ArrowDown');
  const fileSplit = page.getByRole('separator', { name: 'Resize files pane', exact: true });
  const bounds = await fileSplit.boundingBox();
  await page.mouse.move(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
  await page.mouse.down(); await page.mouse.move(bounds.x + 80, bounds.y + bounds.height / 2, { steps: 8 }); await page.mouse.up();
  await expect.poll(async () => (await page.evaluate(() => window.__TL1__.size())).cols).not.toBe(originalSize.cols);
  expect(await sessionId()).toBe(id);
  expect((await invoke('terminal_list')).map(session => session.id)).toEqual([id]);
  const documentSplit = page.getByRole('separator', { name: 'Resize Markdown panes', exact: true });
  await documentSplit.focus(); await page.keyboard.press('ArrowRight');
  await expect(documentSplit).toHaveAttribute('aria-valuenow', '55');
  for (const selector of ['#root-split', '#content-split', '#document-split']) {
    const geometry = await page.locator(selector).evaluate(parent => {
      const first = parent.children[0].getBoundingClientRect();
      const last = parent.children[2].getBoundingClientRect();
      const horizontal = parent.classList.contains('horizontal');
      const firstSize = horizontal ? first.width : first.height;
      const lastSize = horizontal ? last.width : last.height;
      return { actual: 100 * firstSize / (firstSize + lastSize), declared: Number(parent.children[1].getAttribute('aria-valuenow')) };
    });
    expect(Math.abs(geometry.actual - geometry.declared)).toBeLessThan(1);
  }
  await page.locator('[data-path="src/main.rs"]').click();
  await expect(page.locator('#source')).toContainText('CHANGED_ON_DISK_TL1');
  expect(await sessionId()).toBe(id);
  expect(processNames().some(name => /^claude\.exe$/i.test(name))).toBe(true);
  evidence.checks.nestedKeyboardAndPointerSplitPreservesAgentSession = true;
  // Never send a shell marker until the native agent's exit is established.
  await page.locator('.xterm-helper-textarea').focus();
  await page.keyboard.press('Escape'); await page.keyboard.press('Escape');
  await page.keyboard.press('Control+c');
  await input('/exit');
  await expect.poll(() => processNames().some(name => /^claude\.exe$/i.test(name)), { timeout: 20000 }).toBe(false);
  evidence.checks.agentExitedBeforeShellProbe = true;
  await input("Write-Output ('TL1_' + 'AFTER_AGENT')");
  await expect.poll(() => terminalContains('TL1_AFTER_AGENT'), { timeout: 15000 }).toBe(true);
  evidence.checks.shellRemainsResponsiveAfterAgentExit = true;
  // Clear any native account presentation before writing the only screenshot.
  await input('Clear-Host'); await delay(350);
  await input("Write-Output ('TL1_' + 'PROOF_COMPLETE')");
  await expect.poll(() => terminalContains('TL1_PROOF_COMPLETE')).toBe(true);
  await page.locator('[data-path="notes/proof.md"]').click();
  await page.screenshot({ path: resolve(output, 'tl1-nested-panes.png') });
  expect(errors).toEqual([]); expect(requests).toEqual([]);
  evidence.checks.noFrontendErrorsOrRemotePreviewRequests = true;
  const descendants = processNames();
  expect(descendants.some(name => /^(?:node|chrome|msedge|firefox)\.exe$/i.test(name))).toBe(false);
  expect(descendants.some(name => /^msedgewebview2\.exe$/i.test(name))).toBe(true);
  evidence.checks.embeddedRuntimeWithoutNodeDevServerOrExternalBrowserChild = true;
  evidence.processNames = [...new Set(descendants)].sort();
  await page.locator('#stop').click();
  await expect.poll(() => invoke('terminal_list')).toHaveLength(0);
  await writeFile(resolve(output, 'result.json'), `${JSON.stringify({ ...evidence, status: 'passed' }, null, 2)}\n`);
  console.log(`PASS TL1: interactive native agent, local /help, nested panes, bounded real file reads, inert Markdown, embedded ${evidence.runtime}. No model prompt sent.`);
  console.log(`Sanitized evidence: ${resolve(output, 'result.json')}`);
} catch (error) {
  await writeFile(resolve(output, 'result.json'), `${JSON.stringify({ ...evidence, status: 'failed', failure: error.message }, null, 2)}\n`);
  throw error;
} finally {
  if (page && !page.isClosed()) await page.evaluate(() => window.__TAURI_INTERNALS__?.invoke('terminal_shutdown')).catch(() => {});
  if (browser) await browser.close().catch(() => {});
  if (app.exitCode === null) {
    const exited = new Promise(resolve => app.once('exit', resolve));
    app.kill(); await Promise.race([exited, delay(3000)]);
  }
  // Only remove the newly allocated synthetic home, after its native handles close.
  if (dirname(temporaryHome) === resolve(tmpdir()) && basename(temporaryHome).startsWith('shinatama-tl1-home-')) {
    await rm(temporaryHome, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  }
}
