// The answers of the browser for the commands of this area (see mock/index.ts). The agent that owns the area fills them in.
import type { Answers } from './index.ts';

export const answers: Answers = {
  workset_list: (args) => ({
    rows: (args?.request as { projectId?: string } | undefined)?.projectId === 'p_aaaaaaaaaaaa'
      ? [{ id: 'w_aaaaaaaaaaaa', projectId: 'p_aaaaaaaaaaaa', title: 'Migrate to Imago', archived: false, revision: 1, usedAt: 1_700_000_000_000, sessions: 0, open: 0, lead: null }]
      : [],
    more: false,
  }),
  workset_threads: [],
  workset_of: null,
  thread_sidebar_load: { revision: 1, settings: { version: 1, width: 236, collapsed: false, activeProjects: ['p_aaaaaaaaaaaa'] } },
  thread_sidebar_save: (args) => ({ revision: 2, settings: (args?.request as { settings: unknown }).settings }),
  thread_chat_list: [],
  thread_names: [],
};
