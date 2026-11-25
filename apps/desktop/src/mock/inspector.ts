// The answers of the browser for what the inspector's panels read besides the sessions and the files: what agents left on disk.
// Development only.
import type { Answers } from './index.ts';

const NOW = Date.now();
export const answers: Answers = {
  project_leftovers: {
    available: true, reason: null,
    projects: [{
      folders: ['C:\\work\\shinatama'], liveSessions: 2, suspects: { wt: [{ terminalId: 'i_aaaaaaaaaaaa', harness: 'claude_code', open: true }] }, phase: 'watching', root: 'C:\\work\\shinatama', name: 'shinatama', isRepo: true,
      newItems: [
        { key: 'wt', category: 'worktrees', path: 'C:\\work\\shinatama-wt-retry', label: null, bytes: 1_288_490_188, bytesPartial: false, level: 'recommended', blocked: [], reason: 'A worktree an agent made', firstSeenMs: NOW - 600_000, sessions: [] },
        { key: 'copy', category: 'copies', path: 'C:\\work\\shinatama-copy', label: null, bytes: 901_775_360, bytesPartial: false, level: 'review', blocked: [], reason: 'A copy of the project', firstSeenMs: NOW - 3_600_000, sessions: [{ harness: 'codex', id: 'cx-idem-0001', strength: 'created' }] },
      ],
      newItemsOmitted: 0, growthBytes: 2_190_265_548, attention: ['worktrees'], stale: false,
      cleanCommand: 'xiox clean --recommended', cleanCommandTruncated: false, startedAtMs: NOW - 7_200_000, checkedAtMs: NOW - 60_000, message: null,
    }],
  },
  project_leftovers_check: null,
  leftovers_reveal: null,
};
