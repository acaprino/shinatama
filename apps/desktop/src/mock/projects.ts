// The answers of the browser for the commands of this area (see mock/index.ts). The agent that owns the area fills them in.
import type { Answers } from './index.ts';

export const PROJECTS = [
  { id: 'p_aaaaaaaaaaaa', path: 'C:\\work\\shinatama', name: 'shinatama', last_opened: 1_700_000_000_000, available: true },
  { id: 'p_bbbbbbbbbbbb', path: 'C:\\work\\xiox', name: 'xiox', last_opened: 1_690_000_000_000, available: true },
];

export const answers: Answers = {
  project_path: PROJECTS[0].path,
  project_recent: PROJECTS,
};
