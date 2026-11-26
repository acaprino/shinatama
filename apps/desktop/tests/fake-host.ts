// A window with no Rust: the controllers of the app on a table of answers, for the tests of the controllers. Every command that is
// not in the table refuses; a test that needs one adds it.
import { overrideInvoke } from '../src/api/core.ts';
import { Shinatama } from '../src/app.ts';

export type Answers = Record<string, unknown | ((args: Record<string, unknown> | undefined) => unknown)>;

export interface FakeHost { app: Shinatama; calls: { command: string; args: Record<string, unknown> | undefined }[]; notices: { text: string; error: boolean }[] }

export function fakeHost(answers: Answers = {}): FakeHost {
  const calls: FakeHost['calls'] = [];
  overrideInvoke(async (command, args) => {
    calls.push({ command, args });
    if (!(command in answers)) throw `no answer for ${command}`;
    const one = answers[command];
    return typeof one === 'function' ? (one as (args: Record<string, unknown> | undefined) => unknown)(args) : one;
  });
  const app = new Shinatama();
  const notices: FakeHost['notices'] = [];
  app.ui.notice = (text, say) => { notices.push({ text, error: say?.error === true }); };
  return { app, calls, notices };
}

export const tick = (ms = 0): Promise<void> => new Promise(resolve => setTimeout(resolve, ms));

export const WORKSET = { id: 'w_aaaaaaaaaaaa', projectId: 'p_aaaaaaaaaaaa' };
export const row = (id: string, extra: Record<string, unknown> = {}) => ({
  id, projectId: WORKSET.projectId, kind: 'work', title: null, revision: 1, updatedAt: 1, harness: 'claude_code', nativeAvailable: true, status: 'inactive', ...extra,
});
