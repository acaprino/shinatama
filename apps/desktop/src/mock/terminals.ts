// The answers of the browser for the commands of this area (see mock/index.ts). The agent that owns the area fills them in.
import type { Answers } from './index.ts';

export const answers: Answers = {
  terminal_list: [],
  terminal_resources: [],
  harness_catalog: [
    { id: 'claude_code', label: 'Claude Code', nativeResume: true },
    { id: 'codex', label: 'Codex', nativeResume: true },
    { id: 'opencode', label: 'OpenCode', nativeResume: false },
    { id: 'pi', label: 'Pi', nativeResume: false },
  ],
};
