// CX4 runtime probe v3: codex app-server + externalAgentConfig detect/import.
// Phases: [A] named synthetic session import + resume; [B] thread/items/list shape;
//         [C] age window; [D] session still being appended during import.
// Scratch ONLY (CODEX_HOME, USERPROFILE, CLAUDE_CONFIG_DIR inside the scratch root); counts only for
// anything not synthetic; no login, no model calls. Rerunnable without edits (scratch reset each run).
// Run through verify/CX4/run-runtime.ps1.
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { mkdirSync, rmSync, writeFileSync, appendFileSync, readFileSync, existsSync, readdirSync, utimesSync } from 'node:fs';
import { join } from 'node:path';
import { randomUUID } from 'node:crypto';

const scratch = process.env.STX_CX4_SCRATCH || join(process.env.LOCALAPPDATA || process.env.TEMP, 'shinatama-verify', 'CX4');
const codexBin = process.env.STX_CODEX_BIN || 'codex';
const codexHome = join(scratch, 'codex-home');
const profile = join(scratch, 'profile'); // scratch USERPROFILE: detect reads this ~/.claude, not the real one
const claudeHome = join(profile, '.claude');
const proj = join(scratch, 'proj');
rmSync(codexHome, { recursive: true, force: true });
rmSync(profile, { recursive: true, force: true });
rmSync(proj, { recursive: true, force: true });
for (const d of [codexHome, profile, claudeHome, proj]) mkdirSync(d, { recursive: true });

const marker = 'STX-CX4-MARKER-ALPHA';
const markerOld = 'STX-CX4-MARKER-OLD';
const markerApp = 'STX-CX4-MARKER-APPEND';
writeFileSync(join(proj, 'notes.txt'), `${marker}: the notes file says hello.\n`);
const flatten = proj.replace(/[^A-Za-z0-9]/g, '-');
const projDir = join(claudeHome, 'projects', flatten);
mkdirSync(projDir, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---- fixtures ----
const sid = randomUUID();
const u = (n) => `${n}-${sid}`;
const entries = [
  { parentUuid: null, isSidechain: false, userType: 'external', cwd: proj, sessionId: sid, version: '2.1.311', type: 'user', message: { role: 'user', content: `${marker} please read notes.txt and summarise it` }, uuid: u('u1'), timestamp: new Date().toISOString() },
  { parentUuid: u('u1'), isSidechain: false, userType: 'external', cwd: proj, sessionId: sid, version: '2.1.311', type: 'assistant', message: { id: `msg_${sid}`, type: 'message', role: 'assistant', model: 'claude-test', content: [{ type: 'text', text: 'Reading the file.' }, { type: 'tool_use', id: `toolu_${sid}`, name: 'Read', input: { file_path: join(proj, 'notes.txt') } }], stop_reason: null, usage: { input_tokens: 12, output_tokens: 18 } }, uuid: u('a1'), timestamp: new Date().toISOString() },
  { parentUuid: u('a1'), isSidechain: false, userType: 'external', cwd: proj, sessionId: sid, version: '2.1.311', type: 'user', message: { role: 'user', content: [{ type: 'tool_result', tool_use_id: `toolu_${sid}`, content: `${marker} hello` }] }, uuid: u('t1'), timestamp: new Date().toISOString() },
  { parentUuid: u('t1'), isSidechain: false, userType: 'external', cwd: proj, sessionId: sid, version: '2.1.311', type: 'assistant', message: { id: `msg2_${sid}`, type: 'message', role: 'assistant', model: 'claude-test', content: [{ type: 'text', text: `The file says hello. ${marker} END` }], stop_reason: 'end_turn', usage: { input_tokens: 30, output_tokens: 20 } }, uuid: u('a2'), timestamp: new Date().toISOString() },
];
const transcriptPath = join(projDir, `${sid}.jsonl`);
writeFileSync(transcriptPath, entries.map((e) => JSON.stringify(e)).join('\n') + '\n');
console.log(`fixture cwd: ${proj}`);
console.log(`fixture [A] session: ${transcriptPath}`);

// [C] old session, 40 days
const sidOld = randomUUID();
const oldFile = join(projDir, `${sidOld}.jsonl`);
const oldTs = (n) => new Date(Date.now() - 40 * 86400e3 + n * 1000).toISOString();
const entriesOld = [
  { parentUuid: null, isSidechain: false, userType: 'external', cwd: proj, sessionId: sidOld, version: '2.1.311', type: 'user', message: { role: 'user', content: `${markerOld} content from 40 days ago` }, uuid: `u1-${sidOld}`, timestamp: oldTs(0) },
  { parentUuid: `u1-${sidOld}`, isSidechain: false, userType: 'external', cwd: proj, sessionId: sidOld, version: '2.1.311', type: 'assistant', message: { id: `msg_${sidOld}`, type: 'message', role: 'assistant', model: 'claude-test', content: [{ type: 'text', text: `${markerOld} acknowledged` }], stop_reason: 'end_turn', usage: { input_tokens: 5, output_tokens: 5 } }, uuid: `a1-${sidOld}`, timestamp: oldTs(1) },
];
writeFileSync(oldFile, entriesOld.map((e) => JSON.stringify(e)).join('\n') + '\n');
const past = new Date(Date.now() - 40 * 86400e3);
utimesSync(oldFile, past, past);
console.log(`fixture [C] session (40 days old): ${oldFile}`);

// [D] session with a writer appending while we detect/import
const sidApp = randomUUID();
const appFile = join(projDir, `${sidApp}.jsonl`);
writeFileSync(appFile, [
  JSON.stringify({ parentUuid: null, isSidechain: false, userType: 'external', cwd: proj, sessionId: sidApp, version: '2.1.311', type: 'user', message: { role: 'user', content: `${markerApp} initial line` }, uuid: `u1-${sidApp}`, timestamp: new Date().toISOString() }),
  JSON.stringify({ parentUuid: `u1-${sidApp}`, isSidechain: false, userType: 'external', cwd: proj, sessionId: sidApp, version: '2.1.311', type: 'assistant', message: { id: `msg_${sidApp}`, type: 'message', role: 'assistant', model: 'claude-test', content: [{ type: 'text', text: `${markerApp} initial reply` }], stop_reason: 'end_turn', usage: { input_tokens: 5, output_tokens: 5 } }, uuid: `a1-${sidApp}`, timestamp: new Date().toISOString() }),
].join('\n') + '\n');
console.log(`fixture [D] session (being appended): ${appFile}`);

const child = spawn(codexBin, ['app-server'], {
  cwd: proj,
  env: { ...process.env, USERPROFILE: profile, HOME: profile, CODEX_HOME: codexHome, CLAUDE_CONFIG_DIR: claudeHome },
  stdio: ['pipe', 'pipe', 'pipe'],
  windowsHide: true,
});
let nextId = 1;
const pending = new Map();
const notificationMethods = [];
const importCompletions = [];
const rl = createInterface({ input: child.stdout });
rl.on('line', (line) => {
  let m;
  try { m = JSON.parse(line); } catch { notificationMethods.push('non-json-line'); return; }
  if (m.id !== undefined && pending.has(m.id)) { const p = pending.get(m.id); pending.delete(m.id); p(m); return; }
  if (m.method) {
    notificationMethods.push(m.method);
    if (m.method === 'externalAgentConfig/import/completed') importCompletions.push(m.params);
  }
});
child.stderr.on('data', (d) => { const s = d.toString().trim(); if (s) console.log('stderr:', s.slice(0, 500)); });
child.on('exit', (code, sig) => console.log(`app-server exit: code=${code} sig=${sig}`));

function send(method, params) {
  const id = nextId++;
  child.stdin.write(JSON.stringify({ id, method, params }) + '\n');
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { if (pending.delete(id)) reject(new Error(`timeout: ${method}`)); }, 90000);
    pending.set(id, (m) => { clearTimeout(timer); resolve(m); });
  });
}

function walk(dir) { return readdirSync(dir, { withFileTypes: true }).flatMap((e) => e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]); }
function collectSessions(result) {
  const items = result?.items ?? [];
  return items.filter((i) => i.itemType === 'SESSIONS').flatMap((i) => i.details?.sessions ?? []);
}
function reportPresence(label, result, markers) {
  if (!result) { console.log(`${label}: <empty>`); return; }
  const items = result.items ?? [];
  const counts = {};
  for (const it of items) counts[it.itemType] = (counts[it.itemType] ?? 0) + 1;
  const s = JSON.stringify(items);
  const seen = Object.entries(markers).map(([k, v]) => `${k}=${s.includes(v)}`).join(' ');
  console.log(`${label}: items ${items.length}; byType ${JSON.stringify(counts)}; ${seen}`);
}
async function importAndWait(item, label) {
  const before = importCompletions.length;
  const resp = await send('externalAgentConfig/import', { migrationItems: [item], providerId: 'shinatama-verify' });
  if (resp.error) { console.log(`${label} import error: ${JSON.stringify(resp.error).slice(0, 500)}`); return null; }
  console.log(`${label} import response: ${JSON.stringify(resp.result).slice(0, 300)}`);
  const importId = resp.result?.importId;
  for (let i = 0; i < 40; i++) {
    const c = importCompletions.slice(before).find((c) => !importId || c.importId === importId);
    if (c) { console.log(`${label} import completed: ${JSON.stringify(c).slice(0, 1200)}`); return c; }
    await sleep(500);
  }
  console.log(`${label} import completed: <none seen>`);
  return null;
}

async function main() {
  const init = await send('initialize', { clientInfo: { name: 'shinatama-verify-cx4', title: 'Shinatama verification probe', version: '0.3' }, capabilities: { experimentalApi: true } });
  if (init.error) console.log('initialize error:', JSON.stringify(init.error).slice(0, 400));
  else console.log('initialize result:', JSON.stringify(init.result).slice(0, 400));

  const detRepo = await send('externalAgentConfig/detect', { cwds: [proj], includeHome: false, maxSessions: 50, maxSessionAgeDays: 90 });
  reportPresence('[A] detect(repo, cwds=proj)', detRepo.result, { A: marker, old: markerOld, app: markerApp });

  const detHome = await send('externalAgentConfig/detect', { includeHome: true, maxSessions: 50, maxSessionAgeDays: 90 });
  reportPresence('[A] detect(home, maxSessionAgeDays=90)', detHome.result, { A: marker, old: markerOld, app: markerApp });
  const all = collectSessions(detHome.result);
  const entryA = all.find((e) => e.path && e.path.includes(sid));
  const itemA = entryA ? { itemType: 'SESSIONS', description: `synthetic ${marker}`, cwd: proj, details: { sessions: [entryA] } } : null;
  if (!itemA) { console.log('[A] marker session not detected; falling back to crafted item'); }
  await importAndWait(itemA ?? { itemType: 'SESSIONS', description: `synthetic ${marker}`, cwd: proj, details: { sessions: [{ cwd: proj, path: transcriptPath, title: `${marker} notes` }] } }, '[A]');

  const listCwd = await send('thread/list', { cwd: proj, limit: 20, useStateDbOnly: false });
  const threads = listCwd.result?.data ?? [];
  console.log(`[A] thread/list(cwd=proj): threads ${threads.length}`);
  for (const t of threads) console.log('  thread:', JSON.stringify({ id: t.id, preview: (t.preview ?? '').slice(0, 60) }).slice(0, 240));

  const threadA = threads.find((t) => (t.preview ?? '').startsWith(marker));
  if (threadA) {
    const res = await send('thread/resume', { threadId: threadA.id });
    if (res.error) console.log('[A] thread/resume error:', JSON.stringify(res.error).slice(0, 400));
    else {
      const t = res.result?.thread ?? res.result ?? {};
      console.log('[A] thread/resume ok:', JSON.stringify({ id: t.id, path: t.path, turns: Array.isArray(t.turns) ? t.turns.length : null }).slice(0, 300));
    }
    const items = await send('thread/items/list', { threadId: threadA.id, limit: 50 });
    if (items.error) console.log('[B] thread/items/list error:', JSON.stringify(items.error).slice(0, 400));
    else {
      const list = items.result?.data ?? items.result?.items ?? [];
      const counts = {};
      for (const it of list) counts[it.type] = (counts[it.type] ?? 0) + 1;
      console.log(`[B] thread/items/list: ${list.length} item(s), types ${JSON.stringify(counts)}`);
    }
  }

  const detDefault = await send('externalAgentConfig/detect', { includeHome: true, maxSessions: 50 });
  reportPresence('[C] detect(home, default age limit)', detDefault.result, { A: marker, old: markerOld });
  const det90 = await send('externalAgentConfig/detect', { includeHome: true, maxSessions: 50, maxSessionAgeDays: 90 });
  reportPresence('[C] detect(home, maxSessionAgeDays=90)', det90.result, { A: marker, old: markerOld });
  const entryOld = collectSessions(det90.result).find((e) => e.path && e.path.includes(sidOld));
  await importAndWait({ itemType: 'SESSIONS', description: `synthetic old ${markerOld}`, cwd: proj, details: { sessions: [entryOld ?? { cwd: proj, path: oldFile }] } }, '[C]');

  let appendCount = 0;
  const appender = setInterval(() => {
    appendCount++;
    const entry = { parentUuid: `a1-${sidApp}`, isSidechain: false, userType: 'external', cwd: proj, sessionId: sidApp, version: '2.1.311', type: 'assistant', message: { id: `msg_${sidApp}_${appendCount}`, type: 'message', role: 'assistant', model: 'claude-test', content: [{ type: 'text', text: `${markerApp} filler ${appendCount}` }], stop_reason: 'end_turn', usage: { input_tokens: 5, output_tokens: 5 } }, uuid: `a-${sidApp}-${appendCount}`, timestamp: new Date().toISOString() };
    try { appendFileSync(appFile, JSON.stringify(entry) + '\n'); } catch {}
  }, 400);
  await sleep(1200);
  const detApp = await send('externalAgentConfig/detect', { includeHome: true, maxSessions: 50, maxSessionAgeDays: 90 });
  reportPresence('[D] detect(home) while session is appended', detApp.result, { A: marker, old: markerOld, app: markerApp });
  const entryApp = collectSessions(detApp.result).find((e) => e.path && e.path.includes(sidApp));
  await importAndWait({ itemType: 'SESSIONS', description: `synthetic appended ${markerApp}`, cwd: proj, details: { sessions: [entryApp ?? { cwd: proj, path: appFile }] } }, '[D]');
  clearInterval(appender);
  console.log(`[D] appender wrote ${appendCount} lines during detect+import`);

  const listFinal = await send('thread/list', { cwd: proj, limit: 20, useStateDbOnly: false });
  const finalThreads = listFinal.result?.data ?? [];
  console.log(`[end] thread/list(cwd=proj): threads ${finalThreads.length}`);
  for (const t of finalThreads) console.log('  thread:', JSON.stringify({ id: t.id, preview: (t.preview ?? '').slice(0, 60) }).slice(0, 240));

  const sessionsDir = join(codexHome, 'sessions');
  const files = existsSync(sessionsDir) ? walk(sessionsDir).filter((f) => f.endsWith('.jsonl')) : [];
  console.log(`[end] codex-home rollouts: ${files.length}`);
  for (const f of files) {
    const txt = readFileSync(f, 'utf8');
    console.log(`  rollout ${f.slice(scratch.length + 1)} A=${txt.includes(marker)} old=${txt.includes(markerOld)} app=${txt.includes(markerApp)} toolMarker=${txt.includes('external_agent_tool_call')} bytes=${txt.length}`);
  }
  console.log('[end] notification methods seen:', [...new Set(notificationMethods)].join(', ') || '<none>');
}

main()
  .catch((e) => console.error('FAIL:', e.message))
  .finally(() => { child.kill(); setTimeout(() => process.exit(0), 700); });
