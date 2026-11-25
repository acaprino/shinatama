// The answers of the browser for the commands of this area (see mock/index.ts): the terminals Rust holds and what they print. Four
// sessions run (an agent of each kind and a shell), each with a script of output that looks like the real thing, so the workspace shows
// a believable window with no Rust. What the person types is echoed back, a stopped terminal ends, and a session opened in the browser
// (`agent_open`, `shell_open`, `thread_resume`) is a new terminal with its own greeting.
import type { Answers } from './index.ts';

export type MockSession = { id: string; cwd: string; shell: string };

/** The terminals that run at the start, with the thread each belongs to (see mock/threads.ts). */
export const RUNNING: MockSession[] = [
  { id: 'i_aaaaaaaaaaaa', cwd: 'C:\\work\\shinatama', shell: 'claude_code' },
  { id: 'i_aaaaaaaaaaab', cwd: 'C:\\work\\shinatama', shell: 'codex' },
  { id: 'i_aaaaaaaaaaac', cwd: 'C:\\work\\shinatama', shell: 'PowerShell' },
  { id: 'i_bbbbbbbbbbba', cwd: 'C:\\work\\xiox', shell: 'opencode' },
];

const E = '\x1b[';
const reset = `${E}0m`;
const bold = (text: string) => `${E}1m${text}${reset}`;
const dim = (text: string) => `${E}2m${text}${reset}`;
const green = (text: string) => `${E}32m${text}${reset}`;
const red = (text: string) => `${E}31m${text}${reset}`;
const cyan = (text: string) => `${E}36m${text}${reset}`;
const yellow = (text: string) => `${E}33m${text}${reset}`;
const violet = (text: string) => `${E}35m${text}${reset}`;
const lines = (...rows: string[]) => `${rows.join('\r\n')}\r\n`;

function claude(): string {
  const turns: string[] = [];
  for (let step = 1; step <= 5; step++) {
    turns.push(
      `${green('●')} I will read the plan and the module ${step} first.`, '',
      `${green('●')} ${bold('Read')}(src/controllers/layout.ts)`, dim(`  ⎿  Read ${80 + step * 12} lines`), '',
      `${green('●')} ${bold('Update')}(src/controllers/layout.ts)`,
      dim(`  ⎿  Updated src/controllers/layout.ts with ${10 + step} additions and ${step} removals`),
      `${red(`    ${38 + step} -  const wait = base * attempt;`)}`,
      `${green(`    ${38 + step} +  const wait = Math.min(cap, base * 2 ** attempt) + jitter();`)}`, '',
    );
  }
  turns.push(
    `${green('●')} ${bold('Bash')}(npm test -- layout)`,
    dim('  ⎿  PASS src/layout.test.ts'), `${green('     ✓')} focuses the pane that was asked for (12 ms)`,
    `${green('     ✓')} keeps the saved tabs of a thread that is on (3 ms)`, `${red('     ✗')} drops a tab that no session holds`,
    '     Tests: 1 failed, 6 passed', '',
    `${green('●')} The key is built inside the loop, so each attempt gets a new one. I will build it once, before the first attempt.`, '',
    `${green('●')} ${bold('Bash')}(git push origin feature/workspace)`, '  Do you want to proceed?',
    `  ${violet('❯')} 1. Yes`, '    2. Yes, and do not ask again for git push', '    3. No, and tell Claude what to do differently',
  );
  return `${bold('✻ Welcome to Claude Code')}\r\n${dim('  cwd: C:\\work\\shinatama')}\r\n\r\n${lines(...turns)}`;
}

function codex(): string {
  return lines(
    bold('>_ OpenAI Codex'), dim('   model: gpt-5 · C:\\work\\shinatama'), '',
    `${cyan('user')}  review the webhook retry change`, '',
    `${yellow('codex')}  Reading the diff of src/retry.ts. Two things stand out:`,
    '  1. the idempotency key is built inside the loop',
    '  2. a retry of a 4xx is never useful', '',
    `${green('exec')}  git diff --stat main`, dim(' src/retry.ts      | 24 ++++++++++-----'), dim(' src/retry.test.ts | 41 +++++++++++++++++++++++'),
    dim(' 2 files changed, 59 insertions(+), 6 deletions(-)'), '',
    `${yellow('codex')}  I would reject payloads older than 5 minutes: the header carries the timestamp.`,
  );
}

function shell(cwd: string): string {
  return lines(
    'Windows PowerShell', 'Copyright (C) Microsoft Corporation. All rights reserved.', '',
    `${green('PS')} ${cwd}> git status -sb`, '## develop...origin/develop', ' M apps/desktop/src/screens/workspace/index.tsx',
    '?? apps/desktop/src/screens/workspace/terminal-tab.tsx', '',
  ) + `${green('PS')} ${cwd}> `;
}

function opencode(): string {
  return lines(
    bold(' OpenCode '), dim(' C:\\work\\xiox'), '',
    `${cyan('›')} port the cleaner's report to the new rows`, '',
    `${violet('◆')} read crates/xiox-cleaner/src/report.rs`, `${violet('◆')} edit crates/xiox-cleaner/src/report.rs`, dim('   +14 -3'),
    `${green('✓')} cargo fmt --all --check`,
  );
}

function greeting(session: MockSession): string {
  if (session.shell === 'claude_code') return claude();
  if (session.shell === 'codex') return codex();
  if (session.shell === 'opencode') return opencode();
  return shell(session.cwd);
}

type Live = { session: MockSession; pending: string[]; ended: boolean };
const live = new Map<string, Live>();
let sequence = 0;

function start(session: MockSession, quiet = false): void {
  live.set(session.id, { session, pending: quiet ? [] : [greeting(session)], ended: false });
}
for (const session of RUNNING) start(session);

const bytes = (text: string) => Array.from(new TextEncoder().encode(text));
const wait = (ms: number) => new Promise<void>(resolve => setTimeout(resolve, ms));
const ALPHABET = 'abcdefghijklmnopqrstuvwxyz234567';
const code = (n: number): string => { let out = ''; for (let at = 0; at < 11; at++) { out = ALPHABET.charAt(n % 32) + out; n = Math.floor(n / 32); } return out; };
const newId = () => `i_z${code(++sequence)}`;

function open(shellName: string, cwd: string): MockSession {
  const session = { id: newId(), cwd, shell: shellName };
  start(session);
  return session;
}

export const answers: Answers = {
  terminal_list: () => [...live.values()].filter(one => !one.ended).map(one => one.session),
  terminal_read: async args => {
    const id = (args as { id: string }).id;
    const one = live.get(id);
    if (!one) return { chunks: [], eof: true, exit_code: 0, error: null };
    if (one.pending.length === 0 && !one.ended) await wait(150);
    const chunks = one.pending.splice(0).map(bytes);
    return { chunks, eof: one.ended && one.pending.length === 0, exit_code: one.ended ? 0 : null, error: null };
  },
  terminal_write: args => {
    const { id, data } = args as { id: string; data: number[] };
    const one = live.get(id);
    if (!one || one.ended) return null;
    const text = new TextDecoder().decode(new Uint8Array(data));
    // A line the person typed is echoed, and Enter answers with a new prompt.
    one.pending.push(text.replace(/\r/g, '\r\n') + (text.includes('\r') ? `${green('PS')} ${one.session.cwd}> ` : ''));
    return null;
  },
  terminal_resize: null,
  terminal_stop: args => {
    const one = live.get((args as { id: string }).id);
    if (one) one.ended = true;
    return null;
  },
  terminal_shutdown: null,
  terminal_resources: () => [...live.values()].filter(one => !one.ended).map((one, index) => ({
    id: one.session.id, phase: 'running', reason: 'observed', sampleAgeMs: 400 + index,
    observed: { privateCommitBytes: (84 + index * 31) * 1024 * 1024, residentBytes: (120 + index * 20) * 1024 * 1024, peakJobCommitBytes: 300 * 1024 * 1024, activeProcesses: 3 + index, cpuTime100ns: 42_000_000 * (index + 1), membershipComplete: true },
    applied: { jobCommitBytes: 2048 * 1024 * 1024, processCommitBytes: 1024 * 1024 * 1024, limitsEnforced: true, processes: 32, cpuBasisPoints: 5000, queueBytes: 4_194_304, queueItems: 1024 },
    queues: { bytes: 0, items: 0, peakBytes: 2048, peakItems: 2, refusals: 0, closed: false },
  })),
  resources_telemetry: {
    accepted: true, policy: { stage: 'normal', reason: 'observed', reduce_optional: false, defer_new_starts: false },
    resources: { owned_private_commit_bytes: 1_150_000_000, system_remaining_commit_bytes: 12_000_000_000, observed_processes: 14 },
  },
  terminal_paste_image: 'C:\\Users\\me\\AppData\\Roaming\\Shinatama\\pastes\\clipboard.png',
  terminal_clipboard_files: [],
  shell_open: args => {
    const { cwd } = args as { cwd: string };
    const session = open('PowerShell', cwd);
    return { session, threadId: `t_${session.id.slice(2)}` };
  },
  agent_open: args => {
    const { agentId, cwd } = args as { agentId: string; cwd: string };
    const session = open(agentId, cwd);
    return { session, threadId: `t_${session.id.slice(2)}` };
  },
  thread_resume: args => {
    const { threadId } = (args as { request: { threadId: string } }).request;
    const session = open('claude_code', 'C:\\work\\shinatama');
    return { session, threadId };
  },
  harness_catalog: [
    { id: 'claude_code', label: 'Claude Code', nativeResume: true },
    { id: 'codex', label: 'Codex', nativeResume: true },
    { id: 'opencode', label: 'OpenCode', nativeResume: false },
    { id: 'pi', label: 'Pi', nativeResume: false },
  ],
};
